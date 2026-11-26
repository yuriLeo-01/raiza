//! O [`Provider`] evdev: descoberta, leitura, teste de vibração e eventos.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use raiza_core::{
    Capability, CapabilityKind, CapabilityValue, DeviceId, DeviceInfo, EventStream, Provider,
    ProviderEvent, RaizaError,
};
use tracing::{info, warn};

use crate::PROVIDER_ID;
use crate::rumble::{EvdevRumble, Pulse, RumbleBackend};
use crate::source::{ChangeStream, PadSource, SystemUdev};
use crate::sysfs::{Pad, Sysfs};

/// Depois de um evento, espera este silêncio antes de reler: plugar um controle gera
/// uma rajada de eventos.
const DEBOUNCE: Duration = Duration::from_millis(300);
/// Limite de espera numa rajada que não acaba.
const DEBOUNCE_MAX: Duration = Duration::from_secs(2);

type Known = Arc<Mutex<BTreeMap<DeviceId, DeviceInfo>>>;

/// Provedor de controles de jogo.
pub struct EvdevProvider<S: PadSource = SystemUdev, R: RumbleBackend = EvdevRumble> {
    source: Arc<S>,
    sysfs: Arc<Sysfs>,
    rumble: Arc<R>,
    known: Known,
}

impl EvdevProvider {
    /// Provedor ligado ao udev, ao sysfs e aos nós evdev do sistema.
    pub fn new() -> Self {
        Self::with_parts(SystemUdev, Sysfs::default(), EvdevRumble)
    }
}

impl Default for EvdevProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: PadSource, R: RumbleBackend> EvdevProvider<S, R> {
    /// Provedor com as camadas de I/O injetadas (testes).
    pub fn with_parts(source: S, sysfs: Sysfs, rumble: R) -> Self {
        EvdevProvider {
            source: Arc::new(source),
            sysfs: Arc::new(sysfs),
            rumble: Arc::new(rumble),
            known: Arc::default(),
        }
    }

    async fn scan(&self) -> Result<Vec<Pad>, RaizaError> {
        scan(&self.source, &self.sysfs).await
    }
}

/// Lê os controles fora do runtime. Um mesmo controle pode expor mais de um nó de
/// joystick: fica um por id, preferindo o que tem vibração.
async fn scan<S: PadSource>(source: &Arc<S>, sysfs: &Arc<Sysfs>) -> Result<Vec<Pad>, RaizaError> {
    let (source, sysfs) = (Arc::clone(source), Arc::clone(sysfs));
    tokio::task::spawn_blocking(move || {
        let mut nodes = source.list()?;
        nodes.sort_by(|a, b| a.devnode.cmp(&b.devnode));
        let mut pads: BTreeMap<DeviceId, Pad> = BTreeMap::new();
        for pad in nodes.iter().filter_map(|n| sysfs.read_pad(n)) {
            let has_rumble = |p: &Pad| p.info.capability(CapabilityKind::Rumble).is_some();
            match pads.get(&pad.info.id) {
                Some(old) if has_rumble(old) || !has_rumble(&pad) => {}
                _ => {
                    pads.insert(pad.info.id.clone(), pad);
                }
            }
        }
        // Joystick sem bateria nem vibração não tem nada a oferecer e só "roubaria" o tipo
        // do aparelho na fusão: um teclado magnético com modo gamepad (ex.: Akko 5087 V3
        // HE) viraria "controle". O tipo fica com o `udev`. Se a bateria aparecer depois,
        // o próximo rescan inclui o aparelho.
        Ok(pads
            .into_values()
            .filter(|p| !p.info.capabilities.is_empty())
            .collect())
    })
    .await
    .map_err(|e| RaizaError::Io(format!("leitura dos controles interrompida: {e}")))?
}

fn lock(known: &Known) -> std::sync::MutexGuard<'_, BTreeMap<DeviceId, DeviceInfo>> {
    known.lock().unwrap_or_else(PoisonError::into_inner)
}

fn to_map(pads: Vec<Pad>) -> BTreeMap<DeviceId, DeviceInfo> {
    pads.into_iter()
        .map(|p| (p.info.id.clone(), p.info))
        .collect()
}

/// Mudou só o valor de capacidades (ex.: bateria)? Então dá para usar `CapabilityChanged`.
fn same_shape(old: &DeviceInfo, new: &DeviceInfo) -> bool {
    old.name == new.name
        && old.vendor == new.vendor
        && old.kind == new.kind
        && old.capabilities.len() == new.capabilities.len()
        && old.capabilities.iter().all(|c| {
            new.capability(c.kind()).is_some_and(|n| {
                // Só a bateria varia de valor sem mudar a forma do dispositivo.
                c.kind() == CapabilityKind::Battery || n == c
            })
        })
}

/// Eventos que levam de `old` a `new`: sai e volta quando o aparelho mudou de forma;
/// `CapabilityChanged` quando só a bateria mudou.
pub fn diff(
    old: &BTreeMap<DeviceId, DeviceInfo>,
    new: &BTreeMap<DeviceId, DeviceInfo>,
) -> Vec<ProviderEvent> {
    let mut removed = Vec::new();
    let mut added = Vec::new();
    let mut changed = Vec::new();
    for (id, device) in old {
        match new.get(id) {
            None => removed.push(ProviderEvent::DeviceRemoved { id: id.clone() }),
            Some(n) if n == device => {}
            Some(n) if same_shape(device, n) => {
                changed.extend(
                    n.capabilities
                        .iter()
                        .filter(|c| device.capability(c.kind()) != Some(*c))
                        .map(|c| ProviderEvent::CapabilityChanged {
                            id: id.clone(),
                            capability: c.clone(),
                        }),
                );
            }
            Some(n) => {
                removed.push(ProviderEvent::DeviceRemoved { id: id.clone() });
                added.push(ProviderEvent::DeviceAdded { device: n.clone() });
            }
        }
    }
    for (id, device) in new {
        if !old.contains_key(id) {
            added.push(ProviderEvent::DeviceAdded {
                device: device.clone(),
            });
        }
    }
    removed.into_iter().chain(added).chain(changed).collect()
}

#[async_trait]
impl<S: PadSource, R: RumbleBackend> Provider for EvdevProvider<S, R> {
    fn id(&self) -> &str {
        PROVIDER_ID
    }

    async fn discover(&self) -> Result<Vec<DeviceInfo>, RaizaError> {
        let devices = match self.scan().await {
            Ok(pads) => pads.into_iter().map(|p| p.info).collect(),
            Err(e) => {
                info!(error = %e, "udev indisponível; nenhum controle listado");
                Vec::new()
            }
        };
        *lock(&self.known) = to_map_infos(&devices);
        Ok(devices)
    }

    async fn read(&self, id: &DeviceId, kind: CapabilityKind) -> Result<Capability, RaizaError> {
        let pad = self
            .scan()
            .await?
            .into_iter()
            .find(|p| &p.info.id == id)
            .ok_or_else(|| RaizaError::NotFound(id.clone()))?;
        pad.info
            .capability(kind)
            .cloned()
            .ok_or(RaizaError::Unsupported(kind))
    }

    async fn write(&self, id: &DeviceId, value: CapabilityValue) -> Result<(), RaizaError> {
        let CapabilityValue::Rumble {
            strength,
            duration_ms,
        } = value
        else {
            return Err(RaizaError::Unsupported(value.kind()));
        };
        // Estado mais recente, direto do sistema, e nova validação (pilar 1).
        let pad = self
            .scan()
            .await?
            .into_iter()
            .find(|p| &p.info.id == id)
            .ok_or_else(|| RaizaError::NotFound(id.clone()))?;
        let capability = pad
            .info
            .capability(CapabilityKind::Rumble)
            .ok_or(RaizaError::Unsupported(CapabilityKind::Rumble))?;
        if !capability.is_writable() {
            return Err(RaizaError::ReadOnly(CapabilityKind::Rumble));
        }
        value.validate(capability)?;
        let pulse = Pulse::new(strength, duration_ms)?;
        if pulse.magnitude() == 0 || pulse.duration_ms() == 0 {
            // Nada para vibrar: não toca no hardware.
            return Ok(());
        }
        let rumble = Arc::clone(&self.rumble);
        tokio::task::spawn_blocking(move || rumble.play(&pad.devnode, pulse))
            .await
            .map_err(|e| RaizaError::Io(format!("vibração interrompida: {e}")))?
    }

    fn events(&self) -> EventStream {
        let state = Watch {
            source: Arc::clone(&self.source),
            sysfs: Arc::clone(&self.sysfs),
            known: Arc::clone(&self.known),
            changes: None,
            pending: VecDeque::new(),
            ended: false,
        };
        Box::pin(futures_util::stream::unfold(state, Watch::next))
    }
}

fn to_map_infos(devices: &[DeviceInfo]) -> BTreeMap<DeviceId, DeviceInfo> {
    devices.iter().map(|d| (d.id.clone(), d.clone())).collect()
}

/// Estado do fluxo de eventos.
struct Watch<S: PadSource> {
    source: Arc<S>,
    sysfs: Arc<Sysfs>,
    known: Known,
    changes: Option<ChangeStream>,
    pending: VecDeque<ProviderEvent>,
    ended: bool,
}

impl<S: PadSource> Watch<S> {
    async fn next(mut self) -> Option<(ProviderEvent, Self)> {
        loop {
            if let Some(event) = self.pending.pop_front() {
                return Some((event, self));
            }
            if self.ended {
                return None;
            }
            match self.changes.as_mut() {
                None => match self.source.watch() {
                    Ok(changes) => {
                        self.changes = Some(changes);
                        // Cobre o que mudou entre o `discover` e o início da vigília.
                        self.refresh().await;
                    }
                    Err(e) => {
                        info!(error = %e, "monitor do udev indisponível; sem eventos");
                        return None;
                    }
                },
                Some(changes) => {
                    // Fim do fluxo de mudanças: o monitor foi encerrado.
                    changes.next().await?;
                    self.ended = !debounce(changes).await;
                    self.refresh().await;
                }
            }
        }
    }

    /// Relê os controles e enfileira a diferença em relação ao que já foi anunciado.
    async fn refresh(&mut self) {
        let new = match scan(&self.source, &self.sysfs).await {
            Ok(pads) => to_map(pads),
            Err(e) => {
                warn!(error = %e, "falha ao reler os controles; mantendo a lista anterior");
                return;
            }
        };
        let mut known = lock(&self.known);
        self.pending.extend(diff(&known, &new));
        *known = new;
    }
}

/// Espera a rajada de eventos acabar. Devolve `false` se o fluxo de mudanças terminou.
async fn debounce(changes: &mut ChangeStream) -> bool {
    let deadline = tokio::time::Instant::now() + DEBOUNCE_MAX;
    loop {
        let wait = DEBOUNCE.min(deadline.saturating_duration_since(tokio::time::Instant::now()));
        if wait.is_zero() {
            return true;
        }
        match tokio::time::timeout(wait, changes.next()).await {
            Ok(Some(())) => continue,
            Ok(None) => return false,
            Err(_) => return true,
        }
    }
}
