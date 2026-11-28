//! Uma linha da página de detalhes por capacidade.
//!
//! [`row_for`] é o **ponto único** em que a página pede "a linha para esta capacidade".
//! O widget sai do [`raiza_core::CapabilityKind`], nunca da marca (pilar 3).
//!
//! A lógica que não depende de GTK fica em [`debounce`] e [`sync`], testadas à parte.
//! [`link`] junta as duas com o widget e com as escritas no daemon.

mod basic;
pub mod debounce;
mod level;
pub mod link;
mod polling;
mod rgb;
mod stages;
pub mod sync;

use raiza_core::dbus::RaizaClient;
use raiza_core::{Capability, DeviceId};
use relm4::adw;
use relm4::adw::prelude::*;
use relm4::gtk;

pub use link::Reporter;
use link::Writer;

use crate::text;

/// O que uma linha precisa saber além da capacidade.
pub struct RowContext<'a> {
    /// Dispositivo dono da capacidade.
    pub device: &'a DeviceId,
    /// Cliente do daemon, para escritas. `None` enquanto não há conexão.
    pub client: Option<&'a RaizaClient>,
    /// Como avisar a janela de um erro (toast).
    pub report: &'a Reporter,
}

impl RowContext<'_> {
    fn writer(&self, capability: &Capability) -> Writer {
        Writer::new(
            self.device.clone(),
            self.client.cloned(),
            self.report.clone(),
            capability.kind(),
        )
    }
}

/// Uma linha da página de detalhes.
pub trait CapabilityRow {
    /// O widget a pôr no grupo de capacidades.
    fn widget(&self) -> &gtk::Widget;
    /// Mostra o valor novo da mesma capacidade, sem recriar o widget.
    fn update(&self, capability: &Capability);
    /// Grupo à parte que a linha quer na página, depois do grupo de capacidades
    /// (ex.: os estágios de DPI). A linha cuida da visibilidade dele.
    fn extra_group(&self) -> Option<&adw::PreferencesGroup> {
        None
    }
}

/// A linha para esta capacidade.
pub fn row_for(capability: &Capability, ctx: &RowContext<'_>) -> Box<dyn CapabilityRow> {
    match capability {
        Capability::Battery { .. } => Box::new(basic::BatteryRow::new(capability)),
        Capability::Dpi { .. } => Box::new(level::DpiRow::new(capability, ctx.writer(capability))),
        Capability::Volume { .. } => {
            Box::new(level::VolumeRow::new(capability, ctx.writer(capability)))
        }
        Capability::PollingRate { .. } | Capability::ScanRate { .. } => {
            Box::new(polling::RateRow::new(capability, ctx.writer(capability)))
        }
        Capability::Rgb { .. } => Box::new(rgb::RgbRow::new(capability, ctx.writer(capability))),
        Capability::Mute { .. } => {
            Box::new(basic::ToggleRow::new(capability, ctx.writer(capability)))
        }
        Capability::Connection { writable: true, .. } => {
            Box::new(basic::ToggleRow::new(capability, ctx.writer(capability)))
        }
        Capability::Connection { .. } => Box::new(basic::ReadOnlyRow::new(capability)),
        Capability::Rumble { .. } => {
            Box::new(basic::RumbleRow::new(capability, ctx.writer(capability)))
        }
    }
}

/// Deixa `widget` habilitado ou, se a capacidade é somente leitura, desabilitado com a
/// explicação na dica (pilar 1).
fn set_writable(widget: &impl IsA<gtk::Widget>, writable: bool) {
    widget.set_sensitive(writable);
    widget.set_tooltip_text((!writable).then_some(text::read_only_tooltip()));
}
