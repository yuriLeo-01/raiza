//! Formatação do que o `raiza-ctl` imprime. Funções puras: devolvem texto, não escrevem.

use raiza_core::{
    Capability, CapabilityKind, ConnectionKind, DeviceInfo, DeviceKind, ProviderEvent,
};
use serde_json::json;

/// Tipo do dispositivo em português.
pub fn kind_label(kind: DeviceKind) -> &'static str {
    match kind {
        DeviceKind::Mouse => "mouse",
        DeviceKind::Keyboard => "teclado",
        DeviceKind::Headset => "headset",
        DeviceKind::Gamepad => "controle",
        DeviceKind::Microphone => "microfone",
        DeviceKind::RgbController => "controlador RGB",
        DeviceKind::Other => "outro",
    }
}

/// Nome da capacidade para leitura humana.
pub fn capability_label(kind: CapabilityKind) -> &'static str {
    match kind {
        CapabilityKind::Battery => "bateria",
        CapabilityKind::Dpi => "DPI",
        CapabilityKind::PollingRate => "polling",
        CapabilityKind::Rgb => "RGB",
        CapabilityKind::Volume => "volume",
        CapabilityKind::Mute => "mudo",
        CapabilityKind::Rumble => "vibração",
        CapabilityKind::Connection => "conexão",
    }
}

fn medium_label(medium: ConnectionKind) -> &'static str {
    match medium {
        ConnectionKind::Wired => "cabo",
        ConnectionKind::Wireless => "sem fio",
        ConnectionKind::Bluetooth => "bluetooth",
        ConnectionKind::Other => "outro meio",
    }
}

fn yes_no(b: bool) -> &'static str {
    if b { "sim" } else { "não" }
}

fn join<T: ToString>(items: &[T]) -> String {
    items
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Só o valor atual, sem o nome: `80%`, `1600`, `1000 Hz`...
pub fn capability_value(cap: &Capability) -> String {
    match cap {
        Capability::Battery { percent, charging } => {
            if *charging {
                format!("{percent}% (carregando)")
            } else {
                format!("{percent}%")
            }
        }
        Capability::Dpi { level, .. } => level.current.to_string(),
        Capability::PollingRate { current, .. } => format!("{current} Hz"),
        Capability::Rgb {
            zones,
            current_mode,
            ..
        } => {
            let cores = zones
                .iter()
                .map(|z| z.color.to_string())
                .collect::<Vec<_>>()
                .join(",");
            if cores.is_empty() {
                current_mode.clone()
            } else {
                format!("{current_mode} {cores}")
            }
        }
        Capability::Volume { level, .. } => format!("{}%", level.current),
        Capability::Mute { muted, .. } => yes_no(*muted).to_owned(),
        Capability::Rumble {
            max_duration_ms, ..
        } => format!("até {max_duration_ms} ms"),
        Capability::Connection {
            medium, connected, ..
        } => format!(
            "{}, {}",
            medium_label(*medium),
            if *connected {
                "conectado"
            } else {
                "desconectado"
            }
        ),
    }
}

/// Resumo curto para a tabela: `bateria 80%`, `DPI 1600`.
pub fn capability_summary(cap: &Capability) -> String {
    format!("{} {}", capability_label(cap.kind()), capability_value(cap))
}

/// Tabela de `list`. Sem dispositivos, uma frase.
pub fn format_list(devices: &[DeviceInfo]) -> String {
    if devices.is_empty() {
        return "nenhum dispositivo encontrado\n".to_owned();
    }
    let mut rows: Vec<[String; 5]> =
        vec![["ID", "TIPO", "NOME", "PROVEDORES", "CAPACIDADES"].map(str::to_owned)];
    for d in devices {
        rows.push([
            d.id.to_string(),
            kind_label(d.kind).to_owned(),
            d.name.clone(),
            join(&d.providers),
            d.capabilities
                .iter()
                .map(capability_summary)
                .collect::<Vec<_>>()
                .join(", "),
        ]);
    }
    // A última coluna não precisa de preenchimento (largura 0).
    let widths: [usize; 5] = std::array::from_fn(|i| {
        if i == 4 {
            0
        } else {
            rows.iter().map(|r| r[i].chars().count()).max().unwrap_or(0)
        }
    });
    let mut out = String::new();
    for row in &rows {
        let mut line = String::new();
        for (cell, width) in row.iter().zip(widths) {
            line.push_str(cell);
            line.push_str(&" ".repeat(width.saturating_sub(cell.chars().count()) + 2));
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

/// Limites e opções de uma capacidade, quando há.
fn capability_details(cap: &Capability) -> Vec<String> {
    match cap {
        Capability::Dpi { level, stages, .. } => {
            let mut v = vec![format!(
                "limites: {} a {}, passo {}",
                level.min, level.max, level.step
            )];
            if !stages.is_empty() {
                v.push(format!("estágios: {}", join(stages)));
            }
            v
        }
        Capability::Volume { level, .. } => vec![format!(
            "limites: {} a {}, passo {}",
            level.min, level.max, level.step
        )],
        Capability::PollingRate { supported, .. } => {
            vec![format!("aceitas: {} Hz", join(supported))]
        }
        Capability::Rgb { zones, modes, .. } => {
            let zonas = zones
                .iter()
                .enumerate()
                .map(|(i, z)| format!("{i} {} {}", z.name, z.color))
                .collect::<Vec<_>>()
                .join("; ");
            vec![format!("zonas: {zonas}"), format!("modos: {}", join(modes))]
        }
        Capability::Rumble {
            max_duration_ms, ..
        } => {
            vec![format!("duração máxima: {max_duration_ms} ms")]
        }
        Capability::Battery { .. } | Capability::Mute { .. } | Capability::Connection { .. } => {
            Vec::new()
        }
    }
}

/// Texto de `get`: cabeçalho do dispositivo e todas as capacidades.
pub fn format_device(d: &DeviceInfo) -> String {
    let mut out = format!("{} ({})\n", d.name, d.id);
    out.push_str(&format!("  tipo: {}\n", kind_label(d.kind)));
    if let Some(vendor) = &d.vendor {
        out.push_str(&format!("  fabricante: {vendor}\n"));
    }
    out.push_str(&format!("  provedores: {}\n", join(&d.providers)));
    if d.capabilities.is_empty() {
        out.push_str("  capacidades: nenhuma\n");
        return out;
    }
    out.push_str("  capacidades:\n");
    for cap in &d.capabilities {
        out.push_str(&format!(
            "    {}: {}\n",
            capability_label(cap.kind()),
            capability_value(cap)
        ));
        for detail in capability_details(cap) {
            out.push_str(&format!("      {detail}\n"));
        }
        let acesso = if cap.is_writable() {
            "leitura e escrita"
        } else {
            "somente leitura"
        };
        out.push_str(&format!("      acesso: {acesso}\n"));
    }
    out
}

/// Uma linha de `watch`: `+ id nome`, `- id` ou `~ id capacidade valor`.
pub fn format_event(event: &ProviderEvent) -> String {
    match event {
        ProviderEvent::DeviceAdded { device } => format!("+ {} {}", device.id, device.name),
        ProviderEvent::DeviceRemoved { id } => format!("- {id}"),
        ProviderEvent::CapabilityChanged { id, capability } => {
            format!(
                "~ {id} {} {}",
                capability.kind(),
                capability_value(capability)
            )
        }
    }
}

/// Uma linha de `watch --json`: um objeto com `event` e os dados do sinal.
pub fn event_json(event: &ProviderEvent) -> Result<String, serde_json::Error> {
    let value = match event {
        ProviderEvent::DeviceAdded { device } => {
            json!({ "event": "device_added", "device": device })
        }
        ProviderEvent::DeviceRemoved { id } => {
            json!({ "event": "device_removed", "id": id })
        }
        ProviderEvent::CapabilityChanged { id, capability } => {
            json!({ "event": "capability_changed", "id": id, "capability": capability })
        }
    };
    serde_json::to_string(&value)
}

#[cfg(test)]
pub(crate) mod tests {
    use raiza_core::{Color, DeviceId, Level, RgbZone};

    use super::*;

    pub(crate) fn mouse() -> DeviceInfo {
        DeviceInfo {
            id: DeviceId::usb(0x24ae, 0x1423, "0"),
            name: "Rapoo VT9".into(),
            vendor: Some("Rapoo".into()),
            kind: DeviceKind::Mouse,
            providers: vec!["hid-perfis".into(), "udev".into()],
            capabilities: vec![
                Capability::Battery {
                    percent: 80,
                    charging: false,
                },
                Capability::Dpi {
                    level: Level {
                        current: 1600,
                        min: 400,
                        max: 16000,
                        step: 50,
                    },
                    stages: vec![800, 1600, 3200],
                    writable: true,
                },
                Capability::PollingRate {
                    current: 1000,
                    supported: vec![125, 500, 1000],
                    writable: false,
                },
                Capability::Rgb {
                    zones: vec![RgbZone {
                        name: "logo".into(),
                        color: Color { r: 255, g: 0, b: 0 },
                    }],
                    modes: vec!["static".into(), "wave".into()],
                    current_mode: "static".into(),
                    writable: true,
                },
            ],
        }
    }

    pub(crate) fn fone() -> DeviceInfo {
        DeviceInfo {
            id: DeviceId::bluetooth("de:1e:59:99:7c:ae").unwrap(),
            name: "Fone Ágil".into(),
            vendor: None,
            kind: DeviceKind::Headset,
            providers: vec!["bluez".into()],
            capabilities: vec![
                Capability::Battery {
                    percent: 55,
                    charging: true,
                },
                Capability::Mute {
                    muted: true,
                    writable: true,
                },
                Capability::Connection {
                    medium: ConnectionKind::Bluetooth,
                    connected: true,
                    writable: true,
                },
            ],
        }
    }

    #[test]
    fn resumos() {
        let m = mouse();
        let resumos: Vec<_> = m.capabilities.iter().map(capability_summary).collect();
        assert_eq!(
            resumos,
            [
                "bateria 80%",
                "DPI 1600",
                "polling 1000 Hz",
                "RGB static #ff0000"
            ]
        );
        let f = fone();
        let resumos: Vec<_> = f.capabilities.iter().map(capability_summary).collect();
        assert_eq!(
            resumos,
            [
                "bateria 55% (carregando)",
                "mudo sim",
                "conexão bluetooth, conectado"
            ]
        );
    }

    #[test]
    fn lista_vazia() {
        assert_eq!(format_list(&[]), "nenhum dispositivo encontrado\n");
    }

    #[test]
    fn lista_alinha_colunas() {
        let texto = format_list(&[mouse(), fone()]);
        let linhas: Vec<&str> = texto.lines().collect();
        assert_eq!(linhas.len(), 3);
        assert!(linhas[0].starts_with("ID "));
        // A coluna TIPO começa no mesmo ponto em todas as linhas.
        let col_tipo = linhas[0].find("TIPO").unwrap();
        assert_eq!(linhas[1].find("mouse"), Some(col_tipo));
        assert_eq!(linhas[2].find("headset"), Some(col_tipo));
        let col_nome = linhas[0].find("NOME").unwrap();
        assert_eq!(linhas[1].find("Rapoo VT9"), Some(col_nome));
        assert_eq!(linhas[2].find("Fone Ágil"), Some(col_nome));
        assert!(linhas[1].ends_with("RGB static #ff0000"));
        assert!(linhas[2].ends_with("conexão bluetooth, conectado"));
        assert!(!texto.lines().any(|l| l.ends_with(' ')));
    }

    #[test]
    fn get_mostra_limites_e_acesso() {
        let texto = format_device(&mouse());
        let esperado = "\
Rapoo VT9 (usb:24ae:1423:0)
  tipo: mouse
  fabricante: Rapoo
  provedores: hid-perfis, udev
  capacidades:
    bateria: 80%
      acesso: somente leitura
    DPI: 1600
      limites: 400 a 16000, passo 50
      estágios: 800, 1600, 3200
      acesso: leitura e escrita
    polling: 1000 Hz
      aceitas: 125, 500, 1000 Hz
      acesso: somente leitura
    RGB: static #ff0000
      zonas: 0 logo #ff0000
      modos: static, wave
      acesso: leitura e escrita
";
        assert_eq!(texto, esperado);
    }

    #[test]
    fn get_sem_capacidades() {
        let mut d = fone();
        d.capabilities.clear();
        d.vendor = None;
        assert!(format_device(&d).ends_with("  capacidades: nenhuma\n"));
    }

    #[test]
    fn eventos_em_texto() {
        let m = mouse();
        assert_eq!(
            format_event(&ProviderEvent::DeviceAdded { device: m.clone() }),
            "+ usb:24ae:1423:0 Rapoo VT9"
        );
        assert_eq!(
            format_event(&ProviderEvent::DeviceRemoved { id: m.id.clone() }),
            "- usb:24ae:1423:0"
        );
        let mut dpi = m.capabilities[1].clone();
        dpi.apply(&raiza_core::CapabilityValue::Dpi(800)).unwrap();
        assert_eq!(
            format_event(&ProviderEvent::CapabilityChanged {
                id: m.id,
                capability: dpi
            }),
            "~ usb:24ae:1423:0 dpi 800"
        );
        assert_eq!(
            format_event(&ProviderEvent::CapabilityChanged {
                id: fone().id,
                capability: fone().capabilities[0].clone()
            }),
            "~ bt:DE:1E:59:99:7C:AE battery 55% (carregando)"
        );
    }

    #[test]
    fn eventos_em_json() {
        let m = mouse();
        let linha = event_json(&ProviderEvent::DeviceRemoved { id: m.id.clone() }).unwrap();
        assert_eq!(
            linha,
            r#"{"event":"device_removed","id":"usb:24ae:1423:0"}"#
        );
        let linha = event_json(&ProviderEvent::CapabilityChanged {
            id: m.id.clone(),
            capability: m.capabilities[0].clone(),
        })
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&linha).unwrap();
        assert_eq!(v["event"], "capability_changed");
        assert_eq!(v["capability"]["kind"], "battery");
        let linha = event_json(&ProviderEvent::DeviceAdded { device: m }).unwrap();
        assert!(!linha.contains('\n'));
        let v: serde_json::Value = serde_json::from_str(&linha).unwrap();
        assert_eq!(v["device"]["name"], "Rapoo VT9");
    }
}
