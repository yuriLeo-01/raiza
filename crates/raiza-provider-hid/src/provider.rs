//! O [`Provider`] `hid-perfis`: casa perfis com interfaces hidraw e executa as leituras e
//! escritas que eles descrevem.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use futures_util::stream::{self, BoxStream};
use raiza_core::{
    Capability, CapabilityKind, CapabilityValue, DeviceId, DeviceInfo, EventStream, Provider,
    ProviderEvent, RaizaError,
};
use raiza_profiles::Profile;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tracing::{debug, info, warn};

use crate::PROVIDER_ID;
use crate::backend::{HidBackend, HidInterface, Inventory};
use crate::device::{Device, is_spontaneous};
use crate::profiles::{DirProfiles, ProfileSource};
use crate::system::SystemHid;

/// Tempos do provedor.
#[derive(Debug, Clone, Copy)]
pub struct Timeouts {
    /// Tempo máximo por resposta, contando as repetições de "ainda não pronto".
    pub response: Duration,
    /// Tempo máximo esperando um relatório espontâneo que ainda não chegou desde a conexão.
    pub spontaneous: Duration,
    /// Silêncio esperado depois de um evento de hotplug ou de perfil antes de reler tudo:
    /// plugar um aparelho gera uma rajada de eventos.
    pub debounce: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Timeouts {
            response: Duration::from_millis(500),
            spontaneous: Duration::from_secs(4),
            debounce: Duration::from_millis(300),
        }
    }
}

/// Limite de espera numa rajada de eventos que não acaba.
const DEBOUNCE_MAX: Duration = Duration::from_secs(2);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Default)]
struct State {
    profiles: Vec<Arc<Profile>>,
    /// Aparelhos abertos e anunciados, um por id.
    devices: BTreeMap<DeviceId, Arc<Device>>,
}

struct Inner<B, P> {
    backend: B,
    source: P,
    timeouts: Timeouts,
    state: Mutex<State>,
    /// Uma releitura por vez.
    refresh_lock: tokio::sync::Mutex<()>,
    events_tx: UnboundedSender<ProviderEvent>,
    events_rx: Mutex<Option<UnboundedReceiver<ProviderEvent>>>,
}

/// Provedor de maior prioridade: perfis HID declarativos via `/dev/hidraw*`.
///
/// - Carrega os perfis de um [`ProfileSource`] (por padrão, [`DirProfiles`]) e recarrega
///   quando os arquivos mudam.
/// - Casa cada perfil com as interfaces hidraw pelo `match` (VID:PID, interface, usage).
/// - Com duas conexões do mesmo perfil presentes (ex.: cabo e dongle), só a de menor
///   índice no `match` é anunciada.
/// - Se o perfil tem `identify`, só escreve depois que a identificação confirmar o
///   aparelho; perfis `"verified": false` nunca escrevem (pilar 1).
pub struct HidProfilesProvider<B: HidBackend = SystemHid, P: ProfileSource = DirProfiles> {
    inner: Arc<Inner<B, P>>,
}

impl HidProfilesProvider<SystemHid, DirProfiles> {
    /// Provedor ligado ao HID do sistema, com os diretórios de perfis padrão
    /// ([`DirProfiles::default_dirs`]).
    pub fn new() -> Self {
        Self::with_parts(
            SystemHid::new(),
            DirProfiles::default(),
            Timeouts::default(),
        )
    }
}

impl Default for HidProfilesProvider<SystemHid, DirProfiles> {
    fn default() -> Self {
        Self::new()
    }
}

impl<B: HidBackend, P: ProfileSource> HidProfilesProvider<B, P> {
    /// Provedor com backend, fonte de perfis e tempos injetados.
    pub fn with_parts(backend: B, source: P, timeouts: Timeouts) -> Self {
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        HidProfilesProvider {
            inner: Arc::new(Inner {
                backend,
                source,
                timeouts,
                state: Mutex::default(),
                refresh_lock: tokio::sync::Mutex::new(()),
                events_tx,
                events_rx: Mutex::new(Some(events_rx)),
            }),
        }
    }

    fn device(&self, id: &DeviceId) -> Result<Arc<Device>, RaizaError> {
        lock(&self.inner.state)
            .devices
            .get(id)
            .cloned()
            .ok_or_else(|| RaizaError::NotFound(id.clone()))
    }
}

/// Índice do `match` do perfil que serve para a interface, olhando todas as coleções.
pub(crate) fn match_index(profile: &Profile, iface: &HidInterface) -> Option<usize> {
    let (vid, pid, interface) = (iface.vendor_id, iface.product_id, iface.interface);
    if iface.collections.is_empty() {
        return profile.match_index(vid, pid, interface, None, None);
    }
    iface
        .collections
        .iter()
        .filter_map(|(page, usage)| {
            profile.match_index(vid, pid, interface, Some(*page), Some(*usage))
        })
        .min()
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, RaizaError> + Send + 'static,
) -> Result<T, RaizaError> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| RaizaError::Io(format!("operação HID interrompida: {e}")))?
}

impl<B: HidBackend, P: ProfileSource> Inner<B, P> {
    /// Relê (e, se `reload`, recarrega os perfis) e devolve os eventos da diferença.
    async fn refresh(self: &Arc<Self>, reload: bool) -> Vec<ProviderEvent> {
        let _one = self.refresh_lock.lock().await;
        let inner = Arc::clone(self);
        blocking(move || {
            if reload {
                inner.reload_profiles();
            }
            Ok(inner.refresh_blocking())
        })
        .await
        .unwrap_or_else(|e| {
            warn!(error = %e, "releitura dos aparelhos HID falhou");
            Vec::new()
        })
    }

    fn reload_profiles(&self) {
        let profiles: Vec<Arc<Profile>> = self.source.load().into_iter().map(Arc::new).collect();
        lock(&self.state).profiles = profiles;
    }

    fn refresh_blocking(&self) -> Vec<ProviderEvent> {
        let inventory = match self.backend.inventory() {
            Ok(inventory) => inventory,
            Err(e) => {
                warn!(error = %e, "não deu para listar os hidraw; mantendo a lista anterior");
                return Vec::new();
            }
        };
        let (profiles, old) = {
            let state = lock(&self.state);
            (state.profiles.clone(), state.devices.clone())
        };
        let new = self.select(&inventory, &profiles, &old);

        let mut events = Vec::new();
        for (id, device) in &old {
            if new.get(id).is_none_or(|d| !Arc::ptr_eq(d, device)) {
                events.push(ProviderEvent::DeviceRemoved { id: id.clone() });
            }
        }
        for (id, device) in &new {
            if old.get(id).is_none_or(|d| !Arc::ptr_eq(d, device)) {
                events.push(ProviderEvent::DeviceAdded {
                    device: device.info(),
                });
            }
            device.set_announced();
        }
        lock(&self.state).devices = new;
        events
    }

    /// Escolhe, para cada perfil, as conexões a anunciar, e abre as que faltam.
    ///
    /// Perfis na ordem de prioridade. Em cada um, as conexões presentes vão em ordem de
    /// índice no `match`; só as do menor índice que abrir são anunciadas (empates são
    /// aparelhos diferentes do mesmo modelo). Um aparelho cuja identificação falhou fica
    /// disponível para os perfis seguintes; se nenhum o confirmar, fica com o primeiro,
    /// somente leitura.
    fn select(
        &self,
        inventory: &Inventory,
        profiles: &[Arc<Profile>],
        old: &BTreeMap<DeviceId, Arc<Device>>,
    ) -> BTreeMap<DeviceId, Arc<Device>> {
        let by_device = inventory.by_device();
        let mut claimed: BTreeMap<DeviceId, Arc<Device>> = BTreeMap::new();
        let mut fallback: BTreeMap<DeviceId, Arc<Device>> = BTreeMap::new();
        for profile in profiles {
            let mut conns: Vec<(usize, &DeviceId, &HidInterface)> = by_device
                .iter()
                .filter(|(id, _)| !claimed.contains_key(*id))
                .filter_map(|(id, ifaces)| {
                    ifaces
                        .iter()
                        .filter_map(|i| match_index(profile, i).map(|m| (m, *i)))
                        .min_by_key(|(m, _)| *m)
                        .map(|(m, i)| (m, id, i))
                })
                .collect();
            conns.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
            let mut best: Option<usize> = None;
            for (index, id, iface) in conns {
                if best.is_some_and(|b| index > b) {
                    debug!(%id, profile = %profile.name, "conexão secundária, não anunciada");
                    break;
                }
                let Some(device) = self.reuse_or_open(old, id, iface, profile) else {
                    continue;
                };
                best = Some(index);
                if device.identified() {
                    claimed.insert(id.clone(), device);
                } else {
                    fallback.entry(id.clone()).or_insert(device);
                }
            }
        }
        for (id, device) in fallback {
            claimed.entry(id).or_insert(device);
        }
        claimed
    }

    fn reuse_or_open(
        &self,
        old: &BTreeMap<DeviceId, Arc<Device>>,
        id: &DeviceId,
        iface: &HidInterface,
        profile: &Arc<Profile>,
    ) -> Option<Arc<Device>> {
        if let Some(d) = old.get(id)
            && d.path == iface.path
            && d.token == iface.token
            && *d.profile == **profile
            && !d.is_dead()
        {
            return Some(Arc::clone(d));
        }
        match Device::open(
            &self.backend,
            id.clone(),
            iface,
            Arc::clone(profile),
            self.timeouts.response,
            self.events_tx.clone(),
        ) {
            Ok(device) => Some(device),
            Err(e @ RaizaError::Permission(_)) => {
                warn!(%id, path = %iface.path, error = %e, "sem permissão no hidraw");
                None
            }
            Err(e) => {
                warn!(%id, path = %iface.path, error = %e, "não deu para abrir o hidraw");
                None
            }
        }
    }
}

#[async_trait]
impl<B: HidBackend, P: ProfileSource> Provider for HidProfilesProvider<B, P> {
    fn id(&self) -> &str {
        PROVIDER_ID
    }

    async fn discover(&self) -> Result<Vec<DeviceInfo>, RaizaError> {
        // Os eventos desta releitura não interessam: a lista devolvida já é o estado.
        let _ = self.inner.refresh(true).await;
        Ok(lock(&self.inner.state)
            .devices
            .values()
            .map(|d| d.info())
            .collect())
    }

    async fn read(&self, id: &DeviceId, kind: CapabilityKind) -> Result<Capability, RaizaError> {
        let device = self.device(id)?;
        let cached = device.cached(kind).ok_or(RaizaError::Unsupported(kind))?;
        // RGB e capacidades sem leitura no perfil: o último estado conhecido.
        if device.profile.response_spec(kind).is_none() {
            return Ok(cached);
        }
        let spontaneous = is_spontaneous(&device.profile, kind);
        if spontaneous && device.seen(kind) {
            return Ok(cached);
        }
        let timeout = if spontaneous {
            self.inner.timeouts.spontaneous
        } else {
            self.inner.timeouts.response
        };
        let id = id.clone();
        blocking(move || {
            device.read_now(kind, timeout).inspect_err(|e| {
                warn!(%id, %kind, error = %e, "leitura falhou");
            })
        })
        .await
    }

    async fn write(&self, id: &DeviceId, value: CapabilityValue) -> Result<(), RaizaError> {
        let device = self.device(id)?;
        let kind = value.kind();
        if !device.identified() {
            info!(%id, %kind, "escrita recusada: identificação do aparelho não confirmada");
            return Err(RaizaError::ReadOnly(kind));
        }
        if !device.profile.verified {
            info!(%id, %kind, "escrita recusada: perfil não verificado");
            return Err(RaizaError::ReadOnly(kind));
        }
        let id = id.clone();
        blocking(move || {
            device.write_now(&value).inspect_err(|e| {
                warn!(%id, %kind, error = %e, "escrita falhou");
            })
        })
        .await
    }

    fn events(&self) -> EventStream {
        let Some(rx) = lock(&self.inner.events_rx).take() else {
            warn!("events() chamado mais de uma vez; o segundo fluxo fica vazio");
            return Box::pin(stream::empty());
        };
        let watch = Watch {
            inner: Arc::clone(&self.inner),
            rx,
            changes: None,
            started: false,
            pending: VecDeque::new(),
        };
        Box::pin(stream::unfold(watch, Watch::next))
    }
}

/// O que mudou fora do provedor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Change {
    Hid,
    Profiles,
}

/// Estado do fluxo de eventos.
struct Watch<B, P> {
    inner: Arc<Inner<B, P>>,
    rx: UnboundedReceiver<ProviderEvent>,
    changes: Option<BoxStream<'static, Change>>,
    started: bool,
    pending: VecDeque<ProviderEvent>,
}

enum Step {
    Event(Option<ProviderEvent>),
    Change(Option<Change>),
}

async fn next_change(changes: &mut Option<BoxStream<'static, Change>>) -> Option<Change> {
    match changes {
        Some(c) => c.next().await,
        None => std::future::pending().await,
    }
}

impl<B: HidBackend, P: ProfileSource> Watch<B, P> {
    async fn next(mut self) -> Option<(ProviderEvent, Self)> {
        loop {
            if let Some(event) = self.pending.pop_front() {
                return Some((event, self));
            }
            if !self.started {
                self.started = true;
                self.start();
                // Cobre o que mudou (aparelhos e perfis) entre o `discover` e o início
                // da vigília.
                let events = self.inner.refresh(true).await;
                self.pending.extend(events);
                continue;
            }
            let step = tokio::select! {
                event = self.rx.recv() => Step::Event(event),
                change = next_change(&mut self.changes) => Step::Change(change),
            };
            match step {
                Step::Event(event) => return event.map(|e| (e, self)),
                Step::Change(None) => {
                    info!("vigias de hotplug e de perfis encerradas");
                    self.changes = None;
                }
                Step::Change(Some(first)) => {
                    let reload = self.debounce(first).await;
                    let events = self.inner.refresh(reload).await;
                    self.pending.extend(events);
                }
            }
        }
    }

    fn start(&mut self) {
        let mut streams: Vec<BoxStream<'static, Change>> = Vec::new();
        match self.inner.backend.watch() {
            Ok(s) => streams.push(s.map(|()| Change::Hid).boxed()),
            Err(e) => info!(error = %e, "monitor de hidraw indisponível; sem hotplug"),
        }
        match self.inner.source.watch() {
            Ok(s) => streams.push(s.map(|()| Change::Profiles).boxed()),
            Err(e) => info!(error = %e, "vigia dos perfis indisponível; sem recarga"),
        }
        if !streams.is_empty() {
            self.changes = Some(stream::select_all(streams).boxed());
        }
    }

    /// Espera a rajada de mudanças acabar. Devolve se algum perfil mudou.
    async fn debounce(&mut self, first: Change) -> bool {
        let mut reload = first == Change::Profiles;
        let deadline = tokio::time::Instant::now() + DEBOUNCE_MAX;
        while let Some(changes) = self.changes.as_mut() {
            let now = tokio::time::Instant::now();
            let wait = self
                .inner
                .timeouts
                .debounce
                .min(deadline.saturating_duration_since(now));
            if wait.is_zero() {
                break;
            }
            match tokio::time::timeout(wait, changes.next()).await {
                Ok(Some(c)) => reload |= c == Change::Profiles,
                Ok(None) => self.changes = None,
                Err(_) => break,
            }
        }
        reload
    }
}
