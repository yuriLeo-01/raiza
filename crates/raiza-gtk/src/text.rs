//! Textos e nomes de ícones da interface, sem GTK.
//!
//! Os ícones são nomes do tema simbólico do GNOME (Adwaita).

use raiza_core::dbus::{ClientError, DbusError};
use raiza_core::{
    Capability, CapabilityKind, ConnectionKind, DeviceInfo, DeviceKind, Level, RaizaError,
};

/// Título do grupo na barra lateral.
pub fn kind_group_title(kind: DeviceKind) -> &'static str {
    match kind {
        DeviceKind::Mouse => "Mouses",
        DeviceKind::Keyboard => "Teclados",
        DeviceKind::Headset => "Headsets",
        DeviceKind::Gamepad => "Controles",
        DeviceKind::Microphone => "Microfones",
        DeviceKind::RgbController => "Iluminação RGB",
        DeviceKind::Other => "Outros",
    }
}

/// Nome do tipo no singular, para a página de detalhes.
pub fn kind_name(kind: DeviceKind) -> &'static str {
    match kind {
        DeviceKind::Mouse => "Mouse",
        DeviceKind::Keyboard => "Teclado",
        DeviceKind::Headset => "Headset",
        DeviceKind::Gamepad => "Controle",
        DeviceKind::Microphone => "Microfone",
        DeviceKind::RgbController => "Controlador RGB",
        DeviceKind::Other => "Outro dispositivo",
    }
}

/// Ícone do tipo de dispositivo.
pub fn kind_icon(kind: DeviceKind) -> &'static str {
    match kind {
        DeviceKind::Mouse => "input-mouse-symbolic",
        DeviceKind::Keyboard => "input-keyboard-symbolic",
        DeviceKind::Headset => "audio-headset-symbolic",
        DeviceKind::Gamepad => "input-gaming-symbolic",
        DeviceKind::Microphone => "audio-input-microphone-symbolic",
        DeviceKind::RgbController => "color-select-symbolic",
        DeviceKind::Other => "drive-removable-media-symbolic",
    }
}

/// Nome da capacidade, usado como título da linha.
pub fn capability_title(kind: CapabilityKind) -> &'static str {
    match kind {
        CapabilityKind::Battery => "Bateria",
        CapabilityKind::Dpi => "DPI",
        CapabilityKind::PollingRate => "Taxa de atualização",
        CapabilityKind::ScanRate => "Taxa de varredura",
        CapabilityKind::Rgb => "Iluminação",
        CapabilityKind::Volume => "Volume",
        CapabilityKind::Mute => "Mudo",
        CapabilityKind::Rumble => "Vibração",
        CapabilityKind::Connection => "Conexão",
    }
}

/// Explicação curta da capacidade, para o subtítulo da linha, quando o nome não basta.
pub fn capability_hint(kind: CapabilityKind) -> Option<&'static str> {
    match kind {
        CapabilityKind::ScanRate => {
            Some("Key Scan Rate: quantas vezes por segundo o aparelho lê os botões")
        }
        _ => None,
    }
}

/// Título do grupo com a tabela de estágios de DPI.
pub const DPI_STAGES_TITLE: &str = "Estágios de DPI";

/// Descrição do grupo de estágios de DPI.
pub const DPI_STAGES_DESCRIPTION: &str = "O botão de DPI do mouse alterna entre estes valores";

/// Título da linha de um estágio de DPI (`index` a partir de 0).
pub fn dpi_stage_title(index: usize) -> String {
    format!("Estágio {}", index + 1)
}

/// Marca do estágio de DPI em uso.
pub const DPI_STAGE_IN_USE: &str = "Em uso";

/// Ícone da capacidade. A bateria usa [`battery_icon`], que depende do nível.
pub fn capability_icon(capability: &Capability) -> String {
    let name = match capability {
        Capability::Battery { percent, charging } => return battery_icon(*percent, *charging),
        Capability::Dpi { .. } => "input-mouse-symbolic",
        Capability::PollingRate { .. } | Capability::ScanRate { .. } => {
            "power-profile-performance-symbolic"
        }
        Capability::Rgb { .. } => "color-select-symbolic",
        Capability::Volume { .. } => "audio-volume-high-symbolic",
        Capability::Mute { muted: true, .. } => "audio-volume-muted-symbolic",
        Capability::Mute { muted: false, .. } => "audio-volume-high-symbolic",
        Capability::Rumble { .. } => "input-gaming-symbolic",
        Capability::Connection { medium, .. } => match medium {
            ConnectionKind::Bluetooth => "bluetooth-symbolic",
            ConnectionKind::Wireless => "network-wireless-symbolic",
            ConnectionKind::Wired | ConnectionKind::Other => "network-wired-symbolic",
        },
    };
    name.to_owned()
}

/// Ícone de bateria do tema, em degraus de 10% (`battery-level-80-charging-symbolic`).
pub fn battery_icon(percent: u8, charging: bool) -> String {
    let level = (u32::from(percent.min(100)) + 5) / 10 * 10;
    let suffix = if charging && level == 100 {
        "-charged"
    } else if charging {
        "-charging"
    } else {
        ""
    };
    format!("battery-level-{level}{suffix}-symbolic")
}

/// Bateria curta, para a barra lateral (`80%`).
pub fn battery_short(percent: u8) -> String {
    format!("{percent}%")
}

fn level_text(level: &Level, unit: &str) -> String {
    let step = if level.step > 1 {
        format!(", passo {}", level.step)
    } else {
        String::new()
    };
    format!(
        "{}{unit} (de {} a {}{step})",
        level.current, level.min, level.max
    )
}

fn join(items: impl IntoIterator<Item = String>) -> String {
    items.into_iter().collect::<Vec<_>>().join(", ")
}

/// Descrição do estado atual da capacidade, usada como subtítulo da linha.
pub fn capability_summary(capability: &Capability) -> String {
    match capability {
        Capability::Battery { percent, charging } => {
            if *charging {
                format!("{percent}%, carregando")
            } else {
                format!("{percent}%")
            }
        }
        Capability::Dpi { level, stages, .. } => {
            let mut text = level_text(level, " DPI");
            if !stages.is_empty() {
                text.push_str(" · estágios: ");
                text.push_str(&join(stages.iter().map(u32::to_string)));
            }
            text
        }
        Capability::PollingRate {
            current, supported, ..
        }
        | Capability::ScanRate {
            current, supported, ..
        } => {
            if supported.is_empty() {
                format!("{current} Hz")
            } else {
                format!(
                    "{current} Hz (aceita {})",
                    join(supported.iter().map(|hz| format!("{hz} Hz")))
                )
            }
        }
        Capability::Rgb {
            zones,
            current_mode,
            ..
        } => {
            let mut text = format!("Modo {current_mode}");
            if !zones.is_empty() {
                text.push_str(" · ");
                text.push_str(&join(
                    zones.iter().map(|z| format!("{} {}", z.name, z.color)),
                ));
            }
            text
        }
        Capability::Volume { level, .. } => level_text(level, "%"),
        Capability::Mute { muted, .. } => {
            if *muted {
                "Ativado".into()
            } else {
                "Desativado".into()
            }
        }
        Capability::Rumble {
            max_duration_ms, ..
        } => format!("Pulsos de até {max_duration_ms} ms"),
        Capability::Connection {
            medium, connected, ..
        } => {
            let medium = match medium {
                ConnectionKind::Wired => "Cabo USB",
                ConnectionKind::Wireless => "Sem fio (dongle)",
                ConnectionKind::Bluetooth => "Bluetooth",
                ConnectionKind::Other => "Outro meio",
            };
            let state = if *connected {
                "conectado"
            } else {
                "desconectado"
            };
            format!("{medium}, {state}")
        }
    }
}

/// O que a barra lateral mostra de um dispositivo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidebarEntry {
    /// Nome do dispositivo.
    pub title: String,
    /// Fabricante, se houver.
    pub subtitle: Option<String>,
    /// Ícone do tipo.
    pub icon: &'static str,
    /// Bateria: texto curto e ícone.
    pub battery: Option<(String, String)>,
}

impl SidebarEntry {
    /// Monta a entrada a partir do dispositivo.
    pub fn new(device: &DeviceInfo) -> Self {
        let battery = match device.capability(CapabilityKind::Battery) {
            Some(Capability::Battery { percent, charging }) => {
                Some((battery_short(*percent), battery_icon(*percent, *charging)))
            }
            _ => None,
        };
        SidebarEntry {
            title: device.name.clone(),
            subtitle: device.vendor.clone().filter(|v| !v.is_empty()),
            icon: kind_icon(device.kind),
            battery,
        }
    }
}

/// Subtítulo da página de detalhes: tipo e fabricante (`Mouse · Rapoo`).
pub fn device_subtitle(device: &DeviceInfo) -> String {
    match device.vendor.as_deref().filter(|v| !v.is_empty()) {
        Some(vendor) => format!("{} · {vendor}", kind_name(device.kind)),
        None => kind_name(device.kind).to_owned(),
    }
}

/// Mensagens quando não é possível falar com o daemon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// Título do estado vazio.
    pub title: String,
    /// Explicação e o que fazer.
    pub description: String,
}

/// Nomes de erro D-Bus que significam "ninguém atende o serviço do `raizad`".
pub fn is_daemon_down_name(name: &str) -> bool {
    matches!(
        name,
        "org.freedesktop.DBus.Error.ServiceUnknown" | "org.freedesktop.DBus.Error.NameHasNoOwner"
    ) || name.starts_with("org.freedesktop.DBus.Error.Spawn.")
}

/// Estado mostrado quando o daemon não está no barramento.
pub fn daemon_down() -> Problem {
    Problem {
        title: "O serviço do Raiza não está rodando".into(),
        description: "Inicie o daemon com <tt>systemctl --user start raizad</tt>. \
                      A janela se conecta sozinha quando ele aparecer."
            .into(),
    }
}

/// Explica uma falha de conexão ou de sincronização com o daemon.
pub fn describe_connect_error(err: &ClientError) -> Problem {
    match err {
        ClientError::Bus(DbusError::ZBus(zbus::Error::MethodError(name, _, _)))
            if is_daemon_down_name(name.as_str()) =>
        {
            daemon_down()
        }
        ClientError::Bus(DbusError::ZBus(
            zbus::Error::InputOutput(_) | zbus::Error::Address(_),
        )) => Problem {
            title: "Sem barramento de sessão".into(),
            description: "Não foi possível falar com o D-Bus da sessão. \
                              Há uma sessão de usuário ativa?"
                .into(),
        },
        ClientError::Decode(msg) => Problem {
            title: "Versões incompatíveis".into(),
            description: format!(
                "A resposta do daemon não pôde ser lida. Atualize o Raiza. ({})",
                escape_markup(msg)
            ),
        },
        other => Problem {
            title: "Erro ao falar com o daemon".into(),
            description: escape_markup(&other.to_string()),
        },
    }
}

/// Estado mostrado quando o daemon fala outra versão da API.
pub fn version_mismatch(daemon_api: u32, daemon_version: &str) -> Problem {
    Problem {
        title: "Versões incompatíveis".into(),
        description: format!(
            "O daemon {} usa a API {daemon_api}, mas esta interface usa a API {}. \
             Atualize o Raiza.",
            escape_markup(daemon_version),
            raiza_core::dbus::API_VERSION
        ),
    }
}

/// Dica das capacidades que não aceitam escrita (pilar 1).
pub const READ_ONLY_TOOLTIP: &str =
    "somente leitura: perfil não verificado ou o aparelho não aceita mudança";

/// Toast de uma escrita que não saiu porque não há conexão com o daemon.
pub fn write_without_connection(kind: CapabilityKind) -> String {
    format!(
        "Não deu para mudar {}: sem conexão com o serviço do Raiza",
        capability_title(kind)
    )
}

/// Toast de uma escrita recusada ou que falhou. Texto puro (escape antes de usar como markup).
pub fn write_error(kind: CapabilityKind, err: &ClientError) -> String {
    let title = capability_title(kind);
    let reason = match err {
        ClientError::Raiza(RaizaError::OutOfRange { detail, .. }) => {
            format!("valor fora dos limites ({detail})")
        }
        ClientError::Raiza(RaizaError::ReadOnly(_)) => "a capacidade é somente leitura".into(),
        ClientError::Raiza(RaizaError::Unsupported(_)) => {
            "o aparelho não tem essa capacidade".into()
        }
        ClientError::Raiza(RaizaError::NotFound(_)) => "o aparelho não está mais conectado".into(),
        ClientError::Raiza(RaizaError::Io(msg)) => format!("falha ao falar com o aparelho ({msg})"),
        ClientError::Raiza(RaizaError::Permission(msg)) => {
            format!("sem permissão para acessar o aparelho ({msg})")
        }
        ClientError::Bus(_) => "o serviço do Raiza não respondeu".into(),
        ClientError::Decode(msg) => format!("resposta inválida do daemon ({msg})"),
    };
    format!("Não deu para mudar {title}: {reason}")
}

/// Escapa texto livre para entrar em markup do Pango.
pub fn escape_markup(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use raiza_core::{Color, DeviceId, RgbZone};

    use super::*;

    fn level(current: u32, min: u32, max: u32, step: u32) -> Level {
        Level {
            current,
            min,
            max,
            step,
        }
    }

    #[test]
    fn texto_de_cada_capacidade() {
        let casos = [
            (
                Capability::Battery {
                    percent: 80,
                    charging: false,
                },
                "80%",
            ),
            (
                Capability::Battery {
                    percent: 15,
                    charging: true,
                },
                "15%, carregando",
            ),
            (
                Capability::Dpi {
                    level: level(1600, 400, 16000, 50),
                    stages: vec![800, 1600, 3200],
                    active_stage: None,
                    writable: true,
                },
                "1600 DPI (de 400 a 16000, passo 50) · estágios: 800, 1600, 3200",
            ),
            (
                Capability::Dpi {
                    level: level(800, 100, 8000, 1),
                    stages: vec![],
                    active_stage: None,
                    writable: false,
                },
                "800 DPI (de 100 a 8000)",
            ),
            (
                Capability::PollingRate {
                    current: 1000,
                    supported: vec![125, 500, 1000],
                    writable: true,
                },
                "1000 Hz (aceita 125 Hz, 500 Hz, 1000 Hz)",
            ),
            (
                Capability::PollingRate {
                    current: 500,
                    supported: vec![],
                    writable: false,
                },
                "500 Hz",
            ),
            (
                Capability::Rgb {
                    zones: vec![
                        RgbZone {
                            name: "logo".into(),
                            color: Color { r: 255, g: 0, b: 0 },
                        },
                        RgbZone {
                            name: "roda".into(),
                            color: Color { r: 0, g: 0, b: 255 },
                        },
                    ],
                    modes: vec!["static".into()],
                    current_mode: "static".into(),
                    writable: true,
                },
                "Modo static · logo #ff0000, roda #0000ff",
            ),
            (
                Capability::Volume {
                    level: level(50, 0, 150, 1),
                    writable: true,
                },
                "50% (de 0 a 150)",
            ),
            (
                Capability::Mute {
                    muted: true,
                    writable: true,
                },
                "Ativado",
            ),
            (
                Capability::Mute {
                    muted: false,
                    writable: true,
                },
                "Desativado",
            ),
            (
                Capability::Rumble {
                    max_duration_ms: 2000,
                    writable: true,
                },
                "Pulsos de até 2000 ms",
            ),
            (
                Capability::Connection {
                    medium: ConnectionKind::Bluetooth,
                    connected: true,
                    writable: true,
                },
                "Bluetooth, conectado",
            ),
            (
                Capability::Connection {
                    medium: ConnectionKind::Wireless,
                    connected: false,
                    writable: false,
                },
                "Sem fio (dongle), desconectado",
            ),
        ];
        for (cap, esperado) in casos {
            assert_eq!(capability_summary(&cap), esperado, "{:?}", cap.kind());
        }
    }

    #[test]
    fn textos_dos_estagios_e_da_varredura() {
        assert_eq!(dpi_stage_title(0), "Estágio 1");
        assert_eq!(dpi_stage_title(4), "Estágio 5");
        assert!(capability_hint(CapabilityKind::ScanRate).is_some());
        assert_eq!(capability_hint(CapabilityKind::PollingRate), None);
    }

    #[test]
    fn todo_tipo_tem_titulo() {
        for kind in CapabilityKind::ALL {
            assert!(!capability_title(kind).is_empty());
        }
    }

    #[test]
    fn icone_da_bateria() {
        assert_eq!(battery_icon(0, false), "battery-level-0-symbolic");
        assert_eq!(battery_icon(4, false), "battery-level-0-symbolic");
        assert_eq!(battery_icon(76, false), "battery-level-80-symbolic");
        assert_eq!(battery_icon(42, true), "battery-level-40-charging-symbolic");
        assert_eq!(
            battery_icon(100, true),
            "battery-level-100-charged-symbolic"
        );
        assert_eq!(battery_icon(250, false), "battery-level-100-symbolic");
    }

    #[test]
    fn entrada_da_barra_lateral() {
        let mut d = DeviceInfo {
            id: DeviceId::usb(0x24ae, 0x1423, "0"),
            name: "Rapoo VT9".into(),
            vendor: Some("Rapoo".into()),
            kind: DeviceKind::Mouse,
            providers: vec!["mock".into()],
            capabilities: vec![Capability::Battery {
                percent: 80,
                charging: true,
            }],
        };
        assert_eq!(
            SidebarEntry::new(&d),
            SidebarEntry {
                title: "Rapoo VT9".into(),
                subtitle: Some("Rapoo".into()),
                icon: "input-mouse-symbolic",
                battery: Some(("80%".into(), "battery-level-80-charging-symbolic".into())),
            }
        );
        assert_eq!(device_subtitle(&d), "Mouse · Rapoo");

        d.vendor = Some(String::new());
        d.capabilities.clear();
        d.kind = DeviceKind::Headset;
        let e = SidebarEntry::new(&d);
        assert_eq!(e.subtitle, None);
        assert_eq!(e.battery, None);
        assert_eq!(e.icon, "audio-headset-symbolic");
        assert_eq!(device_subtitle(&d), "Headset");
    }

    #[test]
    fn erros_de_conexao() {
        assert!(is_daemon_down_name(
            "org.freedesktop.DBus.Error.ServiceUnknown"
        ));
        assert!(is_daemon_down_name(
            "org.freedesktop.DBus.Error.Spawn.ChildExited"
        ));
        assert!(!is_daemon_down_name(
            "org.freedesktop.DBus.Error.UnknownMethod"
        ));

        let p = describe_connect_error(&ClientError::Decode("campo <x>".into()));
        assert_eq!(p.title, "Versões incompatíveis");
        assert!(p.description.contains("campo &lt;x&gt;"));

        let p = describe_connect_error(&ClientError::Raiza(RaizaError::Io("falhou".into())));
        assert!(p.description.contains("falhou"));

        let p = version_mismatch(2, "0.9");
        assert!(p.description.contains("API 2") && p.description.contains("API 1"));
    }

    #[test]
    fn erros_de_escrita() {
        let err = ClientError::Raiza(RaizaError::OutOfRange {
            kind: CapabilityKind::Dpi,
            detail: "1625 não é múltiplo do passo 50 a partir de 400".into(),
        });
        assert_eq!(
            write_error(CapabilityKind::Dpi, &err),
            "Não deu para mudar DPI: valor fora dos limites \
             (1625 não é múltiplo do passo 50 a partir de 400)"
        );
        let err = ClientError::Raiza(RaizaError::Permission("hidraw".into()));
        assert!(write_error(CapabilityKind::Rgb, &err).contains("Iluminação: sem permissão"));
        let err = ClientError::Raiza(RaizaError::ReadOnly(CapabilityKind::Mute));
        assert!(write_error(CapabilityKind::Mute, &err).ends_with("somente leitura"));
        let err = ClientError::Raiza(RaizaError::Io("cano quebrado".into()));
        assert!(write_error(CapabilityKind::Volume, &err).contains("cano quebrado"));
        assert!(write_without_connection(CapabilityKind::Volume).contains("Volume"));
    }

    #[test]
    fn escapa_markup() {
        assert_eq!(
            escape_markup("a & <b> \"c\" 'd'"),
            "a &amp; &lt;b&gt; &quot;c&quot; &#39;d&#39;"
        );
    }
}
