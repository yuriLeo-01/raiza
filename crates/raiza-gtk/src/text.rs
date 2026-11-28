//! Textos e nomes de ícones da interface, sem GTK.
//!
//! Os ícones são nomes do tema simbólico do GNOME (Adwaita). Os textos passam pelo
//! `raiza-i18n`: os msgids são em inglês e `po/pt_BR.po` traz o português.

use raiza_i18n::{fill, gettext, ngettext};

use raiza_core::dbus::{ClientError, DbusError};
use raiza_core::{
    Capability, CapabilityKind, ConnectionKind, DeviceInfo, DeviceKind, Level, RaizaError,
};

/// Título do grupo na barra lateral.
pub fn kind_group_title(kind: DeviceKind) -> &'static str {
    match kind {
        DeviceKind::Mouse => gettext("Mice"),
        DeviceKind::Keyboard => gettext("Keyboards"),
        DeviceKind::Headset => gettext("Headsets"),
        DeviceKind::Gamepad => gettext("Controllers"),
        DeviceKind::Microphone => gettext("Microphones"),
        DeviceKind::RgbController => gettext("RGB Lighting"),
        DeviceKind::Other => gettext("Others"),
    }
}

/// Nome do tipo no singular, para a página de detalhes.
pub fn kind_name(kind: DeviceKind) -> &'static str {
    match kind {
        DeviceKind::Mouse => gettext("Mouse"),
        DeviceKind::Keyboard => gettext("Keyboard"),
        DeviceKind::Headset => gettext("Headset"),
        DeviceKind::Gamepad => gettext("Controller"),
        DeviceKind::Microphone => gettext("Microphone"),
        DeviceKind::RgbController => gettext("RGB Controller"),
        DeviceKind::Other => gettext("Other Device"),
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
        CapabilityKind::Battery => gettext("Battery"),
        CapabilityKind::Dpi => gettext("DPI"),
        CapabilityKind::PollingRate => gettext("Polling Rate"),
        CapabilityKind::ScanRate => gettext("Scan Rate"),
        CapabilityKind::Rgb => gettext("Lighting"),
        CapabilityKind::Volume => gettext("Volume"),
        CapabilityKind::Mute => gettext("Mute"),
        CapabilityKind::Rumble => gettext("Rumble"),
        CapabilityKind::Connection => gettext("Connection"),
    }
}

/// Explicação curta da capacidade, para o subtítulo da linha, quando o nome não basta.
pub fn capability_hint(kind: CapabilityKind) -> Option<&'static str> {
    match kind {
        CapabilityKind::ScanRate => Some(gettext(
            "Key Scan Rate: how many times per second the device reads its keys",
        )),
        _ => None,
    }
}

/// Título do grupo com a tabela de estágios de DPI.
pub fn dpi_stages_title() -> &'static str {
    gettext("DPI Stages")
}

/// Descrição do grupo de estágios de DPI.
pub fn dpi_stages_description() -> &'static str {
    gettext("The mouse DPI button cycles through these values")
}

/// Título da linha de um estágio de DPI (`index` a partir de 0).
pub fn dpi_stage_title(index: usize) -> String {
    fill(gettext("Stage {number}"), &[("number", &(index + 1))])
}

/// Marca do estágio de DPI em uso.
pub fn dpi_stage_in_use() -> &'static str {
    gettext("In use")
}

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
    let current = format!("{}{unit}", level.current);
    let template = if level.step > 1 {
        // Translators: {current} já traz a unidade ("1600 DPI", "50%").
        gettext("{current} (from {min} to {max}, step {step})")
    } else {
        gettext("{current} (from {min} to {max})")
    };
    fill(
        template,
        &[
            ("current", &current),
            ("min", &level.min),
            ("max", &level.max),
            ("step", &level.step),
        ],
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
                fill(gettext("{percent}%, charging"), &[("percent", percent)])
            } else {
                format!("{percent}%")
            }
        }
        Capability::Dpi { level, stages, .. } => {
            let mut text = level_text(level, " DPI");
            if !stages.is_empty() {
                text.push_str(" · ");
                text.push_str(&fill(
                    gettext("stages: {list}"),
                    &[("list", &join(stages.iter().map(u32::to_string)))],
                ));
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
                fill(
                    gettext("{current} Hz (accepts {list})"),
                    &[
                        ("current", current),
                        ("list", &join(supported.iter().map(|hz| format!("{hz} Hz")))),
                    ],
                )
            }
        }
        Capability::Rgb {
            zones,
            current_mode,
            ..
        } => {
            let mut text = fill(gettext("Mode {mode}"), &[("mode", current_mode)]);
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
                gettext("On").into()
            } else {
                gettext("Off").into()
            }
        }
        Capability::Rumble {
            max_duration_ms, ..
        } => fill(
            gettext("Pulses of up to {duration} ms"),
            &[("duration", max_duration_ms)],
        ),
        Capability::Connection {
            medium, connected, ..
        } => {
            let medium = match medium {
                ConnectionKind::Wired => gettext("USB cable"),
                ConnectionKind::Wireless => gettext("Wireless (dongle)"),
                ConnectionKind::Bluetooth => gettext("Bluetooth"),
                ConnectionKind::Other => gettext("Other medium"),
            };
            let template = if *connected {
                // Translators: {medium} é "USB cable", "Bluetooth"…
                gettext("{medium}, connected")
            } else {
                gettext("{medium}, disconnected")
            };
            fill(template, &[("medium", &medium)])
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
        title: gettext("The Raiza service is not running").into(),
        description: gettext(
            "Start the daemon with <tt>systemctl --user start raizad</tt>. \
             The window connects by itself when it shows up.",
        )
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
            title: gettext("No session bus").into(),
            description: gettext(
                "Could not talk to the session D-Bus. Is there an active user session?",
            )
            .into(),
        },
        ClientError::Decode(msg) => Problem {
            title: gettext("Incompatible versions").into(),
            description: fill(
                gettext("The daemon reply could not be read. Update Raiza. ({detail})"),
                &[("detail", &escape_markup(msg))],
            ),
        },
        other => Problem {
            title: gettext("Error talking to the daemon").into(),
            description: escape_markup(&other.to_string()),
        },
    }
}

/// Estado mostrado quando o daemon fala outra versão da API.
pub fn version_mismatch(daemon_api: u32, daemon_version: &str) -> Problem {
    Problem {
        title: gettext("Incompatible versions").into(),
        description: fill(
            gettext(
                "Daemon {version} uses API {daemon_api}, but this interface uses API \
                 {api}. Update Raiza.",
            ),
            &[
                ("version", &escape_markup(daemon_version)),
                ("daemon_api", &daemon_api),
                ("api", &raiza_core::dbus::API_VERSION),
            ],
        ),
    }
}

/// Dica das capacidades que não aceitam escrita (pilar 1).
pub fn read_only_tooltip() -> &'static str {
    gettext("read-only: unverified profile or the device does not accept changes")
}

/// Toast de uma escrita que não saiu porque não há conexão com o daemon.
pub fn write_without_connection(kind: CapabilityKind) -> String {
    fill(
        gettext("Could not change {capability}: no connection to the Raiza service"),
        &[("capability", &capability_title(kind))],
    )
}

/// Toast de uma escrita recusada ou que falhou. Texto puro (escape antes de usar como markup).
pub fn write_error(kind: CapabilityKind, err: &ClientError) -> String {
    let title = capability_title(kind);
    let reason = match err {
        ClientError::Raiza(RaizaError::OutOfRange { detail, .. }) => fill(
            gettext("value out of range ({detail})"),
            &[("detail", detail)],
        ),
        ClientError::Raiza(RaizaError::ReadOnly(_)) => {
            gettext("the capability is read-only").into()
        }
        ClientError::Raiza(RaizaError::Unsupported(_)) => {
            gettext("the device does not have this capability").into()
        }
        ClientError::Raiza(RaizaError::NotFound(_)) => {
            gettext("the device is no longer connected").into()
        }
        ClientError::Raiza(RaizaError::Io(msg)) => fill(
            gettext("failed to talk to the device ({detail})"),
            &[("detail", msg)],
        ),
        ClientError::Raiza(RaizaError::Permission(msg)) => fill(
            gettext("no permission to access the device ({detail})"),
            &[("detail", msg)],
        ),
        ClientError::Bus(_) => gettext("the Raiza service did not respond").into(),
        ClientError::Decode(msg) => fill(
            gettext("invalid reply from the daemon ({detail})"),
            &[("detail", msg)],
        ),
    };
    fill(
        gettext("Could not change {capability}: {reason}"),
        &[("capability", &title), ("reason", &reason)],
    )
}

/// Página de carregamento, enquanto a janela procura o daemon.
pub fn connecting() -> &'static str {
    gettext("Connecting to the Raiza service…")
}

/// Botão da página de problema.
pub fn retry() -> &'static str {
    gettext("Try Again")
}

/// Título da página sem dispositivos.
pub fn no_devices_title() -> &'static str {
    gettext("No Devices")
}

/// Descrição da página sem dispositivos.
pub fn no_devices_description() -> &'static str {
    gettext("Connect a supported peripheral. It shows up here by itself.")
}

/// Dica do botão que mostra ou esconde os aparelhos sem controles.
pub fn show_all_tooltip(active: bool, hidden: usize) -> String {
    if active {
        gettext("Hide devices without controls").into()
    } else if hidden > 0 {
        fill(
            ngettext(
                "Show all devices ({count} without controls hidden)",
                "Show all devices ({count} without controls hidden)",
                hidden as u64,
            ),
            &[("count", &hidden)],
        )
    } else {
        gettext("Show all devices").into()
    }
}

/// Título da página de detalhes na navegação.
pub fn device_page_title() -> &'static str {
    gettext("Device")
}

/// Linha com os provedores do dispositivo.
pub fn providers_title() -> &'static str {
    gettext("Providers")
}

/// Linha com o id do dispositivo.
pub fn identifier_title() -> &'static str {
    gettext("Identifier")
}

/// Grupo das capacidades na página de detalhes.
pub fn capabilities_title() -> &'static str {
    gettext("Capabilities")
}

/// Botão que manda um pulso de vibração.
pub fn test_rumble() -> &'static str {
    gettext("Test")
}

/// Subtítulo da linha de nível (DPI, volume): os limites.
pub fn level_limits(level: &Level) -> String {
    let template = if level.step > 1 {
        gettext("From {min} to {max}, step {step}")
    } else {
        gettext("From {min} to {max}")
    };
    fill(
        template,
        &[
            ("min", &level.min),
            ("max", &level.max),
            ("step", &level.step),
        ],
    )
}

/// Título do seletor de cor de uma zona RGB.
pub fn zone_color_title(zone: &str) -> String {
    fill(gettext("Zone {zone} color"), &[("zone", &zone)])
}

/// Linha do modo de iluminação.
pub fn rgb_mode_title() -> &'static str {
    gettext("Mode")
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
                "15%, charging",
            ),
            (
                Capability::Dpi {
                    level: level(1600, 400, 16000, 50),
                    stages: vec![800, 1600, 3200],
                    active_stage: None,
                    writable: true,
                },
                "1600 DPI (from 400 to 16000, step 50) · stages: 800, 1600, 3200",
            ),
            (
                Capability::Dpi {
                    level: level(800, 100, 8000, 1),
                    stages: vec![],
                    active_stage: None,
                    writable: false,
                },
                "800 DPI (from 100 to 8000)",
            ),
            (
                Capability::PollingRate {
                    current: 1000,
                    supported: vec![125, 500, 1000],
                    writable: true,
                },
                "1000 Hz (accepts 125 Hz, 500 Hz, 1000 Hz)",
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
                "Mode static · logo #ff0000, roda #0000ff",
            ),
            (
                Capability::Volume {
                    level: level(50, 0, 150, 1),
                    writable: true,
                },
                "50% (from 0 to 150)",
            ),
            (
                Capability::Mute {
                    muted: true,
                    writable: true,
                },
                "On",
            ),
            (
                Capability::Mute {
                    muted: false,
                    writable: true,
                },
                "Off",
            ),
            (
                Capability::Rumble {
                    max_duration_ms: 2000,
                    writable: true,
                },
                "Pulses of up to 2000 ms",
            ),
            (
                Capability::Connection {
                    medium: ConnectionKind::Bluetooth,
                    connected: true,
                    writable: true,
                },
                "Bluetooth, connected",
            ),
            (
                Capability::Connection {
                    medium: ConnectionKind::Wireless,
                    connected: false,
                    writable: false,
                },
                "Wireless (dongle), disconnected",
            ),
        ];
        for (cap, esperado) in casos {
            assert_eq!(capability_summary(&cap), esperado, "{:?}", cap.kind());
        }
    }

    #[test]
    fn textos_dos_estagios_e_da_varredura() {
        assert_eq!(dpi_stage_title(0), "Stage 1");
        assert_eq!(dpi_stage_title(4), "Stage 5");
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
        assert_eq!(p.title, "Incompatible versions");
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
            "Could not change DPI: value out of range \
             (1625 não é múltiplo do passo 50 a partir de 400)"
        );
        let err = ClientError::Raiza(RaizaError::Permission("hidraw".into()));
        assert!(write_error(CapabilityKind::Rgb, &err).contains("Lighting: no permission"));
        let err = ClientError::Raiza(RaizaError::ReadOnly(CapabilityKind::Mute));
        assert!(write_error(CapabilityKind::Mute, &err).ends_with("read-only"));
        let err = ClientError::Raiza(RaizaError::Io("cano quebrado".into()));
        assert!(write_error(CapabilityKind::Volume, &err).contains("cano quebrado"));
        assert!(write_without_connection(CapabilityKind::Volume).contains("Volume"));
    }

    #[test]
    fn textos_soltos_da_janela() {
        assert_eq!(show_all_tooltip(true, 3), "Hide devices without controls");
        assert_eq!(
            show_all_tooltip(false, 1),
            "Show all devices (1 without controls hidden)"
        );
        assert_eq!(show_all_tooltip(false, 0), "Show all devices");
        let mut l = level(800, 100, 8000, 1);
        assert_eq!(level_limits(&l), "From 100 to 8000");
        l.step = 50;
        assert_eq!(level_limits(&l), "From 100 to 8000, step 50");
        assert_eq!(zone_color_title("logo"), "Zone logo color");
        assert!(daemon_down().description.contains("<tt>systemctl"));
    }

    #[test]
    fn escapa_markup() {
        assert_eq!(
            escape_markup("a & <b> \"c\" 'd'"),
            "a &amp; &lt;b&gt; &quot;c&quot; &#39;d&#39;"
        );
    }
}
