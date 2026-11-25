//! raiza-ctl: cliente de linha de comando do daemon `raizad`.
//!
//! A lógica fica separada do I/O: `cli` (comandos e conversão para `CapabilityValue`),
//! `format` (texto de saída), `errors` (mensagens) e `run` (conversa com o daemon).

mod cli;
mod errors;
mod format;
mod run;

use std::{io::Write, process::ExitCode};

use anyhow::Result;
use clap::Parser;
use raiza_core::dbus::{API_VERSION, RaizaClient};

use cli::{Cli, Command};
use run::client_err;

/// Conecta ao daemon e confere a versão da API.
async fn connect() -> Result<RaizaClient> {
    let client = RaizaClient::connect_session().await.map_err(client_err)?;
    // Também serve de teste de vida: sem daemon, a chamada falha com uma mensagem clara
    // (o `watch` sozinho ficaria esperando para sempre).
    let (api, _daemon) = client.version().await.map_err(client_err)?;
    if api != API_VERSION {
        anyhow::bail!(
            "o daemon fala a versão {api} da API, mas este raiza-ctl fala a {API_VERSION}; atualize os dois"
        );
    }
    Ok(client)
}

async fn real_main(cli: Cli) -> Result<()> {
    let mut out = std::io::stdout().lock();
    if let Command::Completions { shell } = cli.command {
        return Ok(cli::completions(shell, &mut out)?);
    }
    let client = connect().await?;
    run::run(&client, cli.command, &mut out).await?;
    out.flush()?;
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match real_main(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            // `raiza-ctl watch | head`: o leitor fechou o pipe, não é falha.
            let pipe_fechado = err.chain().any(|e| {
                e.downcast_ref::<std::io::Error>()
                    .is_some_and(|io| io.kind() == std::io::ErrorKind::BrokenPipe)
            });
            if pipe_fechado {
                return ExitCode::SUCCESS;
            }
            eprintln!("raiza-ctl: {err:#}");
            ExitCode::FAILURE
        }
    }
}
