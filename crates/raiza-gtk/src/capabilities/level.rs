//! Valores com limites: DPI (`AdwSpinRow`, mais o grupo de estágios de [`super::stages`])
//! e volume (slider).
//! Os dois passam pelo debounce, porque segurar a seta ou arrastar gera rajadas.

use std::cell::Cell;
use std::rc::Rc;

use raiza_core::{Capability, CapabilityValue, Level};
use relm4::adw;
use relm4::adw::prelude::*;
use relm4::gtk;

use super::link::{Link, Writer};
use super::stages::DpiStages;
use super::{CapabilityRow, set_writable};
use crate::text;

/// Arredonda o valor do widget para o valor aceito mais próximo (`min`, `max` e `step`).
pub fn snap(value: f64, level: &Level) -> u32 {
    let (min, max) = (f64::from(level.min), f64::from(level.max.max(level.min)));
    let value = if value.is_finite() { value } else { min };
    let value = value.clamp(min, max);
    if level.step <= 1 {
        return value.round() as u32;
    }
    let step = f64::from(level.step);
    let snapped = min + ((value - min) / step).round() * step;
    // O último passo pode passar do máximo quando o intervalo não é múltiplo do passo.
    let snapped = if snapped > max {
        snapped - step
    } else {
        snapped
    };
    snapped.max(min) as u32
}

/// Passo do widget: `step`, ou 1 quando qualquer valor vale.
pub(super) fn widget_step(level: &Level) -> f64 {
    f64::from(level.step.max(1))
}

/// Ajuste de um campo numérico com os limites e o passo de `level`.
pub(super) fn spin_adjustment(value: u32, level: &Level) -> gtk::Adjustment {
    gtk::Adjustment::new(
        f64::from(value),
        f64::from(level.min),
        f64::from(level.max),
        widget_step(level),
        widget_step(level) * 10.0,
        0.0,
    )
}

/// Troca os limites e o passo de um ajuste já criado.
pub(super) fn set_limits(adj: &gtk::Adjustment, level: &Level) {
    adj.set_lower(f64::from(level.min));
    adj.set_upper(f64::from(level.max));
    adj.set_step_increment(widget_step(level));
    adj.set_page_increment(widget_step(level) * 10.0);
}

/// DPI: campo numérico com os limites do sensor. A tabela de estágios, quando conhecida,
/// vai num grupo à parte ([`DpiStages`]).
pub struct DpiRow {
    row: adw::SpinRow,
    level: Rc<Cell<Level>>,
    link: Rc<Link<u32>>,
    stages: DpiStages,
    widget: gtk::Widget,
}

impl DpiRow {
    /// Cria a linha. `capability` precisa ser `Dpi`.
    pub fn new(capability: &Capability, writer: Writer) -> Self {
        let Capability::Dpi { level, .. } = capability else {
            unreachable!("DpiRow só recebe Dpi")
        };
        let row = adw::SpinRow::builder()
            .title(text::capability_title(capability.kind()))
            .adjustment(&spin_adjustment(level.current, level))
            .snap_to_ticks(true)
            .numeric(true)
            .build();
        row.add_prefix(&gtk::Image::from_icon_name(&text::capability_icon(
            capability,
        )));

        let level_cell = Rc::new(Cell::new(*level));
        let stages = DpiStages::new(*level, writer.clone());
        let link = Link::new(
            level.current,
            writer,
            true,
            |dpi| CapabilityValue::Dpi(*dpi),
            {
                let row = row.clone();
                move |dpi| row.set_value(f64::from(*dpi))
            },
        );
        row.connect_value_notify({
            let link = Rc::downgrade(&link);
            let level = Rc::clone(&level_cell);
            move |row| {
                if let Some(link) = link.upgrade() {
                    link.user_changed(snap(row.value(), &level.get()));
                }
            }
        });
        let widget = row.clone().upcast();
        let this = DpiRow {
            row,
            level: level_cell,
            link,
            stages,
            widget,
        };
        this.update(capability);
        this
    }
}

impl CapabilityRow for DpiRow {
    fn widget(&self) -> &gtk::Widget {
        &self.widget
    }

    fn extra_group(&self) -> Option<&adw::PreferencesGroup> {
        Some(self.stages.group())
    }

    fn update(&self, capability: &Capability) {
        let Capability::Dpi {
            level,
            stages,
            active_stage,
            writable,
        } = capability
        else {
            return;
        };
        if self.level.get() != *level {
            self.link
                .quietly(|| set_limits(&self.row.adjustment(), level));
            self.level.set(*level);
        }
        let subtitle = if level.step > 1 {
            format!("De {} a {}, passo {}", level.min, level.max, level.step)
        } else {
            format!("De {} a {}", level.min, level.max)
        };
        self.row.set_subtitle(&subtitle);
        self.stages.update(level, stages, *active_stage, *writable);
        set_writable(&self.row, *writable);
        self.link.daemon_changed(level.current);
    }
}

/// Volume: slider com os limites da capacidade.
pub struct VolumeRow {
    row: adw::ActionRow,
    scale: gtk::Scale,
    level: Rc<Cell<Level>>,
    link: Rc<Link<u32>>,
    widget: gtk::Widget,
}

impl VolumeRow {
    /// Cria a linha. `capability` precisa ser `Volume`.
    pub fn new(capability: &Capability, writer: Writer) -> Self {
        let Capability::Volume { level, .. } = capability else {
            unreachable!("VolumeRow só recebe Volume")
        };
        let row = adw::ActionRow::builder()
            .title(text::capability_title(capability.kind()))
            .build();
        row.add_prefix(&gtk::Image::from_icon_name(&text::capability_icon(
            capability,
        )));
        let scale = gtk::Scale::builder()
            .orientation(gtk::Orientation::Horizontal)
            .adjustment(&gtk::Adjustment::new(
                f64::from(level.current),
                f64::from(level.min),
                f64::from(level.max),
                widget_step(level),
                widget_step(level) * 10.0,
                0.0,
            ))
            .digits(0)
            .draw_value(true)
            .value_pos(gtk::PositionType::Right)
            .hexpand(true)
            .width_request(160)
            .valign(gtk::Align::Center)
            .build();
        row.add_suffix(&scale);
        let level_cell = Rc::new(Cell::new(*level));
        let link = Link::new(
            level.current,
            writer,
            true,
            |v| CapabilityValue::Volume(*v),
            {
                let scale = scale.clone();
                move |v| scale.set_value(f64::from(*v))
            },
        );
        scale.connect_value_changed({
            let link = Rc::downgrade(&link);
            let level = Rc::clone(&level_cell);
            move |scale| {
                if let Some(link) = link.upgrade() {
                    link.user_changed(snap(scale.value(), &level.get()));
                }
            }
        });
        let widget = row.clone().upcast();
        let this = VolumeRow {
            row,
            scale,
            level: level_cell,
            link,
            widget,
        };
        this.update(capability);
        this
    }
}

impl CapabilityRow for VolumeRow {
    fn widget(&self) -> &gtk::Widget {
        &self.widget
    }

    fn update(&self, capability: &Capability) {
        let Capability::Volume { level, writable } = capability else {
            return;
        };
        if self.level.get() != *level {
            self.link.quietly(|| {
                self.scale
                    .set_range(f64::from(level.min), f64::from(level.max));
                self.scale
                    .set_increments(widget_step(level), widget_step(level) * 10.0);
            });
            self.level.set(*level);
        }
        set_writable(&self.row, *writable);
        self.link.daemon_changed(level.current);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level(min: u32, max: u32, step: u32) -> Level {
        Level {
            current: min,
            min,
            max,
            step,
        }
    }

    #[test]
    fn arredonda_para_valor_aceito() {
        let dpi = level(400, 16000, 50);
        assert_eq!(snap(1624.0, &dpi), 1600);
        assert_eq!(snap(1626.0, &dpi), 1650);
        assert_eq!(snap(10.0, &dpi), 400);
        assert_eq!(snap(99999.0, &dpi), 16000);
        assert_eq!(snap(f64::NAN, &dpi), 400);
        // Intervalo que não é múltiplo do passo: nunca passa do máximo.
        let torto = level(0, 95, 10);
        assert_eq!(snap(95.0, &torto), 90);
        let vol = level(0, 150, 1);
        assert_eq!(snap(72.4, &vol), 72);
        assert_eq!(snap(-3.0, &vol), 0);
        for v in [0.0, 13.0, 77.7, 95.0, 1000.0] {
            for l in [dpi, torto, vol, level(5, 5, 0)] {
                assert!(l.check(snap(v, &l)).is_ok(), "{v} {l:?}");
            }
        }
    }
}
