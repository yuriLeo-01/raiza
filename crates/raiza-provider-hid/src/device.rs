//! Um aparelho aberto por um perfil: identificação, leituras, escritas e relatórios
//! espontâneos.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use raiza_core::{
    Capability, CapabilityKind, CapabilityValue, DeviceId, DeviceInfo, ProviderEvent, RaizaError,
};
use raiza_profiles::Profile;
use raiza_profiles::format::ReportType;
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, info, warn};

use crate::PROVIDER_ID;
use crate::backend::{HidBackend, HidHandle, HidInterface};
use crate::exchange::{self, Expected, InputHub, ReaderGuard};

/// Capacidades que um perfil pode ler.
const READABLE: [CapabilityKind; 3] = [
    CapabilityKind::Dpi,
    CapabilityKind::PollingRate,
    CapabilityKind::Battery,
];

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Se a leitura de `kind` é um relatório que o aparelho manda sozinho: sem pedido e com
/// resposta `input`.
pub(crate) fn is_spontaneous(profile: &Profile, kind: CapabilityKind) -> bool {
    profile.read_request(kind).is_none()
        && profile
            .response_spec(kind)
            .is_some_and(|r| r.report_type == ReportType::Input)
}

/// Se o perfil espera algum input report (e por isso precisa do leitor contínuo).
fn uses_input(profile: &Profile) -> bool {
    let identify = profile
        .identify
        .as_ref()
        .is_some_and(|i| i.response.report_type == ReportType::Input);
    identify
        || READABLE.iter().any(|k| {
            profile
                .response_spec(*k)
                .is_some_and(|r| r.report_type == ReportType::Input)
        })
}

/// Um aparelho (uma conexão) aberto por um perfil.
pub(crate) struct Device {
    pub id: DeviceId,
    pub path: String,
    /// Muda quando o nó hidraw é recriado (replugue).
    pub token: String,
    pub profile: Arc<Profile>,
    handle: Box<dyn HidHandle>,
    hub: Option<Arc<InputHub>>,
    reader: Mutex<Option<ReaderGuard>>,
    /// Leitor contínuo encerrado por erro: o aparelho sumiu ou travou.
    dead: Arc<AtomicBool>,
    /// Identificação confirmada (ou perfil sem `identify`). Sem ela, nada é escrito.
    identified: AtomicBool,
    /// Já anunciado ao daemon (só então emite `CapabilityChanged`).
    announced: AtomicBool,
    /// Uma troca de relatórios por vez com o aparelho.
    io: Mutex<()>,
    /// Estado mais recente de cada capacidade.
    caps: Mutex<Vec<Capability>>,
    /// Capacidades com leitura descrita no perfil que já tiveram uma leitura válida
    /// (pedida ou espontânea). Só elas são anunciadas: um valor inicial do perfil não é
    /// uma leitura, e mostrá-lo como tal engana (ex.: "bateria 0%").
    seen: Mutex<BTreeSet<CapabilityKind>>,
    events: UnboundedSender<ProviderEvent>,
}

impl Device {
    /// Abre a interface, identifica o aparelho e faz as leituras iniciais. `timeout` vale
    /// para cada resposta.
    pub fn open(
        backend: &dyn HidBackend,
        id: DeviceId,
        iface: &HidInterface,
        profile: Arc<Profile>,
        timeout: Duration,
        events: UnboundedSender<ProviderEvent>,
    ) -> Result<Arc<Device>, RaizaError> {
        let handle = backend.open(&iface.path)?;
        let hub = uses_input(&profile).then(|| Arc::new(InputHub::default()));
        // O cache guarda a escrita que o perfil permite; enquanto a identificação não
        // confirmar o aparelho, tudo sai como somente leitura (veja `visible`).
        let caps = profile.capabilities();
        let device = Arc::new(Device {
            id,
            path: iface.path.clone(),
            token: iface.token.clone(),
            profile,
            handle,
            hub,
            reader: Mutex::new(None),
            dead: Arc::new(AtomicBool::new(false)),
            identified: AtomicBool::new(false),
            announced: AtomicBool::new(false),
            io: Mutex::new(()),
            caps: Mutex::new(caps),
            seen: Mutex::new(BTreeSet::new()),
            events,
        });

        if let Some(hub) = &device.hub {
            // Segunda abertura: o leitor contínuo tem a sua cópia dos input reports.
            let reader_handle = backend.open(&iface.path)?;
            let weak = Arc::downgrade(&device);
            let dead = Arc::clone(&device.dead);
            let guard = exchange::spawn_reader(
                iface.path.clone(),
                reader_handle,
                Arc::clone(hub),
                dead,
                move |raw| {
                    if let Some(d) = weak.upgrade() {
                        d.on_input(raw);
                    }
                },
            )?;
            *lock(&device.reader) = Some(guard);
        }

        let identified = device.identify(timeout);
        device.identified.store(identified, Ordering::Relaxed);
        for kind in READABLE {
            if device.profile.response_spec(kind).is_none() || is_spontaneous(&device.profile, kind)
            {
                continue;
            }
            if let Err(e) = device.read_now(kind, timeout) {
                warn!(id = %device.id, %kind, error = %e, "leitura inicial falhou");
            }
        }
        info!(
            id = %device.id,
            path = %device.path,
            profile = %device.profile.name,
            identified,
            verified = device.profile.verified,
            "aparelho aberto pelo perfil"
        );
        Ok(device)
    }

    /// Manda a consulta de identificação e confere a resposta. Sem `identify`, aceita.
    fn identify(&self, timeout: Duration) -> bool {
        let Some(spec) = &self.profile.identify else {
            return true;
        };
        let Some(request) = self.profile.identify_request() else {
            return true;
        };
        let expected = Expected {
            report_type: spec.response.report_type,
            report_id: spec.response.report_id,
            length: spec.response.length,
        };
        let _io = lock(&self.io);
        let result = exchange::exchange(
            self.handle.as_ref(),
            self.hub.as_deref(),
            Some(&request),
            expected,
            timeout,
            |bytes| self.profile.check_identity(bytes),
        );
        match result {
            Ok(()) => true,
            Err(e) => {
                warn!(
                    id = %self.id,
                    profile = %self.profile.name,
                    error = %e,
                    "identificação não confirmada: aparelho somente leitura"
                );
                false
            }
        }
    }

    /// Identificação confirmada.
    pub fn identified(&self) -> bool {
        self.identified.load(Ordering::Relaxed)
    }

    /// O leitor contínuo morreu (aparelho sumiu): a conexão precisa ser reaberta.
    pub fn is_dead(&self) -> bool {
        self.dead.load(Ordering::Relaxed)
    }

    /// Marca como anunciado ao daemon.
    pub fn set_announced(&self) {
        self.announced.store(true, Ordering::Relaxed);
    }

    /// O aparelho como este provedor o anuncia.
    pub fn info(&self) -> DeviceInfo {
        DeviceInfo {
            id: self.id.clone(),
            name: self.profile.name.clone(),
            vendor: Some(self.profile.vendor.clone()),
            kind: self.profile.kind,
            providers: vec![PROVIDER_ID.into()],
            capabilities: lock(&self.caps)
                .iter()
                .filter(|c| self.valid(c.kind()))
                .map(|c| self.visible(c.clone()))
                .collect(),
        }
    }

    /// Estado em cache de `kind`.
    pub fn cached(&self, kind: CapabilityKind) -> Option<Capability> {
        self.raw(kind).map(|c| self.visible(c))
    }

    /// Se `kind` já teve uma leitura válida.
    pub fn seen(&self, kind: CapabilityKind) -> bool {
        lock(&self.seen).contains(&kind)
    }

    /// Se `kind` pode ser anunciada: sem leitura no perfil (ex.: escrita de RGB), ou já lida.
    fn valid(&self, kind: CapabilityKind) -> bool {
        self.profile.response_spec(kind).is_none() || self.seen(kind)
    }

    /// Registra uma leitura válida de `kind`. Devolve `true` se é a primeira.
    fn mark_seen(&self, kind: CapabilityKind) -> bool {
        lock(&self.seen).insert(kind)
    }

    /// A capacidade `kind` acabou de passar a valer: o conjunto anunciado mudou, então o
    /// aparelho é reanunciado (`DeviceAdded` de id conhecido = atualização, docs/DBUS.md).
    fn reannounce(&self) {
        if self.announced.load(Ordering::Relaxed) {
            let _ = self.events.send(ProviderEvent::DeviceAdded {
                device: self.info(),
            });
        }
    }

    /// `cap` como o daemon deve vê-la: somente leitura sem identificação (pilar 1).
    fn visible(&self, mut cap: Capability) -> Capability {
        if !self.identified() {
            cap.set_read_only();
        }
        cap
    }

    /// Guarda `cap` no cache. Devolve a versão visível se o valor mudou.
    fn store(&self, cap: Capability) -> Option<Capability> {
        let mut caps = lock(&self.caps);
        match caps.iter_mut().find(|c| c.kind() == cap.kind()) {
            Some(old) if *old == cap => None,
            Some(old) => {
                *old = cap.clone();
                Some(self.visible(cap))
            }
            None => {
                caps.push(cap.clone());
                Some(self.visible(cap))
            }
        }
    }

    /// Estado em cache de `kind`, sem a trava de identificação (uso interno).
    fn raw(&self, kind: CapabilityKind) -> Option<Capability> {
        lock(&self.caps).iter().find(|c| c.kind() == kind).cloned()
    }

    fn notify(&self, capability: Capability) {
        if self.announced.load(Ordering::Relaxed) {
            let _ = self.events.send(ProviderEvent::CapabilityChanged {
                id: self.id.clone(),
                capability,
            });
        }
    }

    /// Um input report chegou: atualiza as capacidades espontâneas que ele descreve.
    fn on_input(&self, raw: &[u8]) {
        for kind in READABLE {
            if !is_spontaneous(&self.profile, kind) {
                continue;
            }
            let Some(spec) = self.profile.response_spec(kind) else {
                continue;
            };
            let bytes = exchange::normalize(raw, spec.report_id);
            let Ok(cap) = self.profile.parse_response(kind, &bytes) else {
                continue;
            };
            let first = self.mark_seen(kind);
            let changed = self.store(cap);
            if first {
                debug!(id = %self.id, %kind, "primeira leitura espontânea");
                self.reannounce();
            } else if let Some(changed) = changed {
                debug!(id = %self.id, %kind, "relatório espontâneo");
                self.notify(changed);
            }
        }
    }

    /// Lê `kind` direto do aparelho, com repetição até `timeout`. Bloqueia.
    pub fn read_now(
        &self,
        kind: CapabilityKind,
        timeout: Duration,
    ) -> Result<Capability, RaizaError> {
        let spec = self
            .profile
            .response_spec(kind)
            .ok_or(RaizaError::Unsupported(kind))?;
        let expected = Expected {
            report_type: spec.report_type,
            report_id: spec.report_id,
            length: spec.length,
        };
        let request = self.profile.read_request(kind);
        let cap = {
            let _io = lock(&self.io);
            exchange::exchange(
                self.handle.as_ref(),
                self.hub.as_deref(),
                request.as_ref(),
                expected,
                timeout,
                |bytes| self.profile.parse_response(kind, bytes),
            )?
        };
        let first = self.mark_seen(kind);
        let changed = self.store(cap);
        if first {
            self.reannounce();
        } else if let Some(changed) = changed {
            self.notify(changed);
        }
        self.cached(kind).ok_or(RaizaError::Unsupported(kind))
    }

    /// Valida contra o estado mais recente, gera os relatórios e envia. Bloqueia.
    ///
    /// Quem chama já conferiu identificação e `verified`; aqui confere de novo, porque
    /// nada sai sem isso (pilar 1).
    pub fn write_now(&self, value: &CapabilityValue) -> Result<(), RaizaError> {
        let kind = value.kind();
        if !self.identified() || !self.profile.verified {
            return Err(RaizaError::ReadOnly(kind));
        }
        let _io = lock(&self.io);
        let current = self.raw(kind).ok_or(RaizaError::Unsupported(kind))?;
        // Validação da T15 imediatamente antes de enviar.
        let reports = self.profile.build_write(&current, value)?;
        for report in &reports {
            exchange::send(self.handle.as_ref(), report)?;
        }
        let mut updated = current;
        updated.apply(value)?;
        if let Some(changed) = self.store(updated) {
            self.notify(changed);
        }
        Ok(())
    }
}
