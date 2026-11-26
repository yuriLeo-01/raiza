//! Uma linha da página de detalhes por capacidade.
//!
//! [`row_for`] é o **ponto único** em que a página pede "a linha para esta capacidade".
//! Hoje toda capacidade vira uma linha somente leitura ([`ReadOnlyRow`]); a T14 troca a
//! implementação por widgets interativos sem mexer na página.

use raiza_core::dbus::RaizaClient;
use raiza_core::{Capability, DeviceId};
use relm4::adw;
use relm4::adw::prelude::*;
use relm4::gtk;

use crate::text;

/// O que uma linha precisa saber além da capacidade.
pub struct RowContext<'a> {
    /// Dispositivo dono da capacidade.
    #[expect(dead_code, reason = "usado pelos controles interativos da T14")]
    pub device: &'a DeviceId,
    /// Cliente do daemon, para escritas. `None` enquanto não há conexão.
    #[expect(dead_code, reason = "usado pelos controles interativos da T14")]
    pub client: Option<&'a RaizaClient>,
}

/// Uma linha da página de detalhes.
pub trait CapabilityRow {
    /// O widget a pôr no grupo de capacidades.
    fn widget(&self) -> &gtk::Widget;
    /// Mostra o valor novo da mesma capacidade, sem recriar o widget.
    fn update(&self, capability: &Capability);
}

/// A linha para esta capacidade.
pub fn row_for(capability: &Capability, _ctx: &RowContext<'_>) -> Box<dyn CapabilityRow> {
    Box::new(ReadOnlyRow::new(capability))
}

/// Linha somente leitura: título, ícone e o resumo do valor atual.
pub struct ReadOnlyRow {
    row: adw::ActionRow,
    icon: gtk::Image,
    widget: gtk::Widget,
}

impl ReadOnlyRow {
    /// Cria a linha.
    pub fn new(capability: &Capability) -> Self {
        let icon = gtk::Image::new();
        let row = adw::ActionRow::builder()
            .title(text::capability_title(capability.kind()))
            .subtitle_selectable(true)
            .build();
        row.add_prefix(&icon);
        let widget = row.clone().upcast();
        let this = ReadOnlyRow { row, icon, widget };
        this.update(capability);
        this
    }
}

impl CapabilityRow for ReadOnlyRow {
    fn widget(&self) -> &gtk::Widget {
        &self.widget
    }

    fn update(&self, capability: &Capability) {
        self.icon
            .set_icon_name(Some(&text::capability_icon(capability)));
        self.row
            .set_subtitle(&text::escape_markup(&text::capability_summary(capability)));
    }
}
