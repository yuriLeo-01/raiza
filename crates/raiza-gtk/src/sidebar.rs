//! Barra lateral: os dispositivos agrupados por tipo, com bateria.
//!
//! A lista só é recriada quando o conjunto ou a ordem dos dispositivos muda. Mudanças de
//! valor (bateria, nome do fabricante) atualizam as linhas no lugar.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use raiza_core::{DeviceId, DeviceKind};
use relm4::gtk;
use relm4::gtk::prelude::*;

use crate::model::Devices;
use crate::text::{self, SidebarEntry};

struct Row {
    title: gtk::Label,
    subtitle: gtk::Label,
    icon: gtk::Image,
    battery: gtk::Box,
    battery_icon: gtk::Image,
    battery_label: gtk::Label,
}

impl Row {
    fn new() -> (gtk::ListBoxRow, Row) {
        let icon = gtk::Image::new();
        let title = gtk::Label::builder()
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        let subtitle = gtk::Label::builder()
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(["caption", "dim-label"])
            .build();
        let labels = gtk::Box::new(gtk::Orientation::Vertical, 0);
        labels.set_hexpand(true);
        labels.set_valign(gtk::Align::Center);
        labels.append(&title);
        labels.append(&subtitle);

        let battery_icon = gtk::Image::new();
        let battery_label = gtk::Label::builder()
            .css_classes(["caption", "numeric"])
            .build();
        let battery = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        battery.add_css_class("dim-label");
        battery.append(&battery_icon);
        battery.append(&battery_label);

        let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        content.append(&icon);
        content.append(&labels);
        content.append(&battery);

        let list_row = gtk::ListBoxRow::builder().child(&content).build();
        let row = Row {
            title,
            subtitle,
            icon,
            battery,
            battery_icon,
            battery_label,
        };
        (list_row, row)
    }

    fn set(&self, entry: &SidebarEntry) {
        self.title.set_label(&entry.title);
        self.icon.set_icon_name(Some(entry.icon));
        match &entry.subtitle {
            Some(s) => {
                self.subtitle.set_label(s);
                self.subtitle.set_visible(true);
            }
            None => self.subtitle.set_visible(false),
        }
        match &entry.battery {
            Some((label, icon)) => {
                self.battery_label.set_label(label);
                self.battery_icon.set_icon_name(Some(icon));
                self.battery.set_visible(true);
            }
            None => self.battery.set_visible(false),
        }
    }
}

/// A barra lateral.
pub struct Sidebar {
    list: gtk::ListBox,
    /// Id e tipo de cada linha, na ordem da lista. Lido também pelo cabeçalho.
    layout: Rc<RefCell<Vec<(DeviceId, DeviceKind)>>>,
    rows: Vec<Row>,
    /// Ligado enquanto a própria barra mexe na seleção, para não gerar mensagens.
    quiet: Rc<Cell<bool>>,
}

impl Sidebar {
    /// Cria a barra. `on_activate` recebe o dispositivo que o usuário escolheu.
    pub fn new(on_activate: impl Fn(DeviceId) + 'static) -> Self {
        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .css_classes(["navigation-sidebar"])
            .build();
        let layout: Rc<RefCell<Vec<(DeviceId, DeviceKind)>>> = Rc::default();
        let quiet = Rc::new(Cell::new(false));

        list.set_header_func({
            let layout = Rc::clone(&layout);
            move |row, _before| {
                let index = usize::try_from(row.index()).ok();
                let layout = layout.borrow();
                let kind = index.and_then(|i| layout.get(i)).map(|(_, k)| *k);
                let previous = index
                    .and_then(|i| i.checked_sub(1))
                    .and_then(|i| layout.get(i))
                    .map(|(_, k)| *k);
                match kind {
                    Some(kind) if previous != Some(kind) => {
                        let header = gtk::Label::builder()
                            .label(text::kind_group_title(kind))
                            .xalign(0.0)
                            .margin_start(12)
                            .margin_top(if index == Some(0) { 6 } else { 18 })
                            .margin_bottom(6)
                            .css_classes(["heading", "dim-label"])
                            .build();
                        row.set_header(Some(&header));
                    }
                    _ => row.set_header(None::<&gtk::Widget>),
                }
            }
        });

        // Selecionar (clique ou teclado) troca a página; ativar a linha já selecionada
        // reabre a página quando a janela está estreita.
        let on_activate = Rc::new(on_activate);
        let activate = {
            let layout = Rc::clone(&layout);
            let quiet = Rc::clone(&quiet);
            move |row: &gtk::ListBoxRow| {
                if quiet.get() {
                    return;
                }
                let id = usize::try_from(row.index())
                    .ok()
                    .and_then(|i| layout.borrow().get(i).map(|(id, _)| id.clone()));
                if let Some(id) = id {
                    on_activate(id);
                }
            }
        };
        let activate = Rc::new(activate);
        list.connect_row_selected({
            let activate = Rc::clone(&activate);
            move |_, row| {
                if let Some(row) = row {
                    activate(row);
                }
            }
        });
        list.connect_row_activated(move |_, row| activate(row));

        Sidebar {
            list,
            layout,
            rows: Vec::new(),
            quiet,
        }
    }

    /// O widget da barra.
    pub fn widget(&self) -> &gtk::ListBox {
        &self.list
    }

    /// Mostra o estado de `devices`, inclusive a seleção.
    pub fn sync(&mut self, devices: &Devices) {
        let layout: Vec<(DeviceId, DeviceKind)> = devices
            .groups()
            .into_iter()
            .flat_map(|(kind, list)| list.into_iter().map(move |d| (d.id.clone(), kind)))
            .collect();

        self.quiet.set(true);
        if *self.layout.borrow() != layout {
            self.list.remove_all();
            self.rows.clear();
            for _ in &layout {
                let (list_row, row) = Row::new();
                self.list.append(&list_row);
                self.rows.push(row);
            }
            *self.layout.borrow_mut() = layout;
            self.list.invalidate_headers();
        }
        for (row, (id, _)) in self.rows.iter().zip(self.layout.borrow().iter()) {
            if let Some(device) = devices.get(id) {
                row.set(&SidebarEntry::new(device));
            }
        }
        let selected = devices.selected().and_then(|id| {
            self.layout
                .borrow()
                .iter()
                .position(|(other, _)| other == id)
                .and_then(|i| i32::try_from(i).ok())
                .and_then(|i| self.list.row_at_index(i))
        });
        match selected {
            Some(row) => self.list.select_row(Some(&row)),
            None => self.list.unselect_all(),
        }
        self.quiet.set(false);
    }
}
