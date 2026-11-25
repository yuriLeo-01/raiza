//! Interface D-Bus `io.github.yurileo01.Raiza1` (`docs/DBUS.md`).

use raiza_core::dbus::{API_VERSION, DBUS_PATH, DbusError, from_wire, parse_id, to_wire};
use raiza_core::{CapabilityValue, DeviceId};
use tokio::sync::mpsc;
use tracing::warn;
use zbus::object_server::SignalEmitter;

use crate::daemon::{Daemon, Signal};

/// Objeto D-Bus servido em [`DBUS_PATH`].
pub struct Service {
    daemon: Daemon,
}

impl Service {
    /// Serviço sobre o daemon dado.
    pub fn new(daemon: Daemon) -> Self {
        Service { daemon }
    }
}

#[zbus::interface(name = "io.github.yurileo01.Raiza1")]
impl Service {
    /// `ListDevices() -> s`: JSON de `[DeviceInfo]`, já fundidos.
    fn list_devices(&self) -> Result<String, DbusError> {
        to_wire(&self.daemon.list_devices())
    }

    /// `GetDevice(s id) -> s`: JSON de `DeviceInfo`.
    fn get_device(&self, id: &str) -> Result<String, DbusError> {
        let id = parse_id(id)?;
        to_wire(&self.daemon.get_device(&id)?)
    }

    /// `SetCapability(s id, s value)`: valida e manda ao provedor dono da capacidade.
    async fn set_capability(&self, id: &str, value: &str) -> Result<(), DbusError> {
        let id = parse_id(id)?;
        let value: CapabilityValue = from_wire(value)?;
        Ok(self.daemon.set_capability(&id, value).await?)
    }

    /// `Version() -> (u api, s daemon)`.
    fn version(&self) -> (u32, String) {
        (API_VERSION, env!("CARGO_PKG_VERSION").to_owned())
    }

    /// Sinal `DeviceAdded(s device)`.
    #[zbus(signal)]
    async fn device_added(emitter: &SignalEmitter<'_>, device: &str) -> zbus::Result<()>;

    /// Sinal `DeviceRemoved(s id)`.
    #[zbus(signal)]
    async fn device_removed(emitter: &SignalEmitter<'_>, id: &str) -> zbus::Result<()>;

    /// Sinal `CapabilityChanged(s id, s capability)`.
    #[zbus(signal)]
    async fn capability_changed(
        emitter: &SignalEmitter<'_>,
        id: &str,
        capability: &str,
    ) -> zbus::Result<()>;
}

/// Repassa os [`Signal`]s do daemon como sinais D-Bus, em ordem, até o canal fechar.
pub async fn forward_signals(conn: zbus::Connection, mut signals: mpsc::UnboundedReceiver<Signal>) {
    let emitter = match SignalEmitter::new(&conn, DBUS_PATH) {
        Ok(emitter) => emitter,
        Err(e) => {
            warn!(error = %e, "caminho D-Bus inválido; sinais desligados");
            return;
        }
    };
    while let Some(signal) = signals.recv().await {
        if let Err(e) = emit(&emitter, &signal).await {
            warn!(error = %e, ?signal, "falha ao emitir sinal");
        }
    }
}

async fn emit(emitter: &SignalEmitter<'_>, signal: &Signal) -> Result<(), DbusError> {
    let id_str = |id: &DeviceId| id.to_string();
    match signal {
        Signal::DeviceAdded(device) => Service::device_added(emitter, &to_wire(device)?).await?,
        Signal::DeviceRemoved(id) => Service::device_removed(emitter, &id_str(id)).await?,
        Signal::CapabilityChanged(id, cap) => {
            Service::capability_changed(emitter, &id_str(id), &to_wire(cap)?).await?
        }
    }
    Ok(())
}
