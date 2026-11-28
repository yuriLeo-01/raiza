//! Linhas simples: somente leitura, bateria, chave liga/desliga e teste de vibração.

use std::rc::Rc;

use raiza_core::{Capability, CapabilityValue};
use relm4::adw;
use relm4::adw::prelude::*;
use relm4::gtk;

use super::link::{Link, Writer};
use super::{CapabilityRow, set_writable};
use crate::text;

/// Linha somente leitura: título, ícone e o resumo do valor atual.
///
/// Usada para capacidades que só informam (ex.: conexão que o aparelho não deixa mudar).
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
        // Não fica cinza (é só informação), mas explica por que não há controle.
        // A bateria é somente leitura por natureza: não precisa de explicação.
        let hint = (!capability.is_writable() && !matches!(capability, Capability::Battery { .. }))
            .then_some(text::read_only_tooltip());
        self.row.set_tooltip_text(hint);
    }
}

/// Bateria: nível em texto e numa barra, e o estado de carga.
pub struct BatteryRow {
    base: ReadOnlyRow,
    bar: gtk::LevelBar,
}

impl BatteryRow {
    /// Cria a linha.
    pub fn new(capability: &Capability) -> Self {
        let bar = gtk::LevelBar::builder()
            .min_value(0.0)
            .max_value(100.0)
            .width_request(96)
            .valign(gtk::Align::Center)
            .build();
        let base = ReadOnlyRow::new(capability);
        base.row.add_suffix(&bar);
        let this = BatteryRow { base, bar };
        this.update(capability);
        this
    }
}

impl CapabilityRow for BatteryRow {
    fn widget(&self) -> &gtk::Widget {
        self.base.widget()
    }

    fn update(&self, capability: &Capability) {
        self.base.update(capability);
        if let Capability::Battery { percent, .. } = capability {
            self.bar.set_value(f64::from((*percent).min(100)));
        }
    }
}

/// Valor liga/desliga de uma capacidade: mudo ou conexão.
fn toggle_state(capability: &Capability) -> Option<bool> {
    match capability {
        Capability::Mute { muted, .. } => Some(*muted),
        Capability::Connection { connected, .. } => Some(*connected),
        _ => None,
    }
}

/// Chave liga/desliga: mudo, ou conectar/desconectar.
pub struct ToggleRow {
    row: adw::SwitchRow,
    icon: gtk::Image,
    link: Rc<Link<bool>>,
    widget: gtk::Widget,
}

impl ToggleRow {
    /// Cria a linha. `capability` precisa ser `Mute` ou `Connection`.
    pub fn new(capability: &Capability, writer: Writer) -> Self {
        let icon = gtk::Image::new();
        let row = adw::SwitchRow::builder()
            .title(text::capability_title(capability.kind()))
            .build();
        row.add_prefix(&icon);
        let is_mute = matches!(capability, Capability::Mute { .. });
        let link = Link::new(
            toggle_state(capability).unwrap_or_default(),
            writer,
            false,
            move |on| {
                if is_mute {
                    CapabilityValue::Mute(*on)
                } else {
                    CapabilityValue::Connection(*on)
                }
            },
            {
                let row = row.clone();
                move |on| row.set_active(*on)
            },
        );
        row.connect_active_notify({
            let link = Rc::downgrade(&link);
            move |row| {
                if let Some(link) = link.upgrade() {
                    link.user_changed(row.is_active());
                }
            }
        });
        let widget = row.clone().upcast();
        let this = ToggleRow {
            row,
            icon,
            link,
            widget,
        };
        this.update(capability);
        this
    }
}

impl CapabilityRow for ToggleRow {
    fn widget(&self) -> &gtk::Widget {
        &self.widget
    }

    fn update(&self, capability: &Capability) {
        let Some(on) = toggle_state(capability) else {
            return;
        };
        self.icon
            .set_icon_name(Some(&text::capability_icon(capability)));
        self.row
            .set_subtitle(&text::escape_markup(&text::capability_summary(capability)));
        set_writable(&self.row, capability.is_writable());
        self.link.daemon_changed(on);
    }
}

/// Intensidade do pulso de teste (0 a 100).
const RUMBLE_STRENGTH: u8 = 50;
/// Duração preferida do pulso de teste.
const RUMBLE_MS: u32 = 300;

/// O pulso de teste: curto, de intensidade média e sempre dentro de `max_duration_ms`.
/// `None` se o aparelho não aceita pulso nenhum.
pub fn rumble_pulse(max_duration_ms: u32) -> Option<CapabilityValue> {
    (max_duration_ms > 0).then(|| CapabilityValue::Rumble {
        strength: RUMBLE_STRENGTH,
        duration_ms: RUMBLE_MS.min(max_duration_ms),
    })
}

/// Vibração: botão "Testar" que manda um pulso curto.
pub struct RumbleRow {
    row: adw::ActionRow,
    button: gtk::Button,
    state: Rc<RumbleState>,
    widget: gtk::Widget,
}

struct RumbleState {
    max_duration_ms: std::cell::Cell<u32>,
    writable: std::cell::Cell<bool>,
    busy: std::cell::Cell<bool>,
}

impl RumbleRow {
    /// Cria a linha.
    pub fn new(capability: &Capability, writer: Writer) -> Self {
        let row = adw::ActionRow::builder()
            .title(text::capability_title(capability.kind()))
            .build();
        row.add_prefix(&gtk::Image::from_icon_name(&text::capability_icon(
            capability,
        )));
        let button = gtk::Button::builder()
            .label(text::test_rumble())
            .valign(gtk::Align::Center)
            .build();
        row.add_suffix(&button);
        let state = Rc::new(RumbleState {
            max_duration_ms: Default::default(),
            writable: Default::default(),
            busy: Default::default(),
        });
        button.connect_clicked({
            let state = Rc::clone(&state);
            move |button| {
                let Some(pulse) = rumble_pulse(state.max_duration_ms.get()) else {
                    return;
                };
                if state.busy.replace(true) {
                    return;
                }
                button.set_sensitive(false);
                let state = Rc::clone(&state);
                let button = button.clone();
                writer.send(pulse, move |_| {
                    state.busy.set(false);
                    button.set_sensitive(state.writable.get());
                });
            }
        });
        let widget = row.clone().upcast();
        let this = RumbleRow {
            row,
            button,
            state,
            widget,
        };
        this.update(capability);
        this
    }
}

impl CapabilityRow for RumbleRow {
    fn widget(&self) -> &gtk::Widget {
        &self.widget
    }

    fn update(&self, capability: &Capability) {
        let Capability::Rumble {
            max_duration_ms,
            writable,
        } = capability
        else {
            return;
        };
        let writable = *writable && rumble_pulse(*max_duration_ms).is_some();
        self.state.max_duration_ms.set(*max_duration_ms);
        self.state.writable.set(writable);
        self.row
            .set_subtitle(&text::escape_markup(&text::capability_summary(capability)));
        set_writable(&self.row, writable);
        if !self.state.busy.get() {
            self.button.set_sensitive(writable);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pulso_de_teste_respeita_o_limite() {
        for max in [1, 100, 299, 300, 2000, u32::MAX] {
            let Some(CapabilityValue::Rumble {
                strength,
                duration_ms,
            }) = rumble_pulse(max)
            else {
                panic!("sem pulso para {max}");
            };
            assert!(duration_ms <= max && duration_ms <= RUMBLE_MS && duration_ms > 0);
            assert!(strength <= 100);
            let cap = Capability::Rumble {
                max_duration_ms: max,
                writable: true,
            };
            let pulse = rumble_pulse(max).unwrap();
            assert_eq!(pulse.validate(&cap), Ok(()));
        }
        assert_eq!(rumble_pulse(0), None);
    }

    #[test]
    fn estado_liga_desliga() {
        assert_eq!(
            toggle_state(&Capability::Mute {
                muted: true,
                writable: true
            }),
            Some(true)
        );
        assert_eq!(
            toggle_state(&Capability::Battery {
                percent: 1,
                charging: false
            }),
            None
        );
    }
}
