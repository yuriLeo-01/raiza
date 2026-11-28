//! Tradução das interfaces do Raiza (app GTK, bandeja, CLI e avisos do daemon).
//!
//! Os textos do código são os msgids, em inglês. As traduções ficam em catálogos gettext
//! (`.mo`, domínio [`DOMAIN`]) compilados a partir de `po/*.po`, e são lidas em Rust puro
//! pelo crate `gettext`: sem `setlocale`, sem `unsafe` e sem a libintl do sistema, o que
//! também vale para o daemon multithread.
//!
//! Uso:
//!
//! 1. o binário chama [`init`] uma vez, no começo;
//! 2. os textos passam por [`gettext`] ou [`ngettext`] (os nomes são as palavras-chave do
//!    `xgettext`, veja `po/README.md`) e os valores entram com [`fill`].
//!
//! Sem [`init`] (nos testes, por exemplo), tudo sai em inglês.
//!
//! ## Escolha da língua
//!
//! Vale a primeira variável não vazia entre `LANGUAGE`, `LC_ALL`, `LC_MESSAGES` e `LANG`.
//! `LANGUAGE` pode ser uma lista (`pt_BR:en`); as outras têm uma língua só. Para cada
//! língua, `pt_BR.UTF-8@x` vira `pt_BR` e depois `pt`. `C`, `POSIX` e `en*` querem dizer
//! inglês, ou seja, o próprio msgid.
//!
//! ## Onde ficam os catálogos
//!
//! `<pasta>/<língua>/LC_MESSAGES/raiza.mo`, procurando primeiro na pasta de
//! `RAIZA_LOCALEDIR` (desenvolvimento) e depois em [`SYSTEM_LOCALEDIR`].

#![warn(missing_docs)]

use std::fmt::Display;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use gettext::Catalog;
use tracing::{debug, warn};

/// Domínio gettext do Raiza (o nome do `.mo`).
pub const DOMAIN: &str = "raiza";

/// Pasta dos catálogos instalados pelo pacote.
pub const SYSTEM_LOCALEDIR: &str = "/usr/share/locale";

/// Variável com uma pasta extra de catálogos, consultada antes da do sistema.
pub const LOCALEDIR_ENV: &str = "RAIZA_LOCALEDIR";

/// Variáveis que dizem a língua, na ordem em que valem.
const LANGUAGE_VARS: [&str; 4] = ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"];

/// Catálogo em uso. Vazio enquanto [`init`] não roda: os textos saem em inglês.
static CATALOG: OnceLock<Option<Catalog>> = OnceLock::new();

/// Escolhe a língua pelo ambiente e carrega o catálogo. Só a primeira chamada vale.
///
/// Nunca falha: sem catálogo para a língua (ou com um catálogo ilegível, que fica no log),
/// os textos saem em inglês.
pub fn init() {
    CATALOG.get_or_init(|| {
        let langs = languages(|name| std::env::var(name).ok());
        let mut dirs = Vec::new();
        if let Some(dir) = std::env::var_os(LOCALEDIR_ENV).filter(|d| !d.is_empty()) {
            dirs.push(PathBuf::from(dir));
        }
        dirs.push(PathBuf::from(SYSTEM_LOCALEDIR));
        load(&langs, &dirs)
    });
}

/// Traduz `msgid` para a língua escolhida em [`init`].
pub fn gettext(msgid: &'static str) -> &'static str {
    match CATALOG.get() {
        Some(Some(catalog)) => catalog.gettext(msgid),
        _ => msgid,
    }
}

/// Traduz uma mensagem com plural: `singular` e `plural` são os msgids em inglês e `n`
/// decide a forma.
pub fn ngettext(singular: &'static str, plural: &'static str, n: u64) -> &'static str {
    match CATALOG.get() {
        Some(Some(catalog)) => catalog.ngettext(singular, plural, n),
        _ if n == 1 => singular,
        _ => plural,
    }
}

/// Troca cada `{nome}` de `template` pelo valor de mesmo nome em `args`.
///
/// Os nomes deixam o tradutor mudar a ordem dos valores. A troca é de uma passada só: um
/// valor que contenha `{outro}` não é trocado de novo. `{nome}` sem valor fica como está.
pub fn fill(template: &str, args: &[(&str, &dyn Display)]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let value = after.find('}').and_then(|end| {
            let name = &after[..end];
            args.iter().find(|(n, _)| *n == name).map(|(_, v)| (end, v))
        });
        match value {
            Some((end, v)) => {
                out.push_str(&v.to_string());
                rest = &after[end + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Línguas a tentar, em ordem, segundo o ambiente lido por `env`. Lista vazia é inglês.
pub fn languages(env: impl Fn(&str) -> Option<String>) -> Vec<String> {
    let Some((name, value)) = LANGUAGE_VARS
        .iter()
        .find_map(|name| env(name).filter(|v| !v.is_empty()).map(|v| (*name, v)))
    else {
        return Vec::new();
    };
    // Só `LANGUAGE` aceita lista; nas outras, `:` não tem sentido.
    let entries: Vec<&str> = if name == "LANGUAGE" {
        value.split(':').collect()
    } else {
        vec![value.as_str()]
    };
    let mut langs: Vec<String> = Vec::new();
    for entry in entries {
        // `pt_BR.UTF-8@euro` → `pt_BR`.
        let base = entry
            .split(['.', '@', ':'])
            .next()
            .unwrap_or_default()
            .trim();
        if base.is_empty() {
            continue;
        }
        if base == "C" || base == "POSIX" || base == "en" || base.starts_with("en_") {
            // Inglês é o próprio msgid: o que vem depois na lista não vale mais.
            break;
        }
        let mut push = |lang: &str| {
            if !langs.iter().any(|l| l == lang) {
                langs.push(lang.to_owned());
            }
        };
        push(base);
        if let Some((lang, _country)) = base.split_once('_') {
            push(lang);
        }
    }
    langs
}

/// Caminho do catálogo de `lang` dentro de `dir`.
pub fn catalog_path(dir: &Path, lang: &str) -> PathBuf {
    dir.join(lang)
        .join("LC_MESSAGES")
        .join(format!("{DOMAIN}.mo"))
}

/// Carrega o primeiro catálogo que existir, língua por língua, pasta por pasta.
fn load(langs: &[String], dirs: &[PathBuf]) -> Option<Catalog> {
    for lang in langs {
        for dir in dirs {
            let path = catalog_path(dir, lang);
            let Ok(file) = File::open(&path) else {
                continue;
            };
            match Catalog::parse(BufReader::new(file)) {
                Ok(catalog) => {
                    debug!(path = %path.display(), "catálogo de tradução carregado");
                    return Some(catalog);
                }
                Err(e) => {
                    warn!(path = %path.display(), error = %e, "catálogo de tradução ilegível")
                }
            }
        }
    }
    debug!(?langs, "sem catálogo de tradução: textos em inglês");
    None
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn env(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |name| map.get(name).cloned()
    }

    #[test]
    fn ordem_das_variaveis() {
        let tudo = [
            ("LANGUAGE", "pt_BR"),
            ("LC_ALL", "de_DE.UTF-8"),
            ("LC_MESSAGES", "fr_FR.UTF-8"),
            ("LANG", "es_ES.UTF-8"),
        ];
        assert_eq!(languages(env(&tudo)), ["pt_BR", "pt"]);
        assert_eq!(languages(env(&tudo[1..])), ["de_DE", "de"]);
        assert_eq!(languages(env(&tudo[2..])), ["fr_FR", "fr"]);
        assert_eq!(languages(env(&tudo[3..])), ["es_ES", "es"]);
        assert!(languages(env(&[])).is_empty());
        // Variável vazia não conta.
        assert_eq!(
            languages(env(&[("LANGUAGE", ""), ("LANG", "pt_BR.UTF-8")])),
            ["pt_BR", "pt"]
        );
    }

    #[test]
    fn ingles_e_c_param_a_lista() {
        assert!(languages(env(&[("LANGUAGE", "en")])).is_empty());
        assert!(languages(env(&[("LANG", "en_US.UTF-8")])).is_empty());
        assert!(languages(env(&[("LC_ALL", "C")])).is_empty());
        assert!(languages(env(&[("LC_ALL", "C.UTF-8")])).is_empty());
        assert!(languages(env(&[("LANG", "POSIX")])).is_empty());
        assert!(languages(env(&[("LANGUAGE", "en:pt_BR")])).is_empty());
        assert_eq!(
            languages(env(&[("LANGUAGE", "pt_BR:en:de")])),
            ["pt_BR", "pt"]
        );
    }

    #[test]
    fn lista_do_language() {
        assert_eq!(
            languages(env(&[("LANGUAGE", "pt_PT:pt_BR::de@euro")])),
            ["pt_PT", "pt", "pt_BR", "de"]
        );
        // Só o `LANGUAGE` é lista.
        assert_eq!(languages(env(&[("LANG", "pt_BR:de")])), ["pt_BR", "pt"]);
    }

    #[test]
    fn caminho_do_catalogo() {
        assert_eq!(
            catalog_path(Path::new("/usr/share/locale"), "pt_BR"),
            Path::new("/usr/share/locale/pt_BR/LC_MESSAGES/raiza.mo")
        );
    }

    #[test]
    fn preenche_os_nomes() {
        assert_eq!(
            fill(
                "{name}: {percent}%",
                &[("percent", &80), ("name", &"Rapoo")]
            ),
            "Rapoo: 80%"
        );
        // Valor com chaves não é trocado de novo; nome sem valor e chave solta ficam.
        assert_eq!(fill("{a} {b} {c", &[("a", &"{b}"), ("b", &1)]), "{b} 1 {c");
        assert_eq!(fill("{x}", &[]), "{x}");
        assert_eq!(fill("sem nada", &[("x", &1)]), "sem nada");
        assert_eq!(fill("{x}{x}", &[("x", &"é")]), "éé");
    }

    #[test]
    fn sem_init_sai_em_ingles() {
        // Nenhum teste deste crate chama `init`.
        assert_eq!(gettext("Battery"), "Battery");
        assert_eq!(ngettext("{n} device", "{n} devices", 1), "{n} device");
        assert_eq!(ngettext("{n} device", "{n} devices", 0), "{n} devices");
        assert_eq!(ngettext("{n} device", "{n} devices", 2), "{n} devices");
    }

    /// Monta um `.mo` mínimo (formato little-endian do GNU gettext).
    fn mo(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut entries: Vec<(&str, &str)> = entries.to_vec();
        entries.sort();
        let n = entries.len() as u32;
        let originals = 28;
        let translations = originals + 8 * n;
        let mut data = translations + 8 * n;
        let mut table_o = Vec::new();
        let mut table_t = Vec::new();
        let mut strings = Vec::new();
        for (side, table) in [(0, &mut table_o), (1, &mut table_t)] {
            for e in &entries {
                let s = if side == 0 { e.0 } else { e.1 };
                table.extend_from_slice(&(s.len() as u32).to_le_bytes());
                table.extend_from_slice(&data.to_le_bytes());
                strings.extend_from_slice(s.as_bytes());
                strings.push(0);
                data += s.len() as u32 + 1;
            }
        }
        let mut out = Vec::new();
        for word in [0x9504_12de_u32, 0, n, originals, translations, 0, 0] {
            out.extend_from_slice(&word.to_le_bytes());
        }
        out.extend(table_o);
        out.extend(table_t);
        out.extend(strings);
        out
    }

    #[test]
    fn carrega_pela_ordem_de_linguas_e_pastas() {
        let base = std::env::temp_dir().join(format!("raiza-i18n-{}", std::process::id()));
        let dev = base.join("dev");
        let sys = base.join("sys");
        let header = "Content-Type: text/plain; charset=UTF-8\n\
                      Plural-Forms: nplurals=2; plural=(n > 1);\n";
        let escreve = |dir: &Path, lang: &str, bateria: &str| {
            let path = catalog_path(dir, lang);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let plural = format!("{bateria}\0{bateria}s");
            std::fs::write(
                &path,
                mo(&[
                    ("", header),
                    ("Battery", bateria),
                    ("{n} device\0{n} devices", &plural),
                ]),
            )
            .unwrap();
        };
        escreve(&dev, "pt", "Bateria (dev)");
        escreve(&sys, "pt_BR", "Bateria");
        std::fs::create_dir_all(catalog_path(&sys, "xx").parent().unwrap()).unwrap();
        std::fs::write(catalog_path(&sys, "xx"), b"lixo").unwrap();

        let dirs = [dev.clone(), sys.clone()];
        // pt_BR só existe no sistema; ganha do `pt` da pasta de desenvolvimento.
        let c = load(&["pt_BR".into(), "pt".into()], &dirs).unwrap();
        assert_eq!(c.gettext("Battery"), "Bateria");
        assert_eq!(c.gettext("DPI"), "DPI");
        assert_eq!(c.ngettext("{n} device", "{n} devices", 1), "Bateria");
        assert_eq!(c.ngettext("{n} device", "{n} devices", 0), "Bateria");
        assert_eq!(c.ngettext("{n} device", "{n} devices", 2), "Baterias");
        // pt_PT cai para pt.
        let c = load(&["pt_PT".into(), "pt".into()], &dirs).unwrap();
        assert_eq!(c.gettext("Battery"), "Bateria (dev)");
        // Catálogo ilegível ou ausente: inglês.
        assert!(load(&["xx".into()], &dirs).is_none());
        assert!(load(&["de".into()], &dirs).is_none());
        assert!(load(&[], &dirs).is_none());
        std::fs::remove_dir_all(&base).unwrap();
    }
}
