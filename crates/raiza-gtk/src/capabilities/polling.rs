//! Taxas em Hz (atualização e varredura): `AdwComboRow` com as taxas aceitas.

use std::cell::RefCell;
use std::rc::Rc;

use raiza_core::{Capability, CapabilityKind, CapabilityValue};
use relm4::adw;
use relm4::adw::prelude::*;
use relm4::gtk;

use super::link::{Link, Writer};
use super::{CapabilityRow, set_writable};
use crate::text;

/// As opções da lista: as taxas aceitas e, se a atual não estiver entre elas (aparelho
/// estranho), a atual no fim, para a lista não mentir sobre o estado.
pub fn options(current: u32, supported: &[u32]) -> Vec<u32> {
    let mut rates = supported.to_vec();
    if !rates.contains(&current) {
        rates.push(current);
    }
    rates
}

/// Taxa atual, aceitas e se aceita escrita, de `PollingRate` ou `ScanRate`.
fn rate_parts(capability: &Capability) -> Option<(u32, &[u32], bool)> {
    match capability {
        Capability::PollingRate {
            current,
            supported,
            writable,
        }
        | Capability::ScanRate {
            current,
            supported,
            writable,
        } => Some((*current, supported, *writable)),
        _ => None,
    }
}

/// O valor de escrita de `hz` para a capacidade `kind` (só as duas taxas chegam aqui).
pub fn encode(kind: CapabilityKind, hz: u32) -> CapabilityValue {
    if kind == CapabilityKind::ScanRate {
        CapabilityValue::ScanRate(hz)
    } else {
        CapabilityValue::PollingRate(hz)
    }
}

/// Veja o módulo.
pub struct RateRow {
    row: adw::ComboRow,
    names: gtk::StringList,
    rates: Rc<RefCell<Vec<u32>>>,
    link: Rc<Link<u32>>,
    widget: gtk::Widget,
}

fn rate_name(hz: u32) -> String {
    format!("{hz} Hz")
}

impl RateRow {
    /// Cria a linha. `capability` precisa ser `PollingRate` ou `ScanRate`.
    pub fn new(capability: &Capability, writer: Writer) -> Self {
        let Some((current, ..)) = rate_parts(capability) else {
            unreachable!("RateRow só recebe PollingRate ou ScanRate")
        };
        let kind = capability.kind();
        let names = gtk::StringList::new(&[]);
        let row = adw::ComboRow::builder()
            .title(text::capability_title(kind))
            .model(&names)
            .build();
        if let Some(subtitle) = text::capability_hint(kind) {
            row.set_subtitle(subtitle);
        }
        row.add_prefix(&gtk::Image::from_icon_name(&text::capability_icon(
            capability,
        )));
        let rates = Rc::new(RefCell::new(Vec::<u32>::new()));
        let link = Link::new(current, writer, false, move |hz| encode(kind, *hz), {
            let row = row.clone();
            let rates = Rc::clone(&rates);
            move |hz| {
                if let Some(i) = rates.borrow().iter().position(|r| r == hz) {
                    row.set_selected(i as u32);
                }
            }
        });
        row.connect_selected_notify({
            let link = Rc::downgrade(&link);
            let rates = Rc::clone(&rates);
            move |row| {
                let hz = rates.borrow().get(row.selected() as usize).copied();
                if let (Some(link), Some(hz)) = (link.upgrade(), hz) {
                    link.user_changed(hz);
                }
            }
        });
        let widget = row.clone().upcast();
        let this = RateRow {
            row,
            names,
            rates,
            link,
            widget,
        };
        this.update(capability);
        this
    }
}

impl CapabilityRow for RateRow {
    fn widget(&self) -> &gtk::Widget {
        &self.widget
    }

    fn update(&self, capability: &Capability) {
        let Some((current, supported, writable)) = rate_parts(capability) else {
            return;
        };
        let rates = options(current, supported);
        if *self.rates.borrow() != rates {
            self.link.quietly(|| {
                let labels: Vec<String> = rates.iter().copied().map(rate_name).collect();
                let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
                *self.rates.borrow_mut() = rates.clone();
                self.names.splice(0, self.names.n_items(), &labels);
                // A lista nova pode ter mudado a seleção: volta para o valor mostrado.
                if let Some(i) = rates.iter().position(|r| *r == current) {
                    self.row.set_selected(i as u32);
                }
            });
        }
        set_writable(&self.row, writable);
        self.link.daemon_changed(current);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opcoes_incluem_a_taxa_atual() {
        assert_eq!(options(500, &[125, 500, 1000]), vec![125, 500, 1000]);
        assert_eq!(options(8000, &[125, 1000]), vec![125, 1000, 8000]);
        assert_eq!(options(1000, &[]), vec![1000]);
    }

    #[test]
    fn cada_taxa_escreve_o_seu_valor() {
        assert_eq!(
            encode(CapabilityKind::PollingRate, 500),
            CapabilityValue::PollingRate(500)
        );
        assert_eq!(
            encode(CapabilityKind::ScanRate, 4000),
            CapabilityValue::ScanRate(4000)
        );
    }

    #[test]
    fn partes_das_duas_taxas() {
        let scan = Capability::ScanRate {
            current: 2000,
            supported: vec![1000, 2000, 4000],
            writable: false,
        };
        assert_eq!(
            rate_parts(&scan),
            Some((2000, &[1000, 2000, 4000][..], false))
        );
        assert_eq!(
            rate_parts(&Capability::Battery {
                percent: 1,
                charging: false
            }),
            None
        );
    }
}
