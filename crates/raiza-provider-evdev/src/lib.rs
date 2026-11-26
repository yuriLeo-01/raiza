//! Provedor `evdev` do Raiza: controles de jogo (Xbox, DualShock/DualSense, genéricos).
//!
//! Capacidades:
//! - `Battery` (somente leitura): lida do `power_supply` do kernel ligado ao controle;
//! - `Rumble`: pulso de vibração (`FF_RUMBLE`), só se o controle anunciar o recurso.
//!
//! Camadas, todas injetáveis para testar sem hardware:
//! - [`source`]: enumeração e monitor do udev atrás da trait [`PadSource`];
//! - [`sysfs`]: leitura de atributos, identidade e bateria a partir de uma raiz `/sys`
//!   configurável (os testes usam um sysfs falso);
//! - [`rumble`]: o backend de vibração atrás da trait [`RumbleBackend`];
//! - [`EvdevProvider`]: a implementação de [`raiza_core::Provider`].

#![warn(missing_docs)]

mod provider;
pub mod rumble;
pub mod source;
pub mod sysfs;

pub use provider::{EvdevProvider, diff};
pub use rumble::{EvdevRumble, MAX_RUMBLE_MS, Pulse, RumbleBackend};
pub use source::{ChangeStream, InputNode, PadSource, SystemUdev};
pub use sysfs::{Pad, Sysfs};

/// Id deste provedor.
pub const PROVIDER_ID: &str = "evdev";
