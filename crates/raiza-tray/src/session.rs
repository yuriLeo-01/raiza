//! Conversa com o `raizad`: sincroniza a lista, aplica os sinais e acompanha o dono do
//! nome D-Bus. Se o daemon sai, o ícone fica passivo e a sessão espera ele voltar.

use futures_util::StreamExt;
use raiza_core::ProviderEvent;
use raiza_core::dbus::{API_VERSION, ClientError, RaizaClient};
use tracing::{debug, info, warn};

use crate::model::Model;

/// Para onde vão as mudanças do modelo (no programa, o handle do `ksni`).
pub trait Sink {
    /// O modelo mudou. Devolve `false` se o ícone já encerrou.
    fn changed(&mut self, model: &Model) -> impl Future<Output = bool> + Send;
}

type Events = futures_util::stream::BoxStream<'static, Result<ProviderEvent, ClientError>>;

/// Assina os sinais antes de listar, para nenhum evento se perder no meio.
async fn sync(client: &RaizaClient, model: &mut Model) -> Result<Events, ClientError> {
    let (api, daemon) = client.version().await?;
    if api != API_VERSION {
        warn!(api, %daemon, esperado = API_VERSION, "versão da API do raizad incompatível");
    }
    let events = client.events().await?.boxed();
    let devices = client.list_devices().await?;
    info!(%daemon, dispositivos = devices.len(), "conectado ao raizad");
    model.connected(devices);
    Ok(events)
}

/// Roda até o barramento fechar ou `sink` dizer que o ícone encerrou.
pub async fn run(client: RaizaClient, mut sink: impl Sink) -> Result<(), ClientError> {
    let mut owner = client.proxy().inner().receive_owner_changed().await?;
    let mut model = Model::default();

    loop {
        // `true` quando o daemon já tem um dono novo e dá para sincronizar na hora.
        let mut owner_is_back = false;
        match sync(&client, &mut model).await {
            Ok(mut events) => {
                if !sink.changed(&model).await {
                    return Ok(());
                }
                loop {
                    tokio::select! {
                        event = events.next() => match event {
                            Some(Ok(event)) => {
                                debug!(?event, "sinal do raizad");
                                model.apply(event);
                                if !sink.changed(&model).await {
                                    return Ok(());
                                }
                            }
                            Some(Err(err)) => warn!(%err, "sinal ilegível do raizad"),
                            None => {
                                warn!("o fluxo de sinais do raizad terminou");
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
                            None => return Ok(()),
                        },
                    }
                }
            }
            Err(err) => warn!(%err, "não conectou ao raizad; esperando ele aparecer"),
        }
        model.disconnected();
        if !sink.changed(&model).await {
            return Ok(());
        }
        // Espera o daemon (re)aparecer no barramento.
        while !owner_is_back {
            match owner.next().await {
                Some(Some(_)) => owner_is_back = true,
                Some(None) => {}
                None => return Ok(()),
            }
        }
    }
}
