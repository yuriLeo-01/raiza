//! Troca de relatórios com uma interface aberta: envio, resposta com repetição e
//! leitura contínua dos input reports.
//!
//! Tudo aqui bloqueia e roda fora do runtime assíncrono.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use raiza_core::RaizaError;
use raiza_profiles::HidReport;
use raiza_profiles::format::ReportType;
use tracing::{debug, warn};

use crate::backend::HidHandle;

/// Maior input report aceito do hidraw (os perfis vão até 64 bytes; sobra margem).
const READ_BUFFER: usize = 1024;
/// Espera entre dois GET_FEATURE quando a resposta ainda não está pronta.
const FEATURE_RETRY_GAP: Duration = Duration::from_millis(5);
/// Quanto o leitor contínuo espera por relatório antes de conferir se deve parar.
const READER_TICK: Duration = Duration::from_secs(1);

/// Distribui cada input report recebido para quem está esperando uma resposta.
#[derive(Default)]
pub(crate) struct InputHub {
    subscribers: Mutex<Vec<Sender<Vec<u8>>>>,
}

impl InputHub {
    /// Passa a receber os próximos input reports (crus, como vieram do hidraw).
    pub(crate) fn subscribe(&self) -> Receiver<Vec<u8>> {
        let (tx, rx) = mpsc::channel();
        self.subscribers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(tx);
        rx
    }

    fn publish(&self, report: &[u8]) {
        self.subscribers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|s| s.send(report.to_vec()).is_ok());
    }
}

/// Para a thread do leitor contínuo quando descartado.
pub(crate) struct ReaderGuard {
    stop: Arc<AtomicBool>,
}

impl Drop for ReaderGuard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Começa a ler input reports de `handle` numa thread própria. Cada relatório vai para
/// `on_report` e depois para o `hub`. A thread acaba quando o guarda é descartado ou o
/// aparelho some (e então marca `dead`).
pub(crate) fn spawn_reader(
    name: String,
    handle: Box<dyn HidHandle>,
    hub: Arc<InputHub>,
    dead: Arc<AtomicBool>,
    on_report: impl Fn(&[u8]) + Send + 'static,
) -> Result<ReaderGuard, RaizaError> {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    std::thread::Builder::new()
        .name("raiza-hidraw".into())
        .spawn(move || {
            let mut buf = vec![0u8; READ_BUFFER];
            while !flag.load(Ordering::Relaxed) {
                match handle.read_timeout(&mut buf, READER_TICK) {
                    Ok(0) => {}
                    Ok(n) => {
                        let report = &buf[..n.min(buf.len())];
                        on_report(report);
                        hub.publish(report);
                    }
                    Err(e) => {
                        debug!(%name, error = %e, "leitura contínua encerrada");
                        dead.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            }
        })
        .map_err(|e| RaizaError::Io(format!("thread de leitura: {e}")))?;
    Ok(ReaderGuard { stop })
}

/// Põe o report ID na frente de um input report cru, se preciso. Aparelhos sem
/// relatórios numerados (report ID `0` no perfil) entregam só o conteúdo.
pub(crate) fn normalize(raw: &[u8], report_id: u8) -> Vec<u8> {
    if report_id == 0 {
        std::iter::once(0).chain(raw.iter().copied()).collect()
    } else {
        raw.to_vec()
    }
}

/// Envia um relatório pelo canal certo do tipo dele.
pub(crate) fn send(handle: &dyn HidHandle, report: &HidReport) -> Result<(), RaizaError> {
    match report.report_type {
        ReportType::Feature => handle.send_feature_report(&report.bytes),
        ReportType::Output => handle.write(&report.bytes),
        ReportType::Input => Err(RaizaError::Io(
            "um input report não pode ser enviado ao aparelho".into(),
        )),
    }
}

/// Como reconhecer a resposta esperada.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Expected {
    /// `Feature` (GET_FEATURE) ou `Input` (próximo input report com esse ID).
    pub report_type: ReportType,
    /// Report ID da resposta.
    pub report_id: u8,
    /// Tamanho do conteúdo, sem o report ID.
    pub length: usize,
}

/// Envia `request` (se houver) e espera uma resposta que `accept` aceite.
///
/// Uma resposta recusada com [`RaizaError::Io`] (report ID, tamanho ou `expect` que não
/// batem, como um "ocupado") é descartada, e a leitura se repete até `timeout`. Outros
/// erros de `accept` encerram na hora.
pub(crate) fn exchange<T>(
    handle: &dyn HidHandle,
    inputs: Option<&InputHub>,
    request: Option<&HidReport>,
    expected: Expected,
    timeout: Duration,
    mut accept: impl FnMut(&[u8]) -> Result<T, RaizaError>,
) -> Result<T, RaizaError> {
    let deadline = Instant::now() + timeout;
    // Assina antes de enviar, para não perder uma resposta rápida.
    let rx = match expected.report_type {
        ReportType::Input => Some(
            inputs
                .ok_or_else(|| RaizaError::Io("interface sem leitura de input reports".into()))?
                .subscribe(),
        ),
        _ => None,
    };
    if let Some(request) = request {
        send(handle, request)?;
    }
    let mut last = String::from("nenhuma resposta");
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let bytes = match &rx {
            Some(rx) => match rx.recv_timeout(remaining) {
                Ok(raw) => {
                    let bytes = normalize(&raw, expected.report_id);
                    // Outro relatório do aparelho não é a resposta.
                    (bytes.first() == Some(&expected.report_id)).then_some(bytes)
                }
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(RaizaError::Io("leitura do aparelho encerrada".into()));
                }
            },
            None => {
                let mut buf = vec![0u8; expected.length + 1];
                buf[0] = expected.report_id;
                let n = handle.get_feature_report(&mut buf)?;
                buf.truncate(n);
                Some(buf)
            }
        };
        if let Some(bytes) = bytes {
            match accept(&bytes) {
                Ok(v) => return Ok(v),
                Err(RaizaError::Io(why)) => {
                    debug!(%why, "resposta descartada, tentando de novo");
                    last = why;
                }
                Err(e) => return Err(e),
            }
        }
        if Instant::now() >= deadline {
            let ms = timeout.as_millis();
            warn!(%last, "sem resposta válida do aparelho em {ms} ms");
            return Err(RaizaError::Io(format!(
                "sem resposta válida em {ms} ms (última: {last})"
            )));
        }
        if rx.is_none() {
            std::thread::sleep(FEATURE_RETRY_GAP.min(remaining));
        }
    }
}
