//! Iluminação: `AdwExpanderRow` com o modo (`AdwComboRow`) e uma cor por zona
//! (`GtkColorDialogButton`).

use std::cell::RefCell;
use std::rc::Rc;

use raiza_core::{Capability, CapabilityValue, Color, RgbSetting, RgbZone};
use relm4::adw;
use relm4::adw::prelude::*;
use relm4::gtk;
use relm4::gtk::gdk;

use super::link::{Link, Writer};
use super::{CapabilityRow, set_writable};
use crate::text;

/// Canal de cor do GTK (0.0 a 1.0) para byte.
pub fn channel(value: f32) -> u8 {
    let value = if value.is_finite() { value } else { 0.0 };
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Cor do GTK para cor do Raiza (o alfa é ignorado).
pub fn from_rgba(rgba: &gdk::RGBA) -> Color {
    Color {
        r: channel(rgba.red()),
        g: channel(rgba.green()),
        b: channel(rgba.blue()),
    }
}

fn to_rgba(color: Color) -> gdk::RGBA {
    gdk::RGBA::new(
        f32::from(color.r) / 255.0,
        f32::from(color.g) / 255.0,
        f32::from(color.b) / 255.0,
        1.0,
    )
}

/// Uma zona: linha com o botão de cor.
struct ZoneRow {
    row: adw::ActionRow,
    button: gtk::ColorDialogButton,
    link: Rc<Link<Color>>,
}

impl ZoneRow {
    fn new(index: u32, zone: &RgbZone, writer: Writer) -> Self {
        let dialog = gtk::ColorDialog::builder()
            .title(text::zone_color_title(&zone.name))
            .with_alpha(false)
            .modal(true)
            .build();
        let button = gtk::ColorDialogButton::builder()
            .dialog(&dialog)
            .valign(gtk::Align::Center)
            .build();
        let row = adw::ActionRow::builder()
            .title(text::escape_markup(&zone.name))
            .build();
        row.add_suffix(&button);
        let link = Link::new(
            zone.color,
            writer,
            false,
            move |color| {
                CapabilityValue::Rgb(RgbSetting {
                    zone: Some(index),
                    mode: None,
                    color: Some(*color),
                })
            },
            {
                let button = button.clone();
                move |color| button.set_rgba(&to_rgba(*color))
            },
        );
        button.connect_rgba_notify({
            let link = Rc::downgrade(&link);
            move |button| {
                if let Some(link) = link.upgrade() {
                    link.user_changed(from_rgba(&button.rgba()));
                }
            }
        });
        ZoneRow { row, button, link }
    }
}

/// Veja o módulo.
pub struct RgbRow {
    row: adw::ExpanderRow,
    mode_row: adw::ComboRow,
    mode_names: gtk::StringList,
    modes: Rc<RefCell<Vec<String>>>,
    mode_link: Rc<Link<String>>,
    zones: RefCell<Vec<ZoneRow>>,
    /// Nomes das zonas mostradas: se mudarem, as linhas são recriadas.
    zone_names: RefCell<Vec<String>>,
    writer: Writer,
    widget: gtk::Widget,
}

impl RgbRow {
    /// Cria a linha. `capability` precisa ser `Rgb`.
    pub fn new(capability: &Capability, writer: Writer) -> Self {
        let Capability::Rgb { current_mode, .. } = capability else {
            unreachable!("RgbRow só recebe Rgb")
        };
        let row = adw::ExpanderRow::builder()
            .title(text::capability_title(capability.kind()))
            .build();
        row.add_prefix(&gtk::Image::from_icon_name(&text::capability_icon(
            capability,
        )));
        let mode_names = gtk::StringList::new(&[]);
        let mode_row = adw::ComboRow::builder()
            .title(text::rgb_mode_title())
            .model(&mode_names)
            .build();
        row.add_row(&mode_row);
        let modes = Rc::new(RefCell::new(Vec::<String>::new()));
        let mode_link = Link::new(
            current_mode.clone(),
            writer.clone(),
            false,
            |mode| {
                CapabilityValue::Rgb(RgbSetting {
                    zone: None,
                    mode: Some(mode.clone()),
                    color: None,
                })
            },
            {
                let mode_row = mode_row.clone();
                let modes = Rc::clone(&modes);
                move |mode| {
                    if let Some(i) = modes.borrow().iter().position(|m| m == mode) {
                        mode_row.set_selected(i as u32);
                    }
                }
            },
        );
        mode_row.connect_selected_notify({
            let link = Rc::downgrade(&mode_link);
            let modes = Rc::clone(&modes);
            move |row| {
                let mode = modes.borrow().get(row.selected() as usize).cloned();
                if let (Some(link), Some(mode)) = (link.upgrade(), mode) {
                    link.user_changed(mode);
                }
            }
        });
        let widget = row.clone().upcast();
        let this = RgbRow {
            row,
            mode_row,
            mode_names,
            modes,
            mode_link,
            zones: RefCell::new(Vec::new()),
            zone_names: RefCell::new(Vec::new()),
            writer,
            widget,
        };
        this.update(capability);
        this
    }

    fn set_modes(&self, modes: &[String], current: &str) {
        let mut options = modes.to_vec();
        if !options.iter().any(|m| m == current) {
            options.push(current.to_owned());
        }
        if *self.modes.borrow() == options {
            return;
        }
        self.mode_link.quietly(|| {
            let labels: Vec<&str> = options.iter().map(String::as_str).collect();
            *self.modes.borrow_mut() = options.clone();
            self.mode_names
                .splice(0, self.mode_names.n_items(), &labels);
            if let Some(i) = options.iter().position(|m| m == current) {
                self.mode_row.set_selected(i as u32);
            }
        });
    }

    fn set_zones(&self, zones: &[RgbZone], writable: bool) {
        let names: Vec<String> = zones.iter().map(|z| z.name.clone()).collect();
        if *self.zone_names.borrow() != names {
            for zone in self.zones.borrow_mut().drain(..) {
                self.row.remove(&zone.row);
            }
            let mut rows = self.zones.borrow_mut();
            for (i, zone) in zones.iter().enumerate() {
                let Ok(index) = u32::try_from(i) else { break };
                let zone_row = ZoneRow::new(index, zone, self.writer.clone());
                self.row.add_row(&zone_row.row);
                rows.push(zone_row);
            }
            *self.zone_names.borrow_mut() = names;
        }
        for (zone_row, zone) in self.zones.borrow().iter().zip(zones) {
            set_writable(&zone_row.row, writable);
            zone_row.button.set_sensitive(writable);
            zone_row.link.daemon_changed(zone.color);
        }
    }
}

impl CapabilityRow for RgbRow {
    fn widget(&self) -> &gtk::Widget {
        &self.widget
    }

    fn update(&self, capability: &Capability) {
        let Capability::Rgb {
            zones,
            modes,
            current_mode,
            writable,
        } = capability
        else {
            return;
        };
        self.row
            .set_subtitle(&text::escape_markup(&text::capability_summary(capability)));
        // A linha de fora continua expansível para mostrar as cores, mesmo sem escrita.
        self.row
            .set_tooltip_text((!*writable).then_some(text::read_only_tooltip()));
        self.set_modes(modes, current_mode);
        set_writable(&self.mode_row, *writable);
        self.mode_link.daemon_changed(current_mode.clone());
        self.set_zones(zones, *writable);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canais_de_cor() {
        assert_eq!(channel(0.0), 0);
        assert_eq!(channel(1.0), 255);
        assert_eq!(channel(0.5), 128);
        assert_eq!(channel(-1.0), 0);
        assert_eq!(channel(7.0), 255);
        assert_eq!(channel(f32::NAN), 0);
        // Ida e volta sem perda.
        for c in [0u8, 1, 127, 128, 254, 255] {
            assert_eq!(channel(f32::from(c) / 255.0), c);
        }
    }
}
