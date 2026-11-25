//! Núcleo do daemon: provedores isolados, estado fundido, roteamento de escritas e
//! reaplicação das configurações salvas. Não sabe nada de D-Bus: as mudanças saem como
//! [`Signal`]s por um canal, que o módulo `service` repassa ao barramento.

use std::collections::BTreeMap;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use futures_util::{FutureExt, StreamExt};
use raiza_core::{
    Capability, CapabilityValue, DeviceId, DeviceInfo, EventStream, Provider, ProviderEvent,
    RaizaError, capability_owner, fuse,
};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use crate::config::ConfigStore;

/// Mudança visível para os clientes, já no modelo fundido (`docs/DBUS.md`, "Sinais").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signal {
    /// Um dispositivo apareceu (ou voltou depois de um `DeviceRemoved`, como atualização).
    DeviceAdded(DeviceInfo),
    /// O dispositivo sumiu (ou vai voltar logo em seguida, como atualização).
    DeviceRemoved(DeviceId),
    /// Uma capacidade mudou.
    CapabilityChanged(DeviceId, Capability),
}

/// Provedor registrado, com o id lido uma vez só.
struct Slot {
    id: String,
    provider: Arc<dyn Provider>,
}

struct Inner {
    /// Em ordem de prioridade (o índice é a prioridade).
    slots: Vec<Slot>,
    /// Por provedor (mesmo índice de `slots`), os dispositivos como ele os anunciou.
    state: Mutex<Vec<BTreeMap<DeviceId, DeviceInfo>>>,
    config: Mutex<ConfigStore>,
    signals: mpsc::UnboundedSender<Signal>,
}

/// O daemon. Clonar é barato (compartilha o mesmo estado).
#[derive(Clone)]
pub struct Daemon {
    inner: Arc<Inner>,
}

/// Tarefas dos provedores em execução. Ao ser descartado, encerra todas.
pub struct Running {
    tasks: Vec<JoinHandle<()>>,
}

impl Running {
    /// Encerra as tarefas dos provedores.
    pub fn shutdown(self) {}
}

impl Drop for Running {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

/// Executa `fut` capturando pânico (pilar 1: um provedor não derruba o daemon).
async fn guard<T>(fut: impl Future<Output = T>) -> Result<T, String> {
    AssertUnwindSafe(fut)
        .catch_unwind()
        .await
        .map_err(|p| panic_message(p.as_ref()))
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "pânico sem mensagem".to_owned())
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Resultado de olhar o próximo evento sem esperar.
enum Peek {
    Event(ProviderEvent),
    NotReady,
    Ended,
    Panicked(String),
}

impl Daemon {
    /// Cria o daemon com os provedores **em ordem de prioridade** (`docs/ARQUITETURA.md`).
    ///
    /// Um provedor cujo `id()` entra em pânico, ou com id repetido, é descartado com um log.
    /// Devolve também o canal de [`Signal`]s.
    pub fn new(
        providers: Vec<Box<dyn Provider>>,
        config: ConfigStore,
    ) -> (Daemon, mpsc::UnboundedReceiver<Signal>) {
        let mut slots: Vec<Slot> = Vec::new();
        for provider in providers {
            let provider: Arc<dyn Provider> = Arc::from(provider);
            let id = match std::panic::catch_unwind(AssertUnwindSafe(|| provider.id().to_owned())) {
                Ok(id) => id,
                Err(p) => {
                    error!(
                        panic = panic_message(p.as_ref()),
                        "provedor sem id; descartado"
                    );
                    continue;
                }
            };
            if slots.iter().any(|s| s.id == id) {
                warn!(
                    provider = id,
                    "provedor registrado duas vezes; ignorando a cópia"
                );
                continue;
            }
            info!(
                provider = id,
                prioridade = slots.len() + 1,
                "provedor registrado"
            );
            slots.push(Slot { id, provider });
        }
        let (tx, rx) = mpsc::unbounded_channel();
        let state = Mutex::new(vec![BTreeMap::new(); slots.len()]);
        let daemon = Daemon {
            inner: Arc::new(Inner {
                slots,
                state,
                config: Mutex::new(config),
                signals: tx,
            }),
        };
        (daemon, rx)
    }

    /// Ids dos provedores registrados, em ordem de prioridade.
    pub fn provider_ids(&self) -> Vec<String> {
        self.inner.slots.iter().map(|s| s.id.clone()).collect()
    }

    /// Inicia uma tarefa por provedor: `discover`, depois o fluxo de `events`.
    /// Precisa ser chamado dentro de um runtime tokio.
    pub fn start(&self) -> Running {
        let tasks = (0..self.inner.slots.len())
            .map(|idx| {
                let daemon = self.clone();
                tokio::spawn(async move { daemon.run_provider(idx).await })
            })
            .collect();
        Running { tasks }
    }

    /// Todos os dispositivos, fundidos.
    pub fn list_devices(&self) -> Vec<DeviceInfo> {
        let state = lock(&self.inner.state);
        fuse(state.iter().flat_map(|m| m.values().cloned()))
    }

    /// Um dispositivo, fundido.
    pub fn get_device(&self, id: &DeviceId) -> Result<DeviceInfo, RaizaError> {
        fused(&lock(&self.inner.state), id).ok_or_else(|| RaizaError::NotFound(id.clone()))
    }

    /// Valida `value` contra o dispositivo fundido, manda ao provedor dono da capacidade
    /// e, se deu certo, guarda o valor para reaplicar quando o dispositivo voltar.
    pub async fn set_capability(
        &self,
        id: &DeviceId,
        value: CapabilityValue,
    ) -> Result<(), RaizaError> {
        let idx = {
            let state = lock(&self.inner.state);
            let device = fused(&state, id).ok_or_else(|| RaizaError::NotFound(id.clone()))?;
            // Pilar 1: nada sai sem passar pela validação do core.
            device.validate(&value)?;
            self.owner(&state, id, &value)
                .ok_or(RaizaError::Unsupported(value.kind()))?
        };
        self.write(idx, id, &value).await?;
        if let Err(e) = lock(&self.inner.config).remember(id, &value) {
            warn!(error = %e, "escrita aplicada, mas não foi possível salvar a configuração");
        }
        Ok(())
    }

    /// Índice do provedor dono da capacidade de `value` no dispositivo `id`.
    fn owner(
        &self,
        state: &[BTreeMap<DeviceId, DeviceInfo>],
        id: &DeviceId,
        value: &CapabilityValue,
    ) -> Option<usize> {
        let owner = capability_owner(state.iter().filter_map(|m| m.get(id)), id, value.kind())?;
        self.inner.slots.iter().position(|s| s.id == owner)
    }

    /// Chama `write` do provedor `idx`, isolado, e atualiza o estado em cache.
    async fn write(
        &self,
        idx: usize,
        id: &DeviceId,
        value: &CapabilityValue,
    ) -> Result<(), RaizaError> {
        let slot = &self.inner.slots[idx];
        match guard(slot.provider.write(id, value.clone())).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e),
            Err(panic) => {
                error!(provider = slot.id, %panic, "provedor entrou em pânico ao escrever");
                return Err(RaizaError::Io(format!(
                    "o provedor {} falhou ao escrever",
                    slot.id
                )));
            }
        }
        // O provedor deve emitir `CapabilityChanged`; mantemos o cache certo mesmo se não emitir.
        self.update(idx, id, |info| {
            if let Some(cap) = info.capability_mut(value.kind()) {
                let _ = cap.apply(value);
            }
        });
        Ok(())
    }

    /// Altera o `DeviceInfo` do provedor `idx` e emite a diferença no modelo fundido.
    /// Não faz nada se o provedor não conhece o dispositivo.
    fn update(&self, idx: usize, id: &DeviceId, change: impl FnOnce(&mut DeviceInfo)) {
        let mut state = lock(&self.inner.state);
        let before = fused(&state, id);
        let Some(info) = state[idx].get_mut(id) else {
            return;
        };
        change(info);
        let after = fused(&state, id);
        self.emit_diff(id, before, after);
    }

    async fn run_provider(&self, idx: usize) {
        let slot = &self.inner.slots[idx];
        let name = slot.id.as_str();
        match guard(slot.provider.discover()).await {
            Ok(Ok(devices)) => {
                info!(provider = name, n = devices.len(), "descoberta concluída");
                for device in devices {
                    self.device_added(idx, device).await;
                }
            }
            Ok(Err(e)) => warn!(provider = name, error = %e, "falha na descoberta"),
            Err(panic) => {
                error!(provider = name, %panic, "provedor entrou em pânico na descoberta; desligado");
                return;
            }
        }

        let mut stream = match std::panic::catch_unwind(AssertUnwindSafe(|| slot.provider.events()))
        {
            Ok(stream) => stream,
            Err(p) => {
                error!(
                    provider = name,
                    panic = panic_message(p.as_ref()),
                    "provedor entrou em pânico ao abrir eventos; desligado"
                );
                self.drop_provider(idx);
                return;
            }
        };

        let mut peeked: Option<ProviderEvent> = None;
        loop {
            let event = match peeked.take() {
                Some(event) => event,
                None => match guard(stream.next()).await {
                    Ok(Some(event)) => event,
                    Ok(None) => {
                        info!(provider = name, "fluxo de eventos encerrado");
                        return;
                    }
                    Err(panic) => {
                        self.provider_panicked(idx, &panic);
                        return;
                    }
                },
            };
            debug!(provider = name, ?event, "evento");
            match event {
                ProviderEvent::DeviceAdded { device } => self.device_added(idx, device).await,
                ProviderEvent::CapabilityChanged { id, capability } => {
                    self.update(idx, &id, |info| {
                        match info.capability_mut(capability.kind()) {
                            Some(cap) => *cap = capability,
                            None => info.capabilities.push(capability),
                        }
                    });
                }
                ProviderEvent::DeviceRemoved { id } => {
                    // Um `DeviceAdded` do mesmo id logo em seguida é atualização
                    // (`docs/ARQUITETURA.md`, "Atualização de um dispositivo").
                    match peek(&mut stream) {
                        Peek::Event(ProviderEvent::DeviceAdded { device }) if device.id == id => {
                            self.replace(idx, device);
                        }
                        Peek::Event(other) => {
                            self.device_removed(idx, &id);
                            peeked = Some(other);
                        }
                        Peek::NotReady => self.device_removed(idx, &id),
                        Peek::Ended => {
                            self.device_removed(idx, &id);
                            info!(provider = name, "fluxo de eventos encerrado");
                            return;
                        }
                        Peek::Panicked(panic) => {
                            self.device_removed(idx, &id);
                            self.provider_panicked(idx, &panic);
                            return;
                        }
                    }
                }
            }
        }
    }

    fn provider_panicked(&self, idx: usize, panic: &str) {
        error!(
            provider = self.inner.slots[idx].id,
            %panic,
            "provedor entrou em pânico; desligado e seus dispositivos retirados"
        );
        self.drop_provider(idx);
    }

    /// Retira todos os dispositivos de um provedor que parou de funcionar: o estado dele
    /// não é mais confiável, e as escritas passam a ir para o próximo dono, se houver.
    fn drop_provider(&self, idx: usize) {
        let mut state = lock(&self.inner.state);
        let ids: Vec<DeviceId> = state[idx].keys().cloned().collect();
        for id in ids {
            let before = fused(&state, &id);
            state[idx].remove(&id);
            let after = fused(&state, &id);
            self.emit_diff(&id, before, after);
        }
    }

    fn normalize(&self, idx: usize, mut device: DeviceInfo) -> DeviceInfo {
        // A fusão e o roteamento dependem de `providers` = só o próprio id.
        device.providers = vec![self.inner.slots[idx].id.clone()];
        device
    }

    /// Substitui o `DeviceInfo` do provedor sem reaplicar nada.
    fn replace(&self, idx: usize, device: DeviceInfo) {
        let device = self.normalize(idx, device);
        let id = device.id.clone();
        let mut state = lock(&self.inner.state);
        let before = fused(&state, &id);
        state[idx].insert(id.clone(), device);
        let after = fused(&state, &id);
        self.emit_diff(&id, before, after);
    }

    async fn device_added(&self, idx: usize, device: DeviceInfo) {
        let device = self.normalize(idx, device);
        let id = device.id.clone();
        let writes = {
            let mut state = lock(&self.inner.state);
            let before = fused(&state, &id);
            let already_known = state[idx].insert(id.clone(), device).is_some();
            let after = fused(&state, &id);
            self.emit_diff(&id, before, after.clone());
            if already_known {
                // `DeviceAdded` repetido = substituição do `DeviceInfo` deste provedor.
                Vec::new()
            } else {
                self.pending_reapply(&state, idx, &id, after.as_ref())
            }
        };
        for value in writes {
            let name = &self.inner.slots[idx].id;
            match self.write(idx, &id, &value).await {
                Ok(()) => info!(provider = name, %id, ?value, "configuração reaplicada"),
                Err(e) => warn!(provider = name, %id, ?value, error = %e, "falha ao reaplicar"),
            }
        }
    }

    /// Valores salvos que o provedor `idx` deve receber porque agora é o dono da
    /// capacidade, já validados e só os que mudariam algo.
    fn pending_reapply(
        &self,
        state: &[BTreeMap<DeviceId, DeviceInfo>],
        idx: usize,
        id: &DeviceId,
        device: Option<&DeviceInfo>,
    ) -> Vec<CapabilityValue> {
        let Some(device) = device else {
            return Vec::new();
        };
        let saved = lock(&self.inner.config).saved(id);
        saved
            .into_iter()
            .filter(|value| self.owner(state, id, value) == Some(idx))
            .filter(|value| {
                let Some(cap) = device.capability(value.kind()) else {
                    return false;
                };
                let mut after = cap.clone();
                match after.apply(value) {
                    Ok(()) => after != *cap,
                    Err(e) => {
                        warn!(%id, ?value, error = %e, "valor salvo não vale mais; não reaplicado");
                        false
                    }
                }
            })
            .collect()
    }

    fn device_removed(&self, idx: usize, id: &DeviceId) {
        let mut state = lock(&self.inner.state);
        let before = fused(&state, id);
        if state[idx].remove(id).is_none() {
            return;
        }
        let after = fused(&state, id);
        self.emit_diff(id, before, after);
    }

    /// Emite a diferença entre dois estados fundidos (`docs/DBUS.md`, "Sinais"):
    /// - surgiu: `DeviceAdded`; sumiu: `DeviceRemoved`;
    /// - só capacidades novas ou com outro valor: um `CapabilityChanged` por capacidade;
    /// - qualquer outra mudança (nome, tipo, capacidade perdida): `DeviceRemoved` + `DeviceAdded`.
    ///
    /// Chamado com o estado travado, para os sinais saírem na ordem das mudanças.
    fn emit_diff(&self, id: &DeviceId, before: Option<DeviceInfo>, after: Option<DeviceInfo>) {
        let send = |s| {
            // Sem receptor, o daemon está encerrando: não há a quem avisar.
            let _ = self.inner.signals.send(s);
        };
        match (before, after) {
            (None, None) => {}
            (None, Some(new)) => send(Signal::DeviceAdded(new)),
            (Some(_), None) => send(Signal::DeviceRemoved(id.clone())),
            (Some(old), Some(new)) => {
                if old == new {
                    return;
                }
                let same_meta =
                    old.name == new.name && old.vendor == new.vendor && old.kind == new.kind;
                let nothing_lost = old
                    .capabilities
                    .iter()
                    .all(|c| new.capability(c.kind()).is_some());
                if same_meta && nothing_lost {
                    for cap in &new.capabilities {
                        if old.capability(cap.kind()) != Some(cap) {
                            send(Signal::CapabilityChanged(id.clone(), cap.clone()));
                        }
                    }
                } else {
                    send(Signal::DeviceRemoved(id.clone()));
                    send(Signal::DeviceAdded(new));
                }
            }
        }
    }
}

/// O dispositivo `id` fundido entre os provedores, em ordem de prioridade.
fn fused(state: &[BTreeMap<DeviceId, DeviceInfo>], id: &DeviceId) -> Option<DeviceInfo> {
    fuse(state.iter().filter_map(|m| m.get(id)).cloned()).pop()
}

/// Próximo evento se já estiver disponível, sem esperar.
fn peek(stream: &mut EventStream) -> Peek {
    match AssertUnwindSafe(stream.next())
        .catch_unwind()
        .now_or_never()
    {
        None => Peek::NotReady,
        Some(Ok(Some(event))) => Peek::Event(event),
        Some(Ok(None)) => Peek::Ended,
        Some(Err(p)) => Peek::Panicked(panic_message(p.as_ref())),
    }
}
