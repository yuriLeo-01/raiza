//! Núcleo do daemon com provedores falsos: isolamento, fusão, persistência e atualização.

mod comum;

use std::sync::Arc;

use async_trait::async_trait;
use comum::{dpi, mouse, mouse_id, next, quiet};
use futures_util::stream;
use raiza_core::mock::MockProvider;
use raiza_core::{
    Capability, CapabilityKind, CapabilityValue, DeviceId, DeviceInfo, EventStream, Provider,
    RaizaError,
};
use raizad::{ConfigStore, Daemon, Signal};

/// `MockProvider` compartilhado, para o teste continuar controlando o provedor.
struct Shared(Arc<MockProvider>);

#[async_trait]
impl Provider for Shared {
    fn id(&self) -> &str {
        self.0.id()
    }
    async fn discover(&self) -> Result<Vec<DeviceInfo>, RaizaError> {
        self.0.discover().await
    }
    async fn read(&self, id: &DeviceId, kind: CapabilityKind) -> Result<Capability, RaizaError> {
        self.0.read(id, kind).await
    }
    async fn write(&self, id: &DeviceId, value: CapabilityValue) -> Result<(), RaizaError> {
        self.0.write(id, value).await
    }
    fn events(&self) -> EventStream {
        self.0.events()
    }
}

fn shared(mock: MockProvider) -> (Arc<MockProvider>, Box<dyn Provider>) {
    let mock = Arc::new(mock);
    (mock.clone(), Box::new(Shared(mock)))
}

/// Onde o provedor quebrado entra em pânico.
#[derive(Clone, Copy, PartialEq)]
enum Quebra {
    Discover,
    Write,
    Events,
}

/// Provedor com um mouse que entra em pânico no ponto escolhido.
struct Quebrado(Quebra);

#[async_trait]
impl Provider for Quebrado {
    fn id(&self) -> &str {
        "quebrado"
    }
    async fn discover(&self) -> Result<Vec<DeviceInfo>, RaizaError> {
        assert!(self.0 != Quebra::Discover, "pânico na descoberta");
        Ok(vec![mouse("quebrado", 800)])
    }
    async fn read(&self, _: &DeviceId, kind: CapabilityKind) -> Result<Capability, RaizaError> {
        Err(RaizaError::Unsupported(kind))
    }
    async fn write(&self, _: &DeviceId, _: CapabilityValue) -> Result<(), RaizaError> {
        panic!("pânico na escrita");
    }
    fn events(&self) -> EventStream {
        if self.0 == Quebra::Events {
            Box::pin(stream::poll_fn(|_| panic!("pânico nos eventos")))
        } else {
            Box::pin(stream::pending())
        }
    }
}

fn headset() -> DeviceInfo {
    DeviceInfo {
        id: DeviceId::bluetooth("AA:BB:CC:DD:EE:FF").unwrap(),
        name: "Headset".into(),
        vendor: None,
        kind: raiza_core::DeviceKind::Headset,
        providers: vec!["mock".into()],
        capabilities: vec![Capability::Mute {
            muted: false,
            writable: true,
        }],
    }
}

#[tokio::test]
async fn panico_na_descoberta_nao_derruba_os_outros() {
    let (mock, p) = shared(MockProvider::new("mock").with_device(headset()));
    let (daemon, mut rx) = Daemon::new(
        vec![Box::new(Quebrado(Quebra::Discover)), p],
        ConfigStore::in_memory(),
    );
    let _running = daemon.start();
    assert_eq!(next(&mut rx).await, Signal::DeviceAdded(headset()));
    quiet(&mut rx).await;
    assert_eq!(daemon.list_devices(), [headset()]);

    // O provedor saudável continua recebendo escritas.
    daemon
        .set_capability(&headset().id, CapabilityValue::Mute(true))
        .await
        .unwrap();
    assert_eq!(mock.writes().len(), 1);
}

#[tokio::test]
async fn panico_na_escrita_vira_erro_de_io() {
    let (daemon, mut rx) = Daemon::new(
        vec![Box::new(Quebrado(Quebra::Write))],
        ConfigStore::in_memory(),
    );
    let _running = daemon.start();
    next(&mut rx).await;

    let err = daemon
        .set_capability(&mouse_id(), CapabilityValue::Dpi(1200))
        .await
        .unwrap_err();
    assert!(matches!(err, RaizaError::Io(_)), "{err:?}");
    // O daemon segue respondendo, com o estado intacto.
    assert_eq!(daemon.list_devices(), [mouse("quebrado", 800)]);
}

#[tokio::test]
async fn panico_nos_eventos_retira_so_os_dispositivos_dele() {
    let (_mock, p) = shared(MockProvider::new("mock").with_device(headset()));
    let (daemon, mut rx) = Daemon::new(
        vec![Box::new(Quebrado(Quebra::Events)), p],
        ConfigStore::in_memory(),
    );
    let _running = daemon.start();

    let mut sinais = Vec::new();
    for _ in 0..3 {
        sinais.push(next(&mut rx).await);
    }
    quiet(&mut rx).await;
    assert!(sinais.contains(&Signal::DeviceAdded(mouse("quebrado", 800))));
    assert!(sinais.contains(&Signal::DeviceRemoved(mouse_id())));
    assert!(sinais.contains(&Signal::DeviceAdded(headset())));
    assert_eq!(daemon.list_devices(), [headset()]);
}

#[tokio::test]
async fn validacao_antes_do_provedor() {
    let (mock, p) = shared(MockProvider::new("mock").with_device(mouse("mock", 800)));
    let (daemon, mut rx) = Daemon::new(vec![p], ConfigStore::in_memory());
    let _running = daemon.start();
    next(&mut rx).await;

    let id = mouse_id();
    assert!(matches!(
        daemon.set_capability(&id, CapabilityValue::Dpi(1000)).await,
        Err(RaizaError::OutOfRange { .. })
    ));
    assert!(matches!(
        daemon
            .set_capability(&id, CapabilityValue::Mute(true))
            .await,
        Err(RaizaError::Unsupported(CapabilityKind::Mute))
    ));
    let outro = DeviceId::usb(1, 2, "x");
    assert!(matches!(
        daemon
            .set_capability(&outro, CapabilityValue::Dpi(800))
            .await,
        Err(RaizaError::NotFound(_))
    ));
    assert!(mock.writes().is_empty());
}

#[tokio::test]
async fn fusao_e_roteamento_por_prioridade() {
    // "alto" tem prioridade: dono do DPI. "baixo" acrescenta o mudo.
    let mut baixo_info = mouse("baixo", 400);
    baixo_info.capabilities.push(Capability::Mute {
        muted: false,
        writable: true,
    });
    let (alto, pa) = shared(MockProvider::new("alto").with_device(mouse("alto", 800)));
    let (baixo, pb) = shared(MockProvider::new("baixo"));
    let (daemon, mut rx) = Daemon::new(vec![pa, pb], ConfigStore::in_memory());
    let _running = daemon.start();
    assert_eq!(next(&mut rx).await, Signal::DeviceAdded(mouse("alto", 800)));

    // Um segundo provedor no mesmo aparelho: CapabilityChanged, não DeviceAdded.
    baixo.add_device(baixo_info);
    let mudo = Capability::Mute {
        muted: false,
        writable: true,
    };
    assert_eq!(
        next(&mut rx).await,
        Signal::CapabilityChanged(mouse_id(), mudo)
    );
    let fundido = daemon.get_device(&mouse_id()).unwrap();
    assert_eq!(fundido.providers, ["alto", "baixo"]);
    assert_eq!(fundido.capability(CapabilityKind::Dpi), Some(&dpi(800)));

    daemon
        .set_capability(&mouse_id(), CapabilityValue::Dpi(1200))
        .await
        .unwrap();
    daemon
        .set_capability(&mouse_id(), CapabilityValue::Mute(true))
        .await
        .unwrap();
    assert_eq!(alto.writes(), [(mouse_id(), CapabilityValue::Dpi(1200))]);
    assert_eq!(baixo.writes(), [(mouse_id(), CapabilityValue::Mute(true))]);

    // Só o dono sai: o aparelho continua, sem o mudo (Removed + Added como atualização).
    while rx.try_recv().is_ok() {}
    baixo.remove_device(&mouse_id());
    assert_eq!(next(&mut rx).await, Signal::DeviceRemoved(mouse_id()));
    assert_eq!(
        next(&mut rx).await,
        Signal::DeviceAdded(mouse("alto", 1200))
    );
}

/// Daemon com um mock de um mouse e configurações num diretório temporário.
async fn com_config() -> (
    tempfile::TempDir,
    Arc<MockProvider>,
    Daemon,
    raizad::Running,
    tokio::sync::mpsc::UnboundedReceiver<Signal>,
) {
    let dir = tempfile::tempdir().unwrap();
    let (mock, p) = shared(MockProvider::new("mock").with_device(mouse("mock", 800)));
    let (daemon, mut rx) = Daemon::new(vec![p], ConfigStore::load(dir.path().join("config.json")));
    let running = daemon.start();
    assert_eq!(next(&mut rx).await, Signal::DeviceAdded(mouse("mock", 800)));
    daemon
        .set_capability(&mouse_id(), CapabilityValue::Dpi(1600))
        .await
        .unwrap();
    assert_eq!(
        next(&mut rx).await,
        Signal::CapabilityChanged(mouse_id(), dpi(1600))
    );
    quiet(&mut rx).await;
    (dir, mock, daemon, running, rx)
}

fn salvo(dir: &tempfile::TempDir) -> Vec<CapabilityValue> {
    ConfigStore::load(dir.path().join("config.json")).saved(&mouse_id())
}

#[tokio::test]
async fn removido_e_adicionado_do_mesmo_id_e_atualizacao() {
    let (dir, mock, daemon, _running, mut rx) = com_config().await;

    // O provedor manda o par de uma vez (como o udev quando o tipo muda). O aparelho
    // agora diz 800 de novo; isso não pode disparar reaplicação.
    let mut atualizado = mouse("mock", 800);
    atualizado.name = "Rapoo VT9".into();
    mock.remove_device(&mouse_id());
    mock.add_device(atualizado.clone());

    assert_eq!(next(&mut rx).await, Signal::DeviceRemoved(mouse_id()));
    assert_eq!(next(&mut rx).await, Signal::DeviceAdded(atualizado.clone()));
    quiet(&mut rx).await;
    assert_eq!(
        mock.writes().len(),
        1,
        "não deve reaplicar numa atualização"
    );
    assert_eq!(salvo(&dir), [CapabilityValue::Dpi(1600)]);
    assert_eq!(daemon.list_devices(), [atualizado]);
}

#[tokio::test]
async fn device_added_repetido_e_substituicao() {
    let (dir, mock, daemon, _running, mut rx) = com_config().await;

    // `DeviceAdded` sozinho para um id já conhecido (como fazem o bluez e o pipewire).
    mock.add_device(mouse("mock", 800));
    assert_eq!(
        next(&mut rx).await,
        Signal::CapabilityChanged(mouse_id(), dpi(800))
    );
    quiet(&mut rx).await;
    assert_eq!(
        mock.writes().len(),
        1,
        "não deve reaplicar numa substituição"
    );
    assert_eq!(salvo(&dir), [CapabilityValue::Dpi(1600)]);
    assert_eq!(daemon.list_devices(), [mouse("mock", 800)], "sem duplicar");
}

#[tokio::test]
async fn reaplica_quando_o_dispositivo_volta() {
    let (dir, mock, daemon, _running, mut rx) = com_config().await;

    mock.remove_device(&mouse_id());
    assert_eq!(next(&mut rx).await, Signal::DeviceRemoved(mouse_id()));
    assert!(daemon.list_devices().is_empty());

    // Plugado de novo com o DPI de fábrica: o valor salvo volta.
    mock.add_device(mouse("mock", 800));
    assert_eq!(next(&mut rx).await, Signal::DeviceAdded(mouse("mock", 800)));
    assert_eq!(
        next(&mut rx).await,
        Signal::CapabilityChanged(mouse_id(), dpi(1600))
    );
    quiet(&mut rx).await;
    assert_eq!(
        mock.writes(),
        [
            (mouse_id(), CapabilityValue::Dpi(1600)),
            (mouse_id(), CapabilityValue::Dpi(1600))
        ]
    );
    assert_eq!(salvo(&dir), [CapabilityValue::Dpi(1600)]);
}

#[tokio::test]
async fn reaplica_na_partida_so_o_que_mudou() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    let mut store = ConfigStore::load(&path);
    store
        .remember(&mouse_id(), &CapabilityValue::Dpi(2000))
        .unwrap();
    // O headset já está sem mudo: não há o que reaplicar.
    store
        .remember(&headset().id, &CapabilityValue::Mute(false))
        .unwrap();

    let (mock, p) = shared(
        MockProvider::new("mock")
            .with_device(mouse("mock", 800))
            .with_device(headset()),
    );
    let (daemon, mut rx) = Daemon::new(vec![p], ConfigStore::load(&path));
    let _running = daemon.start();
    for _ in 0..3 {
        next(&mut rx).await;
    }
    quiet(&mut rx).await;
    assert_eq!(mock.writes(), [(mouse_id(), CapabilityValue::Dpi(2000))]);
    assert_eq!(
        daemon
            .get_device(&mouse_id())
            .unwrap()
            .capability(CapabilityKind::Dpi),
        Some(&dpi(2000))
    );
}
