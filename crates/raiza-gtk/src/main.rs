//! raiza-gtk: interface GTK4/libadwaita do Raiza.
//!
//! A lógica (modelo, textos) fica em módulos sem GTK, testados à parte; a conversa com
//! o daemon roda no runtime tokio do relm4 ([`session`]).

mod app;
mod capabilities;
mod detail;
mod model;
mod session;
mod sidebar;
mod text;

use relm4::adw;
use relm4::gtk::gio;
use tracing_subscriber::EnvFilter;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    // O app id é igual ao nome D-Bus do daemon (`raiza_core::DBUS_SERVICE`). Sem
    // `NON_UNIQUE`, o GApplication tentaria ser dono desse nome: falharia com o `raizad`
    // rodando, ou o tomaria para si com ele parado. Veja docs/tarefas/notas/T13.md.
    let app = adw::Application::builder()
        .application_id(app::APP_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    relm4::RelmApp::from_app(app).run::<app::App>(());
}
