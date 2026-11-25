//! Lista os periféricos reais e acompanha o hotplug.
//!
//! ```sh
//! cargo run -p raiza-provider-udev --example listar            # lista e vigia (Ctrl+C sai)
//! cargo run -p raiza-provider-udev --example listar -- --json   # árvore udev crua, p/ fixtures
//! RUST_LOG=raiza_provider_udev=debug cargo run -p raiza-provider-udev --example listar
//! ```

use futures_util::StreamExt;
use raiza_core::{DeviceInfo, Provider, ProviderEvent};
use raiza_provider_udev::{SystemUdev, UdevProvider, UdevSource};

fn linha(d: &DeviceInfo) -> String {
    format!(
        "{:<32} {:<12} {}{}",
        d.id.to_string(),
        format!("{:?}", d.kind),
        d.name,
        d.vendor
            .as_deref()
            .map(|v| format!(" ({v})"))
            .unwrap_or_default()
    )
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "raiza_provider_udev=info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    if std::env::args().any(|a| a == "--json") {
        println!("{}", SystemUdev.snapshot()?.to_json());
        return Ok(());
    }

    let provider = UdevProvider::new();
    let devices = provider.discover().await?;
    println!("{} dispositivo(s):", devices.len());
    for d in &devices {
        println!("  {}", linha(d));
    }

    println!("\nVigiando hotplug (Ctrl+C para sair)...");
    let mut events = provider.events();
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            event = events.next() => match event {
                Some(ProviderEvent::DeviceAdded { device }) => println!("+ {}", linha(&device)),
                Some(ProviderEvent::DeviceRemoved { id }) => println!("- {id}"),
                Some(other) => println!("? {other:?}"),
                None => break,
            },
        }
    }
    Ok(())
}
