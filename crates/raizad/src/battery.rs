//! Aviso de bateria fraca pelas notificações da área de trabalho.
//!
//! O daemon roda o tempo todo na sessão, então avisa mesmo com o app fechado. Só avisa das
//! baterias que o ambiente gráfico não conhece: as do UPower (e do BlueZ, que publica no
//! UPower) o GNOME e o KDE já vigiam, e avisar de novo seria duplicado.

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use raiza_core::{Capability, DeviceId};
use raiza_i18n::{fill, gettext};
use tokio::sync::mpsc::UnboundedReceiver;
use tracing::{debug, info, warn};
use zbus::zvariant::Value;

use crate::daemon::{Daemon, Signal};

/// Abaixo disto (inclusive), aviso de bateria fraca.
pub const LOW: u8 = 15;
/// Abaixo disto (inclusive), aviso de bateria acabando.
pub const CRITICAL: u8 = 5;
/// Acima disto (inclusive), ou carregando, os avisos voltam a valer. A folga entre `LOW` e
/// `REARM` evita repetir o aviso quando a leitura oscila em volta do limite.
pub const REARM: u8 = 20;
/// Id do app (o `.desktop`), para a notificação abrir o Raiza e usar o ícone dele.
const APP_ID: &str = "io.github.yurileo01.Raiza";
/// Provedores cuja bateria o ambiente gráfico já vigia.
const WATCHED_ELSEWHERE: &[&str] = &["upower", "bluez"];

/// Gravidade do aviso.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Até [`LOW`] %.
    Low,
    /// Até [`CRITICAL`] %.
    Critical,
}

/// Um aviso a mostrar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    /// Aparelho.
    pub id: DeviceId,
    /// Nome para mostrar.
    pub name: String,
    /// Carga atual.
    pub percent: u8,
    /// Gravidade.
    pub level: Level,
}

impl Alert {
    /// Título da notificação.
    pub fn summary(&self) -> String {
        let template = match self.level {
            Level::Low => gettext("Low battery: {name}"),
            Level::Critical => gettext("Battery almost empty: {name}"),
        };
        fill(template, &[("name", &self.name)])
    }

    /// Texto da notificação.
    pub fn body(&self) -> String {
        fill(
            gettext("{percent}% remaining. Plug in the charger."),
            &[("percent", &self.percent)],
        )
    }
}

/// Decide quando avisar, por aparelho: uma vez ao chegar em [`LOW`], outra em
/// [`CRITICAL`], e de novo só depois de passar de [`REARM`] ou carregar.
#[derive(Debug, Default)]
pub struct BatteryWatch {
    warned: BTreeMap<DeviceId, Level>,
}

impl BatteryWatch {
    /// Registra uma leitura de bateria e devolve o aviso, se for o caso.
    pub fn observe(
        &mut self,
        id: &DeviceId,
        name: &str,
        percent: u8,
        charging: bool,
    ) -> Option<Alert> {
        if charging || percent >= REARM {
            self.warned.remove(id);
            return None;
        }
        let level = if percent <= CRITICAL {
            Level::Critical
        } else if percent <= LOW {
            Level::Low
        } else {
            return None;
        };
        let already = self.warned.get(id).copied();
        let new = match (already, level) {
            (None, _) | (Some(Level::Low), Level::Critical) => true,
            (Some(Level::Low), Level::Low) | (Some(Level::Critical), _) => false,
        };
        if !new {
            return None;
        }
        self.warned.insert(id.clone(), level);
        Some(Alert {
            id: id.clone(),
            name: name.to_owned(),
            percent,
            level,
        })
    }

    /// Aplica um sinal do daemon. `lookup` devolve o nome do aparelho e o provedor dono da
    /// bateria dele.
    pub fn on_signal(
        &mut self,
        signal: &Signal,
        lookup: impl Fn(&DeviceId) -> Option<(String, Option<String>)>,
    ) -> Option<Alert> {
        let (id, battery) = match signal {
            Signal::DeviceAdded(device) => (
                &device.id,
                device.capability(raiza_core::CapabilityKind::Battery)?,
            ),
            Signal::CapabilityChanged(id, cap) => (id, cap),
            Signal::DeviceRemoved(_) => return None,
        };
        let Capability::Battery { percent, charging } = battery else {
            return None;
        };
        let (name, owner) = lookup(id)?;
        if owner
            .as_deref()
            .is_some_and(|o| WATCHED_ELSEWHERE.contains(&o))
        {
            return None;
        }
        self.observe(id, &name, *percent, *charging)
    }
}

/// Mostra avisos pelo serviço de notificações da sessão (`org.freedesktop.Notifications`).
/// Um aviso novo do mesmo aparelho substitui o anterior na tela.
pub struct DesktopNotifier {
    conn: zbus::Connection,
    shown: Mutex<BTreeMap<DeviceId, u32>>,
}

impl DesktopNotifier {
    /// Notificador sobre a conexão de sessão do daemon. Também escolhe a língua dos avisos
    /// (o `raizad` não tem outros textos para o usuário).
    pub fn new(conn: zbus::Connection) -> Self {
        raiza_i18n::init();
        DesktopNotifier {
            conn,
            shown: Mutex::default(),
        }
    }

    /// Mostra `alert`. Falha (sem serviço de notificações, por exemplo) só fica no log.
    pub async fn notify(&self, alert: &Alert) {
        let replaces = self
            .shown
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&alert.id)
            .copied()
            .unwrap_or(0);
        let urgency: u8 = match alert.level {
            Level::Low => 1,
            Level::Critical => 2,
        };
        let mut hints: HashMap<&str, Value<'_>> = HashMap::new();
        hints.insert("urgency", Value::U8(urgency));
        hints.insert("desktop-entry", Value::from(APP_ID));
        let body = (
            "Raiza",
            replaces,
            APP_ID,
            alert.summary(),
            alert.body(),
            Vec::<&str>::new(),
            hints,
            -1i32,
        );
        let reply = self
            .conn
            .call_method(
                Some("org.freedesktop.Notifications"),
                "/org/freedesktop/Notifications",
                Some("org.freedesktop.Notifications"),
                "Notify",
                &body,
            )
            .await
            .and_then(|m| m.body().deserialize::<u32>());
        match reply {
            Ok(id) => {
                info!(id = %alert.id, percent = alert.percent, "aviso de bateria fraca");
                self.shown
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(alert.id.clone(), id);
            }
            Err(e) => warn!(error = %e, "não foi possível mostrar o aviso de bateria"),
        }
    }
}

/// Vigia as baterias pelos sinais do daemon até o canal fechar.
pub async fn watch(
    daemon: Daemon,
    mut signals: UnboundedReceiver<Signal>,
    notifier: DesktopNotifier,
) {
    let mut state = BatteryWatch::default();
    while let Some(signal) = signals.recv().await {
        let alert = state.on_signal(&signal, |id| {
            let device = daemon.get_device(id).ok()?;
            Some((device.name, daemon.battery_owner(id)))
        });
        if let Some(alert) = alert {
            notifier.notify(&alert).await;
        }
    }
    debug!("vigia de bateria encerrado");
}

#[cfg(test)]
mod tests {
    use raiza_core::{CapabilityKind, DeviceInfo, DeviceKind};

    use super::*;

    fn id() -> DeviceId {
        DeviceId::usb(0x24ae, 0x1423, "0")
    }

    fn nivel(w: &mut BatteryWatch, percent: u8, charging: bool) -> Option<Level> {
        w.observe(&id(), "Rapoo", percent, charging)
            .map(|a| a.level)
    }

    #[test]
    fn avisa_uma_vez_por_nivel() {
        let mut w = BatteryWatch::default();
        assert_eq!(nivel(&mut w, 50, false), None);
        assert_eq!(nivel(&mut w, 16, false), None);
        assert_eq!(nivel(&mut w, 15, false), Some(Level::Low));
        assert_eq!(nivel(&mut w, 14, false), None, "já avisou: não repete");
        // Oscilando em volta do limite: sem novo aviso até passar de REARM.
        assert_eq!(nivel(&mut w, 16, false), None);
        assert_eq!(nivel(&mut w, 15, false), None);
        assert_eq!(nivel(&mut w, 5, false), Some(Level::Critical));
        assert_eq!(nivel(&mut w, 3, false), None);
        assert_eq!(
            nivel(&mut w, 15, false),
            None,
            "voltar para fraca não rebaixa"
        );
    }

    #[test]
    fn carregar_ou_subir_rearma() {
        let mut w = BatteryWatch::default();
        assert_eq!(nivel(&mut w, 10, false), Some(Level::Low));
        assert_eq!(nivel(&mut w, 10, true), None, "carregando não avisa");
        assert_eq!(nivel(&mut w, 10, false), Some(Level::Low), "rearmado");
        assert_eq!(nivel(&mut w, 20, false), None);
        assert_eq!(
            nivel(&mut w, 4, false),
            Some(Level::Critical),
            "direto para acabando"
        );
    }

    /// Manda uma notificação de verdade pela sessão. Fora do `cargo test` normal:
    /// `cargo test -p raizad notificacao_real -- --ignored`.
    #[tokio::test]
    #[ignore = "mostra uma notificação na área de trabalho"]
    async fn notificacao_real() {
        let conn = zbus::Connection::session().await.unwrap();
        let n = DesktopNotifier::new(conn);
        let alert = Alert {
            id: id(),
            name: "Teste do Raiza".into(),
            percent: 15,
            level: Level::Low,
        };
        n.notify(&alert).await;
        assert!(
            n.shown.lock().unwrap().contains_key(&id()),
            "o servidor não aceitou"
        );
    }

    #[test]
    fn texto_do_aviso() {
        let mut w = BatteryWatch::default();
        let a = w.observe(&id(), "Rapoo VT2", 12, false).unwrap();
        assert_eq!(a.summary(), "Low battery: Rapoo VT2");
        assert_eq!(a.body(), "12% remaining. Plug in the charger.");
    }

    fn com_bateria(percent: u8) -> DeviceInfo {
        DeviceInfo {
            id: id(),
            name: "Rapoo".into(),
            vendor: None,
            kind: DeviceKind::Mouse,
            providers: vec!["hid-perfis".into()],
            capabilities: vec![Capability::Battery {
                percent,
                charging: false,
            }],
        }
    }

    #[test]
    fn sinais_e_dono_da_bateria() {
        let bateria = |p| Capability::Battery {
            percent: p,
            charging: false,
        };
        let perfil = |_: &DeviceId| Some(("Rapoo".to_owned(), Some("hid-perfis".to_owned())));
        let upower = |_: &DeviceId| Some(("Rapoo".to_owned(), Some("upower".to_owned())));

        let mut w = BatteryWatch::default();
        let a = w.on_signal(&Signal::DeviceAdded(com_bateria(10)), perfil);
        assert_eq!(a.map(|a| a.level), Some(Level::Low));
        let a = w.on_signal(&Signal::CapabilityChanged(id(), bateria(4)), perfil);
        assert_eq!(a.map(|a| a.level), Some(Level::Critical));

        // Bateria do UPower: o ambiente gráfico já avisa.
        let mut w = BatteryWatch::default();
        assert!(
            w.on_signal(&Signal::CapabilityChanged(id(), bateria(4)), upower)
                .is_none()
        );

        // Outras capacidades e remoções não avisam.
        let mut w = BatteryWatch::default();
        let dpi = DeviceInfo {
            capabilities: vec![],
            ..com_bateria(0)
        };
        assert!(w.on_signal(&Signal::DeviceAdded(dpi), perfil).is_none());
        assert!(w.on_signal(&Signal::DeviceRemoved(id()), perfil).is_none());
        assert!(com_bateria(1).capability(CapabilityKind::Battery).is_some());
    }
}
