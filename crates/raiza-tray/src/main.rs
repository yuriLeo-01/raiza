//! `raiza-tray`: ícone de bandeja (StatusNotifierItem) com a bateria dos periféricos.
//!
//! Para KDE, XFCE, Cinnamon e GNOME com a extensão AppIndicator. Cliente da API D-Bus do
//! `raizad` (`docs/DBUS.md`); só lê, nunca escreve. Sem watcher SNI, avisa no log e espera.
//! Nível de log por `RAIZA_LOG` (ex.: `RAIZA_LOG=debug`).

mod launch;
mod model;
mod session;
mod text;
mod tray;

use anyhow::Context as _;
use ksni::TrayMethods as _;
use raiza_core::dbus::RaizaClient;
use tokio::sync::mpsc;
use tracing::info;
use tracing_subscriber::EnvFilter;

use crate::model::Model;
use crate::tray::{Action, RaizaTray};

/// Repassa o modelo ao item SNI.
struct TraySink(ksni::Handle<RaizaTray>);

impl session::Sink for TraySink {
    async fn changed(&mut self, model: &Model) -> bool {
        let view = model.view();
        self.0
            .update(move |tray| tray.set_view(view))
            .await
            .is_some()
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env("RAIZA_LOG").unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    raiza_i18n::init();

    let client = RaizaClient::connect_session()
        .await
        .context("sem barramento de sessão D-Bus")?;

    let (actions_tx, mut actions) = mpsc::unbounded_channel();
    // Com `assume_sni_available`, a falta do watcher não é erro: o `ksni` chama
    // `watcher_offline` (que avisa no log) e registra o ícone quando o watcher aparecer.
    let handle = RaizaTray::new(actions_tx)
        .assume_sni_available(true)
        .spawn()
        .await
        .context("não deu para criar o ícone de bandeja")?;

    let session = session::run(client, TraySink(handle.clone()));
    tokio::pin!(session);

    loop {
        tokio::select! {
            result = &mut session => {
                result.context("conexão com o barramento de sessão perdida")?;
                break;
            }
            action = actions.recv() => match action {
                Some(Action::OpenApp(token)) => {
                    tokio::spawn(launch::open_app(token));
                }
                Some(Action::Quit) | None => break,
            },
            signal = tokio::signal::ctrl_c() => {
                signal.context("falha ao esperar Ctrl+C")?;
                break;
            }
        }
    }

    info!("encerrando o ícone de bandeja");
    handle.shutdown().await;
    Ok(())
}
