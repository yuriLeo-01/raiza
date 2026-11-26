//! Conversa com o daemon fora da thread da interface.
//!
//! [`run`] roda no runtime tokio do relm4 (o `zbus` usa tokio) e manda à interface só
//! mensagens prontas ([`SessionEvent`]). Ela acompanha o dono do nome D-Bus do daemon:
//! se o `raizad` sai, avisa e espera ele voltar; quando volta, sincroniza de novo.

use futures_util::StreamExt;
use raiza_core::dbus::{API_VERSION, ClientError, RaizaClient};
use raiza_core::{DeviceInfo, ProviderEvent};
use tracing::{debug, info, warn};

use crate::text::{self, Problem};

/// O que a sessão manda à interface.
#[derive(Debug)]
pub enum SessionEvent {
    /// Conectado e sincronizado: o cliente (para escritas) e a lista completa.
    Connected {
        /// Cliente do daemon.
        client: RaizaClient,
        /// Dispositivos já fundidos.
        devices: Vec<DeviceInfo>,
    },
    /// Um sinal do daemon.
    Event(ProviderEvent),
    /// Não foi possível falar com o daemon, ou ele saiu.
    Problem(Problem),
}

enum SyncError {
    Client(ClientError),
    Version(u32, String),
}

impl From<ClientError> for SyncError {
    fn from(err: ClientError) -> Self {
        SyncError::Client(err)
    }
}

/// Fluxo de eventos já assinado. Assinar antes de listar garante que nenhum evento
/// entre a lista e a assinatura se perca.
type Events = futures_util::stream::BoxStream<'static, Result<ProviderEvent, ClientError>>;

async fn sync(client: &RaizaClient) -> Result<(Vec<DeviceInfo>, Events), SyncError> {
    let (api, daemon) = client.version().await?;
    if api != API_VERSION {
        return Err(SyncError::Version(api, daemon));
    }
    let events = client.events().await?.boxed();
    let devices = client.list_devices().await?;
    info!(%daemon, dispositivos = devices.len(), "conectado ao raizad");
    Ok((devices, events))
}

/// Roda a sessão até `emit` devolver `false` (a interface fechou) ou a tarefa ser
/// abortada (botão "tentar de novo").
pub async fn run(emit: impl Fn(SessionEvent) -> bool + Send) {
    let client = match RaizaClient::connect_session().await {
        Ok(client) => client,
        Err(err) => {
            warn!(%err, "sem barramento de sessão");
            emit(SessionEvent::Problem(text::describe_connect_error(&err)));
            return;
        }
    };
    let mut owner = match client.proxy().inner().receive_owner_changed().await {
        Ok(stream) => stream,
        Err(err) => {
            let err = ClientError::from(err);
            warn!(%err, "não deu para acompanhar o nome do daemon");
            emit(SessionEvent::Problem(text::describe_connect_error(&err)));
            return;
        }
    };

    loop {
        // `true` quando o daemon já tem um dono novo e dá para sincronizar na hora.
        let mut owner_is_back = false;
        match sync(&client).await {
            Ok((devices, mut events)) => {
                let connected = SessionEvent::Connected {
                    client: client.clone(),
                    devices,
                };
                if !emit(connected) {
                    return;
                }
                loop {
                    tokio::select! {
                        event = events.next() => match event {
                            Some(Ok(event)) => {
                                if !emit(SessionEvent::Event(event)) {
                                    return;
                                }
                            }
                            Some(Err(err)) => warn!(%err, "sinal ilegível do daemon"),
                            None => {
                                warn!("o fluxo de sinais do daemon terminou");
                                break;
                            }
                        },
                        change = owner.next() => match change {
                            Some(None) => {
                                info!("o raizad saiu do barramento");
                                break;
                            }
                            Some(Some(name)) => {
                                debug!(%name, "o raizad trocou de dono");
                                owner_is_back = true;
                                break;
                            }
                            None => return,
                        },
                    }
                }
                if !owner_is_back && !emit(SessionEvent::Problem(text::daemon_down())) {
                    return;
                }
            }
            Err(SyncError::Version(api, daemon)) => {
                warn!(api, %daemon, "versão da API incompatível");
                if !emit(SessionEvent::Problem(text::version_mismatch(api, &daemon))) {
                    return;
                }
            }
            Err(SyncError::Client(err)) => {
                debug!(%err, "não conectou ao raizad");
                if !emit(SessionEvent::Problem(text::describe_connect_error(&err))) {
                    return;
                }
            }
        }
        // Espera o daemon (re)aparecer no barramento.
        while !owner_is_back {
            match owner.next().await {
                Some(Some(_)) => owner_is_back = true,
                Some(None) => {}
                None => return,
            }
        }
    }
}
