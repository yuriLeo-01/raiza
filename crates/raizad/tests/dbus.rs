//! A API D-Bus de ponta a ponta, numa conexão `zbus` ponto a ponto (barramento privado):
//! o cliente é o `RaizaClient` do `raiza-core`, o servidor é o `Service` do daemon.

mod comum;

use futures_util::StreamExt;
use raiza_core::dbus::{API_VERSION, ClientError, DBUS_PATH, DbusError, RaizaClient};
use raiza_core::{CapabilityKind, CapabilityValue, DeviceId, ProviderEvent, RaizaError};
use raizad::{ConfigStore, Daemon, Service, forward_signals, providers};

struct Ambiente {
    _servidor: zbus::Connection,
    daemon: Daemon,
    client: RaizaClient,
    dir: tempfile::TempDir,
}

/// Espera no máximo 5 s, para o teste não travar.
async fn a_tempo<T>(fut: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(5), fut)
        .await
        .expect("sem resposta em 5 s")
}

async fn subir() -> Ambiente {
    let dir = tempfile::tempdir().unwrap();
    let config = ConfigStore::load(dir.path().join("config.json"));
    let (daemon, sinais) = Daemon::new(providers::mock_providers(), config);

    let (a, b) = tokio::net::UnixStream::pair().unwrap();
    let servidor = zbus::connection::Builder::unix_stream(a)
        .server(zbus::Guid::generate())
        .unwrap()
        .p2p()
        .serve_at(DBUS_PATH, Service::new(daemon.clone()))
        .unwrap()
        .build();
    let cliente = zbus::connection::Builder::unix_stream(b).p2p().build();
    let (servidor, cliente) = tokio::join!(servidor, cliente);
    let (servidor, cliente) = (servidor.unwrap(), cliente.unwrap());
    let client = RaizaClient::new(&cliente).await.unwrap();

    tokio::spawn(forward_signals(servidor.clone(), sinais));
    Ambiente {
        _servidor: servidor,
        daemon,
        client,
        dir,
    }
}

fn mouse_mock() -> DeviceId {
    "other:mock:mouse".parse().unwrap()
}

#[tokio::test]
async fn api_pelo_fio() {
    let env = subir().await;
    let mut eventos = env.client.events().await.unwrap();
    // Os provedores só começam depois da inscrição nos sinais.
    let _running = env.daemon.start();

    // A descoberta chega como sinais.
    let mut adicionados = Vec::new();
    for _ in 0..2 {
        match a_tempo(eventos.next()).await.unwrap().unwrap() {
            ProviderEvent::DeviceAdded { device } => adicionados.push(device),
            outro => panic!("esperava DeviceAdded, veio {outro:?}"),
        }
    }

    let (api, versao) = env.client.version().await.unwrap();
    assert_eq!(api, API_VERSION);
    assert_eq!(versao, env!("CARGO_PKG_VERSION"));

    let mut lista = env.client.list_devices().await.unwrap();
    let mut esperados = providers::mock_devices();
    lista.sort_by(|a, b| a.id.cmp(&b.id));
    esperados.sort_by(|a, b| a.id.cmp(&b.id));
    adicionados.sort_by(|a, b| a.id.cmp(&b.id));
    assert_eq!(lista, esperados);
    assert_eq!(adicionados, esperados);

    let id = mouse_mock();
    env.client
        .set_capability(&id, &CapabilityValue::Dpi(3200))
        .await
        .unwrap();
    match a_tempo(eventos.next()).await.unwrap().unwrap() {
        ProviderEvent::CapabilityChanged { id: i, capability } => {
            assert_eq!(i, id);
            assert_eq!(capability.kind(), CapabilityKind::Dpi);
        }
        outro => panic!("esperava CapabilityChanged, veio {outro:?}"),
    }
    let dpi = env.client.get_device(&id).await.unwrap();
    assert!(matches!(
        dpi.capability(CapabilityKind::Dpi),
        Some(raiza_core::Capability::Dpi { level, .. }) if level.current == 3200
    ));

    // Gravado em disco.
    let salvo = ConfigStore::load(env.dir.path().join("config.json")).saved(&id);
    assert_eq!(salvo, [CapabilityValue::Dpi(3200)]);
}

#[tokio::test]
async fn erros_pelo_fio() {
    let env = subir().await;
    let id = mouse_mock();
    let _running = env.daemon.start();
    // Espera a descoberta.
    a_tempo(async {
        while env.client.list_devices().await.unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await;

    let err = env
        .client
        .set_capability(&id, &CapabilityValue::Dpi(3210))
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            ClientError::Raiza(RaizaError::OutOfRange {
                kind: CapabilityKind::Dpi,
                ..
            })
        ),
        "{err:?}"
    );
    let err = env
        .client
        .set_capability(&id, &CapabilityValue::Volume(10))
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        ClientError::Raiza(RaizaError::Unsupported(CapabilityKind::Volume))
    ));
    // Conexão do mouse mock é somente leitura.
    let err = env
        .client
        .set_capability(&id, &CapabilityValue::Connection(false))
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        ClientError::Raiza(RaizaError::ReadOnly(CapabilityKind::Connection))
    ));
    let outro = DeviceId::usb(1, 2, "x");
    assert!(matches!(
        env.client.get_device(&outro).await.unwrap_err(),
        ClientError::Raiza(RaizaError::NotFound(i)) if i == outro
    ));

    let proxy = env.client.proxy();
    assert!(matches!(
        proxy.get_device("lixo").await.unwrap_err(),
        DbusError::InvalidArgument(_)
    ));
    assert!(matches!(
        proxy
            .set_capability(&id.to_string(), "{lixo")
            .await
            .unwrap_err(),
        DbusError::InvalidArgument(_)
    ));
    // Nada foi salvo.
    assert!(
        ConfigStore::load(env.dir.path().join("config.json"))
            .saved(&id)
            .is_empty()
    );
}
