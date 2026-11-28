//! Página de detalhes de um dispositivo: informações gerais e uma linha por capacidade.
//!
//! Quando o mesmo dispositivo continua com os mesmos tipos de capacidade (o caso comum:
//! um valor mudou, ou o par `DeviceRemoved` + `DeviceAdded`), as linhas são atualizadas
//! no lugar, sem piscar e sem perder o foco. Uma linha pode trazer um grupo extra (os
//! estágios de DPI), que entra na página depois do grupo de capacidades.

use raiza_core::dbus::RaizaClient;
use raiza_core::{CapabilityKind, DeviceId, DeviceInfo};
use relm4::adw;
use relm4::adw::prelude::*;

use crate::capabilities::{self, CapabilityRow, Reporter, RowContext};
use crate::model;
use crate::text;

/// A página de detalhes.
pub struct Detail {
    page: adw::PreferencesPage,
    info: adw::PreferencesGroup,
    providers: adw::ActionRow,
    id_row: adw::ActionRow,
    caps: adw::PreferencesGroup,
    rows: Vec<Box<dyn CapabilityRow>>,
    /// O que as linhas mostram hoje: dispositivo e tipos de capacidade.
    shown: Option<(DeviceId, Vec<CapabilityKind>)>,
    /// Para as linhas avisarem a janela de erros de escrita.
    report: Reporter,
}

impl Detail {
    /// Cria a página vazia. `report` mostra os erros de escrita na janela.
    pub fn new(report: Reporter) -> Self {
        let page = adw::PreferencesPage::new();
        let info = adw::PreferencesGroup::new();
        let providers = adw::ActionRow::builder()
            .title("Provedores")
            .subtitle_selectable(true)
            .css_classes(["property"])
            .build();
        let id_row = adw::ActionRow::builder()
            .title("Identificador")
            .subtitle_selectable(true)
            .css_classes(["property"])
            .build();
        info.add(&providers);
        info.add(&id_row);
        let caps = adw::PreferencesGroup::builder()
            .title("Capacidades")
            .build();
        page.add(&info);
        page.add(&caps);
        Detail {
            page,
            info,
            providers,
            id_row,
            caps,
            rows: Vec::new(),
            shown: None,
            report,
        }
    }

    /// O widget da página.
    pub fn widget(&self) -> &adw::PreferencesPage {
        &self.page
    }

    /// Mostra `device`, reaproveitando as linhas quando possível.
    pub fn show(&mut self, device: &DeviceInfo, client: Option<&RaizaClient>) {
        self.info.set_title(&text::escape_markup(&device.name));
        self.info
            .set_description(Some(&text::escape_markup(&text::device_subtitle(device))));
        self.providers
            .set_subtitle(&text::escape_markup(&device.providers.join(", ")));
        self.id_row
            .set_subtitle(&text::escape_markup(&device.id.to_string()));

        let caps = model::ordered_capabilities(device);
        let kinds = model::capability_kinds(device);
        let same = self
            .shown
            .as_ref()
            .is_some_and(|(id, shown)| *id == device.id && *shown == kinds);
        if same {
            for (row, cap) in self.rows.iter().zip(caps) {
                row.update(cap);
            }
            return;
        }

        self.remove_rows();
        let ctx = RowContext {
            device: &device.id,
            client,
            report: &self.report,
        };
        for cap in caps {
            let row = capabilities::row_for(cap, &ctx);
            self.caps.add(row.widget());
            if let Some(group) = row.extra_group() {
                self.page.add(group);
            }
            self.rows.push(row);
        }
        self.caps.set_visible(!self.rows.is_empty());
        if self.shown.as_ref().is_none_or(|(id, _)| *id != device.id) {
            self.page.scroll_to_top();
        }
        self.shown = Some((device.id.clone(), kinds));
    }

    /// Esquece o que está na página (a próxima chamada a [`Detail::show`] recria tudo).
    pub fn clear(&mut self) {
        self.remove_rows();
        self.shown = None;
    }

    /// Tira da página as linhas e os grupos extras delas.
    fn remove_rows(&mut self) {
        for row in self.rows.drain(..) {
            self.caps.remove(row.widget());
            if let Some(group) = row.extra_group() {
                self.page.remove(group);
            }
        }
    }
}
