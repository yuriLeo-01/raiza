//! Testes do provedor com um PipeWire simulado.
//!
//! A fixture `maquina.json` é um recorte do `pw-dump` da máquina do mantenedor (HDMI da
//! placa de vídeo, áudio onboard, o dongle Akko 36eb:1087 e o microfone fifine 3142:a010),
//! mais o headset Bluetooth Thinkplus K3 Pro simulado.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures_util::StreamExt;
use futures_util::stream::{self, BoxStream};
use raiza_core::{
    Capability, CapabilityKind, CapabilityValue, DeviceId, DeviceKind, Level, Provider,
    ProviderEvent, RaizaError,
};
use raiza_provider_pipewire::curve::percent_to_linear;
use raiza_provider_pipewire::{
    BackendError, CardResolver, MonitorEvent, PipeWireBackend, PipeWireProvider,
};
use serde_json::{Value, json};
use tokio::sync::mpsc;

const FIFINE_NODE: u32 = 45;
const BT_SINK: u32 = 71;

fn fixture() -> Vec<Value> {
    serde_json::from_str(include_str!("fixtures/maquina.json")).unwrap()
}

fn fifine() -> DeviceId {
    DeviceId::usb(0x3142, 0xa010, "0")
}

fn akko() -> DeviceId {
    DeviceId::usb(0x36eb, 0x1087, "00000000000000")
}

fn headset() -> DeviceId {
    DeviceId::bluetooth("DE:1E:59:99:7C:AE").unwrap()
}

/// Placas ALSA USB da fixture: 1 = dongle Akko, 4 = fifine. 0, 2 e 3 são PCI.
struct Placas;

impl CardResolver for Placas {
    fn usb_id(&self, card: u32) -> Option<DeviceId> {
        match card {
            1 => Some(akko()),
            4 => Some(fifine()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Chamada {
    Volume(u32, u32),
    Mudo(u32, bool),
}

/// PipeWire falso: um `pw-dump` em memória. As escritas alteram o nó como o WirePlumber
/// faria (volume linear = cubo da porcentagem) e saem no monitor como mudança.
#[derive(Clone)]
struct Falso {
    estado: Arc<Mutex<HashMap<u32, Value>>>,
    chamadas: Arc<Mutex<Vec<Chamada>>>,
    monitor_tx: Arc<Mutex<Option<mpsc::UnboundedSender<MonitorEvent>>>>,
    monitor_rx: Arc<Mutex<Option<mpsc::UnboundedReceiver<MonitorEvent>>>>,
    indisponivel: bool,
}

impl Falso {
    fn new(itens: Vec<Value>) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let estado = itens
            .into_iter()
            .map(|v| (v["id"].as_u64().unwrap() as u32, v))
            .collect();
        Falso {
            estado: Arc::new(Mutex::new(estado)),
            chamadas: Arc::default(),
            monitor_tx: Arc::new(Mutex::new(Some(tx))),
            monitor_rx: Arc::new(Mutex::new(Some(rx))),
            indisponivel: false,
        }
    }

    fn itens(&self) -> Vec<Value> {
        let estado = self.estado.lock().unwrap();
        let mut ids: Vec<&u32> = estado.keys().collect();
        ids.sort();
        ids.into_iter().map(|id| estado[id].clone()).collect()
    }

    fn chamadas(&self) -> Vec<Chamada> {
        self.chamadas.lock().unwrap().clone()
    }

    fn emitir(&self, e: MonitorEvent) {
        if let Some(tx) = &*self.monitor_tx.lock().unwrap() {
            let _ = tx.send(e);
        }
    }

    /// Altera o nó (como o GNOME faria) e avisa no monitor.
    fn mexer(&self, node: u32, f: impl FnOnce(&mut Value)) {
        let novo = {
            let mut estado = self.estado.lock().unwrap();
            let n = estado.get_mut(&node).unwrap();
            f(&mut n["info"]["params"]["Props"][0]);
            n.clone()
        };
        self.emitir(MonitorEvent::Update(vec![novo]));
    }

    /// Remove objetos e avisa no monitor.
    fn remover(&self, ids: &[u32]) {
        let mut leva = Vec::new();
        for id in ids {
            self.estado.lock().unwrap().remove(id);
            leva.push(json!({"id": id, "info": null}));
        }
        self.emitir(MonitorEvent::Update(leva));
    }

    /// Fecha o monitor (o fluxo de eventos termina).
    fn fechar(&self) {
        self.monitor_tx.lock().unwrap().take();
    }
}

#[async_trait]
impl PipeWireBackend for Falso {
    async fn dump(&self) -> Result<Vec<Value>, BackendError> {
        if self.indisponivel {
            return Err(BackendError::Unavailable("sem servidor".into()));
        }
        Ok(self.itens())
    }

    fn monitor(&self) -> BoxStream<'static, MonitorEvent> {
        let Some(mut rx) = self.monitor_rx.lock().unwrap().take() else {
            return stream::empty().boxed();
        };
        let snapshot = MonitorEvent::Snapshot(self.itens());
        stream::once(async move { snapshot })
            .chain(stream::poll_fn(move |cx| rx.poll_recv(cx)))
            .boxed()
    }

    async fn set_volume(&self, node: u32, percent: u32) -> Result<(), BackendError> {
        self.chamadas
            .lock()
            .unwrap()
            .push(Chamada::Volume(node, percent));
        let linear = percent_to_linear(percent);
        self.mexer(node, |p| p["channelVolumes"] = json!([linear, linear]));
        Ok(())
    }

    async fn set_mute(&self, node: u32, muted: bool) -> Result<(), BackendError> {
        self.chamadas
            .lock()
            .unwrap()
            .push(Chamada::Mudo(node, muted));
        self.mexer(node, |p| p["mute"] = json!(muted));
        Ok(())
    }
}

fn provedor(falso: &Falso) -> PipeWireProvider<Falso, Placas> {
    PipeWireProvider::with_backend(falso.clone(), Placas)
}

fn volume(current: u32) -> Capability {
    Capability::Volume {
        level: Level {
            current,
            min: 0,
            max: 100,
            step: 1,
        },
        writable: true,
    }
}

fn mudo(muted: bool) -> Capability {
    Capability::Mute {
        muted,
        writable: true,
    }
}

#[tokio::test]
async fn descobre_usb_e_bluetooth_e_ignora_onboard() {
    let falso = Falso::new(fixture());
    let p = provedor(&falso);
    assert_eq!(p.id(), "pipewire");
    let devices = p.discover().await.unwrap();
    let ids: Vec<&DeviceId> = devices.iter().map(|d| &d.id).collect();
    // Onboard (placa 3), HDMI (0 e 2) e streams de aplicativos ficam de fora.
    assert_eq!(ids, vec![&fifine(), &akko(), &headset()]);

    let bt = &devices[2];
    assert_eq!(bt.kind, DeviceKind::Headset);
    assert_eq!(bt.name, "联想Thinkplus-K3 pro");
    assert_eq!(bt.providers, vec!["pipewire".to_string()]);
    // Tem sink e source: vale a saída (0.125 linear = 50 %, sem mudo); o mudo do microfone não aparece.
    assert_eq!(bt.capabilities, vec![volume(50), mudo(false)]);

    // fifine: form-factor microphone → vale a entrada, mesmo tendo saída de retorno.
    let mic = &devices[0];
    assert_eq!(mic.kind, DeviceKind::Microphone);
    assert_eq!(mic.name, "fifine Microphone");
    assert_eq!(mic.vendor, None, "o PipeWire repete o VID como fabricante");
    assert_eq!(mic.capabilities, vec![volume(58), mudo(false)]);

    let dongle = &devices[1];
    assert_eq!(dongle.kind, DeviceKind::Headset);
    assert_eq!(dongle.vendor.as_deref(), Some("GTAI"));
    assert_eq!(dongle.capabilities, vec![volume(94), mudo(false)]);
}

#[tokio::test]
async fn so_entrada_vira_microfone() {
    // Sem o sink do headset, sobra a entrada (mudo ligado, 100 %).
    let itens: Vec<Value> = fixture()
        .into_iter()
        .filter(|v| v["id"] != BT_SINK)
        .collect();
    let p = provedor(&Falso::new(itens));
    let bt = p
        .discover()
        .await
        .unwrap()
        .into_iter()
        .find(|d| d.id == headset())
        .unwrap();
    assert_eq!(bt.kind, DeviceKind::Microphone);
    assert_eq!(bt.capabilities, vec![volume(100), mudo(true)]);
}

#[tokio::test]
async fn le_e_escreve_volume_e_mudo() {
    let falso = Falso::new(fixture());
    let p = provedor(&falso);
    p.discover().await.unwrap();

    p.write(&fifine(), CapabilityValue::Volume(37))
        .await
        .unwrap();
    p.write(&headset(), CapabilityValue::Mute(true))
        .await
        .unwrap();
    assert_eq!(
        falso.chamadas(),
        vec![
            Chamada::Volume(FIFINE_NODE, 37),
            Chamada::Mudo(BT_SINK, true)
        ]
    );
    // A leitura vem direto da fonte e passa pela curva cúbica de volta.
    assert_eq!(
        p.read(&fifine(), CapabilityKind::Volume).await.unwrap(),
        volume(37)
    );
    assert_eq!(
        p.read(&headset(), CapabilityKind::Mute).await.unwrap(),
        mudo(true)
    );
    assert_eq!(
        p.read(&fifine(), CapabilityKind::Battery).await,
        Err(RaizaError::Unsupported(CapabilityKind::Battery))
    );
}

#[tokio::test]
async fn recusa_escrita_invalida_sem_tocar_no_pipewire() {
    let falso = Falso::new(fixture());
    let p = provedor(&falso);
    assert!(matches!(
        p.write(&fifine(), CapabilityValue::Volume(101)).await,
        Err(RaizaError::OutOfRange { .. })
    ));
    assert_eq!(
        p.write(&fifine(), CapabilityValue::Dpi(800)).await,
        Err(RaizaError::Unsupported(CapabilityKind::Dpi))
    );
    let sumido = DeviceId::usb(0x1234, 0x5678, "0");
    assert_eq!(
        p.write(&sumido, CapabilityValue::Mute(true)).await,
        Err(RaizaError::NotFound(sumido.clone()))
    );
    // Placa onboard não é atendida.
    assert!(falso.chamadas().is_empty());
}

#[tokio::test]
async fn pipewire_indisponivel_da_lista_vazia() {
    let mut falso = Falso::new(fixture());
    falso.indisponivel = true;
    let p = provedor(&falso);
    assert_eq!(p.discover().await, Ok(vec![]));
    assert_eq!(
        p.read(&fifine(), CapabilityKind::Volume).await,
        Err(RaizaError::NotFound(fifine()))
    );
}

#[tokio::test]
async fn eventos_de_mudanca_e_hotplug() {
    let falso = Falso::new(fixture());
    let p = provedor(&falso);
    p.discover().await.unwrap();
    let mut eventos = p.events();

    // Volume e mudo mudam por fora (ex.: GNOME).
    falso.mexer(FIFINE_NODE, |props| {
        let l = percent_to_linear(20);
        props["channelVolumes"] = json!([l, l]);
    });
    falso.mexer(BT_SINK, |props| props["mute"] = json!(true));
    // Mudança que não afeta volume nem mudo: sem evento.
    falso.emitir(MonitorEvent::Update(vec![
        json!({"id": 999, "type": "PipeWire:Interface:Client", "info": {"props": {}}}),
    ]));
    // Headset desligado: o aparelho e os nós somem.
    falso.remover(&[70, 71, 72]);
    // E volta.
    let bt: Vec<Value> = fixture()
        .into_iter()
        .filter(|v| [70, 71, 72].contains(&v["id"].as_u64().unwrap()))
        .collect();
    for v in &bt {
        falso
            .estado
            .lock()
            .unwrap()
            .insert(v["id"].as_u64().unwrap() as u32, v.clone());
    }
    falso.emitir(MonitorEvent::Update(bt));
    falso.fechar();

    let recebidos: Vec<ProviderEvent> = eventos.by_ref().collect().await;
    let headset_info = p
        .discover()
        .await
        .unwrap()
        .into_iter()
        .find(|d| d.id == headset())
        .unwrap();
    assert_eq!(
        recebidos,
        vec![
            ProviderEvent::CapabilityChanged {
                id: fifine(),
                capability: volume(20),
            },
            ProviderEvent::CapabilityChanged {
                id: headset(),
                capability: mudo(true),
            },
            ProviderEvent::DeviceRemoved { id: headset() },
            ProviderEvent::DeviceAdded {
                device: headset_info,
            },
        ]
    );
}

#[tokio::test]
async fn sem_discover_o_retrato_inicial_anuncia_tudo() {
    let falso = Falso::new(fixture());
    let p = provedor(&falso);
    let mut eventos = p.events();
    falso.fechar();
    let mut adicionados = Vec::new();
    while let Some(e) = eventos.next().await {
        match e {
            ProviderEvent::DeviceAdded { device } => adicionados.push(device.id),
            outro => panic!("evento inesperado: {outro:?}"),
        }
    }
    assert_eq!(adicionados, vec![fifine(), akko(), headset()]);
}
