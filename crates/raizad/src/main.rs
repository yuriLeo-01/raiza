//! raizad: daemon do Raiza no barramento de sessão.
//!
//! Uso: `raizad [--mock] [--config ARQUIVO]`. Nível de log por `RAIZA_LOG`
//! (sintaxe do `tracing-subscriber`, ex.: `RAIZA_LOG=debug` ou `RAIZA_LOG=raizad=trace`).

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use raiza_core::DBUS_SERVICE;
use raiza_core::dbus::DBUS_PATH;
use raizad::battery::{self, DesktopNotifier};
use raizad::{ConfigStore, Daemon, Service, forward_signals, providers};
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::mpsc;
use tracing::info;
use tracing_subscriber::EnvFilter;

const HELP: &str = "\
raizad: daemon do Raiza (periféricos no Linux)

Uso: raizad [opções]

Opções:
  --mock             registra só o provedor falso, com dispositivos de demonstração
  --config ARQUIVO   arquivo de configurações (padrão: ~/.config/raiza/config.json;
                     com --mock, só em memória)
  -h, --help         mostra esta ajuda
  -V, --version      mostra a versão

Variáveis: RAIZA_LOG define o nível de log (ex.: info, debug, raizad=trace).
           RAIZA_PROFILES: diretórios de perfis HID (separados por :) antes dos padrão.";

struct Args {
    mock: bool,
    config: Option<PathBuf>,
}

/// `None` quando já respondeu (ajuda, versão).
fn parse_args() -> Result<Option<Args>> {
    let mut args = Args {
        mock: false,
        config: None,
    };
    let mut it = std::env::args_os().skip(1);
    while let Some(arg) = it.next() {
        match arg.to_str() {
            Some("--mock") => args.mock = true,
            Some("--config") => {
                let path = it.next().context("--config precisa de um arquivo")?;
                args.config = Some(path.into());
            }
            Some("-h" | "--help") => {
                println!("{HELP}");
                return Ok(None);
            }
            Some("-V" | "--version") => {
                println!("raizad {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            _ => bail!(
                "argumento desconhecido: {}\n\n{HELP}",
                arg.to_string_lossy()
            ),
        }
    }
    Ok(Some(args))
}

#[tokio::main]
async fn main() -> Result<()> {
    let Some(args) = parse_args()? else {
        return Ok(());
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env("RAIZA_LOG").unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    // O modo --mock não toca na configuração real do usuário, a menos que --config seja dado.
    let path = match (&args.config, args.mock) {
        (Some(path), _) => Some(path.clone()),
        (None, true) => None,
        (None, false) => ConfigStore::default_path(),
    };
    let config = match path {
        Some(path) => ConfigStore::load(path),
        None if args.mock => ConfigStore::in_memory(),
        None => {
            tracing::warn!("sem HOME nem XDG_CONFIG_HOME: configurações só em memória");
            ConfigStore::in_memory()
        }
    };
    let providers = if args.mock {
        info!("modo --mock: só o provedor falso");
        providers::mock_providers()
    } else {
        providers::system_providers()
    };

    let (daemon, signals) = Daemon::new(providers, config);
    let conn = zbus::connection::Builder::session()
        .context("barramento de sessão indisponível")?
        .name(DBUS_SERVICE)?
        .serve_at(DBUS_PATH, Service::new(daemon.clone()))?
        .build()
        .await
        .with_context(|| {
            format!("não foi possível publicar {DBUS_SERVICE} (outro raizad rodando?)")
        })?;
    info!(servico = DBUS_SERVICE, "no ar");

    // Os sinais vão para a D-Bus e para o vigia de bateria.
    let (dbus_tx, dbus_rx) = mpsc::unbounded_channel();
    let (battery_tx, battery_rx) = mpsc::unbounded_channel();
    let tee = tokio::spawn(async move {
        let mut signals = signals;
        while let Some(signal) = signals.recv().await {
            let _ = battery_tx.send(signal.clone());
            if dbus_tx.send(signal).is_err() {
                break;
            }
        }
    });
    let forwarder = tokio::spawn(forward_signals(conn.clone(), dbus_rx));
    let battery = tokio::spawn(battery::watch(
        daemon.clone(),
        battery_rx,
        DesktopNotifier::new(conn.clone()),
    ));
    let running = daemon.start();

    let mut sigterm = signal(SignalKind::terminate()).context("SIGTERM")?;
    let mut sigint = signal(SignalKind::interrupt()).context("SIGINT")?;
    tokio::select! {
        _ = sigterm.recv() => info!("SIGTERM recebido"),
        _ = sigint.recv() => info!("SIGINT recebido"),
    }

    // As configurações são gravadas a cada escrita: não há nada pendente para salvar.
    running.shutdown();
    forwarder.abort();
    battery.abort();
    tee.abort();
    if let Err(e) = conn.release_name(DBUS_SERVICE).await {
        tracing::debug!(error = %e, "falha ao liberar o nome");
    }
    info!("encerrado");
    Ok(())
}
