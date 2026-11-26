//! Provedor `hid-perfis` do Raiza: executa perfis HID declarativos via `/dev/hidraw*`.
//!
//! É o provedor de maior prioridade (`docs/ARQUITETURA.md`). O crate `raiza-profiles`
//! descreve e valida os relatórios; este crate faz o I/O:
//!
//! - carrega os perfis dos diretórios ([`DirProfiles`]) e recarrega quando mudam;
//! - acha as interfaces hidraw que cada perfil descreve e abre a certa;
//! - identifica o aparelho (`identify`) antes de permitir qualquer escrita (pilar 1);
//! - lê com repetição limitada (respostas "ocupado" são descartadas até um timeout curto);
//! - escuta relatórios espontâneos (ex.: bateria) sem polling e emite `CapabilityChanged`;
//! - acompanha o hotplug pelo monitor do udev.
//!
//! Camadas:
//! - [`backend`]: a trait [`HidBackend`] (listar, abrir e vigiar hidraw) e [`HidHandle`];
//!   [`SystemHid`] é a implementação real (udev + hidapi);
//! - [`profiles`]: a trait [`ProfileSource`] e [`DirProfiles`] (disco + `notify`);
//! - [`HidProfilesProvider`]: a implementação de [`raiza_core::Provider`].

#![warn(missing_docs)]

pub mod backend;
pub mod descriptor;
mod device;
mod exchange;
pub mod profiles;
mod provider;
mod system;

pub use backend::{ChangeStream, HidBackend, HidHandle, HidInterface, Inventory, UsbDevice};
pub use profiles::{DirProfiles, ProfileSource, load_dir};
pub use provider::{HidProfilesProvider, Timeouts};
pub use system::{PERMISSION_HINT, SystemHid};

/// Id deste provedor.
pub const PROVIDER_ID: &str = "hid-perfis";
