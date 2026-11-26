//! raizad: daemon do Raiza.
//!
//! - [`daemon::Daemon`]: provedores isolados (pilar 1), dispositivos fundidos com
//!   [`raiza_core::fuse`], escritas validadas e roteadas com [`raiza_core::capability_owner`];
//! - [`config::ConfigStore`]: último valor escrito por dispositivo, reaplicado quando ele volta;
//! - [`service::Service`]: a API D-Bus de `docs/DBUS.md`;
//! - [`providers`]: a lista de provedores em ordem de prioridade;
//! - [`battery`]: aviso de bateria fraca pelas notificações da área de trabalho.
//!
//! O binário (`main.rs`) só junta as peças. A biblioteca existe para os testes de integração.

pub mod battery;
pub mod config;
pub mod daemon;
pub mod providers;
pub mod service;

pub use config::ConfigStore;
pub use daemon::{Daemon, Running, Signal};
pub use service::{Service, forward_signals};
