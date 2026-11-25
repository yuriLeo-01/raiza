//! O [`Provider`] do PipeWire.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use futures_util::StreamExt;
use futures_util::stream::{self, BoxStream};
use raiza_core::{
    Capability, CapabilityKind, CapabilityValue, DeviceId, DeviceInfo, EventStream, Provider,
    ProviderEvent, RaizaError,
};
use tracing::{debug, info};

use crate::PROVIDER_ID;
use crate::backend::{BackendError, MonitorEvent, PipeWireBackend, PwCli};
use crate::graph::Graph;
use crate::scan::{AudioDevice, scan};
use crate::sysfs::{CardResolver, SysfsCardResolver};

/// Provedor `pipewire`: `Volume` e `Mute` de microfones e headsets USB e Bluetooth.
///
/// `B` fala com o PipeWire e `R` resolve placas ALSA para ids USB; os dois são
/// injetáveis para testes. [`PipeWireProvider::new`] usa os reais.
pub struct PipeWireProvider<B = PwCli, R = SysfsCardResolver> {
    inner: Arc<Inner<B, R>>,
}

struct Inner<B, R> {
    backend: B,
    resolver: R,
    /// Último estado anunciado (por `discover` ou pelos eventos), para calcular diferenças.
    known: Mutex<BTreeMap<DeviceId, DeviceInfo>>,
}

impl PipeWireProvider {
    /// Provedor real: `pw-dump`/`wpctl` e o `/sys` da máquina.
    pub fn new() -> Self {
        Self::with_backend(PwCli::new(), SysfsCardResolver::new())
    }
}

impl Default for PipeWireProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl<B: PipeWireBackend, R: CardResolver> PipeWireProvider<B, R> {
    /// Provedor com backend e resolvedor de placas próprios (testes).
    pub fn with_backend(backend: B, resolver: R) -> Self {
        PipeWireProvider {
            inner: Arc::new(Inner {
                backend,
                resolver,
                known: Mutex::new(BTreeMap::new()),
            }),
        }
    }
}

impl<B: PipeWireBackend, R: CardResolver> Inner<B, R> {
    /// Retrato atual dos periféricos. PipeWire indisponível → lista vazia.
    async fn current(&self) -> Result<Vec<AudioDevice>, RaizaError> {
        match self.backend.dump().await {
            Ok(items) => Ok(scan(&Graph::from_dump(&items), &self.resolver)),
            Err(BackendError::Unavailable(why)) => {
                info!("PipeWire indisponível, nenhum dispositivo de áudio: {why}");
                Ok(Vec::new())
            }
            Err(BackendError::Failed(why)) => Err(RaizaError::Io(why)),
        }
    }

    async fn find(&self, id: &DeviceId) -> Result<AudioDevice, RaizaError> {
        self.current()
            .await?
            .into_iter()
            .find(|d| &d.info.id == id)
            .ok_or_else(|| RaizaError::NotFound(id.clone()))
    }

    fn remember(&self, devices: &[AudioDevice]) -> Vec<ProviderEvent> {
        let mut known = self.known.lock().unwrap_or_else(PoisonError::into_inner);
        diff(&mut known, devices.iter().map(|d| d.info.clone()))
    }
}

/// Compara o estado conhecido com o novo e devolve os eventos:
/// - aparelho novo → `DeviceAdded`;
/// - aparelho sumiu → `DeviceRemoved`;
/// - só valores mudaram → `CapabilityChanged` para cada capacidade alterada;
/// - mudou nome, fabricante, tipo ou o conjunto de capacidades → `DeviceAdded` de novo,
///   que substitui o anterior (como no provedor `bluez`).
fn diff(
    known: &mut BTreeMap<DeviceId, DeviceInfo>,
    devices: impl IntoIterator<Item = DeviceInfo>,
) -> Vec<ProviderEvent> {
    let new: BTreeMap<DeviceId, DeviceInfo> =
        devices.into_iter().map(|d| (d.id.clone(), d)).collect();
    let mut events: Vec<ProviderEvent> = known
        .keys()
        .filter(|id| !new.contains_key(*id))
        .map(|id| ProviderEvent::DeviceRemoved { id: id.clone() })
        .collect();
    for (id, device) in &new {
        match known.get(id) {
            None => events.push(ProviderEvent::DeviceAdded {
                device: device.clone(),
            }),
            Some(old) if same_shape(old, device) => {
                for cap in &device.capabilities {
                    if old.capability(cap.kind()) != Some(cap) {
                        events.push(ProviderEvent::CapabilityChanged {
                            id: id.clone(),
                            capability: cap.clone(),
                        });
                    }
                }
            }
            Some(_) => events.push(ProviderEvent::DeviceAdded {
                device: device.clone(),
            }),
        }
    }
    *known = new;
    events
}

fn same_shape(a: &DeviceInfo, b: &DeviceInfo) -> bool {
    let kinds = |d: &DeviceInfo| {
        d.capabilities
            .iter()
            .map(Capability::kind)
            .collect::<Vec<_>>()
    };
    a.name == b.name && a.vendor == b.vendor && a.kind == b.kind && kinds(a) == kinds(b)
}

#[async_trait]
impl<B: PipeWireBackend, R: CardResolver> Provider for PipeWireProvider<B, R> {
    fn id(&self) -> &str {
        PROVIDER_ID
    }

    async fn discover(&self) -> Result<Vec<DeviceInfo>, RaizaError> {
        let devices = self.inner.current().await?;
        self.inner.remember(&devices);
        Ok(devices.into_iter().map(|d| d.info).collect())
    }

    async fn read(&self, id: &DeviceId, kind: CapabilityKind) -> Result<Capability, RaizaError> {
        let device = self.inner.find(id).await?;
        device
            .info
            .capability(kind)
            .cloned()
            .ok_or(RaizaError::Unsupported(kind))
    }

    async fn write(&self, id: &DeviceId, value: CapabilityValue) -> Result<(), RaizaError> {
        // Pilar 1: valida contra o estado mais recente, lido agora do PipeWire.
        let device = self.inner.find(id).await?;
        device.info.validate(&value)?;
        let result = match value {
            CapabilityValue::Volume(percent) => {
                self.inner.backend.set_volume(device.node, percent).await
            }
            CapabilityValue::Mute(muted) => self.inner.backend.set_mute(device.node, muted).await,
            other => return Err(RaizaError::Unsupported(other.kind())),
        };
        debug!(%id, node = device.node, ok = result.is_ok(), "escrita no PipeWire");
        result.map_err(|e| RaizaError::Io(e.to_string()))
    }

    fn events(&self) -> EventStream {
        struct State<B, R> {
            inner: Arc<Inner<B, R>>,
            monitor: BoxStream<'static, MonitorEvent>,
            graph: Graph,
            pending: VecDeque<ProviderEvent>,
        }
        let state = State {
            inner: Arc::clone(&self.inner),
            monitor: self.inner.backend.monitor(),
            graph: Graph::default(),
            pending: VecDeque::new(),
        };
        stream::unfold(state, |mut s| async move {
            loop {
                if let Some(event) = s.pending.pop_front() {
                    return Some((event, s));
                }
                match s.monitor.next().await? {
                    MonitorEvent::Snapshot(items) => s.graph = Graph::from_dump(&items),
                    MonitorEvent::Update(items) => {
                        if !s.graph.apply(&items) {
                            continue;
                        }
                    }
                }
                let devices = scan(&s.graph, &s.inner.resolver);
                s.pending.extend(s.inner.remember(&devices));
            }
        })
        .boxed()
    }
}
