//! Camada de I/O com o udev: lista os joysticks e avisa quando algo muda.

use std::path::PathBuf;
use std::pin::Pin;

use futures_util::Stream;
use raiza_core::RaizaError;
use tokio::io::unix::AsyncFd;
use tracing::{debug, warn};
use udev::{EventType, MonitorBuilder};

/// Fluxo que emite `()` a cada mudança relevante (controle plugado ou removido,
/// bateria mudou). O provedor relê tudo e calcula a diferença.
pub type ChangeStream = Pin<Box<dyn Stream<Item = ()> + Send>>;

/// Um joystick visto pelo udev.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputNode {
    /// Nó de dispositivo, ex.: `/dev/input/event22`.
    pub devnode: PathBuf,
    /// Caminho sysfs do dispositivo de input (`.../input/inputN`), pai do `eventN`.
    pub input_syspath: PathBuf,
}

/// Fonte de joysticks. Injetável para testar sem hardware.
pub trait PadSource: Send + Sync + 'static {
    /// Joysticks presentes agora. Chamado fora do runtime assíncrono (`spawn_blocking`).
    fn list(&self) -> Result<Vec<InputNode>, RaizaError>;

    /// Começa a vigiar mudanças. Chamado de dentro do runtime tokio.
    fn watch(&self) -> Result<ChangeStream, RaizaError>;
}

/// A fonte real: libudev. Não precisa de root.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemUdev;

fn io_error(context: &str, err: std::io::Error) -> RaizaError {
    RaizaError::Io(format!("{context}: {err}"))
}

impl PadSource for SystemUdev {
    fn list(&self) -> Result<Vec<InputNode>, RaizaError> {
        let mut enumerator =
            udev::Enumerator::new().map_err(|e| io_error("udev indisponível", e))?;
        enumerator
            .match_subsystem("input")
            .and_then(|()| enumerator.match_property("ID_INPUT_JOYSTICK", "1"))
            .map_err(|e| io_error("filtro do udev", e))?;
        let devices = enumerator
            .scan_devices()
            .map_err(|e| io_error("enumeração do udev", e))?;
        Ok(devices
            .filter_map(|dev| {
                let devnode = dev.devnode()?.to_path_buf();
                // Só os nós `eventN` servem; `jsN` e o `inputN` em si ficam de fora.
                let is_event = devnode
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("event"));
                let input_syspath = dev.parent()?.syspath().to_path_buf();
                is_event.then_some(InputNode {
                    devnode,
                    input_syspath,
                })
            })
            .collect())
    }

    fn watch(&self) -> Result<ChangeStream, RaizaError> {
        let socket = MonitorBuilder::new()
            .and_then(|b| b.match_subsystem("input"))
            .and_then(|b| b.match_subsystem("power_supply"))
            .and_then(MonitorBuilder::listen)
            .map_err(|e| io_error("monitor do udev", e))?;
        // `AsyncFd::new` foi depreciado em favor de `AsyncFd::register`, que é `unsafe`
        // (proibido pelas regras do projeto sem autorização da ficha). A condição que o
        // `register` exige vale aqui: o socket é dono do descritor do monitor, que fica
        // aberto e é o mesmo até o `AsyncFd` ser descartado.
        #[allow(deprecated)]
        let fd = AsyncFd::new(socket).map_err(|e| io_error("monitor do udev", e))?;

        let stream = futures_util::stream::unfold(fd, |mut fd| async move {
            loop {
                let mut guard = match fd.readable_mut().await {
                    Ok(guard) => guard,
                    Err(e) => {
                        warn!(error = %e, "monitor do udev falhou; eventos encerrados");
                        return None;
                    }
                };
                let mut relevant = false;
                for event in guard.get_inner().iter() {
                    let action = event.event_type();
                    debug!(
                        %action,
                        syspath = %event.syspath().display(),
                        "evento udev"
                    );
                    let power = event.subsystem().is_some_and(|s| s == "power_supply");
                    relevant |= match action {
                        EventType::Add | EventType::Remove => true,
                        // Bateria muda por `change`; no `input` esse evento é só ruído.
                        EventType::Change => power,
                        EventType::Bind | EventType::Unbind => !power,
                        _ => false,
                    };
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
