//! Provedor UPower: bateria (somente leitura) de periféricos sem fio.

use std::sync::Arc;

use async_trait::async_trait;
use futures_util::StreamExt;
use raiza_core::{
    Capability, CapabilityKind, CapabilityValue, DeviceId, DeviceInfo, DeviceKind, EventStream,
    Provider, RaizaError,
};
use tracing::{debug, info, warn};

use crate::tracker::{self, Change, SharedTracker};
use crate::{BackendError, BackendStream, SysfsUsbResolver, UsbResolver, ZbusUpower};

/// Id deste provedor.
const PROVIDER_ID: &str = "upower";

/// Valores de `Type` do UPower usados aqui.
mod kind {
    pub const LINE_POWER: u32 = 1;
    pub const BATTERY: u32 = 2;
    pub const MOUSE: u32 = 5;
    pub const KEYBOARD: u32 = 6;
    pub const GAMING_INPUT: u32 = 12;
    pub const HEADSET: u32 = 17;
    pub const HEADPHONES: u32 = 19;
}

/// `State` do UPower que significa "carregando".
const STATE_CHARGING: u32 = 1;

/// Um objeto `org.freedesktop.UPower.Device`.
#[derive(Debug, Clone, PartialEq)]
pub struct UpowerDevice {
    /// Caminho D-Bus, ex.: `/org/freedesktop/UPower/devices/mouse_hidpp_battery_0`.
    pub path: String,
    /// `NativePath`: nome em `/sys/class/power_supply` ou caminho do BlueZ.
    pub native_path: String,
    /// `Type` (1 = fonte, 2 = bateria do computador, 5 = mouse...).
    pub kind: u32,
    /// `PowerSupply`: se alimenta o computador.
    pub power_supply: bool,
    /// `Percentage`, de 0 a 100.
    pub percentage: f64,
    /// `State` (1 = carregando, 2 = descarregando...).
    pub state: u32,
    /// `Model`.
    pub model: Option<String>,
    /// `Vendor`.
    pub vendor: Option<String>,
}

/// Mudança vista no UPower.
#[derive(Debug, Clone, PartialEq)]
pub enum UpowerEvent {
    /// O dispositivo apareceu ou mudou; traz o estado completo atual.
    Changed(UpowerDevice),
    /// O dispositivo sumiu (caminho D-Bus).
    Removed(String),
}

/// Acesso ao UPower. A implementação real é [`ZbusUpower`]; os testes usam uma falsa.
#[async_trait]
pub trait UpowerBackend: Send + Sync + 'static {
    /// Todos os dispositivos do UPower (`EnumerateDevices`).
    async fn devices(&self) -> Result<Vec<UpowerDevice>, BackendError>;
    /// Inscreve-se nos sinais do UPower.
    async fn events(&self) -> Result<BackendStream<UpowerEvent>, BackendError>;
}

/// Provedor `upower`: só [`Capability::Battery`], somente leitura.
pub struct UpowerProvider {
    backend: Arc<dyn UpowerBackend>,
    resolver: Arc<dyn UsbResolver>,
    tracker: SharedTracker,
}

impl UpowerProvider {
    /// Cria o provedor sobre um backend e um resolvedor de ids USB quaisquer.
    pub fn new(backend: impl UpowerBackend, resolver: impl UsbResolver) -> Self {
        UpowerProvider {
            backend: Arc::new(backend),
            resolver: Arc::new(resolver),
            tracker: SharedTracker::default(),
        }
    }

    /// Cria o provedor sobre o D-Bus de sistema e o `/sys` real.
    pub fn system() -> Self {
        Self::new(ZbusUpower::new(), SysfsUsbResolver::new())
    }

    async fn find(&self, id: &DeviceId) -> Result<DeviceInfo, RaizaError> {
        let devices = self.backend.devices().await.map_err(|e| match e {
            BackendError::Unavailable(_) => RaizaError::NotFound(id.clone()),
            e => e.into(),
        })?;
        devices
            .iter()
            .filter_map(|d| to_info(d, self.resolver.as_ref()))
            .find(|i| &i.id == id)
            .ok_or_else(|| RaizaError::NotFound(id.clone()))
    }
}

#[async_trait]
impl Provider for UpowerProvider {
    fn id(&self) -> &str {
        PROVIDER_ID
    }

    async fn discover(&self) -> Result<Vec<DeviceInfo>, RaizaError> {
        let devices = match self.backend.devices().await {
            Ok(devices) => devices,
            Err(BackendError::Unavailable(e)) => {
                info!("UPower indisponível, nenhuma bateria de periférico: {e}");
                Vec::new()
            }
            Err(e) => return Err(e.into()),
        };
        let snapshot = snapshot(&devices, self.resolver.as_ref());
        let mut tracker = tracker::lock(&self.tracker);
        tracker.sync(snapshot);
        Ok(tracker.infos())
    }

    async fn read(&self, id: &DeviceId, kind: CapabilityKind) -> Result<Capability, RaizaError> {
        self.find(id)
            .await?
            .capability(kind)
            .cloned()
            .ok_or(RaizaError::Unsupported(kind))
    }

    async fn write(&self, id: &DeviceId, value: CapabilityValue) -> Result<(), RaizaError> {
        // Bateria é somente leitura: a validação sempre recusa, e nada vai ao hardware.
        self.find(id).await?.validate(&value)?;
        Err(RaizaError::ReadOnly(value.kind()))
    }

    fn events(&self) -> EventStream {
        let backend = self.backend.clone();
        let resolver = self.resolver.clone();
        let tracker = self.tracker.clone();
        tracker::event_stream(self.tracker.clone(), async move {
            let changes = match backend.events().await {
                Ok(changes) => changes,
                Err(e) => {
                    info!("sem eventos do UPower: {e}");
                    return None;
                }
            };
            let snapshot = match backend.devices().await {
                Ok(devices) => snapshot(&devices, resolver.as_ref()),
                Err(BackendError::Unavailable(_)) => Vec::new(),
                Err(e) => {
                    warn!("falha ao listar dispositivos do UPower: {e}");
                    tracker::lock(&tracker).snapshot()
                }
            };
            let changes = changes.map(move |event| match event {
                UpowerEvent::Changed(device) => Change {
                    info: to_info(&device, resolver.as_ref()),
                    key: device.path,
                },
                UpowerEvent::Removed(path) => Change {
                    key: path,
                    info: None,
                },
            });
            Some((snapshot, changes))
        })
    }
}

fn snapshot(devices: &[UpowerDevice], resolver: &dyn UsbResolver) -> Vec<(String, DeviceInfo)> {
    devices
        .iter()
        .filter_map(|d| to_info(d, resolver).map(|i| (d.path.clone(), i)))
        .collect()
}

/// Converte um dispositivo do UPower. Bateria do computador, fonte, `DisplayDevice` ou
/// aparelho sem identidade: `None`.
fn to_info(device: &UpowerDevice, resolver: &dyn UsbResolver) -> Option<DeviceInfo> {
    let own_power = device.power_supply || matches!(device.kind, kind::LINE_POWER | kind::BATTERY);
    if own_power || device.path.ends_with("/DisplayDevice") {
        return None;
    }
    let Some(id) = identify(&device.native_path, resolver) else {
        debug!(
            path = %device.path,
            native_path = %device.native_path,
            "ignorando bateria sem identidade"
        );
        return None;
    };
    let non_empty = |s: &Option<String>| {
        s.as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    Some(DeviceInfo {
        id,
        name: non_empty(&device.model).unwrap_or_else(|| "Dispositivo sem fio".into()),
        vendor: non_empty(&device.vendor),
        kind: match device.kind {
            kind::MOUSE => DeviceKind::Mouse,
            kind::KEYBOARD => DeviceKind::Keyboard,
            kind::GAMING_INPUT => DeviceKind::Gamepad,
            kind::HEADSET | kind::HEADPHONES => DeviceKind::Headset,
            _ => DeviceKind::Other,
        },
        providers: vec![PROVIDER_ID.into()],
        capabilities: vec![Capability::Battery {
            // `as` satura; o clamp deixa explícito o intervalo.
            percent: device.percentage.round().clamp(0.0, 100.0) as u8,
            charging: device.state == STATE_CHARGING,
        }],
    })
}

/// Id do aparelho dono da bateria.
///
/// 1. `NativePath` do BlueZ (`/org/bluez/hci0/dev_AA_BB_...`): id Bluetooth.
/// 2. Bateria do kernel cujo dispositivo pai é USB: id USB (`usb_instance`).
/// 3. Um MAC no nome (ex.: `hid-aa:bb:cc:dd:ee:ff-battery`, HID por Bluetooth): id Bluetooth.
///
/// O USB vem antes do MAC porque um receptor USB pode expor um `uniq` com cara de MAC.
fn identify(native_path: &str, resolver: &dyn UsbResolver) -> Option<DeviceId> {
    if native_path.starts_with("/org/bluez/") {
        return find_mac(native_path);
    }
    resolver
        .usb_id(native_path)
        .or_else(|| find_mac(native_path))
}

/// Primeiro endereço `AA:BB:CC:DD:EE:FF` (ou com `_`) dentro de `text`.
fn find_mac(text: &str) -> Option<DeviceId> {
    const LEN: usize = 17;
    let bytes = text.as_bytes();
    (0..=bytes.len().checked_sub(LEN)?)
        .filter(|&i| text.is_char_boundary(i) && text.is_char_boundary(i + LEN))
        .find_map(|i| {
            let window = &bytes[i..i + LEN];
            let sep = window[2];
            let shape = (sep == b':' || sep == b'_')
                && window.iter().enumerate().all(|(j, &b)| {
                    if j % 3 == 2 {
                        b == sep
                    } else {
                        b.is_ascii_hexdigit()
                    }
                });
            shape
                .then(|| DeviceId::bluetooth(&text[i..i + LEN]).ok())
                .flatten()
        })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use futures::channel::mpsc;
    use futures::{FutureExt, StreamExt};
    use raiza_core::ProviderEvent;

    use super::*;

    /// Resolvedor falso: nome em `/sys/class/power_supply` → id USB.
    #[derive(Default)]
    struct FakeSysfs(HashMap<String, DeviceId>);

    impl UsbResolver for FakeSysfs {
        fn usb_id(&self, native_path: &str) -> Option<DeviceId> {
            self.0.get(native_path).cloned()
        }
    }

    fn sysfs() -> FakeSysfs {
        FakeSysfs(HashMap::from([(
            "hidpp_battery_0".to_string(),
            DeviceId::usb(0x046d, 0xc52b, "0"),
        )]))
    }

    #[derive(Default)]
    struct FakeUpower {
        state: Mutex<(Vec<UpowerDevice>, Vec<mpsc::UnboundedSender<UpowerEvent>>)>,
    }

    impl FakeUpower {
        fn with(devices: Vec<UpowerDevice>) -> Arc<Self> {
            let fake = FakeUpower::default();
            fake.state.lock().unwrap().0 = devices;
            Arc::new(fake)
        }

        fn emit(&self, event: UpowerEvent) {
            let mut s = self.state.lock().unwrap();
            match &event {
                UpowerEvent::Changed(d) => {
                    s.0.retain(|x| x.path != d.path);
                    s.0.push(d.clone());
                }
                UpowerEvent::Removed(p) => s.0.retain(|x| &x.path != p),
            }
            s.1.retain(|tx| tx.unbounded_send(event.clone()).is_ok());
        }
    }

    #[async_trait]
    impl UpowerBackend for Arc<FakeUpower> {
        async fn devices(&self) -> Result<Vec<UpowerDevice>, BackendError> {
            Ok(self.state.lock().unwrap().0.clone())
        }

        async fn events(&self) -> Result<BackendStream<UpowerEvent>, BackendError> {
            let (tx, rx) = mpsc::unbounded();
            self.state.lock().unwrap().1.push(tx);
            Ok(Box::pin(rx))
        }
    }

    /// UPower sem serviço no barramento.
    struct Ausente;

    #[async_trait]
    impl UpowerBackend for Ausente {
        async fn devices(&self) -> Result<Vec<UpowerDevice>, BackendError> {
            Err(BackendError::Unavailable("upowerd parado".into()))
        }

        async fn events(&self) -> Result<BackendStream<UpowerEvent>, BackendError> {
            Err(BackendError::Unavailable("upowerd parado".into()))
        }
    }

    fn device(path: &str, native: &str, kind: u32, percentage: f64, state: u32) -> UpowerDevice {
        UpowerDevice {
            path: format!("/org/freedesktop/UPower/devices/{path}"),
            native_path: native.into(),
            kind,
            power_supply: false,
            percentage,
            state,
            model: Some("MX Master 3".into()),
            vendor: Some("Logitech".into()),
        }
    }

    fn mouse(percentage: f64, state: u32) -> UpowerDevice {
        device(
            "mouse_hidpp_battery_0",
            "hidpp_battery_0",
            kind::MOUSE,
            percentage,
            state,
        )
    }

    fn notebook() -> Vec<UpowerDevice> {
        let mut bat0 = device("battery_BAT0", "BAT0", kind::BATTERY, 55.0, 2);
        bat0.power_supply = true;
        let mut ac = device("line_power_AC", "AC", kind::LINE_POWER, 0.0, 0);
        ac.power_supply = true;
        let display = device("DisplayDevice", "", kind::BATTERY, 55.0, 2);
        vec![bat0, ac, display]
    }

    fn mouse_id() -> DeviceId {
        DeviceId::usb(0x046d, 0xc52b, "0")
    }

    fn battery(percent: u8, charging: bool) -> Capability {
        Capability::Battery { percent, charging }
    }

    #[tokio::test]
    async fn mouse_usb_e_bateria_do_notebook_ignorada() {
        let mut devices = notebook();
        devices.push(mouse(72.4, 2));
        let p = UpowerProvider::new(FakeUpower::with(devices), sysfs());

        assert_eq!(p.id(), "upower");
        let found = p.discover().await.unwrap();
        assert_eq!(found.len(), 1, "{found:?}");
        let d = &found[0];
        assert_eq!(d.id, mouse_id());
        assert_eq!(d.name, "MX Master 3");
        assert_eq!(d.vendor.as_deref(), Some("Logitech"));
        assert_eq!(d.kind, DeviceKind::Mouse);
        assert_eq!(d.providers, ["upower"]);
        assert_eq!(d.capabilities, [battery(72, false)]);
    }

    #[tokio::test]
    async fn identidade_bluetooth_e_sem_identidade() {
        let headset = device(
            "headset_dev_DE_1E_59_99_7C_AE",
            "/org/bluez/hci0/dev_DE_1E_59_99_7C_AE",
            kind::HEADSET,
            90.0,
            1,
        );
        let teclado = device(
            "keyboard_hid",
            "hid-dc:2c:26:01:02:03-battery",
            kind::KEYBOARD,
            30.0,
            2,
        );
        let misterio = device("mouse_x", "hid-sem-mac-battery", kind::MOUSE, 10.0, 2);
        let p = UpowerProvider::new(FakeUpower::with(vec![headset, teclado, misterio]), sysfs());
        let found = p.discover().await.unwrap();
        let ids: Vec<String> = found.iter().map(|d| d.id.to_string()).collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"bt:DE:1E:59:99:7C:AE".to_string()));
        assert!(ids.contains(&"bt:DC:2C:26:01:02:03".to_string()));
        let h = found
            .iter()
            .find(|d| d.kind == DeviceKind::Headset)
            .unwrap();
        assert_eq!(h.capabilities, [battery(90, true)]);
    }

    #[tokio::test]
    async fn bateria_mudando_gera_eventos() {
        let mut devices = notebook();
        devices.push(mouse(80.0, 2));
        let fake = FakeUpower::with(devices);
        let p = UpowerProvider::new(fake.clone(), sysfs());
        p.discover().await.unwrap();
        let mut events = p.events();
        assert_eq!(events.next().now_or_never(), None);

        // O notebook mudar não gera nada; o mouse sim.
        let mut bat0 = notebook().remove(0);
        bat0.percentage = 40.0;
        fake.emit(UpowerEvent::Changed(bat0));
        fake.emit(UpowerEvent::Changed(mouse(79.0, 2)));
        assert_eq!(
            events.next().await,
            Some(ProviderEvent::CapabilityChanged {
                id: mouse_id(),
                capability: battery(79, false)
            })
        );
        fake.emit(UpowerEvent::Changed(mouse(79.0, 1)));
        assert_eq!(
            events.next().await,
            Some(ProviderEvent::CapabilityChanged {
                id: mouse_id(),
                capability: battery(79, true)
            })
        );
        fake.emit(UpowerEvent::Removed(mouse(0.0, 0).path));
        assert_eq!(
            events.next().await,
            Some(ProviderEvent::DeviceRemoved { id: mouse_id() })
        );
        fake.emit(UpowerEvent::Changed(mouse(50.0, 2)));
        assert!(matches!(
            events.next().await,
            Some(ProviderEvent::DeviceAdded { .. })
        ));
    }

    #[tokio::test]
    async fn somente_leitura() {
        let fake = FakeUpower::with(vec![mouse(80.0, 2)]);
        let p = UpowerProvider::new(fake, sysfs());
        assert_eq!(
            p.read(&mouse_id(), CapabilityKind::Battery).await.unwrap(),
            battery(80, false)
        );
        assert_eq!(
            p.write(&mouse_id(), CapabilityValue::Connection(false))
                .await,
            Err(RaizaError::Unsupported(CapabilityKind::Connection))
        );
    }

    #[tokio::test]
    async fn upower_ausente_vira_lista_vazia() {
        let p = UpowerProvider::new(Ausente, sysfs());
        assert_eq!(p.discover().await.unwrap(), []);
        let mut events = p.events();
        assert_eq!(events.next().await, None);
    }

    #[test]
    fn mac_no_texto() {
        assert_eq!(
            find_mac("/org/bluez/hci0/dev_aa_bb_cc_dd_ee_ff").map(|i| i.to_string()),
            Some("bt:AA:BB:CC:DD:EE:FF".into())
        );
        assert_eq!(find_mac("hid-0000:0001-battery"), None);
        assert_eq!(find_mac("curto"), None);
        assert_eq!(find_mac("ééééééééééééééééééé"), None);
    }
}
