//! Lista o que os provedores `bluez` e `upower` enxergam nesta máquina.
//!
//! ```sh
//! cargo run -p raiza-provider-bluez --example listar-bluez
//! cargo run -p raiza-provider-bluez --example listar-bluez -- --eventos   # fica ouvindo
//! ```

use futures::StreamExt;
use raiza_core::{DeviceInfo, Provider};
use raiza_provider_bluez::{BluezProvider, UpowerProvider};

fn mostrar(provedor: &str, devices: &[DeviceInfo]) {
    println!("== {provedor}: {} dispositivo(s)", devices.len());
    for d in devices {
        println!("{}  {}  ({:?})", d.id, d.name, d.kind);
        for c in &d.capabilities {
            println!("    {c:?}");
        }
    }
}

#[tokio::main]
async fn main() {
    let bluez = BluezProvider::system();
    let upower = UpowerProvider::system();
    let providers: [&dyn Provider; 2] = [&bluez, &upower];
    for p in providers {
        match p.discover().await {
            Ok(devices) => mostrar(p.id(), &devices),
            Err(e) => println!("== {}: erro: {e}", p.id()),
        }
    }

    if std::env::args().any(|a| a == "--eventos") {
        println!("-- ouvindo eventos (Ctrl+C para sair)");
        let mut events = futures::stream::select(bluez.events(), upower.events());
        while let Some(event) = events.next().await {
            println!("{event:?}");
        }
    }
}
