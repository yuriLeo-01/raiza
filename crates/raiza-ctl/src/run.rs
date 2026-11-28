//! Executa os subcomandos contra o daemon (a parte com I/O).

use std::io::Write;

use anyhow::{Context, Result, anyhow};
use futures_util::{Stream, StreamExt};
use raiza_core::{
    CapabilityValue, DeviceId, ProviderEvent,
    dbus::{ClientError, RaizaClient},
};

use crate::{
    cli::{Command, completions},
    errors::describe,
    format,
};

/// Converte um erro do cliente em erro com mensagem para o usuário.
pub fn client_err(err: ClientError) -> anyhow::Error {
    anyhow!(describe(&err))
}

/// Executa `command` com `client`, escrevendo o resultado em `out`.
pub async fn run(client: &RaizaClient, command: Command, out: &mut dyn Write) -> Result<()> {
    match command {
        Command::List { json, all } => {
            let devices = client.list_devices().await.map_err(client_err)?;
            if json {
                writeln!(out, "{}", serde_json::to_string_pretty(&devices)?)?;
            } else {
                let (shown, hidden): (Vec<_>, Vec<_>) =
                    devices.into_iter().partition(|d| all || d.is_actionable());
                write!(out, "{}", format::format_list(&shown))?;
                if !hidden.is_empty() {
                    writeln!(
                        out,
                        "({} sem controles escondido(s); use --all para ver)",
                        hidden.len()
                    )?;
                }
            }
        }
        Command::Get { id, json } => {
            let device = client.get_device(&id).await.map_err(client_err)?;
            if json {
                writeln!(out, "{}", serde_json::to_string_pretty(&device)?)?;
            } else {
                write!(out, "{}", format::format_device(&device))?;
            }
        }
        Command::Set { id, value } => {
            let value = value.to_value().map_err(|e| anyhow!(e))?;
            client
                .set_capability(&id, &value)
                .await
                .map_err(client_err)?;
            writeln!(out, "ok: {}", format_set(&id, &value))?;
        }
        Command::Watch { json } => {
            let events = client.events().await.map_err(client_err)?;
            watch(events, json, out).await?;
        }
        Command::Completions { shell } => completions(shell, out)?,
    }
    Ok(())
}

/// Resumo do que foi alterado, para confirmar o `set`.
fn format_set(id: &DeviceId, value: &CapabilityValue) -> String {
    let what = match value {
        CapabilityValue::Dpi(v) => format!("dpi {v}"),
        CapabilityValue::DpiStage { stage, value } => format!("dpi-stage {stage} {value}"),
        CapabilityValue::PollingRate(v) => format!("polling-rate {v} Hz"),
        CapabilityValue::ScanRate(v) => format!("scan-rate {v} Hz"),
        CapabilityValue::Volume(v) => format!("volume {v}%"),
        CapabilityValue::Mute(v) => format!("mute {}", on_off(*v)),
        CapabilityValue::Connection(v) => format!("connection {}", on_off(*v)),
        CapabilityValue::Rgb(s) => {
            let mut parts = vec!["rgb".to_owned()];
            if let Some(zone) = s.zone {
                parts.push(format!("zona {zone}"));
            }
            if let Some(mode) = &s.mode {
                parts.push(format!("modo {mode}"));
            }
            if let Some(color) = s.color {
                parts.push(format!("cor {color}"));
            }
            parts.join(" ")
        }
        CapabilityValue::Rumble {
            strength,
            duration_ms,
        } => format!("rumble {strength}% por {duration_ms} ms"),
    };
    format!("{id} {what}")
}

fn on_off(b: bool) -> &'static str {
    if b { "on" } else { "off" }
}

/// Imprime um evento por linha até o fluxo acabar. Sinais ilegíveis viram aviso no
/// stderr e o fluxo continua.
pub async fn watch(
    events: impl Stream<Item = Result<ProviderEvent, ClientError>>,
    json: bool,
    out: &mut dyn Write,
) -> Result<()> {
    futures_util::pin_mut!(events);
    while let Some(item) = events.next().await {
        match item {
            Ok(event) => {
                let line = if json {
                    format::event_json(&event).context("falha ao gerar o JSON do evento")?
                } else {
                    format::format_event(&event)
                };
                writeln!(out, "{line}")?;
                out.flush()?;
            }
            Err(e) => eprintln!("aviso: {}", describe(&e)),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use raiza_core::{
        Capability, RaizaError,
        dbus::{DBUS_PATH, from_wire, parse_id, to_wire},
    };
    use zbus::object_server::SignalEmitter;

    use super::*;
    use crate::{cli::Cli, format::tests::mouse};

    /// Daemon falso, só para exercitar o fio. O de verdade é o `raizad` (T04).
    struct Falso {
        devices: Vec<raiza_core::DeviceInfo>,
    }

    #[zbus::interface(name = "io.github.yurileo01.Raiza1")]
    impl Falso {
        fn list_devices(&self) -> Result<String, raiza_core::dbus::DbusError> {
            to_wire(&self.devices)
        }

        fn get_device(&self, id: &str) -> Result<String, raiza_core::dbus::DbusError> {
            let id = parse_id(id)?;
            let d = self
                .devices
                .iter()
                .find(|d| d.id == id)
                .ok_or(RaizaError::NotFound(id))?;
            to_wire(d)
        }

        async fn set_capability(
            &mut self,
            id: &str,
            value: &str,
            #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        ) -> Result<(), raiza_core::dbus::DbusError> {
            let id = parse_id(id)?;
            let value: CapabilityValue = from_wire(value)?;
            let d = self
                .devices
                .iter_mut()
                .find(|d| d.id == id)
                .ok_or_else(|| RaizaError::NotFound(id.clone()))?;
            let cap = d
                .capability_mut(value.kind())
                .ok_or(RaizaError::Unsupported(value.kind()))?;
            cap.apply(&value)?;
            let json = to_wire(cap)?;
            Self::capability_changed(&emitter, &id.to_string(), &json).await?;
            Ok(())
        }

        fn version(&self) -> (u32, String) {
            (raiza_core::dbus::API_VERSION, "teste".into())
        }

        #[zbus(signal)]
        async fn capability_changed(
            emitter: &SignalEmitter<'_>,
            id: &str,
            capability: &str,
        ) -> zbus::Result<()>;
    }

    async fn par() -> (zbus::Connection, RaizaClient) {
        let (a, b) = tokio::net::UnixStream::pair().unwrap();
        let servidor = zbus::connection::Builder::unix_stream(a)
            .server(zbus::Guid::generate())
            .unwrap()
            .p2p()
            .serve_at(
                DBUS_PATH,
                Falso {
                    devices: vec![mouse()],
                },
            )
            .unwrap()
            .build();
        let cliente = zbus::connection::Builder::unix_stream(b).p2p().build();
        let (servidor, cliente) = tokio::join!(servidor, cliente);
        let client = RaizaClient::new(&cliente.unwrap()).await.unwrap();
        (servidor.unwrap(), client)
    }

    /// Roda `raiza-ctl <args>` contra o servidor falso. Devolve a saída ou a mensagem de erro.
    async fn ctl(client: &RaizaClient, args: &[&str]) -> Result<String, String> {
        let mut argv = vec!["raiza-ctl"];
        argv.extend_from_slice(args);
        let cli = Cli::try_parse_from(argv).unwrap();
        let mut out = Vec::new();
        run(client, cli.command, &mut out)
            .await
            .map_err(|e| e.to_string())?;
        Ok(String::from_utf8(out).unwrap())
    }

    #[tokio::test]
    async fn ponta_a_ponta() {
        let (_servidor, client) = par().await;
        let id = "usb:24ae:1423:0";

        // list: tabela e JSON.
        let tabela = ctl(&client, &["list"]).await.unwrap();
        assert_eq!(tabela, format::format_list(&[mouse()]));
        assert!(tabela.contains("Rapoo VT9") && tabela.contains("DPI 1600"));
        let json = ctl(&client, &["list", "--json"]).await.unwrap();
        let lido: Vec<raiza_core::DeviceInfo> = serde_json::from_str(&json).unwrap();
        assert_eq!(lido, [mouse()]);

        // get: texto e JSON.
        let texto = ctl(&client, &["get", id]).await.unwrap();
        assert_eq!(texto, format::format_device(&mouse()));
        let json = ctl(&client, &["get", id, "--json"]).await.unwrap();
        assert_eq!(
            serde_json::from_str::<raiza_core::DeviceInfo>(&json).unwrap(),
            mouse()
        );

        // set com sucesso: o valor muda de verdade no servidor.
        let ok = ctl(&client, &["set", id, "dpi", "800"]).await.unwrap();
        assert_eq!(ok, "ok: usb:24ae:1423:0 dpi 800\n");
        let texto = ctl(&client, &["get", id]).await.unwrap();
        assert!(texto.contains("DPI: 800"));
        let ok = ctl(
            &client,
            &["set", id, "rgb", "--mode", "wave", "--color", "#00ff00"],
        )
        .await
        .unwrap();
        assert_eq!(ok, "ok: usb:24ae:1423:0 rgb modo wave cor #00ff00\n");

        // set com valor fora dos limites (1625 não é múltiplo do passo 50).
        let err = ctl(&client, &["set", id, "dpi", "1625"]).await.unwrap_err();
        assert!(
            err.contains("fora dos limites") && err.contains("dpi"),
            "{err}"
        );
        let err = ctl(&client, &["set", id, "dpi", "1"]).await.unwrap_err();
        assert!(err.contains("fora dos limites"), "{err}");

        // Somente leitura, não suportado e dispositivo inexistente.
        let err = ctl(&client, &["set", id, "polling-rate", "500"])
            .await
            .unwrap_err();
        assert!(err.contains("somente leitura"), "{err}");
        let err = ctl(&client, &["set", id, "volume", "10"])
            .await
            .unwrap_err();
        assert!(err.contains("não tem a capacidade volume"), "{err}");
        let err = ctl(&client, &["get", "usb:0001:0002:9"]).await.unwrap_err();
        assert!(
            err.contains("não encontrado") && err.contains("usb:0001:0002:9"),
            "{err}"
        );
    }

    #[tokio::test]
    async fn watch_mostra_eventos() {
        let (_servidor, client) = par().await;
        let id: DeviceId = "usb:24ae:1423:0".parse().unwrap();

        // Dois eventos: um em texto e um em JSON, cada um por uma alteração.
        for (json, esperado) in [(false, "~ usb:24ae:1423:0 dpi 3200\n"), (true, "")] {
            let eventos = client.events().await.unwrap();
            client
                .set_capability(&id, &CapabilityValue::Dpi(3200))
                .await
                .unwrap();
            let mut out = Vec::new();
            watch(eventos.take(1), json, &mut out).await.unwrap();
            let saida = String::from_utf8(out).unwrap();
            if json {
                let v: serde_json::Value = serde_json::from_str(saida.trim_end()).unwrap();
                assert_eq!(v["event"], "capability_changed");
                assert_eq!(v["id"], "usb:24ae:1423:0");
                let cap: Capability = serde_json::from_value(v["capability"].clone()).unwrap();
                assert_eq!(cap.kind(), raiza_core::CapabilityKind::Dpi);
            } else {
                assert_eq!(saida, esperado);
            }
        }
    }
}
