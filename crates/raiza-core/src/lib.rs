//! Núcleo do Raiza: tipos e contratos compartilhados por daemon, provedores e interfaces.
//!
//! Este crate não faz I/O. Ele define:
//!
//! - [`DeviceId`], [`DeviceKind`] e [`DeviceInfo`]: o que é um dispositivo;
//! - [`Capability`], [`CapabilityKind`] e [`CapabilityValue`]: o que ele sabe fazer e o
//!   que se pode pedir a ele, com validação contra os limites (pilar 1);
//! - a trait [`Provider`] e os [`ProviderEvent`]s: o contrato de todo provedor;
//! - [`fuse`] e [`capability_owner`]: como o daemon junta o mesmo dispositivo físico
//!   visto por provedores diferentes;
//! - [`RaizaError`]: os erros comuns.
//!
//! Features:
//! - `mock`: o módulo `mock` oferece um provedor falso para testes;
//! - `dbus`: o módulo `dbus` traz o proxy e o cliente tipado da API D-Bus (`docs/DBUS.md`).
//!
//! Veja `docs/ARQUITETURA.md`.

#![warn(missing_docs)]

mod capability;
mod device;
mod error;
mod fusion;
mod id;
mod provider;
mod value;

#[cfg(any(test, feature = "mock"))]
pub mod mock;

#[cfg(feature = "dbus")]
pub mod dbus;

pub use capability::{Capability, CapabilityKind, Color, ConnectionKind, Level, RgbZone};
pub use device::{DeviceInfo, DeviceKind, ProviderId};
pub use error::RaizaError;
pub use fusion::{capability_owner, fuse};
pub use id::{DeviceId, ParseDeviceIdError};
pub use provider::{EventStream, Provider, ProviderEvent};
pub use value::{CapabilityValue, RgbSetting};

/// Nome do serviço D-Bus do daemon (sessão do usuário).
pub const DBUS_SERVICE: &str = "io.github.yurileo01.Raiza";
