//! Identificador estável de dispositivo.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Identificador estável de um dispositivo físico.
///
/// É o que o daemon usa para fundir o mesmo aparelho visto por provedores diferentes
/// (veja [`crate::fuse`]) e para guardar configurações. Por isso precisa ser igual
/// entre reinícios e, sempre que possível, entre portas USB diferentes.
///
/// Forma textual canônica (usada em `Display`, `FromStr`, serde e D-Bus):
///
/// | Variante | Texto | Exemplo |
/// |---|---|---|
/// | [`DeviceId::Usb`] | `usb:VID:PID:instância` | `usb:24ae:1423:0` |
/// | [`DeviceId::Bluetooth`] | `bt:MAC` | `bt:AA:BB:CC:DD:EE:FF` |
/// | [`DeviceId::Other`] | `other:provedor:chave` | `other:openrgb:0` |
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub enum DeviceId {
    /// Dispositivo USB (inclui dongles sem fio).
    Usb {
        /// Vendor ID USB.
        vendor_id: u16,
        /// Product ID USB.
        product_id: u16,
        /// Número de série; sem serial, um índice estável entre aparelhos iguais.
        /// Não pode ser vazio.
        instance: String,
    },
    /// Dispositivo Bluetooth, identificado só pelo endereço. Assim BlueZ e PipeWire
    /// chegam ao mesmo id sem precisar conhecer vendor:product.
    Bluetooth {
        /// Endereço no formato `AA:BB:CC:DD:EE:FF` (maiúsculas).
        address: String,
    },
    /// Qualquer outro dispositivo, com chave definida pelo provedor.
    Other {
        /// Id do provedor que criou a chave.
        provider: String,
        /// Chave estável dentro do provedor.
        key: String,
    },
}

impl DeviceId {
    /// Cria um id USB.
    pub fn usb(vendor_id: u16, product_id: u16, instance: impl Into<String>) -> Self {
        DeviceId::Usb {
            vendor_id,
            product_id,
            instance: instance.into(),
        }
    }

    /// Cria um id Bluetooth a partir de um endereço, normalizando para maiúsculas.
    /// Aceita `:` ou `_` como separador (o BlueZ usa `_` nos caminhos D-Bus).
    pub fn bluetooth(address: &str) -> Result<Self, ParseDeviceIdError> {
        let normalized = address.replace('_', ":").to_ascii_uppercase();
        let parts: Vec<&str> = normalized.split(':').collect();
        let valid = parts.len() == 6
            && parts
                .iter()
                .all(|p| p.len() == 2 && p.chars().all(|c| c.is_ascii_hexdigit()));
        if valid {
            Ok(DeviceId::Bluetooth {
                address: normalized,
            })
        } else {
            Err(ParseDeviceIdError(format!(
                "endereço Bluetooth inválido: {address}"
            )))
        }
    }

    /// Regra única para o campo `instance` de [`DeviceId::Usb`].
    ///
    /// Todo provedor que gera um id USB (udev, PipeWire, UPower, evdev, OpenRGB...) deve
    /// usar esta função, para que o mesmo aparelho receba o mesmo id e a fusão funcione.
    ///
    /// - `serial`: atributo `serial` do dispositivo USB no sysfs (o mesmo que
    ///   `ID_SERIAL_SHORT` do udev). Espaços nas pontas são ignorados; vazio conta como ausente.
    /// - `port`: nome do dispositivo USB no sysfs, ex.: `1-3.2` (o diretório em
    ///   `/sys/bus/usb/devices/`).
    /// - `same_model_ports`: `port` de **todos** os dispositivos USB conectados com o
    ///   mesmo vendor:product e **sem** serial, incluindo o próprio.
    ///
    /// Com serial, a instância é o serial (estável entre portas). Sem serial, é a posição
    /// de `port` em `same_model_ports` em ordem lexicográfica: `0` quando só há um
    /// aparelho daquele modelo (o caso comum), estável entre portas e reinícios.
    pub fn usb_instance(serial: Option<&str>, port: &str, same_model_ports: &[&str]) -> String {
        if let Some(serial) = serial.map(str::trim).filter(|s| !s.is_empty()) {
            return serial.to_owned();
        }
        let mut ports: Vec<&str> = same_model_ports.to_vec();
        if !ports.contains(&port) {
            ports.push(port);
        }
        ports.sort_unstable();
        ports.dedup();
        let index = ports.iter().position(|p| *p == port).unwrap_or(0);
        index.to_string()
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceId::Usb {
                vendor_id,
                product_id,
                instance,
            } => write!(f, "usb:{vendor_id:04x}:{product_id:04x}:{instance}"),
            DeviceId::Bluetooth { address } => write!(f, "bt:{address}"),
            DeviceId::Other { provider, key } => write!(f, "other:{provider}:{key}"),
        }
    }
}

/// Texto que não é um [`DeviceId`] válido.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("id de dispositivo inválido: {0}")]
pub struct ParseDeviceIdError(String);

impl FromStr for DeviceId {
    type Err = ParseDeviceIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = || ParseDeviceIdError(s.to_owned());
        let non_empty = |v: &str| (!v.is_empty()).then(|| v.to_owned()).ok_or_else(invalid);
        let (bus, rest) = s.split_once(':').ok_or_else(invalid)?;
        match bus {
            "usb" => {
                let mut parts = rest.splitn(3, ':');
                let mut hex = || {
                    parts
                        .next()
                        .filter(|p| p.len() == 4)
                        .and_then(|p| u16::from_str_radix(p, 16).ok())
                        .ok_or_else(invalid)
                };
                let vendor_id = hex()?;
                let product_id = hex()?;
                let instance = non_empty(parts.next().unwrap_or_default())?;
                Ok(DeviceId::usb(vendor_id, product_id, instance))
            }
            "bt" => DeviceId::bluetooth(rest).map_err(|_| invalid()),
            "other" => {
                let (provider, key) = rest.split_once(':').ok_or_else(invalid)?;
                Ok(DeviceId::Other {
                    provider: non_empty(provider)?,
                    key: non_empty(key)?,
                })
            }
            _ => Err(invalid()),
        }
    }
}

impl From<DeviceId> for String {
    fn from(id: DeviceId) -> Self {
        id.to_string()
    }
}

impl TryFrom<String> for DeviceId {
    type Error = ParseDeviceIdError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exemplos() -> Vec<DeviceId> {
        vec![
            DeviceId::usb(0x24ae, 0x1423, "0"),
            DeviceId::usb(0x3151, 0x502f, "SN:com:dois-pontos"),
            DeviceId::bluetooth("aa_bb_cc_dd_ee_ff").unwrap(),
            DeviceId::Other {
                provider: "openrgb".into(),
                key: "0".into(),
            },
        ]
    }

    #[test]
    fn texto_ida_e_volta() {
        for id in exemplos() {
            let texto = id.to_string();
            assert_eq!(texto.parse::<DeviceId>().unwrap(), id, "{texto}");
        }
    }

    #[test]
    fn serde_ida_e_volta() {
        for id in exemplos() {
            let json = serde_json::to_string(&id).unwrap();
            assert_eq!(json, format!("\"{id}\""));
            assert_eq!(serde_json::from_str::<DeviceId>(&json).unwrap(), id);
        }
    }

    #[test]
    fn formato_legivel() {
        assert_eq!(
            DeviceId::usb(0x24ae, 0x1423, "0").to_string(),
            "usb:24ae:1423:0"
        );
        assert_eq!(
            DeviceId::bluetooth("aa:bb:cc:dd:ee:ff")
                .unwrap()
                .to_string(),
            "bt:AA:BB:CC:DD:EE:FF"
        );
    }

    #[test]
    fn instancia_usb() {
        // Serial manda, com espaços removidos.
        assert_eq!(
            DeviceId::usb_instance(Some(" ABC123 "), "1-3", &[]),
            "ABC123"
        );
        // Sem serial (ou vazio): índice pela ordem das portas.
        assert_eq!(DeviceId::usb_instance(None, "1-3", &["1-3"]), "0");
        assert_eq!(DeviceId::usb_instance(Some("  "), "1-3", &[]), "0");
        let portas = ["3-1", "1-4.2", "1-3"];
        assert_eq!(DeviceId::usb_instance(None, "1-3", &portas), "0");
        assert_eq!(DeviceId::usb_instance(None, "1-4.2", &portas), "1");
        assert_eq!(DeviceId::usb_instance(None, "3-1", &portas), "2");
        // A ordem da entrada não importa: provedores diferentes chegam ao mesmo índice.
        assert_eq!(
            DeviceId::usb_instance(None, "3-1", &["1-3", "3-1", "1-4.2"]),
            "2"
        );
    }

    #[test]
    fn rejeita_invalidos() {
        for s in [
            "",
            "usb",
            "usb:24ae:1423",
            "usb:24ae:1423:",
            "usb:xyz1:1423:0",
            "usb:24ae1:423:0",
            "bt:AA:BB",
            "bt:GG:BB:CC:DD:EE:FF",
            "display:GSM-5B09-1",
            "other:openrgb",
            "other::0",
            "pci:0",
        ] {
            assert!(s.parse::<DeviceId>().is_err(), "deveria rejeitar {s:?}");
        }
        assert!(serde_json::from_str::<DeviceId>("\"usb:1\"").is_err());
    }
}
