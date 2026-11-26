//! Modelo dos dispositivos, sem GTK: estado, aplicação dos eventos do daemon,
//! agrupamento por tipo e seleção.
//!
//! O par `DeviceRemoved` + `DeviceAdded` com o mesmo id é uma **atualização**
//! (`docs/DBUS.md`). Para não piscar a interface, a remoção não acontece na hora: o
//! dispositivo fica marcado como "saindo" e só é apagado quando a interface confirma,
//! depois de um prazo curto ([`REMOVAL_GRACE`]). Se o `DeviceAdded` chegar antes, a
//! marca some e o dispositivo é só substituído.

use std::collections::BTreeMap;
use std::time::Duration;

use raiza_core::{Capability, CapabilityKind, DeviceId, DeviceInfo, DeviceKind, ProviderEvent};

/// Quanto esperar por um `DeviceAdded` antes de apagar um dispositivo removido.
pub const REMOVAL_GRACE: Duration = Duration::from_millis(500);

/// Ordem dos grupos na barra lateral: o foco do projeto primeiro (pilar 3).
pub const KIND_ORDER: [DeviceKind; 7] = [
    DeviceKind::Mouse,
    DeviceKind::Keyboard,
    DeviceKind::Headset,
    DeviceKind::Microphone,
    DeviceKind::Gamepad,
    DeviceKind::RgbController,
    DeviceKind::Other,
];

/// Ficha que identifica uma remoção pendente. A confirmação só vale para a remoção
/// que a gerou, e não para uma remoção mais nova do mesmo id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemovalToken(u64);

/// O que mudou depois de aplicar um evento. A interface usa isto para decidir o que
/// redesenhar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// Nada mudou (evento de um dispositivo desconhecido, por exemplo).
    None,
    /// Dispositivo novo.
    Added(DeviceId),
    /// O dispositivo foi substituído (nome, tipo ou capacidades podem ter mudado).
    Updated(DeviceId),
    /// Só o valor de uma capacidade mudou.
    CapabilityChanged(DeviceId, CapabilityKind),
    /// O dispositivo vai sair. Chame [`Devices::confirm_removal`] com a ficha depois
    /// de [`REMOVAL_GRACE`].
    RemovalPending(DeviceId, RemovalToken),
}

#[derive(Debug, Clone)]
struct Entry {
    device: DeviceInfo,
    removing: Option<RemovalToken>,
}

/// Os dispositivos conhecidos e qual está selecionado.
#[derive(Debug, Default)]
pub struct Devices {
    entries: BTreeMap<DeviceId, Entry>,
    selected: Option<DeviceId>,
    next_token: u64,
}

impl Devices {
    /// Troca tudo pela lista vinda de `ListDevices`. A seleção continua se o
    /// dispositivo ainda existir.
    pub fn reset(&mut self, devices: Vec<DeviceInfo>) {
        self.entries = devices
            .into_iter()
            .map(|device| {
                let entry = Entry {
                    device,
                    removing: None,
                };
                (entry.device.id.clone(), entry)
            })
            .collect();
        if self
            .selected
            .as_ref()
            .is_some_and(|id| !self.entries.contains_key(id))
        {
            self.selected = None;
        }
    }

    /// Aplica um evento do daemon.
    pub fn apply(&mut self, event: ProviderEvent) -> Change {
        match event {
            ProviderEvent::DeviceAdded { device } => {
                let id = device.id.clone();
                let previous = self.entries.insert(
                    id.clone(),
                    Entry {
                        device,
                        removing: None,
                    },
                );
                if previous.is_some() {
                    Change::Updated(id)
                } else {
                    Change::Added(id)
                }
            }
            ProviderEvent::DeviceRemoved { id } => match self.entries.get_mut(&id) {
                Some(entry) => {
                    let token = RemovalToken(self.next_token);
                    self.next_token += 1;
                    entry.removing = Some(token);
                    Change::RemovalPending(id, token)
                }
                None => Change::None,
            },
            ProviderEvent::CapabilityChanged { id, capability } => {
                let Some(entry) = self.entries.get_mut(&id) else {
                    return Change::None;
                };
                let kind = capability.kind();
                let caps = &mut entry.device.capabilities;
                match caps.iter_mut().find(|c| c.kind() == kind) {
                    Some(current) if *current == capability => return Change::None,
                    Some(current) => *current = capability,
                    // Um provedor novo passou a atender o dispositivo (docs/DBUS.md).
                    None => caps.push(capability),
                }
                Change::CapabilityChanged(id, kind)
            }
        }
    }

    /// Apaga o dispositivo se a remoção `token` ainda estiver pendente. Devolve `true`
    /// se apagou. Um `DeviceAdded` no meio do caminho cancela a remoção.
    pub fn confirm_removal(&mut self, id: &DeviceId, token: RemovalToken) -> bool {
        let pending = self
            .entries
            .get(id)
            .is_some_and(|e| e.removing == Some(token));
        if pending {
            self.entries.remove(id);
            if self.selected.as_ref() == Some(id) {
                self.selected = None;
            }
        }
        pending
    }

    /// Um dispositivo pelo id (inclusive um que está saindo).
    pub fn get(&self, id: &DeviceId) -> Option<&DeviceInfo> {
        self.entries.get(id).map(|e| &e.device)
    }

    /// Quantos dispositivos existem.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Se não há nenhum dispositivo.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Dispositivos agrupados por tipo, na ordem de [`KIND_ORDER`], e dentro de cada
    /// grupo por nome (sem diferenciar maiúsculas) e depois por id. Grupos vazios não
    /// aparecem.
    pub fn groups(&self) -> Vec<(DeviceKind, Vec<&DeviceInfo>)> {
        KIND_ORDER
            .iter()
            .filter_map(|&kind| {
                let mut devices: Vec<&DeviceInfo> = self
                    .entries
                    .values()
                    .map(|e| &e.device)
                    .filter(|d| d.kind == kind)
                    .collect();
                devices.sort_by_cached_key(|d| (d.name.to_lowercase(), d.id.clone()));
                (!devices.is_empty()).then_some((kind, devices))
            })
            .collect()
    }

    /// Ids na ordem em que aparecem na barra lateral.
    pub fn ordered_ids(&self) -> Vec<DeviceId> {
        self.groups()
            .into_iter()
            .flat_map(|(_, devices)| devices.into_iter().map(|d| d.id.clone()))
            .collect()
    }

    /// Dispositivo selecionado.
    pub fn selected(&self) -> Option<&DeviceId> {
        self.selected.as_ref()
    }

    /// Seleciona um dispositivo. Ids desconhecidos limpam a seleção.
    pub fn select(&mut self, id: Option<DeviceId>) {
        self.selected = id.filter(|id| self.entries.contains_key(id));
    }

    /// Se nada estiver selecionado, seleciona o primeiro da barra lateral.
    /// Devolve o selecionado.
    pub fn select_first_if_none(&mut self) -> Option<&DeviceId> {
        if self.selected.is_none() {
            self.selected = self.ordered_ids().into_iter().next();
        }
        self.selected.as_ref()
    }
}

/// Capacidades na ordem em que a página de detalhes as mostra (a de
/// [`CapabilityKind::ALL`]), independente da ordem em que o daemon mandou.
pub fn ordered_capabilities(device: &DeviceInfo) -> Vec<&Capability> {
    let mut caps: Vec<&Capability> = device.capabilities.iter().collect();
    caps.sort_by_key(|c| c.kind());
    caps
}

/// Os tipos de capacidade do dispositivo, na ordem da página. Se este conjunto não
/// muda, a página pode só atualizar as linhas em vez de recriá-las.
pub fn capability_kinds(device: &DeviceInfo) -> Vec<CapabilityKind> {
    ordered_capabilities(device)
        .into_iter()
        .map(Capability::kind)
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use raiza_core::{ConnectionKind, Level};

    use super::*;

    pub(crate) fn device(key: &str, name: &str, kind: DeviceKind) -> DeviceInfo {
        DeviceInfo {
            id: DeviceId::Other {
                provider: "mock".into(),
                key: key.into(),
            },
            name: name.into(),
            vendor: None,
            kind,
            providers: vec!["mock".into()],
            capabilities: vec![
                Capability::Connection {
                    medium: ConnectionKind::Wired,
                    connected: true,
                    writable: false,
                },
                Capability::Battery {
                    percent: 50,
                    charging: false,
                },
            ],
        }
    }

    fn id(key: &str) -> DeviceId {
        DeviceId::Other {
            provider: "mock".into(),
            key: key.into(),
        }
    }

    fn com(devices: Vec<DeviceInfo>) -> Devices {
        let mut m = Devices::default();
        m.reset(devices);
        m
    }

    #[test]
    fn agrupa_por_tipo_na_ordem_e_por_nome() {
        let m = com(vec![
            device("1", "zeta", DeviceKind::Mouse),
            device("2", "Teclado", DeviceKind::Keyboard),
            device("3", "Alfa", DeviceKind::Mouse),
            device("4", "Fone", DeviceKind::Headset),
            device("5", "Coisa", DeviceKind::Other),
            device("6", "beta", DeviceKind::Mouse),
        ]);
        let groups: Vec<(DeviceKind, Vec<&str>)> = m
            .groups()
            .into_iter()
            .map(|(k, ds)| (k, ds.iter().map(|d| d.name.as_str()).collect()))
            .collect();
        assert_eq!(
            groups,
            [
                (DeviceKind::Mouse, vec!["Alfa", "beta", "zeta"]),
                (DeviceKind::Keyboard, vec!["Teclado"]),
                (DeviceKind::Headset, vec!["Fone"]),
                (DeviceKind::Other, vec!["Coisa"]),
            ]
        );
        assert_eq!(m.ordered_ids()[0], id("3"));
    }

    #[test]
    fn nomes_iguais_desempatam_pelo_id() {
        let m = com(vec![
            device("b", "Mouse", DeviceKind::Mouse),
            device("a", "Mouse", DeviceKind::Mouse),
        ]);
        assert_eq!(m.ordered_ids(), [id("a"), id("b")]);
    }

    #[test]
    fn todos_os_tipos_tem_lugar_na_ordem() {
        for kind in [
            DeviceKind::Mouse,
            DeviceKind::Keyboard,
            DeviceKind::Headset,
            DeviceKind::Gamepad,
            DeviceKind::Microphone,
            DeviceKind::RgbController,
            DeviceKind::Other,
        ] {
            assert!(KIND_ORDER.contains(&kind), "{kind:?}");
        }
    }

    #[test]
    fn added_novo_e_repetido() {
        let mut m = Devices::default();
        let d = device("1", "Mouse", DeviceKind::Mouse);
        assert_eq!(
            m.apply(ProviderEvent::DeviceAdded { device: d.clone() }),
            Change::Added(id("1"))
        );
        let mut renomeado = d;
        renomeado.name = "Outro".into();
        assert_eq!(
            m.apply(ProviderEvent::DeviceAdded { device: renomeado }),
            Change::Updated(id("1"))
        );
        assert_eq!(m.len(), 1);
        assert_eq!(m.get(&id("1")).unwrap().name, "Outro");
    }

    #[test]
    fn removed_so_apaga_depois_da_confirmacao() {
        let mut m = com(vec![device("1", "Mouse", DeviceKind::Mouse)]);
        m.select(Some(id("1")));
        let Change::RemovalPending(rid, token) =
            m.apply(ProviderEvent::DeviceRemoved { id: id("1") })
        else {
            panic!("esperava remoção pendente");
        };
        assert_eq!(rid, id("1"));
        // Ainda visível e selecionado durante o prazo.
        assert_eq!(m.len(), 1);
        assert_eq!(m.selected(), Some(&id("1")));

        assert!(m.confirm_removal(&id("1"), token));
        assert!(m.is_empty());
        assert_eq!(m.selected(), None);
        // Confirmar de novo não faz nada.
        assert!(!m.confirm_removal(&id("1"), token));
    }

    #[test]
    fn par_removed_added_e_atualizacao() {
        let mut m = com(vec![device("1", "Mouse", DeviceKind::Mouse)]);
        m.select(Some(id("1")));
        let Change::RemovalPending(_, token) =
            m.apply(ProviderEvent::DeviceRemoved { id: id("1") })
        else {
            panic!("esperava remoção pendente");
        };
        let mut novo = device("1", "Mouse novo", DeviceKind::Mouse);
        novo.capabilities.pop();
        assert_eq!(
            m.apply(ProviderEvent::DeviceAdded { device: novo }),
            Change::Updated(id("1"))
        );
        // O prazo vence, mas a remoção foi cancelada: nada some, a página continua.
        assert!(!m.confirm_removal(&id("1"), token));
        assert_eq!(m.selected(), Some(&id("1")));
        let d = m.get(&id("1")).unwrap();
        assert_eq!(d.name, "Mouse novo");
        assert_eq!(d.capabilities.len(), 1);
    }

    #[test]
    fn ficha_antiga_nao_apaga_remocao_nova() {
        let mut m = com(vec![device("1", "Mouse", DeviceKind::Mouse)]);
        let Change::RemovalPending(_, velha) =
            m.apply(ProviderEvent::DeviceRemoved { id: id("1") })
        else {
            panic!();
        };
        m.apply(ProviderEvent::DeviceAdded {
            device: device("1", "Mouse", DeviceKind::Mouse),
        });
        let Change::RemovalPending(_, nova) = m.apply(ProviderEvent::DeviceRemoved { id: id("1") })
        else {
            panic!();
        };
        assert_ne!(velha, nova);
        assert!(!m.confirm_removal(&id("1"), velha));
        assert_eq!(m.len(), 1);
        assert!(m.confirm_removal(&id("1"), nova));
        assert!(m.is_empty());
    }

    #[test]
    fn eventos_de_dispositivo_desconhecido_sao_ignorados() {
        let mut m = Devices::default();
        assert_eq!(
            m.apply(ProviderEvent::DeviceRemoved { id: id("x") }),
            Change::None
        );
        assert_eq!(
            m.apply(ProviderEvent::CapabilityChanged {
                id: id("x"),
                capability: Capability::Mute {
                    muted: true,
                    writable: true
                },
            }),
            Change::None
        );
        assert!(m.is_empty());
    }

    #[test]
    fn capability_changed_substitui_ou_acrescenta() {
        let mut m = com(vec![device("1", "Fone", DeviceKind::Headset)]);
        let bateria = Capability::Battery {
            percent: 10,
            charging: true,
        };
        assert_eq!(
            m.apply(ProviderEvent::CapabilityChanged {
                id: id("1"),
                capability: bateria.clone(),
            }),
            Change::CapabilityChanged(id("1"), CapabilityKind::Battery)
        );
        let d = m.get(&id("1")).unwrap();
        assert_eq!(d.capability(CapabilityKind::Battery), Some(&bateria));
        assert_eq!(d.capabilities.len(), 2);

        // Valor igual: nada a redesenhar.
        assert_eq!(
            m.apply(ProviderEvent::CapabilityChanged {
                id: id("1"),
                capability: bateria,
            }),
            Change::None
        );

        // Capacidade nova (outro provedor passou a atender).
        let volume = Capability::Volume {
            level: Level {
                current: 30,
                min: 0,
                max: 100,
                step: 1,
            },
            writable: true,
        };
        assert_eq!(
            m.apply(ProviderEvent::CapabilityChanged {
                id: id("1"),
                capability: volume,
            }),
            Change::CapabilityChanged(id("1"), CapabilityKind::Volume)
        );
        assert_eq!(m.get(&id("1")).unwrap().capabilities.len(), 3);
    }

    #[test]
    fn reset_mantem_selecao_valida() {
        let mut m = com(vec![
            device("1", "A", DeviceKind::Mouse),
            device("2", "B", DeviceKind::Mouse),
        ]);
        m.select(Some(id("2")));
        m.reset(vec![device("2", "B", DeviceKind::Mouse)]);
        assert_eq!(m.selected(), Some(&id("2")));
        m.reset(vec![device("1", "A", DeviceKind::Mouse)]);
        assert_eq!(m.selected(), None);
    }

    #[test]
    fn selecao() {
        let mut m = com(vec![
            device("1", "Teclado", DeviceKind::Keyboard),
            device("2", "Mouse", DeviceKind::Mouse),
        ]);
        m.select(Some(id("nada")));
        assert_eq!(m.selected(), None);
        // O primeiro da barra lateral é o mouse (grupo de mouses vem antes).
        assert_eq!(m.select_first_if_none(), Some(&id("2")));
        m.select(Some(id("1")));
        assert_eq!(m.select_first_if_none(), Some(&id("1")));
        assert_eq!(Devices::default().select_first_if_none(), None);
    }

    #[test]
    fn capacidades_em_ordem_fixa() {
        let d = device("1", "Mouse", DeviceKind::Mouse);
        // O dispositivo de teste traz conexão antes de bateria.
        assert_eq!(
            capability_kinds(&d),
            [CapabilityKind::Battery, CapabilityKind::Connection]
        );
    }
}
