//! Estado do ícone de bandeja, sem D-Bus: a lista de aparelhos, as linhas de bateria,
//! o ícone e o menu. A camada SNI (`tray.rs`) só traduz a [`View`] para o `ksni`.

use std::collections::BTreeMap;

use raiza_core::{Capability, CapabilityKind, DeviceId, DeviceInfo, ProviderEvent};

use crate::text;

/// Aparelhos conhecidos, mantidos a partir de `ListDevices` e dos sinais do daemon.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Model {
    /// `None` enquanto o daemon não responde.
    devices: Option<BTreeMap<DeviceId, DeviceInfo>>,
}

/// Uma linha da lista: um aparelho com bateria.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatteryRow {
    /// Nome do aparelho.
    pub name: String,
    /// Carga de 0 a 100.
    pub percent: u8,
    /// Se está carregando.
    pub charging: bool,
}

/// Se o ícone deve aparecer no painel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    /// Há bateria para mostrar.
    Active,
    /// Nada a mostrar: o painel pode esconder o ícone (`Status::Passive`).
    Passive,
}

/// Um item do menu, antes de virar `ksni::MenuItem`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// Linha informativa (desativada), como a bateria de um aparelho.
    Info(String),
    /// Separador.
    Separator,
    /// "Abrir o Raiza".
    OpenApp,
    /// "Sair".
    Quit,
}

/// Tudo o que o ícone mostra.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    /// Ativo ou passivo.
    pub visibility: Visibility,
    /// Nome do ícone no tema.
    pub icon_name: String,
    /// Descrição do tooltip (uma linha por aparelho, já escapada).
    pub tooltip: String,
    /// Itens do menu.
    pub menu: Vec<Entry>,
}

impl Default for View {
    fn default() -> Self {
        Model::default().view()
    }
}

impl Model {
    /// Daemon conectado com esta lista (substitui tudo o que se sabia).
    pub fn connected(&mut self, devices: Vec<DeviceInfo>) {
        self.devices = Some(devices.into_iter().map(|d| (d.id.clone(), d)).collect());
    }

    /// O daemon saiu: esquece os aparelhos.
    pub fn disconnected(&mut self) {
        self.devices = None;
    }

    /// Aplica um sinal do daemon. Sinais antes da lista são ignorados.
    pub fn apply(&mut self, event: ProviderEvent) {
        let Some(devices) = &mut self.devices else {
            return;
        };
        match event {
            // Um par `DeviceRemoved` + `DeviceAdded` com o mesmo id é uma atualização:
            // aqui basta substituir a entrada.
            ProviderEvent::DeviceAdded { device } => {
                devices.insert(device.id.clone(), device);
            }
            ProviderEvent::DeviceRemoved { id } => {
                devices.remove(&id);
            }
            ProviderEvent::CapabilityChanged { id, capability } => {
                if let Some(device) = devices.get_mut(&id) {
                    match device.capability_mut(capability.kind()) {
                        Some(slot) => *slot = capability,
                        None => device.capabilities.push(capability),
                    }
                }
            }
        }
    }

    /// Aparelhos com bateria, em ordem de nome (e de id, para empates).
    pub fn battery_rows(&self) -> Vec<BatteryRow> {
        let Some(devices) = &self.devices else {
            return Vec::new();
        };
        let mut rows: Vec<_> = devices
            .values()
            .filter_map(|d| match d.capability(CapabilityKind::Battery) {
                Some(Capability::Battery { percent, charging }) => Some(BatteryRow {
                    name: d.name.clone(),
                    percent: (*percent).min(100),
                    charging: *charging,
                }),
                _ => None,
            })
            .collect();
        // `BTreeMap` já ordena por id; a ordenação estável por nome mantém isso nos empates.
        rows.sort_by_key(|r| r.name.to_lowercase());
        rows
    }

    /// Monta o que o ícone mostra.
    pub fn view(&self) -> View {
        let rows = self.battery_rows();
        let lowest = lowest(&rows);
        let empty_text = if self.devices.is_some() {
            text::NO_BATTERIES
        } else {
            text::DAEMON_DOWN
        };

        let lines: Vec<String> = rows
            .iter()
            .map(|r| text::battery_line(&r.name, r.percent, r.charging))
            .collect();
        let tooltip = if lines.is_empty() {
            text::escape_markup(empty_text)
        } else {
            lines
                .iter()
                .map(|l| text::escape_markup(l))
                .collect::<Vec<_>>()
                .join("\n")
        };

        let mut menu: Vec<Entry> = if lines.is_empty() {
            vec![Entry::Info(empty_text.to_owned())]
        } else {
            lines.into_iter().map(Entry::Info).collect()
        };
        menu.extend([Entry::Separator, Entry::OpenApp, Entry::Quit]);

        View {
            visibility: if lowest.is_some() {
                Visibility::Active
            } else {
                Visibility::Passive
            },
            icon_name: match lowest {
                Some(r) => battery_icon(r.percent, r.charging),
                None => battery_icon_missing().to_owned(),
            },
            tooltip,
            menu,
        }
    }
}

/// A bateria mais baixa. No empate, a que não está carregando (é a que preocupa).
pub fn lowest(rows: &[BatteryRow]) -> Option<&BatteryRow> {
    rows.iter().min_by_key(|r| (r.percent, r.charging))
}

/// Nome do ícone simbólico de bateria: `battery-level-N0[-charging]-symbolic`.
///
/// Arredonda para baixo em dezenas, como o painel do GNOME. A 100% carregando, o tema
/// Adwaita não tem `battery-level-100-charging-symbolic`: usa `battery-level-100-charged-symbolic`.
pub fn battery_icon(percent: u8, charging: bool) -> String {
    let level = (percent.min(100) / 10) * 10;
    match (level, charging) {
        (100, true) => "battery-level-100-charged-symbolic".to_owned(),
        (level, true) => format!("battery-level-{level}-charging-symbolic"),
        (level, false) => format!("battery-level-{level}-symbolic"),
    }
}

/// Ícone usado quando não há bateria (o ícone fica passivo, mas precisa de um nome).
pub fn battery_icon_missing() -> &'static str {
    "battery-missing-symbolic"
}

#[cfg(test)]
mod tests {
    use raiza_core::{ConnectionKind, DeviceKind};

    use super::*;

    fn device(key: &str, name: &str, battery: Option<(u8, bool)>) -> DeviceInfo {
        let mut capabilities = vec![Capability::Connection {
            medium: ConnectionKind::Wireless,
            connected: true,
            writable: false,
        }];
        if let Some((percent, charging)) = battery {
            capabilities.push(Capability::Battery { percent, charging });
        }
        DeviceInfo {
            id: DeviceId::Other {
                provider: "mock".into(),
                key: key.into(),
            },
            name: name.into(),
            vendor: None,
            kind: DeviceKind::Mouse,
            providers: vec!["mock".into()],
            capabilities,
        }
    }

    fn id(key: &str) -> DeviceId {
        device(key, "", None).id
    }

    #[test]
    fn icone_por_nivel() {
        assert_eq!(battery_icon(0, false), "battery-level-0-symbolic");
        assert_eq!(battery_icon(9, false), "battery-level-0-symbolic");
        assert_eq!(battery_icon(10, false), "battery-level-10-symbolic");
        assert_eq!(battery_icon(57, true), "battery-level-50-charging-symbolic");
        assert_eq!(battery_icon(99, true), "battery-level-90-charging-symbolic");
        assert_eq!(battery_icon(100, false), "battery-level-100-symbolic");
        assert_eq!(
            battery_icon(100, true),
            "battery-level-100-charged-symbolic"
        );
        assert_eq!(battery_icon(250, false), "battery-level-100-symbolic");
    }

    #[test]
    fn mais_baixa_e_empate() {
        let row = |name: &str, percent, charging| BatteryRow {
            name: name.into(),
            percent,
            charging,
        };
        assert_eq!(lowest(&[]), None);
        let rows = [row("a", 40, false), row("b", 20, true), row("c", 60, false)];
        assert_eq!(lowest(&rows).map(|r| r.name.as_str()), Some("b"));
        let empate = [row("a", 20, true), row("b", 20, false)];
        assert_eq!(lowest(&empate).map(|r| r.name.as_str()), Some("b"));
    }

    #[test]
    fn sem_daemon_fica_passivo() {
        let view = Model::default().view();
        assert_eq!(view.visibility, Visibility::Passive);
        assert_eq!(view.icon_name, "battery-missing-symbolic");
        assert_eq!(view.tooltip, text::DAEMON_DOWN);
        assert_eq!(
            view.menu,
            [
                Entry::Info(text::DAEMON_DOWN.into()),
                Entry::Separator,
                Entry::OpenApp,
                Entry::Quit
            ]
        );
    }

    #[test]
    fn sem_bateria_fica_passivo() {
        let mut model = Model::default();
        model.connected(vec![device("teclado", "Teclado", None)]);
        let view = model.view();
        assert_eq!(view.visibility, Visibility::Passive);
        assert_eq!(view.tooltip, text::NO_BATTERIES);
        assert_eq!(view.menu[0], Entry::Info(text::NO_BATTERIES.into()));
    }

    #[test]
    fn lista_icone_e_menu() {
        let mut model = Model::default();
        model.connected(vec![
            device("s9", "S9 Ultra", Some((15, true))),
            device("teclado", "Teclado", None),
            device("vt9", "Rapoo VT9", Some((80, false))),
        ]);
        let view = model.view();
        assert_eq!(view.visibility, Visibility::Active);
        assert_eq!(view.icon_name, "battery-level-10-charging-symbolic");
        assert_eq!(view.tooltip, "Rapoo VT9: 80%\nS9 Ultra: 15%, carregando");
        assert_eq!(
            view.menu,
            [
                Entry::Info("Rapoo VT9: 80%".into()),
                Entry::Info("S9 Ultra: 15%, carregando".into()),
                Entry::Separator,
                Entry::OpenApp,
                Entry::Quit,
            ]
        );
    }

    #[test]
    fn tooltip_escapado() {
        let mut model = Model::default();
        model.connected(vec![device("x", "A&B <x>", Some((50, false)))]);
        assert_eq!(model.view().tooltip, "A&amp;B &lt;x&gt;: 50%");
        // O menu não usa markup.
        assert_eq!(model.view().menu[0], Entry::Info("A&B <x>: 50%".into()));
    }

    #[test]
    fn sinais_atualizam() {
        let mut model = Model::default();
        // Antes da lista, sinais são ignorados.
        model.apply(ProviderEvent::DeviceAdded {
            device: device("vt9", "Rapoo VT9", Some((80, false))),
        });
        assert!(model.battery_rows().is_empty());

        model.connected(vec![device("teclado", "Teclado", None)]);
        model.apply(ProviderEvent::DeviceAdded {
            device: device("vt9", "Rapoo VT9", Some((80, false))),
        });
        assert_eq!(model.view().icon_name, "battery-level-80-symbolic");

        // Bateria muda.
        model.apply(ProviderEvent::CapabilityChanged {
            id: id("vt9"),
            capability: Capability::Battery {
                percent: 30,
                charging: true,
            },
        });
        assert_eq!(model.view().icon_name, "battery-level-30-charging-symbolic");

        // Capacidade nova num aparelho que não tinha bateria.
        model.apply(ProviderEvent::CapabilityChanged {
            id: id("teclado"),
            capability: Capability::Battery {
                percent: 5,
                charging: false,
            },
        });
        assert_eq!(model.battery_rows().len(), 2);
        assert_eq!(model.view().icon_name, "battery-level-0-symbolic");

        // Sinal de um aparelho desconhecido não cria entrada.
        model.apply(ProviderEvent::CapabilityChanged {
            id: id("fantasma"),
            capability: Capability::Battery {
                percent: 1,
                charging: false,
            },
        });
        assert_eq!(model.battery_rows().len(), 2);

        // Par remove + add com o mesmo id: atualização.
        model.apply(ProviderEvent::DeviceRemoved { id: id("teclado") });
        model.apply(ProviderEvent::DeviceAdded {
            device: device("teclado", "Teclado novo", None),
        });
        assert_eq!(model.battery_rows().len(), 1);

        model.apply(ProviderEvent::DeviceRemoved { id: id("vt9") });
        assert_eq!(model.view().visibility, Visibility::Passive);

        model.disconnected();
        assert_eq!(model.view().tooltip, text::DAEMON_DOWN);
    }
}
