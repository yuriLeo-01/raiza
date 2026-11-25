//! O [`Provider`] udev: descoberta, leitura de `Connection` e hotplug.

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
use crate::scan::scan;
use crate::source::{ChangeStream, SystemUdev, UdevSource};

/// Depois de um evento, espera este silêncio antes de reler a árvore: plugar um
/// aparelho gera uma rajada de eventos (dispositivo, interfaces, hidraw, input).
const DEBOUNCE: Duration = Duration::from_millis(300);
/// Limite de espera numa rajada que não acaba.
const DEBOUNCE_MAX: Duration = Duration::from_secs(2);

type Known = Arc<Mutex<BTreeMap<DeviceId, DeviceInfo>>>;

/// Provedor genérico e de menor prioridade: garante que todo periférico USB ou de
/// entrada Bluetooth apareça, com `Connection` somente leitura.
pub struct UdevProvider<S: UdevSource = SystemUdev> {
    source: Arc<S>,
    known: Known,
}

impl UdevProvider<SystemUdev> {
    /// Provedor ligado ao udev do sistema.
    pub fn new() -> Self {
        Self::with_source(SystemUdev)
    }
}

impl Default for UdevProvider<SystemUdev> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: UdevSource> UdevProvider<S> {
    /// Provedor com uma fonte injetada (testes).
    pub fn with_source(source: S) -> Self {
        UdevProvider {
            source: Arc::new(source),
            known: Arc::default(),
        }
    }
}

/// Lê a árvore fora do runtime e aplica a descoberta.
async fn rescan<S: UdevSource>(source: &Arc<S>) -> Result<Vec<DeviceInfo>, RaizaError> {
    let source = Arc::clone(source);
    let tree = tokio::task::spawn_blocking(move || source.snapshot())
        .await
        .map_err(|e| RaizaError::Io(format!("leitura do udev interrompida: {e}")))??;
    Ok(scan(&tree))
}

fn lock(known: &Known) -> std::sync::MutexGuard<'_, BTreeMap<DeviceId, DeviceInfo>> {
    known.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Eventos que levam de `old` a `new`. Um aparelho que mudou (ex.: o tipo só ficou
/// claro depois que o driver ligou) sai e volta, para não depender de como o daemon
/// trata um `DeviceAdded` repetido.
pub fn diff(
    old: &BTreeMap<DeviceId, DeviceInfo>,
    new: &BTreeMap<DeviceId, DeviceInfo>,
) -> Vec<ProviderEvent> {
    let mut events = Vec::new();
    for (id, device) in old {
        if new.get(id) != Some(device) {
            events.push(ProviderEvent::DeviceRemoved { id: id.clone() });
        }
    }
    for (id, device) in new {
        if old.get(id) != Some(device) {
            events.push(ProviderEvent::DeviceAdded {
                device: device.clone(),
            });
        }
    }
    events
}

fn to_map(devices: Vec<DeviceInfo>) -> BTreeMap<DeviceId, DeviceInfo> {
    devices.into_iter().map(|d| (d.id.clone(), d)).collect()
}

#[async_trait]
impl<S: UdevSource> Provider for UdevProvider<S> {
    fn id(&self) -> &str {
        PROVIDER_ID
    }

    async fn discover(&self) -> Result<Vec<DeviceInfo>, RaizaError> {
        let devices = match rescan(&self.source).await {
            Ok(devices) => devices,
            Err(e) => {
                info!(error = %e, "udev indisponível; nenhum dispositivo listado");
                Vec::new()
            }
        };
        *lock(&self.known) = to_map(devices.clone());
        Ok(devices)
    }

    async fn read(&self, id: &DeviceId, kind: CapabilityKind) -> Result<Capability, RaizaError> {
        if kind != CapabilityKind::Connection {
            return Err(RaizaError::Unsupported(kind));
        }
        rescan(&self.source)
            .await?
            .into_iter()
            .find(|d| &d.id == id)
            .and_then(|d| d.capability(kind).cloned())
            .ok_or_else(|| RaizaError::NotFound(id.clone()))
    }

    async fn write(&self, _id: &DeviceId, _value: CapabilityValue) -> Result<(), RaizaError> {
        // Somente leitura: nada chega ao hardware (pilar 1).
        Err(RaizaError::ReadOnly(CapabilityKind::Connection))
    }

    fn events(&self) -> EventStream {
        let state = Watch {
            source: Arc::clone(&self.source),
            known: Arc::clone(&self.known),
            changes: None,
            pending: VecDeque::new(),
            ended: false,
        };
        Box::pin(futures_util::stream::unfold(state, Watch::next))
    }
}

/// Estado do fluxo de eventos.
struct Watch<S: UdevSource> {
    source: Arc<S>,
    known: Known,
    changes: Option<ChangeStream>,
    pending: VecDeque<ProviderEvent>,
    ended: bool,
}

impl<S: UdevSource> Watch<S> {
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
                        info!(error = %e, "monitor do udev indisponível; sem hotplug");
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

    /// Relê a árvore e enfileira a diferença em relação ao que já foi anunciado.
    async fn refresh(&mut self) {
        let new = match rescan(&self.source).await {
            Ok(devices) => to_map(devices),
            Err(e) => {
                warn!(error = %e, "falha ao reler o udev; mantendo a lista anterior");
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

#[cfg(test)]
mod tests {
    use super::*;
    use raiza_core::{Capability, ConnectionKind, DeviceKind};

    fn dispositivo(id: &str, kind: DeviceKind) -> DeviceInfo {
        DeviceInfo {
            id: id.parse().unwrap(),
            name: id.into(),
            vendor: None,
            kind,
            providers: vec![PROVIDER_ID.into()],
            capabilities: vec![Capability::Connection {
                medium: ConnectionKind::Wired,
                connected: true,
                writable: false,
            }],
        }
    }

    #[test]
    fn diff_remove_antes_de_adicionar() {
        let antes = to_map(vec![
            dispositivo("usb:0001:0001:0", DeviceKind::Mouse),
            dispositivo("usb:0002:0002:0", DeviceKind::Other),
        ]);
        let depois = to_map(vec![
            dispositivo("usb:0002:0002:0", DeviceKind::Keyboard),
            dispositivo("usb:0003:0003:0", DeviceKind::Mouse),
        ]);
        let eventos: Vec<String> = diff(&antes, &depois)
            .iter()
            .map(|e| match e {
                ProviderEvent::DeviceAdded { device } => format!("+{}", device.id),
                ProviderEvent::DeviceRemoved { id } => format!("-{id}"),
                ProviderEvent::CapabilityChanged { id, .. } => format!("~{id}"),
            })
            .collect();
        assert_eq!(
            eventos,
            [
                "-usb:0001:0001:0",
                "-usb:0002:0002:0",
                "+usb:0002:0002:0",
                "+usb:0003:0003:0"
            ]
        );
        assert!(diff(&depois, &depois).is_empty());
    }
}
