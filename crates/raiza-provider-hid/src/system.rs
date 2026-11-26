//! Backend real: udev para listar e vigiar `/dev/hidraw*`, hidapi para falar com eles.

use std::ffi::CString;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use hidapi::{HidApi, HidDevice};
use raiza_core::RaizaError;
use tokio::io::unix::AsyncFd;
use tracing::{debug, warn};
use udev::{EventType, MonitorBuilder};

use crate::backend::{ChangeStream, HidBackend, HidHandle, HidInterface, Inventory, UsbDevice};
use crate::descriptor::top_level_collections;

/// Dica mostrada quando falta permissão em `/dev/hidraw*`.
pub const PERMISSION_HINT: &str = "instale a regra udev do Raiza (packaging/udev/70-raiza.rules em \
     /etc/udev/rules.d/), rode `sudo udevadm control --reload` e replugue o aparelho";

/// O HID do sistema. Não precisa de root: o acesso vem da regra udev do projeto.
#[derive(Default)]
pub struct SystemHid {
    api: Mutex<Option<HidApi>>,
}

impl SystemHid {
    /// Backend ligado ao sistema.
    pub fn new() -> Self {
        Self::default()
    }
}

fn io_error(context: &str, err: impl std::fmt::Display) -> RaizaError {
    RaizaError::Io(format!("{context}: {err}"))
}

fn text(v: Option<&std::ffi::OsStr>) -> Option<String> {
    v.and_then(|s| s.to_str())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

fn hex16(dev: &udev::Device, attr: &str) -> Option<u16> {
    u16::from_str_radix(&text(dev.attribute_value(attr))?, 16).ok()
}

fn interface_of(dev: &udev::Device) -> Option<HidInterface> {
    let path = dev.devnode()?.to_str()?.to_owned();
    let usb = dev
        .parent_with_subsystem_devtype("usb", "usb_device")
        .ok()
        .flatten()?;
    let interface = dev
        .parent_with_subsystem_devtype("usb", "usb_interface")
        .ok()
        .flatten()
        .and_then(|i| text(i.attribute_value("bInterfaceNumber")))
        .and_then(|n| u8::from_str_radix(&n, 16).ok());
    // O sysfs é legível por todos; o descritor não exige abrir o hidraw.
    let descriptor = std::fs::read(dev.syspath().join("device/report_descriptor"))
        .inspect_err(|e| debug!(%path, error = %e, "sem descritor de relatório"))
        .unwrap_or_default();
    Some(HidInterface {
        path,
        vendor_id: hex16(&usb, "idVendor")?,
        product_id: hex16(&usb, "idProduct")?,
        usb_port: usb.sysname().to_str()?.to_owned(),
        serial: text(usb.attribute_value("serial")),
        interface,
        collections: top_level_collections(&descriptor),
        // Instante em que o udev inicializou o nó: novo a cada replugue.
        token: text(dev.property_value("USEC_INITIALIZED"))
            .unwrap_or_else(|| format!("{:?}", dev.devnum())),
    })
}

impl HidBackend for SystemHid {
    fn inventory(&self) -> Result<Inventory, RaizaError> {
        let scan = |subsystem: &str| -> Result<Vec<udev::Device>, RaizaError> {
            let mut e = udev::Enumerator::new().map_err(|e| io_error("udev indisponível", e))?;
            e.match_subsystem(subsystem)
                .map_err(|e| io_error("filtro do udev", e))?;
            Ok(e.scan_devices()
                .map_err(|e| io_error("enumeração do udev", e))?
                .collect())
        };
        let interfaces = scan("hidraw")?
            .iter()
            .filter_map(|d| {
                let iface = interface_of(d);
                if iface.is_none() {
                    debug!(syspath = %d.syspath().display(), "hidraw fora de USB, ignorado");
                }
                iface
            })
            .collect();
        let usb_devices = scan("usb")?
            .iter()
            .filter(|d| d.devtype().and_then(|t| t.to_str()) == Some("usb_device"))
            .filter_map(|d| {
                Some(UsbDevice {
                    vendor_id: hex16(d, "idVendor")?,
                    product_id: hex16(d, "idProduct")?,
                    port: d.sysname().to_str()?.to_owned(),
                    serial: text(d.attribute_value("serial")),
                })
            })
            .collect();
        Ok(Inventory {
            interfaces,
            usb_devices,
        })
    }

    fn open(&self, path: &str) -> Result<Box<dyn HidHandle>, RaizaError> {
        // Testa o acesso antes, para separar falta de permissão de outros erros.
        if let Err(e) = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
        {
            return Err(match e.kind() {
                std::io::ErrorKind::PermissionDenied => {
                    RaizaError::Permission(format!("{path}: {e}; {PERMISSION_HINT}"))
                }
                _ => io_error(path, e),
            });
        }
        let cpath = CString::new(path).map_err(|e| io_error(path, e))?;
        let mut api = self.api.lock().unwrap_or_else(PoisonError::into_inner);
        if api.is_none() {
            // A lista vem do udev; o hidapi só abre e conversa.
            HidApi::disable_device_discovery();
            *api = Some(HidApi::new().map_err(|e| io_error("hidapi", e))?);
        }
        let api = api
            .as_ref()
            .ok_or_else(|| RaizaError::Io("hidapi".into()))?;
        let device = api.open_path(&cpath).map_err(|e| io_error(path, e))?;
        Ok(Box::new(SystemHandle {
            device: Mutex::new(device),
        }))
    }

    fn watch(&self) -> Result<ChangeStream, RaizaError> {
        let socket = MonitorBuilder::new()
            .and_then(|b| b.match_subsystem("hidraw"))
            .and_then(MonitorBuilder::listen)
            .map_err(|e| io_error("monitor do udev", e))?;
        // `AsyncFd::new` foi depreciado em favor de `AsyncFd::register`, que é `unsafe`
        // (proibido pelas regras do projeto). A condição que o `register` exige vale aqui:
        // o socket é dono do descritor, que fica aberto até o `AsyncFd` ser descartado.
        #[allow(deprecated)]
        let fd = AsyncFd::new(socket).map_err(|e| io_error("monitor do udev", e))?;
        let stream = futures_util::stream::unfold(fd, |mut fd| async move {
            loop {
                let mut guard = match fd.readable_mut().await {
                    Ok(guard) => guard,
                    Err(e) => {
                        warn!(error = %e, "monitor do udev falhou; hotplug HID encerrado");
                        return None;
                    }
                };
                let relevant = guard
                    .get_inner()
                    .iter()
                    .any(|e| matches!(e.event_type(), EventType::Add | EventType::Remove));
                guard.clear_ready();
                if relevant {
                    return Some(((), fd));
                }
            }
        });
        Ok(Box::pin(stream))
    }
}

/// Uma interface aberta pelo hidapi. O `HidDevice` não é `Sync`; o mutex resolve.
struct SystemHandle {
    device: Mutex<HidDevice>,
}

impl SystemHandle {
    fn with<T>(
        &self,
        what: &str,
        f: impl FnOnce(&HidDevice) -> hidapi::HidResult<T>,
    ) -> Result<T, RaizaError> {
        let device = self.device.lock().unwrap_or_else(PoisonError::into_inner);
        f(&device).map_err(|e| io_error(what, e))
    }
}

impl HidHandle for SystemHandle {
    fn write(&self, report: &[u8]) -> Result<(), RaizaError> {
        let sent = self.with("write", |d| d.write(report))?;
        if sent == report.len() {
            Ok(())
        } else {
            Err(RaizaError::Io(format!(
                "write: {sent} de {} bytes enviados",
                report.len()
            )))
        }
    }

    fn send_feature_report(&self, report: &[u8]) -> Result<(), RaizaError> {
        self.with("SET_FEATURE", |d| d.send_feature_report(report))
    }

    fn get_feature_report(&self, buf: &mut [u8]) -> Result<usize, RaizaError> {
        self.with("GET_FEATURE", |d| d.get_feature_report(buf))
    }

    fn read_timeout(&self, buf: &mut [u8], timeout: Duration) -> Result<usize, RaizaError> {
        let ms = i32::try_from(timeout.as_millis()).unwrap_or(i32::MAX);
        self.with("read", |d| d.read_timeout(buf, ms))
    }
}
