//! Provedor BlueZ: dispositivos Bluetooth pareados, conexão e bateria.

use std::sync::Arc;

use async_trait::async_trait;
use futures_util::StreamExt;
use raiza_core::{
    Capability, CapabilityKind, CapabilityValue, ConnectionKind, DeviceId, DeviceInfo, DeviceKind,
    EventStream, Provider, RaizaError,
};
use tracing::{debug, info, warn};

use crate::tracker::{self, Change, SharedTracker};
use crate::{BackendError, BackendStream, ZbusBluez};

/// Id deste provedor.
const PROVIDER_ID: &str = "bluez";

/// Um objeto `org.bluez.Device1`, com a bateria de `org.bluez.Battery1` se houver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BluezDevice {
    /// Caminho D-Bus, ex.: `/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF`.
    pub path: String,
    /// `Address`, ex.: `AA:BB:CC:DD:EE:FF`.
    pub address: String,
    /// `Name`, se o dispositivo informou.
    pub name: Option<String>,
    /// `Alias` (nome dado pelo usuário, ou o `Name`).
    pub alias: Option<String>,
    /// `Icon`, ex.: `audio-headset`.
    pub icon: Option<String>,
    /// `Paired`.
    pub paired: bool,
    /// `Connected`.
    pub connected: bool,
    /// `Battery1.Percentage`, se a interface existir.
    pub battery: Option<u8>,
}

/// Mudança vista no BlueZ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BluezEvent {
    /// O dispositivo apareceu ou mudou; traz o estado completo atual.
    Changed(BluezDevice),
    /// O objeto do dispositivo sumiu (caminho D-Bus).
    Removed(String),
}

/// Acesso ao BlueZ. A implementação real é [`ZbusBluez`]; os testes usam uma falsa.
#[async_trait]
pub trait BluezBackend: Send + Sync + 'static {
    /// Todos os dispositivos conhecidos pelo BlueZ (pareados ou não).
    async fn devices(&self) -> Result<Vec<BluezDevice>, BackendError>;
    /// `Device1.Connect` no objeto `path`.
    async fn connect(&self, path: &str) -> Result<(), BackendError>;
    /// `Device1.Disconnect` no objeto `path`.
    async fn disconnect(&self, path: &str) -> Result<(), BackendError>;
    /// Inscreve-se nos sinais do BlueZ.
    async fn events(&self) -> Result<BackendStream<BluezEvent>, BackendError>;
}

/// Provedor `bluez`: só dispositivos **pareados**, com id [`DeviceId::bluetooth`].
pub struct BluezProvider {
    backend: Arc<dyn BluezBackend>,
    tracker: SharedTracker,
}

impl BluezProvider {
    /// Cria o provedor sobre um backend qualquer.
    pub fn new(backend: impl BluezBackend) -> Self {
        BluezProvider {
            backend: Arc::new(backend),
            tracker: SharedTracker::default(),
        }
    }

    /// Cria o provedor sobre o D-Bus de sistema. A conexão só é aberta no primeiro uso.
    pub fn system() -> Self {
        Self::new(ZbusBluez::new())
    }

    /// Estado atual do dispositivo `id`, direto do BlueZ.
    async fn find(&self, id: &DeviceId) -> Result<(BluezDevice, DeviceInfo), RaizaError> {
        let devices = self.backend.devices().await.map_err(|e| match e {
            BackendError::Unavailable(_) => RaizaError::NotFound(id.clone()),
            e => e.into(),
        })?;
        devices
            .into_iter()
            .filter_map(|d| to_info(&d).map(|i| (d, i)))
            .find(|(_, i)| &i.id == id)
            .ok_or_else(|| RaizaError::NotFound(id.clone()))
    }
}

#[async_trait]
impl Provider for BluezProvider {
    fn id(&self) -> &str {
        PROVIDER_ID
    }

    async fn discover(&self) -> Result<Vec<DeviceInfo>, RaizaError> {
        let devices = match self.backend.devices().await {
            Ok(devices) => devices,
            Err(BackendError::Unavailable(e)) => {
                info!("BlueZ indisponível, nenhum dispositivo Bluetooth: {e}");
                Vec::new()
            }
            Err(e) => return Err(e.into()),
        };
        let mut tracker = tracker::lock(&self.tracker);
        tracker.sync(snapshot(&devices));
        Ok(tracker.infos())
    }

    async fn read(&self, id: &DeviceId, kind: CapabilityKind) -> Result<Capability, RaizaError> {
        let (_, info) = self.find(id).await?;
        info.capability(kind)
            .cloned()
            .ok_or(RaizaError::Unsupported(kind))
    }

    async fn write(&self, id: &DeviceId, value: CapabilityValue) -> Result<(), RaizaError> {
        // Pilar 1: valida de novo contra o estado mais recente.
        let (device, info) = self.find(id).await?;
        info.validate(&value)?;
        let CapabilityValue::Connection(want) = value else {
            return Err(RaizaError::Unsupported(value.kind()));
        };
        if device.connected == want {
            debug!(%id, want, "conexão já está no estado pedido");
            return Ok(());
        }
        let result = if want {
            self.backend.connect(&device.path).await
        } else {
            self.backend.disconnect(&device.path).await
        };
        result.map_err(|e| {
            warn!(%id, want, "falha ao mudar a conexão: {e}");
            e.into()
        })
    }

    fn events(&self) -> EventStream {
        let backend = self.backend.clone();
        let tracker = self.tracker.clone();
        tracker::event_stream(self.tracker.clone(), async move {
            let changes = match backend.events().await {
                Ok(changes) => changes,
                Err(e) => {
                    info!("sem eventos do BlueZ: {e}");
                    return None;
                }
            };
            let snapshot = match backend.devices().await {
                Ok(devices) => snapshot(&devices),
                Err(BackendError::Unavailable(_)) => Vec::new(),
                Err(e) => {
                    warn!("falha ao listar dispositivos do BlueZ: {e}");
                    tracker::lock(&tracker).snapshot()
                }
            };
            let changes = changes.map(|event| match event {
                BluezEvent::Changed(device) => Change {
                    info: to_info(&device),
                    key: device.path,
                },
                BluezEvent::Removed(path) => Change {
                    key: path,
                    info: None,
                },
            });
            Some((snapshot, changes))
        })
    }
}

fn snapshot(devices: &[BluezDevice]) -> Vec<(String, DeviceInfo)> {
    devices
        .iter()
        .filter_map(|d| to_info(d).map(|i| (d.path.clone(), i)))
        .collect()
}

/// Converte um dispositivo do BlueZ. Não pareado ou sem endereço válido: `None`.
fn to_info(device: &BluezDevice) -> Option<DeviceInfo> {
    if !device.paired {
        return None;
    }
    let id = match DeviceId::bluetooth(&device.address) {
        Ok(id) => id,
        Err(e) => {
            debug!(path = %device.path, "ignorando dispositivo: {e}");
            return None;
        }
    };
    let name = [&device.alias, &device.name]
        .into_iter()
        .flatten()
        .map(|s| s.trim())
        .find(|s| !s.is_empty())
        .map_or_else(|| device.address.clone(), str::to_owned);
    let mut capabilities = vec![Capability::Connection {
        medium: ConnectionKind::Bluetooth,
        connected: device.connected,
        writable: true,
    }];
    if let Some(percent) = device.battery {
        capabilities.push(Capability::Battery {
            percent: percent.min(100),
            charging: false,
        });
    }
    Some(DeviceInfo {
        id,
        name,
        vendor: None,
        kind: kind_from_icon(device.icon.as_deref()),
        providers: vec![PROVIDER_ID.into()],
        capabilities,
    })
}

/// Tipo do dispositivo a partir do `Icon` do BlueZ (nomes do padrão freedesktop).
fn kind_from_icon(icon: Option<&str>) -> DeviceKind {
    match icon.unwrap_or_default() {
        "audio-headset" | "audio-headphones" => DeviceKind::Headset,
        "audio-input-microphone" => DeviceKind::Microphone,
        "input-gaming" => DeviceKind::Gamepad,
        "input-mouse" => DeviceKind::Mouse,
        "input-keyboard" => DeviceKind::Keyboard,
        _ => DeviceKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use futures::channel::mpsc;
    use futures::{FutureExt, StreamExt};
    use raiza_core::ProviderEvent;

    use super::*;

    const MAC: &str = "DE:1E:59:99:7C:AE";
    const PATH: &str = "/org/bluez/hci0/dev_DE_1E_59_99_7C_AE";

    fn headset(connected: bool, battery: Option<u8>) -> BluezDevice {
        BluezDevice {
            path: PATH.into(),
            address: MAC.into(),
            name: Some("Thinkplus-K3 pro".into()),
            alias: Some("联想Thinkplus-K3 pro".into()),
            icon: Some("audio-headset".into()),
            paired: true,
            connected,
            battery,
        }
    }

    /// BlueZ em memória.
    #[derive(Default)]
    struct FakeBluez {
        state: Mutex<FakeState>,
    }

    #[derive(Default)]
    struct FakeState {
        devices: Vec<BluezDevice>,
        calls: Vec<(&'static str, String)>,
        subscribers: Vec<mpsc::UnboundedSender<BluezEvent>>,
        unavailable: bool,
    }

    impl FakeBluez {
        fn with(devices: Vec<BluezDevice>) -> Arc<Self> {
            let fake = FakeBluez::default();
            fake.state.lock().unwrap().devices = devices;
            Arc::new(fake)
        }

        /// Muda um dispositivo e emite o sinal, como o BlueZ faria.
        fn set(&self, device: BluezDevice) {
            let mut s = self.state.lock().unwrap();
            s.devices.retain(|d| d.path != device.path);
            s.devices.push(device.clone());
            s.subscribers.retain(|tx| {
                tx.unbounded_send(BluezEvent::Changed(device.clone()))
                    .is_ok()
            });
        }

        fn remove(&self, path: &str) {
            let mut s = self.state.lock().unwrap();
            s.devices.retain(|d| d.path != path);
            s.subscribers
                .retain(|tx| tx.unbounded_send(BluezEvent::Removed(path.into())).is_ok());
        }

        fn calls(&self) -> Vec<(&'static str, String)> {
            self.state.lock().unwrap().calls.clone()
        }
    }

    #[async_trait]
    impl BluezBackend for Arc<FakeBluez> {
        async fn devices(&self) -> Result<Vec<BluezDevice>, BackendError> {
            let s = self.state.lock().unwrap();
            if s.unavailable {
                return Err(BackendError::Unavailable("bluetoothd parado".into()));
            }
            Ok(s.devices.clone())
        }

        async fn connect(&self, path: &str) -> Result<(), BackendError> {
            self.state
                .lock()
                .unwrap()
                .calls
                .push(("Connect", path.into()));
            Ok(())
        }

        async fn disconnect(&self, path: &str) -> Result<(), BackendError> {
            self.state
                .lock()
                .unwrap()
                .calls
                .push(("Disconnect", path.into()));
            Ok(())
        }

        async fn events(&self) -> Result<BackendStream<BluezEvent>, BackendError> {
            let (tx, rx) = mpsc::unbounded();
            self.state.lock().unwrap().subscribers.push(tx);
            Ok(Box::pin(rx))
        }
    }

    fn id() -> DeviceId {
        DeviceId::bluetooth(MAC).unwrap()
    }

    fn connection(connected: bool) -> Capability {
        Capability::Connection {
            medium: ConnectionKind::Bluetooth,
            connected,
            writable: true,
        }
    }

    fn battery(percent: u8) -> Capability {
        Capability::Battery {
            percent,
            charging: false,
        }
    }

    #[tokio::test]
    async fn descobre_headset_conectado_com_bateria() {
        let mut nao_pareado = headset(false, None);
        nao_pareado.path = "/org/bluez/hci0/dev_11_22_33_44_55_66".into();
        nao_pareado.address = "11:22:33:44:55:66".into();
        nao_pareado.paired = false;
        let p = BluezProvider::new(FakeBluez::with(vec![headset(true, Some(80)), nao_pareado]));

        assert_eq!(p.id(), "bluez");
        let devices = p.discover().await.unwrap();
        assert_eq!(devices.len(), 1, "só pareados");
        let d = &devices[0];
        assert_eq!(d.id.to_string(), "bt:DE:1E:59:99:7C:AE");
        assert_eq!(d.name, "联想Thinkplus-K3 pro");
        assert_eq!(d.kind, DeviceKind::Headset);
        assert_eq!(d.providers, ["bluez"]);
        assert_eq!(d.capabilities, [connection(true), battery(80)]);
        assert_eq!(
            p.read(&id(), CapabilityKind::Battery).await.unwrap(),
            battery(80)
        );
    }

    #[tokio::test]
    async fn desconectado_sem_bateria() {
        let p = BluezProvider::new(FakeBluez::with(vec![headset(false, None)]));
        let d = p.discover().await.unwrap().remove(0);
        assert_eq!(d.capabilities, [connection(false)]);
        assert_eq!(
            p.read(&id(), CapabilityKind::Battery).await,
            Err(RaizaError::Unsupported(CapabilityKind::Battery))
        );
    }

    #[tokio::test]
    async fn write_desconecta_e_conecta() {
        let fake = FakeBluez::with(vec![headset(true, Some(80))]);
        let p = BluezProvider::new(fake.clone());
        p.write(&id(), CapabilityValue::Connection(false))
            .await
            .unwrap();
        assert_eq!(fake.calls(), [("Disconnect", PATH.to_string())]);

        fake.set(headset(false, None));
        p.write(&id(), CapabilityValue::Connection(true))
            .await
            .unwrap();
        assert_eq!(fake.calls()[1], ("Connect", PATH.to_string()));

        // Já conectado: nada a fazer.
        fake.set(headset(true, None));
        p.write(&id(), CapabilityValue::Connection(true))
            .await
            .unwrap();
        assert_eq!(fake.calls().len(), 2);
    }

    #[tokio::test]
    async fn write_recusa_o_que_nao_e_conexao() {
        let fake = FakeBluez::with(vec![headset(true, Some(80))]);
        let p = BluezProvider::new(fake.clone());
        assert_eq!(
            p.write(&id(), CapabilityValue::Volume(10)).await,
            Err(RaizaError::Unsupported(CapabilityKind::Volume))
        );
        let outro = DeviceId::bluetooth("00:00:00:00:00:01").unwrap();
        assert_eq!(
            p.write(&outro, CapabilityValue::Connection(false)).await,
            Err(RaizaError::NotFound(outro))
        );
        assert!(fake.calls().is_empty());
    }

    #[tokio::test]
    async fn bluez_ausente_vira_lista_vazia() {
        let fake = FakeBluez::with(vec![headset(true, None)]);
        fake.state.lock().unwrap().unavailable = true;
        let p = BluezProvider::new(fake);
        assert_eq!(p.discover().await.unwrap(), []);
    }

    #[tokio::test]
    async fn eventos_de_conexao_e_bateria() {
        let fake = FakeBluez::with(vec![headset(true, Some(80))]);
        let p = BluezProvider::new(fake.clone());
        p.discover().await.unwrap();
        let mut events = p.events();
        // Força a inscrição antes de mudar o estado.
        assert_eq!(events.next().now_or_never(), None);

        fake.set(headset(true, Some(70)));
        assert_eq!(
            events.next().await,
            Some(ProviderEvent::CapabilityChanged {
                id: id(),
                capability: battery(70)
            })
        );

        // Desconectou e o BlueZ tirou o Battery1: o dispositivo é reanunciado.
        fake.set(headset(false, None));
        let Some(ProviderEvent::DeviceAdded { device }) = events.next().await else {
            panic!("esperava DeviceAdded");
        };
        assert_eq!(device.capabilities, [connection(false)]);

        // Despareado: some.
        let mut despareado = headset(false, None);
        despareado.paired = false;
        fake.set(despareado);
        assert_eq!(
            events.next().await,
            Some(ProviderEvent::DeviceRemoved { id: id() })
        );

        // Pareado de novo e depois removido do BlueZ.
        fake.set(headset(true, None));
        assert!(matches!(
            events.next().await,
            Some(ProviderEvent::DeviceAdded { .. })
        ));
        fake.remove(PATH);
        assert_eq!(
            events.next().await,
            Some(ProviderEvent::DeviceRemoved { id: id() })
        );
    }

    #[tokio::test]
    async fn eventos_comecam_pelas_diferencas_desde_o_discover() {
        let fake = FakeBluez::with(vec![headset(true, Some(80))]);
        let p = BluezProvider::new(fake.clone());
        p.discover().await.unwrap();
        // Mudou entre o discover e a inscrição.
        fake.state.lock().unwrap().devices = vec![headset(true, Some(75))];
        let mut events = p.events();
        assert_eq!(
            events.next().await,
            Some(ProviderEvent::CapabilityChanged {
                id: id(),
                capability: battery(75)
            })
        );
    }

    #[test]
    fn tipo_pelo_icone() {
        assert_eq!(
            kind_from_icon(Some("audio-headphones")),
            DeviceKind::Headset
        );
        assert_eq!(kind_from_icon(Some("input-gaming")), DeviceKind::Gamepad);
        assert_eq!(kind_from_icon(Some("input-mouse")), DeviceKind::Mouse);
        assert_eq!(kind_from_icon(Some("input-keyboard")), DeviceKind::Keyboard);
        assert_eq!(kind_from_icon(Some("phone")), DeviceKind::Other);
        assert_eq!(kind_from_icon(None), DeviceKind::Other);
    }
}
