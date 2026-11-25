//! Provedores BlueZ e UPower do Raiza.
//!
//! Este crate exporta **dois** provedores, ambos falando com o D-Bus de **sistema**:
//!
//! - [`BluezProvider`] (id `bluez`): dispositivos Bluetooth **pareados**, com
//!   [`Connection`](raiza_core::Capability::Connection) gravável (conectar/desconectar) e
//!   [`Battery`](raiza_core::Capability::Battery) quando o BlueZ expõe `Battery1`;
//! - [`UpowerProvider`] (id `upower`): bateria (somente leitura) de periféricos sem fio,
//!   ignorando a bateria do próprio computador e a fonte.
//!
//! O acesso ao D-Bus fica atrás das traits [`BluezBackend`] e [`UpowerBackend`], e a
//! resolução de ids USB pelo sysfs atrás de [`UsbResolver`], para testar sem hardware.
//! Eventos vêm de sinais D-Bus (`PropertiesChanged`, `InterfacesAdded/Removed`,
//! `DeviceAdded/DeviceRemoved`), sem polling (pilar 2).

mod bluez;
mod bluez_dbus;
mod dbus;
mod error;
mod props;
mod sysfs;
mod tracker;
mod upower;
mod upower_dbus;

pub use bluez::{BluezBackend, BluezDevice, BluezEvent, BluezProvider};
pub use bluez_dbus::ZbusBluez;
pub use error::BackendError;
pub use sysfs::{SysfsUsbResolver, UsbResolver};
pub use upower::{UpowerBackend, UpowerDevice, UpowerEvent, UpowerProvider};
pub use upower_dbus::ZbusUpower;

/// Fluxo de eventos de um backend.
pub type BackendStream<T> = std::pin::Pin<Box<dyn futures_util::Stream<Item = T> + Send + 'static>>;
