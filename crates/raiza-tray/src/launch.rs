//! Abre o app GTK pelo `.desktop` (app id `io.github.yurileo01.Raiza`).
//!
//! Tenta `gtk-launch <app id>` e, se não existir, `gio launch <caminho do .desktop>`.
//! Assim o app abre como o lançador do ambiente o abriria (ativação D-Bus, token de foco).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use tokio::process::Command;
use tracing::{debug, warn};

/// App id da interface GTK.
pub const APP_ID: &str = "io.github.yurileo01.Raiza";

/// Diretórios de dados do XDG, na ordem de busca (`XDG_DATA_HOME` primeiro).
pub fn data_dirs(
    data_home: Option<OsString>,
    data_dirs: Option<OsString>,
    home: Option<OsString>,
) -> Vec<PathBuf> {
    let non_empty = |v: Option<OsString>| v.filter(|s| !s.is_empty());
    let mut dirs = Vec::new();
    match non_empty(data_home) {
        Some(dir) => dirs.push(PathBuf::from(dir)),
        None => {
            if let Some(home) = non_empty(home) {
                dirs.push(Path::new(&home).join(".local/share"));
            }
        }
    }
    let system = non_empty(data_dirs).unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    dirs.extend(std::env::split_paths(&system).filter(|p| !p.as_os_str().is_empty()));
    dirs
}

/// Caminho de `applications/<APP_ID>.desktop` em cada diretório de dados.
pub fn desktop_candidates(dirs: &[PathBuf]) -> Vec<PathBuf> {
    dirs.iter()
        .map(|d| d.join("applications").join(format!("{APP_ID}.desktop")))
        .collect()
}

fn command(program: &str, token: Option<&str>) -> Command {
    let mut cmd = Command::new(program);
    if let Some(token) = token {
        // Com o token, o compositor Wayland (e o X11, via startup id) deixa a janela ganhar foco.
        cmd.env("XDG_ACTIVATION_TOKEN", token)
            .env("DESKTOP_STARTUP_ID", token);
    }
    cmd.kill_on_drop(false);
    cmd
}

/// Abre o app e espera o lançador terminar (ele sai logo depois de ativar o app).
pub async fn open_app(token: Option<String>) {
    let token = token.as_deref();
    match command("gtk-launch", token).arg(APP_ID).status().await {
        Ok(status) if status.success() => {
            debug!("app aberto com gtk-launch");
            return;
        }
        Ok(status) => warn!(%status, "gtk-launch falhou; tentando gio launch"),
        Err(err) => debug!(%err, "gtk-launch indisponível; tentando gio launch"),
    }

    let dirs = data_dirs(
        std::env::var_os("XDG_DATA_HOME"),
        std::env::var_os("XDG_DATA_DIRS"),
        std::env::var_os("HOME"),
    );
    let Some(desktop) = desktop_candidates(&dirs).into_iter().find(|p| p.is_file()) else {
        warn!(app = APP_ID, "não achei o .desktop do app");
        return;
    };
    match command("gio", token)
        .arg("launch")
        .arg(&desktop)
        .status()
        .await
    {
        Ok(status) if status.success() => debug!(path = %desktop.display(), "app aberto com gio"),
        Ok(status) => warn!(%status, "gio launch falhou"),
        Err(err) => warn!(%err, "não consegui abrir o app (sem gtk-launch nem gio)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diretorios_xdg() {
        let dirs = data_dirs(None, None, Some("/home/u".into()));
        assert_eq!(
            dirs,
            [
                PathBuf::from("/home/u/.local/share"),
                PathBuf::from("/usr/local/share"),
                PathBuf::from("/usr/share"),
            ]
        );
        let dirs = data_dirs(
            Some("/dados".into()),
            Some("/a::/b".into()),
            Some("/home/u".into()),
        );
        assert_eq!(dirs, [PathBuf::from("/dados"), "/a".into(), "/b".into()]);
        // Variáveis vazias valem como ausentes.
        assert_eq!(data_dirs(Some("".into()), Some("".into()), None).len(), 2);
    }

    #[test]
    fn candidatos_do_desktop() {
        let c = desktop_candidates(&[PathBuf::from("/usr/share")]);
        assert_eq!(
            c,
            [PathBuf::from(
                "/usr/share/applications/io.github.yurileo01.Raiza.desktop"
            )]
        );
    }
}
