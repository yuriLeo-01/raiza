//! [`BluezBackend`] real, sobre o D-Bus de sistema (`org.bluez`).

use async_trait::async_trait;
use futures_util::StreamExt;
use tracing::debug;
use zbus::Connection;
use zbus::message::Message;
use zbus::zvariant::OwnedObjectPath;

use crate::dbus::{self, LazyConnection};
use crate::props::{self, Props};
use crate::{BackendError, BackendStream, BluezBackend, BluezDevice, BluezEvent};

const BLUEZ: &str = "org.bluez";
const DEVICE: &str = "org.bluez.Device1";
const BATTERY: &str = "org.bluez.Battery1";

/// BlueZ pelo D-Bus de sistema, via `zbus`.
#[derive(Debug, Default)]
pub struct ZbusBluez {
    conn: LazyConnection,
}

impl ZbusBluez {
    /// Usa o D-Bus de sistema, conectando no primeiro uso.
    pub fn new() -> Self {
        Self::default()
    }

    /// Usa uma conexão já aberta.
    pub fn with_connection(conn: Connection) -> Self {
        ZbusBluez {
            conn: LazyConnection::with(conn),
        }
    }
}

#[async_trait]
impl BluezBackend for ZbusBluez {
    async fn devices(&self) -> Result<Vec<BluezDevice>, BackendError> {
        let conn = self.conn.get().await?;
        let objects = dbus::managed_objects(conn, BLUEZ).await?;
        Ok(objects
            .iter()
            .filter_map(|(path, ifaces)| {
                parse_device(path.as_str(), ifaces.get(DEVICE)?, ifaces.get(BATTERY))
            })
            .collect())
    }

    async fn connect(&self, path: &str) -> Result<(), BackendError> {
        let conn = self.conn.get().await?;
        dbus::call(conn, BLUEZ, path, DEVICE, "Connect").await?;
        Ok(())
    }

    async fn disconnect(&self, path: &str) -> Result<(), BackendError> {
        let conn = self.conn.get().await?;
        dbus::call(conn, BLUEZ, path, DEVICE, "Disconnect").await?;
        Ok(())
    }

    async fn events(&self) -> Result<BackendStream<BluezEvent>, BackendError> {
        let conn = self.conn.get().await?.clone();
        let signals = dbus::signals(
            &conn,
            BLUEZ,
            &[
                ("/", "org.freedesktop.DBus.ObjectManager", None),
                (
                    "/org/bluez",
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

/// Traduz um sinal do BlueZ, relendo o dispositivo quando preciso.
async fn event_for(conn: &Connection, msg: &Message) -> Option<BluezEvent> {
    let member = dbus::member_of(msg)?;
    let body = msg.body();
    let (path, refetch) = match member.as_str() {
        "PropertiesChanged" => {
            let (iface, _, _) = body.deserialize::<(String, Props, Vec<String>)>().ok()?;
            (dbus::path_of(msg)?, iface == DEVICE || iface == BATTERY)
        }
        "InterfacesAdded" => {
            let (path, ifaces) = body
                .deserialize::<(OwnedObjectPath, std::collections::HashMap<String, Props>)>()
                .ok()?;
            let relevant = ifaces.contains_key(DEVICE) || ifaces.contains_key(BATTERY);
            (path.to_string(), relevant)
        }
        "InterfacesRemoved" => {
            let (path, ifaces) = body.deserialize::<(OwnedObjectPath, Vec<String>)>().ok()?;
            if ifaces.iter().any(|i| i == DEVICE) {
                return Some(BluezEvent::Removed(path.to_string()));
            }
            (path.to_string(), ifaces.iter().any(|i| i == BATTERY))
        }
        _ => return None,
    };
    if !refetch {
        return None;
    }
    match fetch(conn, &path).await {
        Ok(device) => device.map(BluezEvent::Changed),
        Err(BackendError::Gone(_)) => Some(BluezEvent::Removed(path)),
        Err(e) => {
            debug!(%path, "falha ao reler dispositivo do BlueZ: {e}");
            None
        }
    }
}

/// Lê `Device1` e `Battery1` de um objeto. Objeto que não é dispositivo: `Ok(None)`.
async fn fetch(conn: &Connection, path: &str) -> Result<Option<BluezDevice>, BackendError> {
    let device = dbus::get_all(conn, BLUEZ, path, DEVICE).await?;
    let battery = dbus::get_all(conn, BLUEZ, path, BATTERY).await.ok();
    Ok(parse_device(path, &device, battery.as_ref()))
}

fn parse_device(path: &str, device: &Props, battery: Option<&Props>) -> Option<BluezDevice> {
    Some(BluezDevice {
        path: path.to_owned(),
        address: props::string(device, "Address")?,
        name: props::string(device, "Name"),
        alias: props::string(device, "Alias"),
        icon: props::string(device, "Icon"),
        paired: props::boolean(device, "Paired").unwrap_or(false),
        connected: props::boolean(device, "Connected").unwrap_or(false),
        battery: battery.and_then(|b| props::byte(b, "Percentage")),
    })
}
