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
use tracing_subscriber::EnvFilter;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();
    // A língua vem do ambiente (LANGUAGE, LC_ALL, LC_MESSAGES, LANG); veja `raiza-i18n`.
    raiza_i18n::init();

    // App de instância única: abrir de novo traz a janela existente. O daemon usa outro
    // nome no barramento (`raiza_core::DBUS_SERVICE`, terminado em `.Daemon`).
    let app = adw::Application::builder()
        .application_id(app::APP_ID)
        .build();
    relm4::RelmApp::from_app(app).run::<app::App>(());
}
