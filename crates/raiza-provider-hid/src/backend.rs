//! Camada de I/O injetável: o que o provedor precisa do sistema para falar HID.
//!
//! Em produção ela é o [`crate::SystemHid`] (udev + hidraw). Nos testes, um HID falso.

use std::collections::BTreeMap;
use std::pin::Pin;
use std::time::Duration;

use futures_util::Stream;
use raiza_core::{DeviceId, RaizaError};

/// Fluxo que emite `()` a cada mudança relevante (aparelho plugado ou desplugado,
/// arquivo de perfil alterado). O provedor relê tudo e calcula a diferença.
pub type ChangeStream = Pin<Box<dyn Stream<Item = ()> + Send>>;

/// Uma interface HID (`/dev/hidrawN`) de um dispositivo USB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HidInterface {
    /// Nó do dispositivo, ex.: `/dev/hidraw3`.
    pub path: String,
    /// Vendor ID do dispositivo USB pai.
    pub vendor_id: u16,
    /// Product ID do dispositivo USB pai.
    pub product_id: u16,
    /// Nome do dispositivo USB pai no sysfs (a porta), ex.: `3-2.2`.
    pub usb_port: String,
    /// Atributo `serial` do dispositivo USB pai, se houver.
    pub serial: Option<String>,
    /// `bInterfaceNumber` da interface USB.
    pub interface: Option<u8>,
    /// Coleções de nível mais alto do descritor de relatório: `(usage page, usage)`.
    pub collections: Vec<(u16, u16)>,
    /// Muda quando o nó é recriado (aparelho desplugado e plugado de novo), mesmo que o
    /// caminho se repita. Assim a identificação se repete a cada reconexão.
    pub token: String,
}

/// Um dispositivo USB conectado (para a regra de instância de [`DeviceId::usb_instance`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbDevice {
    /// Vendor ID.
    pub vendor_id: u16,
    /// Product ID.
    pub product_id: u16,
    /// Nome no sysfs (a porta), ex.: `3-2.2`.
    pub port: String,
    /// Atributo `serial`, se houver.
    pub serial: Option<String>,
}

/// Retrato do que está conectado agora.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Inventory {
    /// Interfaces hidraw de dispositivos USB.
    pub interfaces: Vec<HidInterface>,
    /// Todos os dispositivos USB conectados (com ou sem hidraw).
    pub usb_devices: Vec<UsbDevice>,
}

impl Inventory {
    /// Id do aparelho dono da interface, pela regra de `docs/ARQUITETURA.md`.
    pub fn device_id(&self, iface: &HidInterface) -> DeviceId {
        let no_serial =
            |serial: &Option<String>| serial.as_deref().is_none_or(|s| s.trim().is_empty());
        let mut ports: Vec<&str> = self
            .usb_devices
            .iter()
            .filter(|d| d.vendor_id == iface.vendor_id && d.product_id == iface.product_id)
            .filter(|d| no_serial(&d.serial))
            .map(|d| d.port.as_str())
            .collect();
        // O próprio aparelho entra mesmo que a lista de USB venha incompleta.
        if no_serial(&iface.serial) {
            ports.push(&iface.usb_port);
        }
        let instance = DeviceId::usb_instance(iface.serial.as_deref(), &iface.usb_port, &ports);
        DeviceId::usb(iface.vendor_id, iface.product_id, instance)
    }

    /// Interfaces agrupadas pelo id do aparelho, em ordem de interface e de caminho.
    pub fn by_device(&self) -> BTreeMap<DeviceId, Vec<&HidInterface>> {
        let mut map: BTreeMap<DeviceId, Vec<&HidInterface>> = BTreeMap::new();
        for iface in &self.interfaces {
            map.entry(self.device_id(iface)).or_default().push(iface);
        }
        for list in map.values_mut() {
            list.sort_by(|a, b| (a.interface, &a.path).cmp(&(b.interface, &b.path)));
        }
        map
    }
}

/// Uma interface hidraw aberta. Os métodos bloqueiam: o provedor os chama fora do
/// runtime assíncrono.
///
/// Em todos os relatórios, `[0]` é o report ID (`0` em aparelhos sem relatórios numerados),
/// como no hidraw.
pub trait HidHandle: Send + Sync {
    /// Envia um output report.
    fn write(&self, report: &[u8]) -> Result<(), RaizaError>;
    /// Envia um feature report (SET_FEATURE).
    fn send_feature_report(&self, report: &[u8]) -> Result<(), RaizaError>;
    /// Lê um feature report (GET_FEATURE). `buf[0]` traz o report ID pedido. Devolve
    /// quantos bytes vieram, contando o report ID.
    fn get_feature_report(&self, buf: &mut [u8]) -> Result<usize, RaizaError>;
    /// Espera o próximo input report por até `timeout`. Devolve `0` se nada chegou.
    /// Como no hidraw, aparelhos sem relatórios numerados entregam o conteúdo sem o
    /// report ID.
    fn read_timeout(&self, buf: &mut [u8], timeout: Duration) -> Result<usize, RaizaError>;
}

/// Acesso ao HID do sistema. Injetável para testar sem hardware.
pub trait HidBackend: Send + Sync + 'static {
    /// O que está conectado agora. Chamado fora do runtime assíncrono.
    fn inventory(&self) -> Result<Inventory, RaizaError>;

    /// Abre uma interface. Chamado fora do runtime assíncrono. Pode ser chamado duas vezes
    /// para a mesma interface: cada abertura recebe a sua cópia dos input reports.
    /// Sem permissão: [`RaizaError::Permission`].
    fn open(&self, path: &str) -> Result<Box<dyn HidHandle>, RaizaError>;

    /// Começa a vigiar aparelhos plugados e desplugados. Chamado de dentro do runtime tokio.
    fn watch(&self) -> Result<ChangeStream, RaizaError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iface(port: &str, serial: Option<&str>, interface: u8) -> HidInterface {
        HidInterface {
            path: format!("/dev/hidraw-{port}-{interface}"),
            vendor_id: 0x1234,
            product_id: 0x5678,
            usb_port: port.into(),
            serial: serial.map(Into::into),
            interface: Some(interface),
            collections: vec![],
            token: String::new(),
        }
    }

    fn usb(port: &str, serial: Option<&str>) -> UsbDevice {
        UsbDevice {
            vendor_id: 0x1234,
            product_id: 0x5678,
            port: port.into(),
            serial: serial.map(Into::into),
        }
    }

    #[test]
    fn id_pela_regra_de_instancia() {
        let inv = Inventory {
            interfaces: vec![
                iface("3-1", None, 1),
                iface("1-4", None, 0),
                iface("1-4", None, 1),
                iface("2-2", Some("ABC"), 0),
            ],
            usb_devices: vec![usb("3-1", None), usb("1-4", None), usb("2-2", Some("ABC"))],
        };
        let ids: Vec<String> = inv.by_device().keys().map(ToString::to_string).collect();
        assert_eq!(
            ids,
            ["usb:1234:5678:0", "usb:1234:5678:1", "usb:1234:5678:ABC"]
        );
        let grupo = &inv.by_device()[&DeviceId::usb(0x1234, 0x5678, "0")];
        assert_eq!(grupo.len(), 2);
        assert_eq!(grupo[0].interface, Some(0));
    }
}
