//! Fusão: o mesmo dispositivo físico visto por provedores diferentes vira um só.
//!
//! Exemplo: um headset Bluetooth aparece no BlueZ (bateria, conexão) e no PipeWire
//! (volume, mudo). Os dois usam o mesmo [`DeviceId::Bluetooth`], e a fusão produz um
//! único [`DeviceInfo`] com as quatro capacidades.
//!
//! Regra única, usada tanto por [`fuse`] quanto por [`capability_owner`]: a entrada vem
//! em ordem de prioridade dos provedores, e para cada tipo de capacidade **vence o
//! primeiro** provedor que a anuncia. Assim, o que a interface mostra é sempre o que o
//! provedor que vai receber a escrita anunciou.
//!
//! [`DeviceId::Bluetooth`]: crate::DeviceId::Bluetooth

use crate::{CapabilityKind, DeviceId, DeviceInfo, DeviceKind};

/// Funde os [`DeviceInfo`]s com o mesmo id.
///
/// `infos` deve vir em ordem de prioridade dos provedores. Para cada id:
/// - `name`: o primeiro não vazio;
/// - `vendor`: o primeiro conhecido;
/// - `kind`: o primeiro diferente de [`DeviceKind::Other`];
/// - `providers`: união, na ordem de chegada;
/// - `capabilities`: uma por tipo, do primeiro provedor que a anuncia.
///
/// A saída preserva a ordem em que cada id apareceu pela primeira vez.
pub fn fuse(infos: impl IntoIterator<Item = DeviceInfo>) -> Vec<DeviceInfo> {
    let mut out: Vec<DeviceInfo> = Vec::new();
    for info in infos {
        let Some(merged) = out.iter_mut().find(|d| d.id == info.id) else {
            let mut first = info;
            dedup_capabilities(&mut first);
            out.push(first);
            continue;
        };
        if merged.name.is_empty() {
            merged.name = info.name;
        }
        if merged.vendor.is_none() {
            merged.vendor = info.vendor;
        }
        if merged.kind == DeviceKind::Other {
            merged.kind = info.kind;
        }
        for p in info.providers {
            if !merged.providers.contains(&p) {
                merged.providers.push(p);
            }
        }
        for cap in info.capabilities {
            if merged.capability(cap.kind()).is_none() {
                merged.capabilities.push(cap);
            }
        }
    }
    out
}

/// Qual provedor atende a capacidade `kind` do dispositivo `id`, pela mesma regra de [`fuse`].
///
/// `infos` são os [`DeviceInfo`]s de cada provedor (antes da fusão), em ordem de prioridade.
/// O daemon usa isto para rotear `read` e `write`.
pub fn capability_owner<'a>(
    infos: impl IntoIterator<Item = &'a DeviceInfo>,
    id: &DeviceId,
    kind: CapabilityKind,
) -> Option<&'a str> {
    infos
        .into_iter()
        .filter(|d| &d.id == id && d.capability(kind).is_some())
        .find_map(|d| d.providers.first())
        .map(String::as_str)
}

/// Mantém só a primeira capacidade de cada tipo.
fn dedup_capabilities(info: &mut DeviceInfo) {
    let mut seen = Vec::new();
    info.capabilities.retain(|c| {
        let novo = !seen.contains(&c.kind());
        seen.push(c.kind());
        novo
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Capability, ConnectionKind, Level};

    fn headset_id() -> DeviceId {
        DeviceId::bluetooth("AA:BB:CC:DD:EE:FF").unwrap()
    }

    fn bluez() -> DeviceInfo {
        DeviceInfo {
            id: headset_id(),
            name: "Headset BT".into(),
            vendor: None,
            kind: DeviceKind::Headset,
            providers: vec!["bluez".into()],
            capabilities: vec![
                Capability::Battery {
                    percent: 60,
                    charging: false,
                },
                Capability::Connection {
                    medium: ConnectionKind::Bluetooth,
                    connected: true,
                },
            ],
        }
    }

    fn volume(current: u32) -> Capability {
        Capability::Volume {
            level: Level {
                current,
                min: 0,
                max: 150,
                step: 1,
            },
            writable: true,
        }
    }

    fn pipewire() -> DeviceInfo {
        DeviceInfo {
            id: headset_id(),
            name: String::new(),
            vendor: Some("Fabricante".into()),
            kind: DeviceKind::Other,
            providers: vec!["pipewire".into()],
            capabilities: vec![
                volume(40),
                Capability::Mute {
                    muted: false,
                    writable: true,
                },
                // Conflito: o BlueZ, com prioridade maior, vence.
                Capability::Connection {
                    medium: ConnectionKind::Other,
                    connected: false,
                },
            ],
        }
    }

    fn mouse() -> DeviceInfo {
        DeviceInfo {
            id: DeviceId::usb(0x24ae, 0x1423, "0"),
            name: "Rapoo".into(),
            vendor: Some("Rapoo".into()),
            kind: DeviceKind::Mouse,
            providers: vec!["udev".into()],
            capabilities: vec![],
        }
    }

    #[test]
    fn headset_bluetooth_vira_um_dispositivo() {
        let fundidos = fuse([bluez(), mouse(), pipewire()]);
        assert_eq!(fundidos.len(), 2);

        let h = &fundidos[0];
        assert_eq!(h.id, headset_id());
        assert_eq!(h.name, "Headset BT");
        assert_eq!(h.vendor.as_deref(), Some("Fabricante"));
        assert_eq!(h.kind, DeviceKind::Headset);
        assert_eq!(h.providers, ["bluez", "pipewire"]);
        let kinds: Vec<_> = h.capabilities.iter().map(Capability::kind).collect();
        assert_eq!(
            kinds,
            [
                CapabilityKind::Battery,
                CapabilityKind::Connection,
                CapabilityKind::Volume,
                CapabilityKind::Mute
            ]
        );
        assert_eq!(
            h.capability(CapabilityKind::Connection),
            bluez().capability(CapabilityKind::Connection)
        );

        assert_eq!(fundidos[1], mouse());
    }

    #[test]
    fn dono_segue_a_mesma_regra() {
        let infos = [bluez(), mouse(), pipewire()];
        let id = headset_id();
        assert_eq!(
            capability_owner(&infos, &id, CapabilityKind::Battery),
            Some("bluez")
        );
        assert_eq!(
            capability_owner(&infos, &id, CapabilityKind::Volume),
            Some("pipewire")
        );
        assert_eq!(
            capability_owner(&infos, &id, CapabilityKind::Connection),
            Some("bluez")
        );
        assert_eq!(capability_owner(&infos, &id, CapabilityKind::Dpi), None);
    }

    #[test]
    fn capacidade_duplicada_no_mesmo_provedor() {
        let mut p = pipewire();
        p.capabilities.push(volume(99));
        let fundidos = fuse([p]);
        assert_eq!(fundidos[0].capabilities.len(), 3);
        assert_eq!(
            fundidos[0].capability(CapabilityKind::Volume),
            Some(&volume(40))
        );
    }
}
