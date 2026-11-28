//! O item SNI: traduz a [`View`] para o `ksni` e repassa os cliques como [`Action`].

use ksni::menu::StandardItem;
use ksni::{MenuItem, OfflineReason, Status, ToolTip};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{info, warn};

use crate::model::{Entry, View, Visibility};
use crate::text;

/// O que o usuário pediu pelo ícone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Abrir o app, com o token de ativação XDG se o painel mandou um.
    OpenApp(Option<String>),
    /// Encerrar o `raiza-tray`.
    Quit,
}

/// Estado do item SNI.
#[derive(Debug)]
pub struct RaizaTray {
    view: View,
    actions: UnboundedSender<Action>,
}

impl RaizaTray {
    /// Item novo, ainda sem daemon.
    pub fn new(actions: UnboundedSender<Action>) -> Self {
        RaizaTray {
            view: View::default(),
            actions,
        }
    }

    /// Troca o que o ícone mostra.
    pub fn set_view(&mut self, view: View) {
        self.view = view;
    }

    fn send(&self, action: Action) {
        if self.actions.send(action).is_err() {
            warn!("o laço principal já encerrou; clique ignorado");
        }
    }
}

fn open_app(tray: &mut RaizaTray) {
    tray.send(Action::OpenApp(ksni::current_xdg_activation_token()));
}

impl ksni::Tray for RaizaTray {
    fn id(&self) -> String {
        "raiza-tray".into()
    }

    fn title(&self) -> String {
        text::TITLE.into()
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::Hardware
    }

    fn status(&self) -> Status {
        match self.view.visibility {
            Visibility::Active => Status::Active,
            Visibility::Passive => Status::Passive,
        }
    }

    fn icon_name(&self) -> String {
        self.view.icon_name.clone()
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            icon_name: self.view.icon_name.clone(),
            icon_pixmap: Vec::new(),
            title: text::TITLE.into(),
            description: self.view.tooltip.clone(),
        }
    }

    /// Clique principal: abre o app.
    fn activate(&mut self, _x: i32, _y: i32) {
        open_app(self);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        self.view.menu.iter().map(menu_item).collect()
    }

    fn watcher_online(&self) {
        info!("StatusNotifierWatcher disponível; ícone registrado");
    }

    fn watcher_offline(&self, reason: OfflineReason) -> bool {
        // Sem watcher (GNOME sem a extensão AppIndicator, painel reiniciando): avisa e
        // continua rodando. O `ksni` registra o ícone quando o watcher aparecer.
        match reason {
            OfflineReason::Error(err) => {
                warn!(%err, "sem StatusNotifierWatcher; esperando ele aparecer");
            }
            _ => warn!("o StatusNotifierWatcher saiu; esperando ele voltar"),
        }
        true
    }
}

fn menu_item(entry: &Entry) -> MenuItem<RaizaTray> {
    match entry {
        Entry::Info(line) => StandardItem {
            label: text::menu_label(line),
            enabled: false,
            ..Default::default()
        }
        .into(),
        Entry::Separator => MenuItem::Separator,
        Entry::OpenApp => StandardItem {
            label: text::menu_label(text::OPEN_APP),
            icon_name: "io.github.yurileo01.Raiza".into(),
            activate: Box::new(open_app),
            ..Default::default()
        }
        .into(),
        Entry::Quit => StandardItem {
            label: text::menu_label(text::QUIT),
            icon_name: "application-exit-symbolic".into(),
            activate: Box::new(|tray: &mut RaizaTray| tray.send(Action::Quit)),
            ..Default::default()
        }
        .into(),
    }
}

#[cfg(test)]
mod tests {
    use ksni::Tray as _;

    use super::*;
    use crate::model::Model;

    #[test]
    fn menu_e_status_seguem_a_view() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut tray = RaizaTray::new(tx);
        assert!(matches!(tray.status(), Status::Passive));
        assert_eq!(tray.menu().len(), 4);

        let mut model = Model::default();
        model.connected(Vec::new());
        tray.set_view(model.view());
        assert_eq!(tray.tool_tip().description, text::NO_BATTERIES);

        // "Sair" manda a ação ao laço principal.
        let menu = tray.menu();
        let Some(MenuItem::Standard(quit)) = menu.last() else {
            panic!("o último item deveria ser \"Sair\"");
        };
        assert_eq!(quit.label, text::QUIT);
        (quit.activate)(&mut tray);
        assert_eq!(rx.try_recv().ok(), Some(Action::Quit));

        // As linhas informativas ficam desativadas.
        let Some(MenuItem::Standard(info)) = menu.first() else {
            panic!("o primeiro item deveria ser uma linha");
        };
        assert!(!info.enabled);
    }
}
