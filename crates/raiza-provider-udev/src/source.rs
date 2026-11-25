//! Camada de I/O: de onde vem a árvore udev e o aviso de mudanças.

use std::pin::Pin;

use futures_util::Stream;
use raiza_core::RaizaError;
use tokio::io::unix::AsyncFd;
use tracing::{debug, warn};
use udev::{EventType, MonitorBuilder};

use crate::tree::{Node, Tree};

/// Fluxo que emite `()` a cada mudança relevante na árvore (plugar, desplugar, driver
/// ligado ou desligado). O provedor relê a árvore inteira e calcula a diferença.
pub type ChangeStream = Pin<Box<dyn Stream<Item = ()> + Send>>;

/// Fonte da árvore udev. Injetável para testar sem hardware.
pub trait UdevSource: Send + Sync + 'static {
    /// Retrato atual da árvore. Chamado fora do runtime assíncrono (`spawn_blocking`).
    fn snapshot(&self) -> Result<Tree, RaizaError>;

    /// Começa a vigiar mudanças. Chamado de dentro do runtime tokio.
    fn watch(&self) -> Result<ChangeStream, RaizaError>;
}

/// Subsistemas lidos e vigiados.
const SUBSYSTEMS: [&str; 5] = ["usb", "input", "hidraw", "sound", "hid"];

/// Atributos sysfs copiados para o [`Node`]. Ler todos seria lento e inútil.
const ATTRIBUTES: [&str; 13] = [
    "idVendor",
    "idProduct",
    "serial",
    "product",
    "manufacturer",
    "removable",
    "bDeviceClass",
    "bInterfaceNumber",
    "bInterfaceClass",
    "bInterfaceSubClass",
    "bInterfaceProtocol",
    "name",
    "uniq",
];

/// A fonte real: libudev. Não precisa de root.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemUdev;

fn io_error(context: &str, err: std::io::Error) -> RaizaError {
    RaizaError::Io(format!("{context}: {err}"))
}

fn node_from(dev: &udev::Device) -> Option<Node> {
    let syspath = dev.syspath().to_str()?.to_owned();
    let text = |v: &std::ffi::OsStr| v.to_str().map(str::to_owned);
    Some(Node {
        syspath,
        subsystem: dev.subsystem().and_then(text).unwrap_or_default(),
        devtype: dev.devtype().and_then(text),
        properties: dev
            .properties()
            .filter_map(|p| Some((text(p.name())?, text(p.value())?)))
            .collect(),
        attributes: ATTRIBUTES
            .iter()
            .filter_map(|a| Some(((*a).to_owned(), text(dev.attribute_value(a)?)?)))
            .collect(),
    })
}

impl UdevSource for SystemUdev {
    fn snapshot(&self) -> Result<Tree, RaizaError> {
        let mut nodes = Vec::new();
        for subsystem in SUBSYSTEMS {
            let mut enumerator =
                udev::Enumerator::new().map_err(|e| io_error("udev indisponível", e))?;
            enumerator
                .match_subsystem(subsystem)
                .map_err(|e| io_error("filtro do udev", e))?;
            let devices = enumerator
                .scan_devices()
                .map_err(|e| io_error("enumeração do udev", e))?;
            nodes.extend(devices.filter_map(|d| node_from(&d)));
        }
        Ok(Tree::new(nodes))
    }

    fn watch(&self) -> Result<ChangeStream, RaizaError> {
        let mut builder = MonitorBuilder::new().map_err(|e| io_error("monitor do udev", e))?;
        for subsystem in SUBSYSTEMS {
            builder = builder
                .match_subsystem(subsystem)
                .map_err(|e| io_error("filtro do monitor", e))?;
        }
        let socket = builder
            .listen()
            .map_err(|e| io_error("monitor do udev", e))?;
        // `AsyncFd::new` foi depreciado em favor de `AsyncFd::register`, que é `unsafe`
        // (proibido pelas regras do projeto sem autorização da ficha). A condição que o
        // `register` exige vale aqui: o socket é dono do descritor do monitor, que fica
        // aberto e é o mesmo até o `AsyncFd` ser descartado.
        #[allow(deprecated)]
        let fd = AsyncFd::new(socket).map_err(|e| io_error("monitor do udev", e))?;

        let stream = futures_util::stream::unfold(fd, |mut fd| async move {
            loop {
                // `readable_mut` só exige `Send` do socket (o libudev não é `Sync`).
                let mut guard = match fd.readable_mut().await {
                    Ok(guard) => guard,
                    Err(e) => {
                        warn!(error = %e, "monitor do udev falhou; hotplug encerrado");
                        return None;
                    }
                };
                let mut relevant = false;
                for event in guard.get_inner().iter() {
                    debug!(
                        action = %event.event_type(),
                        syspath = %event.syspath().display(),
                        "evento udev"
                    );
                    relevant |= matches!(
                        event.event_type(),
                        EventType::Add | EventType::Remove | EventType::Bind | EventType::Unbind
                    );
                }
                // O socket é não bloqueante: `iter` esvaziou a fila.
                guard.clear_ready();
                if relevant {
                    return Some(((), fd));
                }
            }
        });
        Ok(Box::pin(stream))
    }
}
