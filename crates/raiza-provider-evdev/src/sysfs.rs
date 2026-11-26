//! Leitura do sysfs: identidade do controle, suporte a vibração e bateria.
//!
//! Tudo parte de uma raiz configurável (`/sys` no sistema), o que permite testar com
//! um sysfs falso em diretório temporário.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use raiza_core::{Capability, DeviceId, DeviceInfo, DeviceKind};

use crate::PROVIDER_ID;
use crate::rumble::MAX_RUMBLE_MS;
use crate::source::InputNode;

/// `BUS_USB` e `BUS_BLUETOOTH` de `linux/input.h`.
const BUS_USB: u16 = 0x0003;
const BUS_BLUETOOTH: u16 = 0x0005;
/// Código de `FF_RUMBLE` em `linux/input-event-codes.h`.
const FF_RUMBLE: usize = 0x50;

/// Um controle com tudo o que o provedor precisa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pad {
    /// Descrição para o daemon.
    pub info: DeviceInfo,
    /// Nó `/dev/input/eventN` usado para vibrar.
    pub devnode: PathBuf,
}

/// Leitor de sysfs.
#[derive(Debug, Clone)]
pub struct Sysfs {
    root: PathBuf,
}

impl Default for Sysfs {
    fn default() -> Self {
        Sysfs::new("/sys")
    }
}

fn attr(dir: &Path, name: &str) -> Option<String> {
    let text = fs::read_to_string(dir.join(name)).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

fn hex(dir: &Path, name: &str) -> Option<u16> {
    u16::from_str_radix(attr(dir, name)?.trim_start_matches("0x"), 16).ok()
}

/// `0003:054C:0CE6.0005`: o nome de um dispositivo HID no sysfs.
fn is_hid_name(name: &str) -> bool {
    let Some((ids, instance)) = name.split_once('.') else {
        return false;
    };
    let parts: Vec<&str> = ids.split(':').collect();
    parts.len() == 3 && parts.iter().all(|p| is_hex(p)) && is_hex(instance)
}

fn is_hex(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Suporte a `FF_RUMBLE` em `capabilities/ff`: palavras hexadecimais separadas por
/// espaço, a mais significativa primeiro.
fn supports_rumble(input: &Path) -> bool {
    let Some(text) = attr(&input.join("capabilities"), "ff") else {
        return false;
    };
    let words: Vec<u64> = text
        .split_whitespace()
        .rev()
        .map_while(|w| u64::from_str_radix(w, 16).ok())
        .collect();
    let width = usize::BITS as usize;
    words
        .get(FF_RUMBLE / width)
        .is_some_and(|w| (w >> (FF_RUMBLE % width)) & 1 == 1)
}

/// Mapeia `capacity_level` para um percentual aproximado.
fn level_to_percent(level: &str) -> Option<u8> {
    match level.to_ascii_lowercase().as_str() {
        "critical" => Some(5),
        "low" => Some(20),
        "normal" => Some(50),
        "high" => Some(80),
        "full" => Some(100),
        _ => None,
    }
}

impl Sysfs {
    /// Leitor com outra raiz (testes).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Sysfs { root: root.into() }
    }

    /// Lê um joystick. `None` se não der para montar uma identidade.
    pub fn read_pad(&self, node: &InputNode) -> Option<Pad> {
        let input = fs::canonicalize(&node.input_syspath).ok()?;
        let name = attr(&input, "name").unwrap_or_else(|| "Controle".to_owned());
        let (id, vendor) = self.identity(&input, &name)?;

        let mut capabilities = Vec::new();
        if let Some(battery) = self.battery(&input) {
            capabilities.push(battery);
        }
        if supports_rumble(&input) {
            capabilities.push(Capability::Rumble {
                max_duration_ms: MAX_RUMBLE_MS,
                writable: true,
            });
        }
        Some(Pad {
            info: DeviceInfo {
                id,
                name,
                vendor,
                kind: DeviceKind::Gamepad,
                providers: vec![PROVIDER_ID.into()],
                capabilities,
            },
            devnode: node.devnode.clone(),
        })
    }

    /// Id e fabricante, conforme a tabela "Identidade" de `docs/ARQUITETURA.md`.
    fn identity(&self, input: &Path, name: &str) -> Option<(DeviceId, Option<String>)> {
        let id_dir = input.join("id");
        let bus = hex(&id_dir, "bustype");
        let usb = input
            .ancestors()
            .skip(1)
            .find(|d| d.join("idVendor").is_file());

        if bus == Some(BUS_BLUETOOTH)
            && let Some(uniq) = attr(input, "uniq")
            && let Ok(id) = DeviceId::bluetooth(&uniq)
        {
            return Some((id, None));
        }
        if bus == Some(BUS_USB)
            && let Some(usb) = usb
            && let (Some(vid), Some(pid)) = (hex(usb, "idVendor"), hex(usb, "idProduct"))
        {
            let port = usb.file_name()?.to_str()?;
            let serial = attr(usb, "serial");
            let ports = self.same_model_ports(vid, pid);
            let ports: Vec<&str> = ports.iter().map(String::as_str).collect();
            let instance = DeviceId::usb_instance(serial.as_deref(), port, &ports);
            return Some((DeviceId::usb(vid, pid, instance), attr(usb, "manufacturer")));
        }
        // Sem USB nem Bluetooth: chave estável do próprio provedor, que não funde.
        let phys = attr(input, "phys").unwrap_or_else(|| name.to_owned());
        let key = format!(
            "{:04x}:{:04x}:{phys}",
            hex(&id_dir, "vendor").unwrap_or(0),
            hex(&id_dir, "product").unwrap_or(0)
        );
        Some((
            DeviceId::Other {
                provider: PROVIDER_ID.into(),
                key,
            },
            None,
        ))
    }

    /// Portas de todos os dispositivos USB do mesmo modelo e sem serial (contrato de
    /// `DeviceId::usb_instance`: conta todos, não só os controles).
    fn same_model_ports(&self, vid: u16, pid: u16) -> Vec<String> {
        let Ok(entries) = fs::read_dir(self.root.join("bus/usb/devices")) else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter(|e| {
                let dir = e.path();
                hex(&dir, "idVendor") == Some(vid)
                    && hex(&dir, "idProduct") == Some(pid)
                    && attr(&dir, "serial").is_none()
            })
            .filter_map(|e| e.file_name().into_string().ok())
            .collect()
    }

    /// Bateria do `power_supply` ligado a este controle.
    fn battery(&self, input: &Path) -> Option<Capability> {
        // Dispositivos "donos" possíveis: do pai do input até o dispositivo HID (ou o
        // dispositivo USB, para drivers como o `xpad`), sem subir mais que isso.
        let mut owners = BTreeSet::new();
        for dir in input.ancestors().skip(1) {
            owners.insert(dir.to_path_buf());
            let hid = dir
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(is_hid_name);
            if hid || dir.join("idVendor").is_file() {
                break;
            }
        }

        let mut supplies: Vec<PathBuf> = fs::read_dir(self.root.join("class/power_supply"))
            .ok()?
            .flatten()
            .map(|e| e.path())
            .collect();
        supplies.sort();
        supplies
            .into_iter()
            .filter(|dir| fs::canonicalize(dir.join("device")).is_ok_and(|d| owners.contains(&d)))
            .find_map(|dir| read_battery(&dir))
    }
}

/// Lê `capacity` (ou `capacity_level`) e `status` de um `power_supply`.
fn read_battery(dir: &Path) -> Option<Capability> {
    if attr(dir, "type").is_some_and(|t| !t.eq_ignore_ascii_case("battery")) {
        return None;
    }
    let percent = attr(dir, "capacity")
        .and_then(|c| c.parse::<u32>().ok())
        .map(|c| c.min(100) as u8)
        .or_else(|| level_to_percent(&attr(dir, "capacity_level")?))?;
    let charging = attr(dir, "status").is_some_and(|s| s.eq_ignore_ascii_case("charging"));
    Some(Capability::Battery { percent, charging })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nomes_hid() {
        assert!(is_hid_name("0003:054C:0CE6.0005"));
        assert!(!is_hid_name("1-2:1.0"));
        assert!(!is_hid_name("input23"));
    }

    #[test]
    fn niveis_de_capacidade() {
        assert_eq!(level_to_percent("Critical"), Some(5));
        assert_eq!(level_to_percent("Low"), Some(20));
        assert_eq!(level_to_percent("Normal"), Some(50));
        assert_eq!(level_to_percent("High"), Some(80));
        assert_eq!(level_to_percent("Full"), Some(100));
        assert_eq!(level_to_percent("Unknown"), None);
    }
}
