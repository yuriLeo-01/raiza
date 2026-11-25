//! Estado conhecido de um provedor e tradução de mudanças em [`ProviderEvent`]s.

use std::collections::{BTreeMap, HashSet};
use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use futures_util::{Stream, StreamExt, stream};
use raiza_core::{CapabilityKind, DeviceInfo, EventStream, ProviderEvent};

/// Uma mudança vista no backend, já convertida para o modelo do Raiza.
#[derive(Debug)]
pub(crate) struct Change {
    /// Chave do objeto no backend (caminho D-Bus).
    pub key: String,
    /// Novo estado; `None` quando o objeto sumiu ou deixou de ser atendido.
    pub info: Option<DeviceInfo>,
}

/// Dispositivos conhecidos, por chave do backend.
#[derive(Debug, Default)]
pub(crate) struct Tracker {
    entries: BTreeMap<String, DeviceInfo>,
}

pub(crate) type SharedTracker = Arc<Mutex<Tracker>>;

pub(crate) fn lock(tracker: &SharedTracker) -> MutexGuard<'_, Tracker> {
    tracker.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Tracker {
    /// Dispositivos atuais, sem ids repetidos (vence a menor chave).
    pub fn infos(&self) -> Vec<DeviceInfo> {
        let mut seen = HashSet::new();
        self.entries
            .values()
            .filter(|i| seen.insert(i.id.clone()))
            .cloned()
            .collect()
    }

    /// Cópia do estado atual, por chave (para um `sync` que não muda nada).
    pub fn snapshot(&self) -> Vec<(String, DeviceInfo)> {
        self.entries
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Troca todo o estado por `all` e devolve os eventos que levam do antigo ao novo.
    pub fn sync(&mut self, all: Vec<(String, DeviceInfo)>) -> Vec<ProviderEvent> {
        let keys: HashSet<&String> = all.iter().map(|(k, _)| k).collect();
        let stale: Vec<String> = self
            .entries
            .keys()
            .filter(|k| !keys.contains(k))
            .cloned()
            .collect();
        let mut events = Vec::new();
        for key in stale {
            events.extend(self.apply(Change { key, info: None }));
        }
        for (key, info) in all {
            events.extend(self.apply(Change {
                key,
                info: Some(info),
            }));
        }
        events
    }

    /// Aplica uma mudança e devolve os eventos correspondentes.
    ///
    /// Mudanças só de valores viram [`ProviderEvent::CapabilityChanged`]. Mudanças de
    /// estrutura (nome, tipo, capacidades que surgem ou somem) reenviam
    /// [`ProviderEvent::DeviceAdded`] com o dispositivo completo, que substitui o anterior.
    pub fn apply(&mut self, change: Change) -> Vec<ProviderEvent> {
        let Change { key, info } = change;
        let old = match &info {
            Some(new) => self.entries.insert(key, new.clone()),
            None => self.entries.remove(&key),
        };
        let mut events = Vec::new();
        match (old, info) {
            (None, None) => {}
            (Some(old), None) => events.extend(self.removed(old)),
            (None, Some(new)) => events.push(ProviderEvent::DeviceAdded { device: new }),
            (Some(old), Some(new)) if old == new => {}
            (Some(old), Some(new)) if old.id != new.id => {
                events.extend(self.removed(old));
                events.push(ProviderEvent::DeviceAdded { device: new });
            }
            (Some(old), Some(new)) => {
                let kinds = |d: &DeviceInfo| -> Vec<CapabilityKind> {
                    d.capabilities.iter().map(|c| c.kind()).collect()
                };
                let same_shape = old.name == new.name
                    && old.vendor == new.vendor
                    && old.kind == new.kind
                    && kinds(&old) == kinds(&new);
                if same_shape {
                    for cap in &new.capabilities {
                        if old.capability(cap.kind()) != Some(cap) {
                            events.push(ProviderEvent::CapabilityChanged {
                                id: new.id.clone(),
                                capability: cap.clone(),
                            });
                        }
                    }
                } else {
                    events.push(ProviderEvent::DeviceAdded { device: new });
                }
            }
        }
        events
    }

    /// `DeviceRemoved` para `old`, a não ser que outra chave ainda tenha o mesmo id.
    fn removed(&self, old: DeviceInfo) -> Option<ProviderEvent> {
        let still_there = self.entries.values().any(|i| i.id == old.id);
        (!still_there).then_some(ProviderEvent::DeviceRemoved { id: old.id })
    }
}

/// Monta o fluxo de eventos de um provedor.
///
/// `setup` se inscreve nos sinais do backend **antes** de tirar o retrato inicial (para
/// não perder nada no meio) e devolve os dois. O retrato é comparado com o que o
/// `discover` já entregou, e as diferenças saem primeiro. Se `setup` devolver `None`
/// (barramento indisponível), o fluxo termina vazio.
pub(crate) fn event_stream<F, S>(tracker: SharedTracker, setup: F) -> EventStream
where
    F: Future<Output = Option<(Vec<(String, DeviceInfo)>, S)>> + Send + 'static,
    S: Stream<Item = Change> + Send + 'static,
{
    let stream = stream::once(setup).flat_map(move |ready| match ready {
        None => stream::empty().boxed(),
        Some((snapshot, changes)) => {
            let initial = lock(&tracker).sync(snapshot);
            let tracker = tracker.clone();
            stream::iter(initial)
                .chain(changes.flat_map(move |c| stream::iter(lock(&tracker).apply(c))))
                .boxed()
        }
    });
    Box::pin(stream)
}

#[cfg(test)]
mod tests {
    use raiza_core::{Capability, ConnectionKind, DeviceId, DeviceKind};

    use super::*;

    fn info(mac: &str, connected: bool, battery: Option<u8>) -> DeviceInfo {
        let mut capabilities = vec![Capability::Connection {
            medium: ConnectionKind::Bluetooth,
            connected,
            writable: true,
        }];
        if let Some(percent) = battery {
            capabilities.push(Capability::Battery {
                percent,
                charging: false,
            });
        }
        DeviceInfo {
            id: DeviceId::bluetooth(mac).unwrap(),
            name: "Fone".into(),
            vendor: None,
            kind: DeviceKind::Headset,
            providers: vec!["bluez".into()],
            capabilities,
        }
    }

    const MAC: &str = "AA:BB:CC:DD:EE:FF";

    #[test]
    fn adiciona_muda_e_remove() {
        let mut t = Tracker::default();
        let add = t.apply(Change {
            key: "a".into(),
            info: Some(info(MAC, false, Some(50))),
        });
        assert!(matches!(add[..], [ProviderEvent::DeviceAdded { .. }]));

        // Igual: nada.
        assert!(
            t.apply(Change {
                key: "a".into(),
                info: Some(info(MAC, false, Some(50))),
            })
            .is_empty()
        );

        let ev = t.apply(Change {
            key: "a".into(),
            info: Some(info(MAC, true, Some(40))),
        });
        assert_eq!(ev.len(), 2);
        assert!(
            ev.iter()
                .all(|e| matches!(e, ProviderEvent::CapabilityChanged { .. }))
        );

        // A bateria sumiu: reanuncia o dispositivo.
        let ev = t.apply(Change {
            key: "a".into(),
            info: Some(info(MAC, false, None)),
        });
        assert!(matches!(ev[..], [ProviderEvent::DeviceAdded { .. }]));

        let ev = t.apply(Change {
            key: "a".into(),
            info: None,
        });
        assert_eq!(
            ev,
            [ProviderEvent::DeviceRemoved {
                id: DeviceId::bluetooth(MAC).unwrap()
            }]
        );
    }

    #[test]
    fn sync_remove_o_que_sumiu_e_nao_duplica_ids() {
        let mut t = Tracker::default();
        t.sync(vec![
            ("a".into(), info(MAC, true, None)),
            ("b".into(), info(MAC, true, None)),
        ]);
        assert_eq!(t.infos().len(), 1);
        // "a" sai, mas "b" tem o mesmo id: nada de DeviceRemoved.
        let ev = t.sync(vec![("b".into(), info(MAC, true, None))]);
        assert!(ev.is_empty());
        let ev = t.sync(vec![]);
        assert!(matches!(ev[..], [ProviderEvent::DeviceRemoved { .. }]));
    }
}
