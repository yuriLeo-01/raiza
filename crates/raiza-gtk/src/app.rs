//! Janela principal: componente do relm4 que junta a sessão D-Bus, o modelo, a barra
//! lateral e a página de detalhes.
//!
//! Estados da janela (uma `GtkStack`): carregando, problema (daemon fora do ar, com
//! botão "Tentar de novo"), nenhum dispositivo, e a lista com a página de detalhes num
//! `AdwNavigationSplitView`, que vira navegação em pilha quando a janela é estreita.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use raiza_core::DeviceId;
use raiza_core::dbus::RaizaClient;
use relm4::adw;
use relm4::adw::prelude::*;
use relm4::gtk;
use relm4::{Component, ComponentParts, ComponentSender};

use tokio::task::JoinHandle;
use tracing::debug;

use crate::detail::Detail;
use crate::model::{Change, Devices, REMOVAL_GRACE, RemovalToken};
use crate::session::{self, SessionEvent};
use crate::sidebar::Sidebar;
use crate::text::Problem;

/// Id do aplicativo (também o nome do ícone e do `.desktop`).
pub const APP_ID: &str = "io.github.yurileo01.Raiza";

/// Em que estado a conversa com o daemon está.
#[derive(Debug)]
enum State {
    Loading,
    Problem(Problem),
    Ready,
}

/// O aplicativo.
pub struct App {
    devices: Devices,
    client: Option<RaizaClient>,
    state: State,
    session: Option<JoinHandle<()>>,
    /// Geração da sessão atual. Mensagens de uma sessão abortada são descartadas.
    generation: u64,
}

/// Mensagens da interface e da sessão.
#[derive(Debug)]
pub enum AppMsg {
    /// Algo veio da sessão de geração `.0`.
    Session(u64, SessionEvent),
    /// O usuário escolheu um dispositivo na barra lateral.
    Select(DeviceId),
    /// Botão "Tentar de novo".
    Retry,
    /// Mostrar (ou esconder) os aparelhos sem nada útil.
    ShowAll(bool),
}

/// Resultados dos *commands*.
#[derive(Debug)]
pub enum AppCmd {
    /// Passou o prazo de uma remoção pendente.
    ConfirmRemoval(DeviceId, RemovalToken),
}

/// Widgets que a janela atualiza.
pub struct AppWidgets {
    stack: gtk::Stack,
    problem: adw::StatusPage,
    split: adw::NavigationSplitView,
    content: adw::NavigationPage,
    sidebar: Sidebar,
    detail: Detail,
    show_all: gtk::ToggleButton,
}

/// Quanto tempo um toast fica na tela.
const TOAST_SECONDS: u32 = 5;

/// Filtra toasts repetidos: a mesma mensagem não volta enquanto a anterior ainda está na
/// tela (ex.: o slider recusado que o GTK reaplica ao soltar o botão do mouse).
#[derive(Debug, Default)]
struct RecentMessages {
    last: Option<(String, Instant)>,
}

impl RecentMessages {
    /// Se `message` deve virar toast agora.
    fn admit(&mut self, message: &str, now: Instant) -> bool {
        let window = Duration::from_secs(TOAST_SECONDS.into());
        let repeated = self.last.as_ref().is_some_and(|(last, at)| {
            last == message && now.saturating_duration_since(*at) < window
        });
        if !repeated {
            self.last = Some((message.to_owned(), now));
        }
        !repeated
    }
}

const PAGE_LOADING: &str = "loading";
const PAGE_PROBLEM: &str = "problem";
const PAGE_EMPTY: &str = "empty";
const PAGE_DEVICES: &str = "devices";

/// Página simples com barra de título e uma `AdwStatusPage`.
fn status_page(status: &adw::StatusPage) -> adw::ToolbarView {
    let view = adw::ToolbarView::new();
    view.add_top_bar(&adw::HeaderBar::new());
    view.set_content(Some(status));
    view
}

impl App {
    fn start_session(&mut self, sender: &ComponentSender<Self>) {
        if let Some(old) = self.session.take() {
            old.abort();
        }
        self.generation += 1;
        let generation = self.generation;
        let input = sender.input_sender().clone();
        self.session = Some(relm4::spawn(session::run(move |event| {
            input.send(AppMsg::Session(generation, event)).is_ok()
        })));
    }

    /// Mostra o estado atual do modelo.
    fn render(&mut self, widgets: &mut AppWidgets) {
        let hidden = self.devices.hidden_count();
        widgets
            .show_all
            .set_tooltip_text(Some(&if widgets.show_all.is_active() {
                "Esconder aparelhos sem controles".to_string()
            } else if hidden > 0 {
                format!("Mostrar todos os aparelhos ({hidden} sem controles escondidos)")
            } else {
                "Mostrar todos os aparelhos".to_string()
            }));
        match &self.state {
            State::Loading => widgets.stack.set_visible_child_name(PAGE_LOADING),
            State::Problem(problem) => {
                widgets.problem.set_title(&problem.title);
                widgets.problem.set_description(Some(&problem.description));
                widgets.stack.set_visible_child_name(PAGE_PROBLEM);
            }
            State::Ready if self.devices.is_empty() => {
                widgets.stack.set_visible_child_name(PAGE_EMPTY);
            }
            State::Ready => {
                self.devices.select_first_if_none();
                widgets.sidebar.sync(&self.devices);
                let selected = self.devices.selected().and_then(|id| self.devices.get(id));
                if let Some(device) = selected {
                    widgets.content.set_title(&device.name);
                    widgets.detail.show(device, self.client.as_ref());
                }
                widgets.stack.set_visible_child_name(PAGE_DEVICES);
            }
        }
    }

    fn on_session(
        &mut self,
        widgets: &mut AppWidgets,
        event: SessionEvent,
        sender: &ComponentSender<Self>,
    ) {
        match event {
            SessionEvent::Connected { client, devices } => {
                self.devices.reset(devices);
                self.client = Some(client);
                self.state = State::Ready;
                // As linhas guardam o cliente: recria com o novo.
                widgets.detail.clear();
            }
            SessionEvent::Problem(problem) => {
                self.client = None;
                self.state = State::Problem(problem);
            }
            SessionEvent::Event(event) => match self.devices.apply(event) {
                Change::None => return,
                Change::RemovalPending(id, token) => {
                    sender.oneshot_command(async move {
                        tokio::time::sleep(REMOVAL_GRACE).await;
                        AppCmd::ConfirmRemoval(id, token)
                    });
                    return;
                }
                change => debug!(?change, "modelo atualizado"),
            },
        }
        self.render(widgets);
    }
}

impl Component for App {
    type CommandOutput = AppCmd;
    type Input = AppMsg;
    type Output = ();
    type Init = ();
    type Root = adw::ApplicationWindow;
    type Widgets = AppWidgets;

    fn init_root() -> Self::Root {
        adw::ApplicationWindow::builder()
            .title("Raiza")
            .default_width(900)
            .default_height(640)
            .width_request(360)
            .height_request(294)
            .build()
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        gtk::Window::set_default_icon_name(APP_ID);

        // Carregando.
        let spinner = gtk::Spinner::builder()
            .spinning(true)
            .width_request(32)
            .height_request(32)
            .halign(gtk::Align::Center)
            .build();
        let loading = adw::StatusPage::builder()
            .title("Conectando ao serviço do Raiza…")
            .child(&spinner)
            .build();

        // Problema.
        let retry = gtk::Button::builder()
            .label("Tentar de novo")
            .halign(gtk::Align::Center)
            .css_classes(["pill", "suggested-action"])
            .build();
        retry.connect_clicked({
            let input = sender.input_sender().clone();
            move |_| {
                let _ = input.send(AppMsg::Retry);
            }
        });
        let problem = adw::StatusPage::builder()
            .icon_name("dialog-warning-symbolic")
            .child(&retry)
            .build();

        // Nenhum dispositivo.
        let empty = adw::StatusPage::builder()
            .icon_name("input-mouse-symbolic")
            .title("Nenhum dispositivo")
            .description("Conecte um periférico compatível. Ele aparece aqui sozinho.")
            .build();

        // Lista e detalhes.
        let sidebar = Sidebar::new({
            let input = sender.input_sender().clone();
            move |id| {
                let _ = input.send(AppMsg::Select(id));
            }
        });
        // Aparelhos que só dizem "conectado" ficam escondidos; este botão mostra todos.
        let show_all = gtk::ToggleButton::builder()
            .icon_name("view-reveal-symbolic")
            .tooltip_text("Mostrar todos os aparelhos")
            .build();
        show_all.connect_toggled({
            let input = sender.input_sender().clone();
            move |b| {
                let _ = input.send(AppMsg::ShowAll(b.is_active()));
            }
        });
        let sidebar_header = adw::HeaderBar::new();
        sidebar_header.pack_end(&show_all);
        let sidebar_view = adw::ToolbarView::new();
        sidebar_view.add_top_bar(&sidebar_header);
        sidebar_view.set_content(Some(
            &gtk::ScrolledWindow::builder()
                .hscrollbar_policy(gtk::PolicyType::Never)
                .child(sidebar.widget())
                .build(),
        ));
        let sidebar_page = adw::NavigationPage::builder()
            .title("Raiza")
            .tag("sidebar")
            .child(&sidebar_view)
            .build();

        // Erros de escrita das linhas viram toasts.
        let toasts = adw::ToastOverlay::new();
        let detail = Detail::new(Rc::new({
            let toasts = toasts.clone();
            let recent = RefCell::new(RecentMessages::default());
            move |message: &str| {
                if !recent.borrow_mut().admit(message, Instant::now()) {
                    return;
                }
                let toast = adw::Toast::builder()
                    .title(crate::text::escape_markup(message))
                    .timeout(TOAST_SECONDS)
                    .build();
                toasts.add_toast(toast);
            }
        }));
        let content_view = adw::ToolbarView::new();
        content_view.add_top_bar(&adw::HeaderBar::new());
        content_view.set_content(Some(detail.widget()));
        let content = adw::NavigationPage::builder()
            .title("Dispositivo")
            .tag("device")
            .child(&content_view)
            .build();

        let split = adw::NavigationSplitView::builder()
            .sidebar(&sidebar_page)
            .content(&content)
            .build();

        let stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .build();
        stack.add_named(&status_page(&loading), Some(PAGE_LOADING));
        stack.add_named(&status_page(&problem), Some(PAGE_PROBLEM));
        stack.add_named(&status_page(&empty), Some(PAGE_EMPTY));
        stack.add_named(&split, Some(PAGE_DEVICES));
        toasts.set_child(Some(&stack));
        root.set_content(Some(&toasts));

        let breakpoint = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            550.0,
            adw::LengthUnit::Sp,
        ));
        breakpoint.add_setter(&split, "collapsed", Some(&true.to_value()));
        root.add_breakpoint(breakpoint);

        let mut model = App {
            devices: Devices::default(),
            client: None,
            state: State::Loading,
            session: None,
            generation: 0,
        };
        let mut widgets = AppWidgets {
            stack,
            problem,
            split,
            content,
            sidebar,
            detail,
            show_all,
        };
        model.render(&mut widgets);
        model.start_session(&sender);
        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        message: Self::Input,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match message {
            AppMsg::Session(generation, event) => {
                if generation == self.generation {
                    self.on_session(widgets, event, &sender);
                }
            }
            AppMsg::Select(id) => {
                self.devices.select(Some(id));
                self.render(widgets);
                widgets.split.set_show_content(true);
            }
            AppMsg::ShowAll(show) => {
                self.devices.set_show_all(show);
                self.render(widgets);
            }
            AppMsg::Retry => {
                self.client = None;
                self.state = State::Loading;
                self.render(widgets);
                self.start_session(&sender);
            }
        }
    }

    fn update_cmd_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        message: Self::CommandOutput,
        _sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match message {
            AppCmd::ConfirmRemoval(id, token) => {
                let was_selected = self.devices.selected() == Some(&id);
                if self.devices.confirm_removal(&id, token) {
                    debug!(%id, "dispositivo removido");
                    if was_selected && widgets.split.is_collapsed() {
                        // A página aberta sumiu: volta para a lista.
                        widgets.split.set_show_content(false);
                    }
                    self.render(widgets);
                }
            }
        }
    }

    fn shutdown(&mut self, _widgets: &mut Self::Widgets, _output: relm4::Sender<Self::Output>) {
        if let Some(session) = self.session.take() {
            session.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toast_repetido_e_filtrado() {
        let mut recent = RecentMessages::default();
        let t = Instant::now();
        assert!(recent.admit("a", t));
        assert!(!recent.admit("a", t + Duration::from_secs(1)));
        assert!(recent.admit("b", t + Duration::from_secs(2)));
        assert!(recent.admit("a", t + Duration::from_secs(3)));
        assert!(recent.admit("a", t + Duration::from_secs(9)));
    }
}
