//! Taxa de atualização: `AdwComboRow` com as taxas aceitas.

use std::cell::RefCell;
use std::rc::Rc;

use raiza_core::{Capability, CapabilityValue};
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

/// Veja o módulo.
pub struct PollingRow {
    row: adw::ComboRow,
    names: gtk::StringList,
    rates: Rc<RefCell<Vec<u32>>>,
    link: Rc<Link<u32>>,
    widget: gtk::Widget,
}

fn rate_name(hz: u32) -> String {
    format!("{hz} Hz")
}

impl PollingRow {
    /// Cria a linha. `capability` precisa ser `PollingRate`.
    pub fn new(capability: &Capability, writer: Writer) -> Self {
        let Capability::PollingRate { current, .. } = capability else {
            unreachable!("PollingRow só recebe PollingRate")
        };
        let names = gtk::StringList::new(&[]);
        let row = adw::ComboRow::builder()
            .title(text::capability_title(capability.kind()))
            .model(&names)
            .build();
        row.add_prefix(&gtk::Image::from_icon_name(&text::capability_icon(
            capability,
        )));
        let rates = Rc::new(RefCell::new(Vec::<u32>::new()));
        let link = Link::new(
            *current,
            writer,
            false,
            |hz| CapabilityValue::PollingRate(*hz),
            {
                let row = row.clone();
                let rates = Rc::clone(&rates);
                move |hz| {
                    if let Some(i) = rates.borrow().iter().position(|r| r == hz) {
                        row.set_selected(i as u32);
                    }
                }
            },
        );
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
        let this = PollingRow {
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

impl CapabilityRow for PollingRow {
    fn widget(&self) -> &gtk::Widget {
        &self.widget
    }

    fn update(&self, capability: &Capability) {
        let Capability::PollingRate {
            current,
            supported,
            writable,
        } = capability
        else {
            return;
        };
        let rates = options(*current, supported);
        if *self.rates.borrow() != rates {
            self.link.quietly(|| {
                let labels: Vec<String> = rates.iter().copied().map(rate_name).collect();
                let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
                *self.rates.borrow_mut() = rates.clone();
                self.names.splice(0, self.names.n_items(), &labels);
                // A lista nova pode ter mudado a seleção: volta para o valor mostrado.
                if let Some(i) = rates.iter().position(|r| r == current) {
                    self.row.set_selected(i as u32);
                }
            });
        }
        set_writable(&self.row, *writable);
        self.link.daemon_changed(*current);
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
}
