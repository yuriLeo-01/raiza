//! Lista as interfaces hidraw e o que os perfis enxergam nelas.
//!
//! ```sh
//! cargo run -p raiza-provider-hid --example listar-hid -- profiles/
//! RUST_LOG=debug cargo run -p raiza-provider-hid --example listar-hid -- profiles/ --ler
//! ```
//!
//! Sem argumentos, usa os diretórios padrão. Com `--ler`, lê cada capacidade dos
//! aparelhos encontrados (só consultas fixas; nada é escrito).

use std::path::PathBuf;

use raiza_core::Provider;
use raiza_provider_hid::{DirProfiles, HidBackend, HidProfilesProvider, SystemHid, Timeouts};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let ler = args.iter().any(|a| a == "--ler");
    let dirs: Vec<PathBuf> = args
        .iter()
        .filter(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .collect();
    let fonte = if dirs.is_empty() {
        DirProfiles::default()
    } else {
        DirProfiles::new(dirs)
    };

    let inventario = SystemHid::new().inventory()?;
    for (id, ifaces) in inventario.by_device() {
        for i in ifaces {
            let colecoes: Vec<String> = i
                .collections
                .iter()
                .map(|(p, u)| format!("{p:04x}:{u:04x}"))
                .collect();
            println!(
                "{id}  {}  interface {:?}  coleções [{}]",
                i.path,
                i.interface,
                colecoes.join(", ")
            );
        }
    }

    let provedor = HidProfilesProvider::with_parts(SystemHid::new(), fonte, Timeouts::default());
    let aparelhos = provedor.discover().await?;
    println!("\n{} aparelho(s) com perfil:", aparelhos.len());
    for d in &aparelhos {
        println!(
            "{}  {} ({})",
            d.id,
            d.name,
            d.vendor.as_deref().unwrap_or("?")
        );
        for c in &d.capabilities {
            println!("  {c:?}");
            if ler {
                match provedor.read(&d.id, c.kind()).await {
                    Ok(lido) => println!("    lido: {lido:?}"),
                    Err(e) => println!("    erro: {e}"),
                }
            }
        }
    }
    Ok(())
}
