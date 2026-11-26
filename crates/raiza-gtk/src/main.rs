//! raiza-gtk: interface GTK4/libadwaita (T13, em andamento).
// WIP: a interface (app, barra lateral, detalhes) ainda não usa estes módulos.
#![allow(dead_code)]

mod model;
mod session;
mod text;

fn main() {
    println!(
        "raiza-gtk (em construção) — serviço {}",
        raiza_core::DBUS_SERVICE
    );
}
