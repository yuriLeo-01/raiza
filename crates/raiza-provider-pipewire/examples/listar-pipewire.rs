//! Lista os nós de áudio com o `DeviceId` calculado e os periféricos que o provedor anuncia.
//!
//! ```sh
//! cargo run -p raiza-provider-pipewire --example listar-pipewire               # lista
//! cargo run -p raiza-provider-pipewire --example listar-pipewire -- --eventos  # e vigia (Ctrl+C sai)
//! RUST_LOG=raiza_provider_pipewire=debug cargo run -p raiza-provider-pipewire --example listar-pipewire
//! ```

use futures_util::StreamExt;
use raiza_core::{Capability, DeviceInfo, Provider, ProviderEvent};
use raiza_provider_pipewire::scan::node_reports;
use raiza_provider_pipewire::{Graph, PipeWireBackend, PipeWireProvider, PwCli, SysfsCardResolver};

fn resumo(caps: &[Capability]) -> String {
    caps.iter()
        .map(|c| match c {
            Capability::Volume { level, .. } => format!("volume {}%", level.current),
            Capability::Mute { muted, .. } => format!("mudo {muted}"),
            other => format!("{other:?}"),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn linha(d: &DeviceInfo) -> String {
    format!(
        "{:<36} {:<12} {} [{}]",
        d.id.to_string(),
        format!("{:?}", d.kind),
        d.name,
        resumo(&d.capabilities)
    )
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "raiza_provider_pipewire=info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let items = PwCli::new().dump().await?;
    let graph = Graph::from_dump(&items);
    println!("Nós de áudio:");
    for n in node_reports(&graph, &SysfsCardResolver::new()) {
        let id = n
            .device
            .map_or_else(|| "(ignorado)".to_owned(), |id| id.to_string());
        let volume = n
            .volume
            .map(|(v, m)| format!("{v}%{}", if m { " mudo" } else { "" }))
            .unwrap_or_default();
        println!(
            "  {:>4} {:<13} {:<36} {:<10} {}",
            n.node, n.media_class, id, volume, n.name
        );
    }

    let provider = PipeWireProvider::new();
    let devices = provider.discover().await?;
    println!("\n{} dispositivo(s):", devices.len());
    for d in &devices {
        println!("  {}", linha(d));
    }

    if !std::env::args().any(|a| a == "--eventos") {
        return Ok(());
    }
    println!("\nVigiando mudanças (Ctrl+C para sair)...");
    let mut events = provider.events();
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            event = events.next() => match event {
                Some(ProviderEvent::DeviceAdded { device }) => println!("+ {}", linha(&device)),
                Some(ProviderEvent::DeviceRemoved { id }) => println!("- {id}"),
                Some(ProviderEvent::CapabilityChanged { id, capability }) => {
                    println!("~ {id} {}", resumo(std::slice::from_ref(&capability)))
                }
                None => break,
            },
        }
    }
    Ok(())
}
