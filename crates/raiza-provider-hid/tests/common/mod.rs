//! HID falso para os testes: um mouse que responde como `fixtures/mouse.json`.
//!
//! Protocolo do mouse falso (interface 1, usage page `ff00`):
//! - output `08 03 …`: pede o DPI; responde com input `08 03 <status> 00 <dpi/50 u16le> …`,
//!   precedido de `busy` respostas "ocupado" (status `02`). Status `01` = pronto.
//! - output `08 04 00 00 <dpi/50 u16le> …`: grava o DPI. Com `write_ack`, responde com
//!   input `08 04 <status> …` (`01` = aceito; outro valor = recusado, sem gravar).
//! - GET_FEATURE do report `0`: devolve `identity` (resposta da identificação).
//! - input `07 20 <bateria> <carregando no bit 7> …`: relatório espontâneo ([`FakeMouse::push_input`]).

#![allow(dead_code)]

use std::collections::{BTreeSet, VecDeque};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use raiza_core::{EventStream, ProviderEvent, RaizaError};
use raiza_profiles::Profile;
use raiza_profiles::format::ReportType;
use raiza_provider_hid::{
    ChangeStream, HidBackend, HidHandle, HidInterface, Inventory, ProfileSource, UsbDevice,
};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

pub const VID: u16 = 0x1234;
pub const PID_CABO: u16 = 0x5678;
pub const PID_DONGLE: u16 = 0x9abc;

pub const MOUSE_JSON: &str = include_str!("../fixtures/mouse.json");

/// Sempre ocupado: nunca responde "pronto".
pub const SEMPRE_OCUPADO: usize = usize::MAX;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// O perfil de teste, com ajustes no JSON antes da validação.
pub fn perfil(ajuste: impl FnOnce(&mut serde_json::Value)) -> Profile {
    let mut v: serde_json::Value = serde_json::from_str(MOUSE_JSON).unwrap();
    ajuste(&mut v);
    raiza_profiles::load(&v.to_string()).unwrap()
}

/// Acrescenta ao perfil uma identificação: feature `00 8f …`, resposta `00 8f 0d …`.
pub fn com_identify(v: &mut serde_json::Value) {
    v["identify"] = serde_json::json!({
        "request": {
            "type": "feature", "report_id": 0, "data": "8f 00 00 00 00 00 00 00",
            "checksum": { "kind": "sum8_inv", "at": 7, "from": 0, "to": 7 }
        },
        "response": {
            "type": "feature", "report_id": 0, "length": 8,
            "expect": [{ "at": 0, "value": 143 }, { "at": 1, "value": 13 }]
        }
    });
}

/// Resposta de identificação que confere com [`com_identify`].
pub const IDENTIDADE_CERTA: [u8; 9] = [0, 0x8f, 0x0d, 0x0b, 0, 0, 0, 0, 0];
/// Resposta de outro aparelho com o mesmo VID:PID.
pub const IDENTIDADE_ERRADA: [u8; 9] = [0, 0x8f, 0x0e, 0x01, 0, 0, 0, 0, 0];

/// Fila de input reports de uma abertura (como cada fd do hidraw).
#[derive(Default)]
struct Queue {
    reports: Mutex<(VecDeque<Vec<u8>>, bool)>,
    ready: Condvar,
}

impl Queue {
    fn push(&self, report: &[u8]) {
        lock(&self.reports).0.push_back(report.to_vec());
        self.ready.notify_all();
    }

    fn close(&self) {
        lock(&self.reports).1 = true;
        self.ready.notify_all();
    }
}

/// Estado do mouse falso.
pub struct MouseState {
    /// DPI dividido por 50.
    pub dpi_raw: u16,
    /// Quantas respostas "ocupado" antes de cada "pronto".
    pub busy: usize,
    /// Resposta do GET_FEATURE do report 0.
    pub identity: Vec<u8>,
    /// Status da confirmação de escrita (`None`: não confirma). Só `01` grava.
    pub write_ack: Option<u8>,
    /// Tudo o que o host mandou: `(tipo, bytes com o report ID)`.
    pub sent: Vec<(ReportType, Vec<u8>)>,
}

/// Um mouse USB com duas interfaces hidraw: 0 (mouse comum) e 1 (comandos, `ff00`).
pub struct FakeMouse {
    pub port: String,
    pub product_id: u16,
    pub token: String,
    pub state: Mutex<MouseState>,
    queues: Mutex<Vec<Arc<Queue>>>,
}

impl FakeMouse {
    pub fn new(product_id: u16, port: &str) -> Arc<Self> {
        Arc::new(FakeMouse {
            port: port.into(),
            product_id,
            token: format!("{port}-{:?}", Instant::now()),
            state: Mutex::new(MouseState {
                dpi_raw: 32,
                busy: 0,
                identity: IDENTIDADE_CERTA.to_vec(),
                write_ack: None,
                sent: Vec::new(),
            }),
            queues: Mutex::default(),
        })
    }

    pub fn path(&self, interface: u8) -> String {
        format!("/dev/hidraw-{}-{interface}", self.port)
    }

    fn interfaces(&self) -> Vec<HidInterface> {
        [(0u8, (0x0001u16, 0x0002u16)), (1, (0xff00, 0x0001))]
            .into_iter()
            .map(|(n, coll)| HidInterface {
                path: self.path(n),
                vendor_id: VID,
                product_id: self.product_id,
                usb_port: self.port.clone(),
                serial: None,
                interface: Some(n),
                collections: vec![coll],
                token: self.token.clone(),
            })
            .collect()
    }

    pub fn with_state<T>(&self, f: impl FnOnce(&mut MouseState) -> T) -> T {
        f(&mut lock(&self.state))
    }

    pub fn sent(&self) -> Vec<(ReportType, Vec<u8>)> {
        lock(&self.state).sent.clone()
    }

    /// Relatórios de escrita de DPI (`08 04 …`) que chegaram ao aparelho.
    pub fn dpi_writes(&self) -> Vec<Vec<u8>> {
        self.sent()
            .into_iter()
            .filter(|(_, b)| b.len() > 1 && b[0] == 8 && b[1] == 4)
            .map(|(_, b)| b)
            .collect()
    }

    /// O aparelho manda um input report sozinho.
    pub fn push_input(&self, report: &[u8]) {
        for q in lock(&self.queues).iter() {
            q.push(report);
        }
    }

    fn close(&self) {
        for q in lock(&self.queues).drain(..) {
            q.close();
        }
    }

    fn on_output(&self, bytes: &[u8]) {
        let responses = {
            let mut st = lock(&self.state);
            st.sent.push((ReportType::Output, bytes.to_vec()));
            match bytes {
                [8, 3, ..] => {
                    let mut out = Vec::new();
                    let [lo, hi] = st.dpi_raw.to_le_bytes();
                    let mut busy = vec![0u8; 17];
                    busy[..3].copy_from_slice(&[8, 3, 2]);
                    for _ in 0..st.busy.min(3) {
                        out.push(busy.clone());
                    }
                    if st.busy != SEMPRE_OCUPADO {
                        let mut ready = vec![0u8; 17];
                        ready[..7].copy_from_slice(&[8, 3, 1, 0, 0, lo, hi]);
                        out.push(ready);
                    }
                    out
                }
                [8, 4, _, _, _, lo, hi, ..] => match st.write_ack {
                    None => {
                        st.dpi_raw = u16::from_le_bytes([*lo, *hi]);
                        Vec::new()
                    }
                    Some(status) => {
                        if status == 1 {
                            st.dpi_raw = u16::from_le_bytes([*lo, *hi]);
                        }
                        let mut ack = vec![0u8; 17];
                        ack[..3].copy_from_slice(&[8, 4, status]);
                        vec![ack]
                    }
                },
                _ => Vec::new(),
            }
        };
        for r in responses {
            self.push_input(&r);
        }
    }
}

struct FakeHandle {
    mouse: Arc<FakeMouse>,
    queue: Arc<Queue>,
    interface: u8,
}

impl HidHandle for FakeHandle {
    fn write(&self, report: &[u8]) -> Result<(), RaizaError> {
        if self.interface == 1 {
            self.mouse.on_output(report);
        }
        Ok(())
    }

    fn send_feature_report(&self, report: &[u8]) -> Result<(), RaizaError> {
        self.mouse
            .with_state(|st| st.sent.push((ReportType::Feature, report.to_vec())));
        Ok(())
    }

    fn get_feature_report(&self, buf: &mut [u8]) -> Result<usize, RaizaError> {
        match buf.first() {
            Some(0) => {
                let identity = self.mouse.with_state(|st| st.identity.clone());
                let n = identity.len().min(buf.len());
                buf[..n].copy_from_slice(&identity[..n]);
                Ok(n)
            }
            _ => Err(RaizaError::Io("feature report desconhecido".into())),
        }
    }

    fn read_timeout(&self, buf: &mut [u8], timeout: Duration) -> Result<usize, RaizaError> {
        let guard = lock(&self.queue.reports);
        let (mut guard, _) = self
            .queue
            .ready
            .wait_timeout_while(guard, timeout, |(q, closed)| q.is_empty() && !*closed)
            .unwrap_or_else(PoisonError::into_inner);
        if guard.1 {
            return Err(RaizaError::Io("aparelho desplugado".into()));
        }
        match guard.0.pop_front() {
            Some(r) => {
                let n = r.len().min(buf.len());
                buf[..n].copy_from_slice(&r[..n]);
                Ok(n)
            }
            None => Ok(0),
        }
    }
}

#[derive(Default)]
struct BackendState {
    mice: Vec<Arc<FakeMouse>>,
    denied: BTreeSet<String>,
    opened: Vec<String>,
    watchers: Vec<UnboundedSender<()>>,
}

/// Backend HID falso. Clonável: o teste guarda uma cópia para plugar e desplugar.
#[derive(Clone, Default)]
pub struct FakeBackend(Arc<Mutex<BackendState>>);

impl FakeBackend {
    pub fn plug(&self, mouse: &Arc<FakeMouse>) {
        let mut st = lock(&self.0);
        st.mice.push(Arc::clone(mouse));
        st.watchers.retain(|w| w.send(()).is_ok());
    }

    pub fn unplug(&self, mouse: &Arc<FakeMouse>) {
        let mut st = lock(&self.0);
        st.mice.retain(|m| !Arc::ptr_eq(m, mouse));
        mouse.close();
        st.watchers.retain(|w| w.send(()).is_ok());
    }

    /// Abrir este caminho dá erro de permissão (falta a regra udev).
    pub fn deny(&self, path: &str) {
        lock(&self.0).denied.insert(path.into());
    }

    /// Caminhos abertos até agora.
    pub fn opened(&self) -> Vec<String> {
        lock(&self.0).opened.clone()
    }
}

impl HidBackend for FakeBackend {
    fn inventory(&self) -> Result<Inventory, RaizaError> {
        let st = lock(&self.0);
        Ok(Inventory {
            interfaces: st.mice.iter().flat_map(|m| m.interfaces()).collect(),
            usb_devices: st
                .mice
                .iter()
                .map(|m| UsbDevice {
                    vendor_id: VID,
                    product_id: m.product_id,
                    port: m.port.clone(),
                    serial: None,
                })
                .collect(),
        })
    }

    fn open(&self, path: &str) -> Result<Box<dyn HidHandle>, RaizaError> {
        let mut st = lock(&self.0);
        if st.denied.contains(path) {
            return Err(RaizaError::Permission(format!("{path}: acesso negado")));
        }
        let (mouse, interface) = st
            .mice
            .iter()
            .find_map(|m| {
                (0..2u8)
                    .find(|n| m.path(*n) == path)
                    .map(|n| (Arc::clone(m), n))
            })
            .ok_or_else(|| RaizaError::Io(format!("{path} não existe")))?;
        st.opened.push(path.into());
        let queue = Arc::new(Queue::default());
        lock(&mouse.queues).push(Arc::clone(&queue));
        Ok(Box::new(FakeHandle {
            mouse,
            queue,
            interface,
        }))
    }

    fn watch(&self) -> Result<ChangeStream, RaizaError> {
        let (tx, rx) = unbounded_channel();
        lock(&self.0).watchers.push(tx);
        Ok(Box::pin(futures_util::stream::unfold(
            rx,
            |mut rx| async move { rx.recv().await.map(|()| ((), rx)) },
        )))
    }
}

/// Fonte de perfis em memória.
#[derive(Clone, Default)]
pub struct FakeProfiles(Arc<Mutex<ProfilesState>>);

/// Perfis atuais e quem vigia.
type ProfilesState = (Vec<Profile>, Vec<UnboundedSender<()>>);

impl FakeProfiles {
    pub fn new(profiles: Vec<Profile>) -> Self {
        FakeProfiles(Arc::new(Mutex::new((profiles, Vec::new()))))
    }

    /// Troca os perfis e avisa quem vigia, como se os arquivos tivessem mudado.
    pub fn set(&self, profiles: Vec<Profile>) {
        let mut st = lock(&self.0);
        st.0 = profiles;
        st.1.retain(|w| w.send(()).is_ok());
    }
}

impl ProfileSource for FakeProfiles {
    fn load(&self) -> Vec<Profile> {
        lock(&self.0).0.clone()
    }

    fn watch(&self) -> Result<ChangeStream, RaizaError> {
        let (tx, rx) = unbounded_channel();
        lock(&self.0).1.push(tx);
        Ok(Box::pin(futures_util::stream::unfold(
            rx,
            |mut rx| async move { rx.recv().await.map(|()| ((), rx)) },
        )))
    }
}

/// Próximo evento, ou `None` se nada chegar em `ms` milissegundos.
pub async fn next_event(events: &mut EventStream, ms: u64) -> Option<ProviderEvent> {
    use futures_util::StreamExt;
    tokio::time::timeout(Duration::from_millis(ms), events.next())
        .await
        .ok()
        .flatten()
}

/// Resumo legível de um evento: `+id`, `-id` ou `~id:capacidade`.
pub fn resumo(e: &ProviderEvent) -> String {
    match e {
        ProviderEvent::DeviceAdded { device } => format!("+{}", device.id),
        ProviderEvent::DeviceRemoved { id } => format!("-{id}"),
        ProviderEvent::CapabilityChanged { id, capability } => {
            format!("~{id}:{}", capability.kind())
        }
    }
}
