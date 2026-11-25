//! Provedor completo com uma fonte falsa: descoberta, leitura, escrita e hotplug.

use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use raiza_core::{
    Capability, CapabilityKind, CapabilityValue, ConnectionKind, DeviceId, Provider, ProviderEvent,
    RaizaError,
};
use raiza_provider_udev::{ChangeStream, Tree, UdevProvider, UdevSource};
use tokio::sync::mpsc;

fn fixture(nome: &str) -> Tree {
    let caminho = format!("{}/tests/fixtures/{nome}", env!("CARGO_MANIFEST_DIR"));
    Tree::from_json(&std::fs::read_to_string(caminho).unwrap()).unwrap()
}

/// Fonte falsa: a árvore muda quando o teste quer, e `avisar` simula um evento udev.
#[derive(Clone)]
struct Falsa {
    arvore: Arc<Mutex<Option<Tree>>>,
    eventos: Arc<Mutex<Option<mpsc::UnboundedReceiver<()>>>>,
}

impl Falsa {
    fn nova(arvore: Option<Tree>) -> (Self, mpsc::UnboundedSender<()>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let falsa = Falsa {
            arvore: Arc::new(Mutex::new(arvore)),
            eventos: Arc::new(Mutex::new(Some(rx))),
        };
        (falsa, tx)
    }

    fn trocar(&self, arvore: Tree) {
        *self.arvore.lock().unwrap() = Some(arvore);
    }
}

impl UdevSource for Falsa {
    fn snapshot(&self) -> Result<Tree, RaizaError> {
        self.arvore
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| RaizaError::Io("udev ausente".into()))
    }

    fn watch(&self) -> Result<ChangeStream, RaizaError> {
        let rx = self
            .eventos
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| RaizaError::Io("monitor ausente".into()))?;
        Ok(Box::pin(futures_util::stream::unfold(rx, |mut rx| async {
            rx.recv().await.map(|()| ((), rx))
        })))
    }
}

#[tokio::test]
async fn descobre_le_e_recusa_escrita() {
    let (falsa, _tx) = Falsa::nova(Some(fixture("teclado_tres_hidraw.json")));
    let provider = UdevProvider::with_source(falsa);
    assert_eq!(provider.id(), "udev");

    let devices = provider.discover().await.unwrap();
    assert_eq!(devices.len(), 1);
    let id = devices[0].id.clone();

    assert_eq!(
        provider.read(&id, CapabilityKind::Connection).await,
        Ok(Capability::Connection {
            medium: ConnectionKind::Wired,
            connected: true,
            writable: false,
        })
    );
    assert_eq!(
        provider.read(&id, CapabilityKind::Battery).await,
        Err(RaizaError::Unsupported(CapabilityKind::Battery))
    );
    let outro = DeviceId::usb(0xdead, 0xbeef, "0");
    assert_eq!(
        provider.read(&outro, CapabilityKind::Connection).await,
        Err(RaizaError::NotFound(outro))
    );
    assert_eq!(
        provider
            .write(&id, CapabilityValue::Connection(false))
            .await,
        Err(RaizaError::ReadOnly(CapabilityKind::Connection))
    );
}

#[tokio::test]
async fn udev_ausente_vira_lista_vazia() {
    let (falsa, _tx) = Falsa::nova(None);
    let provider = UdevProvider::with_source(falsa);
    assert_eq!(provider.discover().await, Ok(vec![]));
}

#[tokio::test(start_paused = true)]
async fn hotplug_emite_added_e_removed() {
    let (falsa, tx) = Falsa::nova(Some(Tree::default()));
    let provider = UdevProvider::with_source(falsa.clone());
    assert!(provider.discover().await.unwrap().is_empty());
    let mut eventos = provider.events();

    // Plugou o teclado: uma rajada de eventos vira um único DeviceAdded.
    falsa.trocar(fixture("teclado_tres_hidraw.json"));
    for _ in 0..5 {
        tx.send(()).unwrap();
    }
    let Some(ProviderEvent::DeviceAdded { device }) = eventos.next().await else {
        panic!("esperava DeviceAdded");
    };
    assert_eq!(device.id, DeviceId::usb(0x3151, 0x502f, "0"));

    // Desplugou.
    falsa.trocar(Tree::default());
    tx.send(()).unwrap();
    assert_eq!(
        eventos.next().await,
        Some(ProviderEvent::DeviceRemoved { id: device.id })
    );

    // Monitor encerrado: o fluxo termina.
    drop(tx);
    assert_eq!(eventos.next().await, None);
}

#[tokio::test(start_paused = true)]
async fn mudanca_entre_discover_e_events_nao_se_perde() {
    let (falsa, _tx) = Falsa::nova(Some(Tree::default()));
    let provider = UdevProvider::with_source(falsa.clone());
    provider.discover().await.unwrap();

    falsa.trocar(fixture("bluetooth.json"));
    let mut eventos = provider.events();
    let mut adicionados = Vec::new();
    for _ in 0..2 {
        match eventos.next().await {
            Some(ProviderEvent::DeviceAdded { device }) => adicionados.push(device.id.to_string()),
            outro => panic!("inesperado: {outro:?}"),
        }
    }
    assert_eq!(
        adicionados,
        ["bt:AA:BB:CC:00:11:22", "bt:D1:2A:33:44:55:66"]
    );
}
