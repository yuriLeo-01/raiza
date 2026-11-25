//! Provedor PipeWire do Raiza: as capacidades `Volume` e `Mute` de microfones e headsets.
//!
//! - Leitura e eventos: `pw-dump` e `pw-dump --monitor` (sem polling; o PipeWire avisa
//!   cada mudança). Veja [`backend`].
//! - Escrita: `wpctl set-volume` e `wpctl set-mute`, que passam pelo WirePlumber e usam a
//!   mesma curva cúbica que o GNOME e o próprio `wpctl` mostram. Veja [`curve`].
//! - Identidade: Bluetooth por `api.bluez5.address`; USB pela placa ALSA
//!   (`/sys/class/sound/cardN/device` até o dispositivo USB pai). Veja [`scan`] e [`sysfs`].
//!
//! Placas onboard/PCI e HDMI ficam de fora: só periféricos USB e Bluetooth.

pub mod backend;
pub mod curve;
pub mod graph;
mod provider;
pub mod scan;
pub mod sysfs;

pub use backend::{BackendError, MonitorEvent, PipeWireBackend, PwCli};
pub use graph::Graph;
pub use provider::PipeWireProvider;
pub use sysfs::{CardResolver, SysfsCardResolver};

/// Id deste provedor.
pub const PROVIDER_ID: &str = "pipewire";
