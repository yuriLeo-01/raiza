//! De onde vêm os perfis: diretórios no disco, vigiados com `notify`.

use std::path::{Path, PathBuf};

use notify::{EventKind, RecursiveMode, Watcher};
use raiza_core::RaizaError;
use raiza_profiles::{Profile, ProfileError};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::backend::ChangeStream;

/// Fonte dos perfis. Injetável para testar sem disco.
pub trait ProfileSource: Send + Sync + 'static {
    /// Carrega todos os perfis válidos, em ordem de prioridade (o primeiro que servir
    /// para um aparelho vence). Perfis inválidos ficam de fora, com o erro no log.
    /// Chamado fora do runtime assíncrono.
    fn load(&self) -> Vec<Profile>;

    /// Começa a vigiar mudanças nos perfis. Chamado de dentro do runtime tokio.
    fn watch(&self) -> Result<ChangeStream, RaizaError>;
}

/// Perfis em diretórios (`<dir>/<fabricante>/<modelo>.json`, em qualquer profundidade).
#[derive(Debug, Clone)]
pub struct DirProfiles {
    dirs: Vec<PathBuf>,
}

impl DirProfiles {
    /// Lê os diretórios nesta ordem: os primeiros têm prioridade.
    pub fn new(dirs: impl IntoIterator<Item = PathBuf>) -> Self {
        DirProfiles {
            dirs: dirs.into_iter().collect(),
        }
    }

    /// Diretórios padrão, do mais prioritário ao menos: o do usuário
    /// (`$XDG_DATA_HOME/raiza/profiles`, normalmente `~/.local/share/raiza/profiles`) e os
    /// do sistema (`<cada $XDG_DATA_DIRS>/raiza/profiles`, normalmente
    /// `/usr/local/share` e `/usr/share`), onde o pacote instala o `profiles/` do repositório.
    pub fn default_dirs() -> Vec<PathBuf> {
        let env = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty());
        let mut dirs = Vec::new();
        let user = env("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| env("HOME").map(|h| PathBuf::from(h).join(".local/share")));
        dirs.extend(user.map(|d| d.join("raiza/profiles")));
        let system = env("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into());
        dirs.extend(std::env::split_paths(&system).map(|d| d.join("raiza/profiles")));
        dirs
    }

    /// Diretórios lidos, em ordem de prioridade.
    pub fn dirs(&self) -> &[PathBuf] {
        &self.dirs
    }
}

impl Default for DirProfiles {
    fn default() -> Self {
        Self::new(Self::default_dirs())
    }
}

/// Todos os `.json` de `dir` (recursivo, em ordem de caminho) e o resultado da leitura de
/// cada um. `schema.json` (o JSON Schema dos perfis) fica de fora. Diretório ausente: vazio.
pub fn load_dir(dir: &Path) -> Vec<(PathBuf, Result<Profile, ProfileError>)> {
    let mut files = Vec::new();
    collect_json(dir, &mut files);
    files.sort();
    files
        .into_iter()
        .map(|path| {
            let result = std::fs::read_to_string(&path)
                .map_err(|e| ProfileError::Json(format!("não deu para ler o arquivo: {e}")))
                .and_then(|json| raiza_profiles::load(&json));
            (path, result)
        })
        .collect()
}

fn collect_json(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_json(&path, out);
        } else if path.extension().is_some_and(|e| e == "json")
            && path.file_name().is_some_and(|n| n != "schema.json")
        {
            out.push(path);
        }
    }
}

impl ProfileSource for DirProfiles {
    fn load(&self) -> Vec<Profile> {
        let mut profiles = Vec::new();
        for dir in &self.dirs {
            for (path, result) in load_dir(dir) {
                match result {
                    Ok(p) => {
                        debug!(path = %path.display(), name = %p.name, "perfil carregado");
                        profiles.push(p);
                    }
                    Err(e) => {
                        warn!(path = %path.display(), error = %e, "perfil inválido, ignorado")
                    }
                }
            }
        }
        info!(count = profiles.len(), "perfis HID carregados");
        profiles
    }

    fn watch(&self) -> Result<ChangeStream, RaizaError> {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut watcher =
            notify::recommended_watcher(move |res: notify::Result<notify::Event>| match res {
                Ok(event) if !matches!(event.kind, EventKind::Access(_)) => {
                    let _ = tx.send(());
                }
                Ok(_) => {}
                Err(e) => warn!(error = %e, "falha ao vigiar os perfis"),
            })
            .map_err(|e| RaizaError::Io(format!("vigia dos perfis: {e}")))?;
        let mut watching = 0;
        for dir in &self.dirs {
            match watcher.watch(dir, RecursiveMode::Recursive) {
                Ok(()) => watching += 1,
                // Diretório que ainda não existe (comum no do usuário).
                Err(e) => {
                    debug!(dir = %dir.display(), error = %e, "diretório de perfis não vigiado")
                }
            }
        }
        if watching == 0 {
            info!("nenhum diretório de perfis existe; mudanças nos perfis não serão vistas");
        }
        // O watcher vive junto com o fluxo.
        let stream = futures_util::stream::unfold((watcher, rx), |(watcher, mut rx)| async move {
            rx.recv().await.map(|()| ((), (watcher, rx)))
        });
        Ok(Box::pin(stream))
    }
}
