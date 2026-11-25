//! [`UpowerBackend`] real, sobre o D-Bus de sistema (`org.freedesktop.UPower`).

use async_trait::async_trait;
use futures_util::StreamExt;
use tracing::debug;
use zbus::Connection;
use zbus::message::Message;
use zbus::zvariant::OwnedObjectPath;

use crate::dbus::{self, LazyConnection};
use crate::props::{self, Props};
use crate::{BackendError, BackendStream, UpowerBackend, UpowerDevice, UpowerEvent};

const UPOWER: &str = "org.freedesktop.UPower";
const UPOWER_PATH: &str = "/org/freedesktop/UPower";
const DEVICE: &str = "org.freedesktop.UPower.Device";

/// UPower pelo D-Bus de sistema, via `zbus`.
#[derive(Debug, Default)]
pub struct ZbusUpower {
    conn: LazyConnection,
}

impl ZbusUpower {
    /// Usa o D-Bus de sistema, conectando no primeiro uso.
    pub fn new() -> Self {
        Self::default()
    }

    /// Usa uma conexão já aberta.
    pub fn with_connection(conn: Connection) -> Self {
        ZbusUpower {
            conn: LazyConnection::with(conn),
        }
    }
}

#[async_trait]
impl UpowerBackend for ZbusUpower {
    async fn devices(&self) -> Result<Vec<UpowerDevice>, BackendError> {
        let conn = self.conn.get().await?;
        let reply = dbus::call(conn, UPOWER, UPOWER_PATH, UPOWER, "EnumerateDevices").await?;
        let paths = reply
            .body()
            .deserialize::<Vec<OwnedObjectPath>>()
            .map_err(dbus::from_zbus)?;
        let mut devices = Vec::with_capacity(paths.len());
        for path in paths {
            // Um dispositivo com problema não impede os outros.
            match fetch(conn, path.as_str()).await {
                Ok(device) => devices.push(device),
                Err(e) => debug!(%path, "falha ao ler dispositivo do UPower: {e}"),
            }
        }
        Ok(devices)
    }

    async fn events(&self) -> Result<BackendStream<UpowerEvent>, BackendError> {
        let conn = self.conn.get().await?.clone();
        let signals = dbus::signals(
            &conn,
            UPOWER,
            &[
                (UPOWER_PATH, UPOWER, None),
                (
                    "/org/freedesktop/UPower/devices",
                    "org.freedesktop.DBus.Properties",
                    Some("PropertiesChanged"),
                ),
            ],
        )
        .await?;
        let events = signals.filter_map(move |msg| {
            let conn = conn.clone();
            async move { event_for(&conn, &msg).await }
        });
        Ok(Box::pin(events))
    }
}

/// Traduz um sinal do UPower, relendo o dispositivo quando preciso.
async fn event_for(conn: &Connection, msg: &Message) -> Option<UpowerEvent> {
    let member = dbus::member_of(msg)?;
    let body = msg.body();
    let path = match member.as_str() {
        "DeviceAdded" => body.deserialize::<OwnedObjectPath>().ok()?.to_string(),
        "DeviceRemoved" => {
            let path = body.deserialize::<OwnedObjectPath>().ok()?;
            return Some(UpowerEvent::Removed(path.to_string()));
        }
        "PropertiesChanged" => {
            let (iface, _, _) = body.deserialize::<(String, Props, Vec<String>)>().ok()?;
            if iface != DEVICE {
                return None;
            }
            dbus::path_of(msg)?
        }
        _ => return None,
    };
    match fetch(conn, &path).await {
        Ok(device) => Some(UpowerEvent::Changed(device)),
        Err(BackendError::Gone(_)) => Some(UpowerEvent::Removed(path)),
        Err(e) => {
            debug!(%path, "falha ao reler dispositivo do UPower: {e}");
            None
        }
    }
}

async fn fetch(conn: &Connection, path: &str) -> Result<UpowerDevice, BackendError> {
    let p = dbus::get_all(conn, UPOWER, path, DEVICE).await?;
    Ok(parse_device(path, &p))
}

fn parse_device(path: &str, p: &Props) -> UpowerDevice {
    UpowerDevice {
        path: path.to_owned(),
        native_path: props::string(p, "NativePath").unwrap_or_default(),
        kind: props::uint(p, "Type").unwrap_or(0),
        power_supply: props::boolean(p, "PowerSupply").unwrap_or(false),
        percentage: props::double(p, "Percentage").unwrap_or(0.0),
        state: props::uint(p, "State").unwrap_or(0),
        model: props::string(p, "Model"),
        vendor: props::string(p, "Vendor"),
    }
}
