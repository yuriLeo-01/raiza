//! Utilidades comuns às implementações zbus dos backends.

use std::collections::HashMap;

use futures_util::{Stream, StreamExt};
use tokio::sync::OnceCell;
use zbus::message::Message;
use zbus::zvariant::OwnedObjectPath;
use zbus::{Connection, MatchRule, MessageStream};

use crate::BackendError;
use crate::props::Props;

/// Resposta de `GetManagedObjects`: caminho → interface → propriedades.
pub(crate) type ManagedObjects = HashMap<OwnedObjectPath, HashMap<String, Props>>;

/// Conexão com o D-Bus de sistema, aberta no primeiro uso.
#[derive(Debug, Default)]
pub(crate) struct LazyConnection(OnceCell<Connection>);

impl LazyConnection {
    pub fn with(conn: Connection) -> Self {
        LazyConnection(OnceCell::new_with(Some(conn)))
    }

    pub async fn get(&self) -> Result<&Connection, BackendError> {
        self.0
            .get_or_try_init(Connection::system)
            .await
            .map_err(|e| BackendError::Unavailable(format!("D-Bus de sistema: {e}")))
    }
}

/// Chama um método sem argumentos.
pub(crate) async fn call(
    conn: &Connection,
    dest: &str,
    path: &str,
    iface: &str,
    method: &str,
) -> Result<Message, BackendError> {
    conn.call_method(Some(dest), path, Some(iface), method, &())
        .await
        .map_err(from_zbus)
}

/// `org.freedesktop.DBus.Properties.GetAll`.
pub(crate) async fn get_all(
    conn: &Connection,
    dest: &str,
    path: &str,
    iface: &str,
) -> Result<Props, BackendError> {
    let reply = conn
        .call_method(
            Some(dest),
            path,
            Some("org.freedesktop.DBus.Properties"),
            "GetAll",
            &(iface,),
        )
        .await
        .map_err(from_zbus)?;
    reply.body().deserialize::<Props>().map_err(from_zbus)
}

/// `org.freedesktop.DBus.ObjectManager.GetManagedObjects` no objeto `/`.
pub(crate) async fn managed_objects(
    conn: &Connection,
    dest: &str,
) -> Result<ManagedObjects, BackendError> {
    let reply = call(
        conn,
        dest,
        "/",
        "org.freedesktop.DBus.ObjectManager",
        "GetManagedObjects",
    )
    .await?;
    reply
        .body()
        .deserialize::<ManagedObjects>()
        .map_err(from_zbus)
}

/// Sinais de `sender` que casam com a regra, fundidos num fluxo só.
///
/// `rules` recebe pares `(caminho, interface, membro)`; o caminho é um *namespace*.
pub(crate) async fn signals(
    conn: &Connection,
    sender: &str,
    rules: &[(&str, &str, Option<&str>)],
) -> Result<impl Stream<Item = Message> + Send + 'static, BackendError> {
    let mut streams = Vec::new();
    for (path, iface, member) in rules {
        let mut rule = MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(sender)
            .and_then(|b| b.interface(*iface))
            .and_then(|b| b.path_namespace(*path))
            .map_err(from_zbus)?;
        if let Some(member) = member {
            rule = rule.member(*member).map_err(from_zbus)?;
        }
        let stream = MessageStream::for_match_rule(rule.build(), conn, None)
            .await
            .map_err(from_zbus)?;
        streams.push(stream);
    }
    Ok(futures_util::stream::select_all(streams).filter_map(|msg| async move { msg.ok() }))
}

/// Classifica um erro do zbus.
pub(crate) fn from_zbus(err: zbus::Error) -> BackendError {
    let name = match &err {
        zbus::Error::MethodError(name, _, _) => Some(name.as_str().to_owned()),
        zbus::Error::FDO(fdo) => Some(fdo_name(fdo)),
        _ => None,
    };
    match name.as_deref() {
        Some(
            "org.freedesktop.DBus.Error.ServiceUnknown"
            | "org.freedesktop.DBus.Error.NameHasNoOwner",
        ) => BackendError::Unavailable(err.to_string()),
        Some(
            "org.freedesktop.DBus.Error.UnknownObject"
            | "org.freedesktop.DBus.Error.UnknownInterface"
            | "org.freedesktop.DBus.Error.UnknownMethod"
            | "org.bluez.Error.DoesNotExist",
        ) => BackendError::Gone(err.to_string()),
        _ => BackendError::Other(err.to_string()),
    }
}

fn fdo_name(err: &zbus::fdo::Error) -> String {
    use zbus::fdo::Error;
    let short = match err {
        Error::ServiceUnknown(_) => "ServiceUnknown",
        Error::NameHasNoOwner(_) => "NameHasNoOwner",
        Error::UnknownObject(_) => "UnknownObject",
        Error::UnknownInterface(_) => "UnknownInterface",
        Error::UnknownMethod(_) => "UnknownMethod",
        _ => "Failed",
    };
    format!("org.freedesktop.DBus.Error.{short}")
}

/// Caminho do objeto que emitiu a mensagem.
pub(crate) fn path_of(msg: &Message) -> Option<String> {
    msg.header().path().map(ToString::to_string)
}

/// Nome do membro (sinal) da mensagem.
pub(crate) fn member_of(msg: &Message) -> Option<String> {
    msg.header().member().map(ToString::to_string)
}
