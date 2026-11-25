//! Camada de I/O com o PipeWire, atrás da trait [`PipeWireBackend`].
//!
//! A implementação real, [`PwCli`], usa as ferramentas que vêm com o PipeWire e o
//! WirePlumber:
//! - `pw-dump`: retrato completo do grafo, em JSON;
//! - `pw-dump --monitor`: o mesmo retrato e depois, a cada mudança, os objetos alterados.
//!   É orientado a eventos: nada de polling (pilar 2);
//! - `wpctl set-volume`/`set-mute`: a escrita passa pelo WirePlumber, que sabe aplicar o
//!   volume na rota certa da placa (ACP) e usa a curva cúbica. O `--limit 1.0` garante
//!   que nunca passa de 100 % (pilar 1).

use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use futures_util::stream::{self, BoxStream};
use serde_json::Value;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::curve::percent_to_wpctl;

/// Erro do backend.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BackendError {
    /// O PipeWire (ou a ferramenta) não está disponível nesta sessão.
    #[error("PipeWire indisponível: {0}")]
    Unavailable(String),
    /// O comando falhou.
    #[error("falha ao falar com o PipeWire: {0}")]
    Failed(String),
}

/// Item do fluxo de monitoramento.
#[derive(Debug, Clone, PartialEq)]
pub enum MonitorEvent {
    /// Retrato completo (ao conectar ou reconectar ao PipeWire): substitui o grafo.
    Snapshot(Vec<Value>),
    /// Objetos alterados, ou `{"id": N, "info": null}` para removidos.
    Update(Vec<Value>),
}

/// Acesso ao PipeWire. Injetável para testar sem PipeWire.
#[async_trait]
pub trait PipeWireBackend: Send + Sync + 'static {
    /// Retrato completo do grafo, no formato do `pw-dump`.
    async fn dump(&self) -> Result<Vec<Value>, BackendError>;

    /// Fluxo de mudanças. Começa com um [`MonitorEvent::Snapshot`].
    fn monitor(&self) -> BoxStream<'static, MonitorEvent>;

    /// Ajusta o volume do nó, em porcentagem na curva cúbica (0 a 100).
    async fn set_volume(&self, node: u32, percent: u32) -> Result<(), BackendError>;

    /// Liga ou desliga o mudo do nó.
    async fn set_mute(&self, node: u32, muted: bool) -> Result<(), BackendError>;
}

/// [`PipeWireBackend`] real, com `pw-dump` e `wpctl`.
#[derive(Debug, Clone)]
pub struct PwCli {
    pw_dump: String,
    wpctl: String,
}

impl Default for PwCli {
    fn default() -> Self {
        PwCli {
            pw_dump: "pw-dump".into(),
            wpctl: "wpctl".into(),
        }
    }
}

/// Espera inicial e máxima entre tentativas de reconectar o monitor.
const RETRY_MIN: Duration = Duration::from_secs(1);
const RETRY_MAX: Duration = Duration::from_secs(60);

impl PwCli {
    /// Usa `pw-dump` e `wpctl` do `PATH`.
    pub fn new() -> Self {
        Self::default()
    }

    async fn wpctl(&self, args: &[String]) -> Result<(), BackendError> {
        debug!(?args, "wpctl");
        let output = Command::new(&self.wpctl)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .await
            .map_err(|e| spawn_error(&self.wpctl, e))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(BackendError::Failed(format!(
                "wpctl {}: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            )))
        }
    }
}

fn spawn_error(program: &str, err: std::io::Error) -> BackendError {
    if err.kind() == std::io::ErrorKind::NotFound {
        BackendError::Unavailable(format!("{program} não encontrado"))
    } else {
        BackendError::Failed(format!("{program}: {err}"))
    }
}

#[async_trait]
impl PipeWireBackend for PwCli {
    async fn dump(&self) -> Result<Vec<Value>, BackendError> {
        let output = Command::new(&self.pw_dump)
            .arg("--no-colors")
            .stdin(Stdio::null())
            .output()
            .await
            .map_err(|e| spawn_error(&self.pw_dump, e))?;
        if !output.status.success() {
            // Sem servidor, o pw-dump sai com erro ao conectar.
            return Err(BackendError::Unavailable(
                String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            ));
        }
        serde_json::from_slice(&output.stdout)
            .map_err(|e| BackendError::Failed(format!("JSON inválido do pw-dump: {e}")))
    }

    fn monitor(&self) -> BoxStream<'static, MonitorEvent> {
        let (tx, rx) = mpsc::channel(32);
        let program = self.pw_dump.clone();
        tokio::spawn(run_monitor(program, tx));
        stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|e| (e, rx)) }).boxed()
    }

    async fn set_volume(&self, node: u32, percent: u32) -> Result<(), BackendError> {
        let value = format!("{:.2}", percent_to_wpctl(percent));
        self.wpctl(&[
            "set-volume".into(),
            "--limit".into(),
            "1.0".into(),
            node.to_string(),
            value,
        ])
        .await
    }

    async fn set_mute(&self, node: u32, muted: bool) -> Result<(), BackendError> {
        let flag = if muted { "1" } else { "0" };
        self.wpctl(&["set-mute".into(), node.to_string(), flag.into()])
            .await
    }
}

/// Mantém um `pw-dump --monitor` rodando e repassa as levas. Se o processo terminar
/// (ex.: o PipeWire reiniciou), tenta de novo com espera crescente. Termina quando o
/// fluxo é descartado ou quando o `pw-dump` não existe.
async fn run_monitor(program: String, tx: mpsc::Sender<MonitorEvent>) {
    let mut wait = RETRY_MIN;
    loop {
        let mut child = match Command::new(&program)
            .args(["--monitor", "--no-colors"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                info!("{}; sem eventos do PipeWire", spawn_error(&program, e));
                return;
            }
        };
        let Some(mut stdout) = child.stdout.take() else {
            return;
        };

        let mut buf = Vec::new();
        let mut chunk = vec![0u8; 64 * 1024];
        let mut first = true;
        loop {
            let read = tokio::select! {
                _ = tx.closed() => return,
                read = stdout.read(&mut chunk) => read,
            };
            match read {
                Ok(0) => break,
                Ok(n) => buf.extend_from_slice(&chunk[..n]),
                Err(e) => {
                    warn!("erro lendo o pw-dump --monitor: {e}");
                    break;
                }
            }
            for batch in drain_batches(&mut buf) {
                let event = if first {
                    first = false;
                    wait = RETRY_MIN;
                    MonitorEvent::Snapshot(batch)
                } else {
                    MonitorEvent::Update(batch)
                };
                if tx.send(event).await.is_err() {
                    return;
                }
            }
        }
        drop(child);
        info!(
            "pw-dump --monitor terminou; nova tentativa em {}s",
            wait.as_secs()
        );
        tokio::select! {
            _ = tx.closed() => return,
            _ = tokio::time::sleep(wait) => {}
        }
        wait = (wait * 2).min(RETRY_MAX);
    }
}

/// Tira de `buf` os vetores JSON completos e deixa o resto (incompleto) para a próxima leitura.
pub fn drain_batches(buf: &mut Vec<u8>) -> Vec<Vec<Value>> {
    let mut batches = Vec::new();
    let mut consumed = 0;
    let mut values = serde_json::Deserializer::from_slice(buf).into_iter::<Vec<Value>>();
    loop {
        match values.next() {
            Some(Ok(batch)) => {
                batches.push(batch);
                consumed = values.byte_offset();
            }
            Some(Err(e)) if e.is_eof() => break,
            Some(Err(e)) => {
                warn!("saída inválida do pw-dump --monitor, descartada: {e}");
                consumed = buf.len();
                break;
            }
            None => {
                consumed = values.byte_offset();
                break;
            }
        }
    }
    buf.drain(..consumed);
    batches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separa_levas_em_pedacos() {
        let texto = b"[\n  {\"id\": 1}\n]\n[\n  {\"id\": 2, \"info\": null}\n]\n[ {\"id\"";
        let mut buf = Vec::new();
        let mut levas = Vec::new();
        // Chega em pedaços de 7 bytes, como num pipe.
        for pedaco in texto.chunks(7) {
            buf.extend_from_slice(pedaco);
            levas.extend(drain_batches(&mut buf));
        }
        assert_eq!(levas.len(), 2);
        assert_eq!(levas[0][0]["id"], 1);
        assert!(levas[1][0]["info"].is_null());
        assert_eq!(buf.trim_ascii(), b"[ {\"id\"");
        buf.extend_from_slice(b": 3}]");
        assert_eq!(drain_batches(&mut buf).len(), 1);
        assert!(buf.is_empty());
    }

    #[test]
    fn descarta_lixo() {
        let mut buf = b"oops".to_vec();
        assert!(drain_batches(&mut buf).is_empty());
        assert!(buf.is_empty());
    }
}
