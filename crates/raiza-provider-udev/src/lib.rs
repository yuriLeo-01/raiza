//! Provedor `udev` do Raiza: descoberta e hotplug de periféricos.
//!
//! É o provedor genérico e de menor prioridade: garante que todo periférico USB ou de
//! entrada Bluetooth apareça na lista, com o [`raiza_core::DeviceKind`] inferido e uma
//! única capacidade, `Connection`, somente leitura. As demais capacidades vêm de
//! provedores específicos, que se fundem com este pelo [`raiza_core::DeviceId`].
//!
//! Camadas:
//! - [`source`]: I/O com o udev ([`SystemUdev`]) atrás da trait [`UdevSource`];
//! - [`tree`]: o retrato da árvore udev, serializável em JSON (fixtures de teste);
//! - [`scan()`]: lógica pura que transforma a árvore em dispositivos;
//! - [`UdevProvider`]: a implementação de [`raiza_core::Provider`].

#![warn(missing_docs)]

mod provider;
pub mod scan;
pub mod source;
pub mod tree;

pub use provider::{UdevProvider, diff};
pub use scan::{IGNORED_USB, scan};
pub use source::{ChangeStream, SystemUdev, UdevSource};
pub use tree::{Node, Tree};

/// Id deste provedor.
pub const PROVIDER_ID: &str = "udev";
