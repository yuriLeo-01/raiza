//! API D-Bus do Raiza (feature `dbus`): constantes, erros nomeados, proxy e cliente tipado.
//!
//! O contrato completo está em `docs/DBUS.md`. Resumo do formato no fio: ids e tipos
//! simples vão como tipos D-Bus nativos; dispositivos, capacidades e valores vão como
//! **JSON em string** (`s`), no mesmo formato serde dos tipos deste crate.

use futures_core::Stream;
use serde::{Serialize, de::DeserializeOwned};

use crate::{
    Capability, CapabilityKind, CapabilityValue, DeviceId, DeviceInfo, ProviderEvent, RaizaError,
};

/// Caminho do objeto do daemon.
pub const DBUS_PATH: &str = "/io/github/yurileo01/Raiza";
/// Interface versionada do daemon.
pub const DBUS_INTERFACE: &str = "io.github.yurileo01.Raiza1";
/// Prefixo dos nomes de erro D-Bus.
pub const DBUS_ERROR_PREFIX: &str = "io.github.yurileo01.Raiza1.Error";
/// Versão da API. Muda só com quebra de compatibilidade (junto com o nome da interface).
pub const API_VERSION: u32 = 1;

/// Erros D-Bus nomeados da interface `io.github.yurileo01.Raiza1`.
///
/// A mensagem de cada erro pode ser lida por máquina, para o cliente reconstruir o
/// [`RaizaError`] original (veja [`DbusError::into_raiza`]):
///
/// | Erro | Mensagem |
/// |---|---|
/// | `NotFound` | o id do dispositivo |
/// | `Unsupported`, `ReadOnly` | o tipo de capacidade (`dpi`, `volume`...) |
/// | `OutOfRange` | `tipo: explicação` |
/// | `Io`, `Permission`, `InvalidArgument` | texto livre |
#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "io.github.yurileo01.Raiza1.Error")]
pub enum DbusError {
    /// Erro do próprio transporte D-Bus.
    #[zbus(error)]
    ZBus(zbus::Error),
    /// Veja [`RaizaError::NotFound`].
    NotFound(String),
    /// Veja [`RaizaError::Unsupported`].
    Unsupported(String),
    /// Veja [`RaizaError::ReadOnly`].
    ReadOnly(String),
    /// Veja [`RaizaError::OutOfRange`].
    OutOfRange(String),
    /// Veja [`RaizaError::Io`].
    Io(String),
    /// Veja [`RaizaError::Permission`].
    Permission(String),
    /// O cliente mandou um id ou JSON que não pôde ser lido.
    InvalidArgument(String),
}

impl From<RaizaError> for DbusError {
    fn from(err: RaizaError) -> Self {
        match err {
            RaizaError::NotFound(id) => DbusError::NotFound(id.to_string()),
            RaizaError::Unsupported(kind) => DbusError::Unsupported(kind.to_string()),
            RaizaError::ReadOnly(kind) => DbusError::ReadOnly(kind.to_string()),
            RaizaError::OutOfRange { kind, detail } => {
                DbusError::OutOfRange(format!("{kind}: {detail}"))
            }
            RaizaError::Io(msg) => DbusError::Io(msg),
            RaizaError::Permission(msg) => DbusError::Permission(msg),
        }
    }
}

impl DbusError {
    /// Reconstrói o [`RaizaError`] a partir do erro nomeado. Devolve `Err(self)` para
    /// `ZBus`, `InvalidArgument` ou mensagens que não puderam ser lidas.
    pub fn into_raiza(self) -> Result<RaizaError, DbusError> {
        let kind = |s: &str| s.parse::<CapabilityKind>().ok();
        let converted = match &self {
            DbusError::NotFound(id) => id.parse().ok().map(RaizaError::NotFound),
            DbusError::Unsupported(k) => kind(k).map(RaizaError::Unsupported),
            DbusError::ReadOnly(k) => kind(k).map(RaizaError::ReadOnly),
            DbusError::OutOfRange(msg) => msg.split_once(": ").and_then(|(k, detail)| {
                kind(k).map(|kind| RaizaError::OutOfRange {
                    kind,
                    detail: detail.to_owned(),
                })
            }),
            DbusError::Io(msg) => Some(RaizaError::Io(msg.clone())),
            DbusError::Permission(msg) => Some(RaizaError::Permission(msg.clone())),
            DbusError::ZBus(_) | DbusError::InvalidArgument(_) => None,
        };
        converted.ok_or(self)
    }
}

/// Serializa para o formato do fio (JSON).
pub fn to_wire<T: Serialize>(value: &T) -> Result<String, DbusError> {
    serde_json::to_string(value).map_err(|e| DbusError::Io(format!("falha ao serializar: {e}")))
}

/// Lê do formato do fio (JSON). Erro vira [`DbusError::InvalidArgument`], o que o
/// daemon deve devolver quando o cliente manda algo ilegível.
pub fn from_wire<T: DeserializeOwned>(json: &str) -> Result<T, DbusError> {
    serde_json::from_str(json).map_err(|e| DbusError::InvalidArgument(e.to_string()))
}

/// Lê um id de dispositivo vindo do cliente. Erro vira [`DbusError::InvalidArgument`].
pub fn parse_id(id: &str) -> Result<DeviceId, DbusError> {
    id.parse()
        .map_err(|e: crate::ParseDeviceIdError| DbusError::InvalidArgument(e.to_string()))
}

pub use proxy::*;

/// Proxy gerado pelo `zbus` (os itens gerados não têm doc comment próprio).
#[allow(missing_docs)]
mod proxy {
    use super::DbusError;

    /// Proxy D-Bus de baixo nível (gera `RaizaProxy`). Prefira [`super::RaizaClient`], que já
    /// converte o JSON e os erros.
    #[zbus::proxy(
        interface = "io.github.yurileo01.Raiza1",
        default_service = "io.github.yurileo01.Raiza",
        default_path = "/io/github/yurileo01/Raiza",
        gen_blocking = false
    )]
    pub trait Raiza {
        /// `ListDevices() -> s`: JSON de `Vec<DeviceInfo>` (dispositivos já fundidos).
        fn list_devices(&self) -> Result<String, DbusError>;

        /// `GetDevice(s id) -> s`: JSON de `DeviceInfo`.
        fn get_device(&self, id: &str) -> Result<String, DbusError>;

        /// `SetCapability(s id, s value)`: `value` é JSON de `CapabilityValue`.
        fn set_capability(&self, id: &str, value: &str) -> Result<(), DbusError>;

        /// `Version() -> (u api, s daemon)`.
        fn version(&self) -> Result<(u32, String), DbusError>;

        /// Sinal `DeviceAdded(s device)`: JSON de `DeviceInfo`.
        #[zbus(signal)]
        fn device_added(&self, device: &str) -> zbus::Result<()>;

        /// Sinal `DeviceRemoved(s id)`.
        #[zbus(signal)]
        fn device_removed(&self, id: &str) -> zbus::Result<()>;

        /// Sinal `CapabilityChanged(s id, s capability)`: JSON de `Capability`.
        #[zbus(signal)]
        fn capability_changed(&self, id: &str, capability: &str) -> zbus::Result<()>;
    }
}

/// Erro do lado do cliente.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// O daemon recusou o pedido com um erro do Raiza.
    #[error(transparent)]
    Raiza(#[from] RaizaError),
    /// Erro de transporte D-Bus (daemon fora do ar, permissão...) ou erro não reconhecido.
    #[error("erro de D-Bus: {0}")]
    Bus(DbusError),
    /// O daemon mandou um JSON que este cliente não entende (versões diferentes?).
    #[error("resposta inválida do daemon: {0}")]
    Decode(String),
}

impl From<DbusError> for ClientError {
    fn from(err: DbusError) -> Self {
        match err.into_raiza() {
            Ok(raiza) => ClientError::Raiza(raiza),
            Err(other) => ClientError::Bus(other),
        }
    }
}

impl From<zbus::Error> for ClientError {
    fn from(err: zbus::Error) -> Self {
        DbusError::from(err).into()
    }
}

fn decode<T: DeserializeOwned>(json: &str) -> Result<T, ClientError> {
    serde_json::from_str(json).map_err(|e| ClientError::Decode(e.to_string()))
}

/// Cliente tipado da API D-Bus, para o `raiza-ctl` e as interfaces.
#[derive(Debug, Clone)]
pub struct RaizaClient {
    proxy: RaizaProxy<'static>,
}

impl RaizaClient {
    /// Conecta ao daemon no barramento de sessão.
    pub async fn connect_session() -> Result<Self, ClientError> {
        let conn = zbus::Connection::session().await?;
        Self::new(&conn).await
    }

    /// Cria o cliente sobre uma conexão já aberta.
    pub async fn new(conn: &zbus::Connection) -> Result<Self, ClientError> {
        Ok(RaizaClient {
            proxy: RaizaProxy::new(conn).await?,
        })
    }

    /// O proxy de baixo nível.
    pub fn proxy(&self) -> &RaizaProxy<'static> {
        &self.proxy
    }

    /// Lista os dispositivos (já fundidos).
    pub async fn list_devices(&self) -> Result<Vec<DeviceInfo>, ClientError> {
        decode(&self.proxy.list_devices().await?)
    }

    /// Um dispositivo pelo id.
    pub async fn get_device(&self, id: &DeviceId) -> Result<DeviceInfo, ClientError> {
        decode(&self.proxy.get_device(&id.to_string()).await?)
    }

    /// Altera uma capacidade. O daemon valida o valor antes de mandar ao provedor.
    pub async fn set_capability(
        &self,
        id: &DeviceId,
        value: &CapabilityValue,
    ) -> Result<(), ClientError> {
        let json = serde_json::to_string(value).map_err(|e| ClientError::Decode(e.to_string()))?;
        Ok(self.proxy.set_capability(&id.to_string(), &json).await?)
    }

    /// `(versão da API, versão do daemon)`.
    pub async fn version(&self) -> Result<(u32, String), ClientError> {
        Ok(self.proxy.version().await?)
    }

    /// Fluxo com os três sinais do daemon convertidos em [`ProviderEvent`].
    /// Sinais ilegíveis viram `Err(ClientError::Decode)` sem encerrar o fluxo.
    pub async fn events(
        &self,
    ) -> Result<impl Stream<Item = Result<ProviderEvent, ClientError>> + use<>, ClientError> {
        use futures_util::StreamExt;

        let added = self.proxy.receive_device_added().await?.map(|s| {
            let device = decode::<DeviceInfo>(s.args()?.device)?;
            Ok(ProviderEvent::DeviceAdded { device })
        });
        let removed = self.proxy.receive_device_removed().await?.map(|s| {
            let id = s
                .args()?
                .id
                .parse()
                .map_err(|e: crate::ParseDeviceIdError| ClientError::Decode(e.to_string()))?;
            Ok(ProviderEvent::DeviceRemoved { id })
        });
        let changed = self.proxy.receive_capability_changed().await?.map(|s| {
            let args = s.args()?;
            let id = args
                .id
                .parse()
                .map_err(|e: crate::ParseDeviceIdError| ClientError::Decode(e.to_string()))?;
            let capability = decode::<Capability>(args.capability)?;
            Ok(ProviderEvent::CapabilityChanged { id, capability })
        });
        Ok(futures_util::stream::select_all([
            added.boxed(),
            removed.boxed(),
            changed.boxed(),
        ]))
    }
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;
    use zbus::object_server::SignalEmitter;

    use super::*;
    use crate::{DeviceKind, Level, capability::tests::exemplos};

    #[test]
    fn erros_ida_e_volta() {
        let erros = [
            RaizaError::NotFound(DeviceId::usb(0x24ae, 0x1423, "0")),
            RaizaError::Unsupported(CapabilityKind::PollingRate),
            RaizaError::ReadOnly(CapabilityKind::Dpi),
            RaizaError::OutOfRange {
                kind: CapabilityKind::Volume,
                detail: "151 fora do intervalo 0..=150: sim, com dois-pontos".into(),
            },
            RaizaError::Io("cabo solto".into()),
            RaizaError::Permission("sem regra udev".into()),
        ];
        for e in erros {
            let volta = DbusError::from(e.clone()).into_raiza().unwrap();
            assert_eq!(volta, e);
        }
        assert!(DbusError::InvalidArgument("x".into()).into_raiza().is_err());
        assert!(DbusError::NotFound("lixo".into()).into_raiza().is_err());
    }

    #[test]
    fn nomes_dos_erros() {
        use zbus::DBusError as _;
        assert_eq!(
            DbusError::ReadOnly("dpi".into()).name().as_str(),
            "io.github.yurileo01.Raiza1.Error.ReadOnly"
        );
        assert!(
            DbusError::OutOfRange(String::new())
                .name()
                .starts_with(DBUS_ERROR_PREFIX)
        );
    }

    fn mouse() -> DeviceInfo {
        DeviceInfo {
            id: DeviceId::usb(0x24ae, 0x1423, "0"),
            name: "Rapoo".into(),
            vendor: None,
            kind: DeviceKind::Mouse,
            providers: vec!["mock".into()],
            capabilities: exemplos(),
        }
    }

    /// Daemon falso, só para exercitar o fio. O de verdade é o `raizad` (T04).
    struct Falso {
        devices: Vec<DeviceInfo>,
    }

    #[zbus::interface(name = "io.github.yurileo01.Raiza1")]
    impl Falso {
        fn list_devices(&self) -> Result<String, DbusError> {
            to_wire(&self.devices)
        }

        fn get_device(&self, id: &str) -> Result<String, DbusError> {
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
        ) -> Result<(), DbusError> {
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
            (API_VERSION, "teste".into())
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
        let guid = zbus::Guid::generate();
        let servidor = zbus::connection::Builder::unix_stream(a)
            .server(guid)
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
        let cliente = cliente.unwrap();
        let client = RaizaClient::new(&cliente).await.unwrap();
        (servidor.unwrap(), client)
    }

    #[tokio::test]
    async fn cliente_e_servidor_pelo_fio() {
        let (_servidor, client) = par().await;
        let mut eventos = client.events().await.unwrap();

        assert_eq!(client.version().await.unwrap().0, API_VERSION);
        assert_eq!(client.list_devices().await.unwrap(), [mouse()]);
        let id = mouse().id;
        assert_eq!(client.get_device(&id).await.unwrap(), mouse());

        client
            .set_capability(&id, &CapabilityValue::Dpi(800))
            .await
            .unwrap();
        let dpi_800 = Capability::Dpi {
            level: Level {
                current: 800,
                min: 400,
                max: 16000,
                step: 50,
            },
            stages: vec![800, 1600, 3200],
            writable: true,
        };
        assert_eq!(
            eventos.next().await.unwrap().unwrap(),
            ProviderEvent::CapabilityChanged {
                id: id.clone(),
                capability: dpi_800
            }
        );

        // Erros do Raiza chegam tipados ao cliente.
        let fora = client
            .set_capability(&id, &CapabilityValue::Dpi(1))
            .await
            .unwrap_err();
        assert!(matches!(
            fora,
            ClientError::Raiza(RaizaError::OutOfRange {
                kind: CapabilityKind::Dpi,
                ..
            })
        ));
        let outro = DeviceId::usb(1, 2, "x");
        assert!(matches!(
            client.get_device(&outro).await.unwrap_err(),
            ClientError::Raiza(RaizaError::NotFound(i)) if i == outro
        ));

        // JSON ilegível vira InvalidArgument.
        let err = client
            .proxy()
            .set_capability(&id.to_string(), "{lixo")
            .await
            .unwrap_err();
        assert!(matches!(err, DbusError::InvalidArgument(_)));
    }
}
