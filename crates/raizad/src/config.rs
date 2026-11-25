//! Configurações salvas: o último valor escrito em cada dispositivo.
//!
//! Formato de `~/.config/raiza/config.json`:
//!
//! ```json
//! {
//!   "version": 1,
//!   "devices": {
//!     "usb:24ae:1423:0": {
//!       "dpi": 1600,
//!       "rgb": { "mode": "static", "color": "#ff0000", "zones": { "1": "#00ff00" } }
//!     },
//!     "bt:AA:BB:CC:DD:EE:FF": { "volume": 40, "mute": false }
//!   }
//! }
//! ```
//!
//! Só valores que descrevem um **estado** são salvos (DPI, taxa, RGB, volume, mudo).
//! Vibração (um pulso) e conexão (uma ação) não são reaplicadas.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use raiza_core::{CapabilityValue, Color, DeviceId, RgbSetting};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// Versão do formato do arquivo.
const FORMAT_VERSION: u32 = 1;

/// Iluminação salva, normalizada: o modo vale para o dispositivo todo, a cor geral para
/// todas as zonas e as cores por zona por cima dela.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RgbSaved {
    /// Último modo escolhido.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// Última cor aplicada a todas as zonas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
    /// Cores aplicadas a uma zona só, depois da cor geral.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub zones: BTreeMap<u32, Color>,
}

/// Valores salvos de um dispositivo.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceSettings {
    /// DPI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dpi: Option<u32>,
    /// Taxa de atualização em Hz.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polling_rate: Option<u32>,
    /// Volume (%).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<u32>,
    /// Mudo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mute: Option<bool>,
    /// Iluminação.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rgb: Option<RgbSaved>,
}

impl DeviceSettings {
    /// Guarda `value`. Devolve `false` se o tipo não é salvo (vibração, conexão).
    pub fn remember(&mut self, value: &CapabilityValue) -> bool {
        match value {
            CapabilityValue::Dpi(v) => self.dpi = Some(*v),
            CapabilityValue::PollingRate(v) => self.polling_rate = Some(*v),
            CapabilityValue::Volume(v) => self.volume = Some(*v),
            CapabilityValue::Mute(v) => self.mute = Some(*v),
            CapabilityValue::Rgb(setting) => {
                let rgb = self.rgb.get_or_insert_with(RgbSaved::default);
                if let Some(mode) = &setting.mode {
                    rgb.mode = Some(mode.clone());
                }
                if let Some(color) = setting.color {
                    match setting.zone {
                        Some(zone) => {
                            rgb.zones.insert(zone, color);
                        }
                        // Cor geral cobre as cores por zona anteriores.
                        None => {
                            rgb.color = Some(color);
                            rgb.zones.clear();
                        }
                    }
                }
            }
            CapabilityValue::Rumble { .. } | CapabilityValue::Connection(_) => return false,
        }
        true
    }

    /// Valores a reaplicar, na ordem em que devem ser enviados.
    pub fn values(&self) -> Vec<CapabilityValue> {
        let mut out = Vec::new();
        if let Some(v) = self.dpi {
            out.push(CapabilityValue::Dpi(v));
        }
        if let Some(v) = self.polling_rate {
            out.push(CapabilityValue::PollingRate(v));
        }
        if let Some(rgb) = &self.rgb {
            if rgb.mode.is_some() || rgb.color.is_some() {
                out.push(CapabilityValue::Rgb(RgbSetting {
                    zone: None,
                    mode: rgb.mode.clone(),
                    color: rgb.color,
                }));
            }
            for (zone, color) in &rgb.zones {
                out.push(CapabilityValue::Rgb(RgbSetting {
                    zone: Some(*zone),
                    mode: None,
                    color: Some(*color),
                }));
            }
        }
        if let Some(v) = self.volume {
            out.push(CapabilityValue::Volume(v));
        }
        if let Some(v) = self.mute {
            out.push(CapabilityValue::Mute(v));
        }
        out
    }
}

/// Conteúdo do arquivo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ConfigFile {
    version: u32,
    #[serde(default)]
    devices: BTreeMap<DeviceId, DeviceSettings>,
}

impl Default for ConfigFile {
    fn default() -> Self {
        ConfigFile {
            version: FORMAT_VERSION,
            devices: BTreeMap::new(),
        }
    }
}

/// Erro ao gravar as configurações.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Falha de E/S.
    #[error("falha ao gravar {path}: {source}")]
    Io {
        /// Arquivo.
        path: PathBuf,
        /// Erro original.
        source: io::Error,
    },
    /// Falha ao gerar o JSON.
    #[error("falha ao serializar as configurações: {0}")]
    Json(#[from] serde_json::Error),
}

/// Configurações em memória, gravadas em disco a cada mudança.
#[derive(Debug)]
pub struct ConfigStore {
    path: Option<PathBuf>,
    data: ConfigFile,
}

impl ConfigStore {
    /// Caminho padrão: `$XDG_CONFIG_HOME/raiza/config.json` ou `~/.config/raiza/config.json`.
    pub fn default_path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .filter(|v| !v.is_empty())
                    .map(|h| PathBuf::from(h).join(".config"))
            })?;
        Some(base.join("raiza").join("config.json"))
    }

    /// Configurações só em memória (testes, ou sem `HOME`).
    pub fn in_memory() -> Self {
        ConfigStore {
            path: None,
            data: ConfigFile::default(),
        }
    }

    /// Lê o arquivo. Sem arquivo, começa vazio. Um arquivo ilegível é renomeado para
    /// `config.json.corrompido` (para não ser sobrescrito sem aviso) e o daemon começa vazio.
    pub fn load(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let data = match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<ConfigFile>(&text) {
                Ok(data) => {
                    info!(path = %path.display(), "configurações carregadas");
                    data
                }
                Err(e) => {
                    let backup = path.with_extension("json.corrompido");
                    warn!(
                        path = %path.display(),
                        backup = %backup.display(),
                        error = %e,
                        "configurações ilegíveis; começando do zero"
                    );
                    if let Err(e) = std::fs::rename(&path, &backup) {
                        warn!(error = %e, "não foi possível guardar a cópia do arquivo ilegível");
                    }
                    ConfigFile::default()
                }
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => ConfigFile::default(),
            Err(e) => {
                warn!(path = %path.display(), error = %e, "não foi possível ler as configurações");
                ConfigFile::default()
            }
        };
        ConfigStore {
            path: Some(path),
            data,
        }
    }

    /// Arquivo usado, se houver.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Valores salvos do dispositivo, na ordem de reaplicação.
    pub fn saved(&self, id: &DeviceId) -> Vec<CapabilityValue> {
        self.data
            .devices
            .get(id)
            .map(DeviceSettings::values)
            .unwrap_or_default()
    }

    /// Guarda `value` como o último escrito em `id` e grava o arquivo.
    /// Tipos que não são salvos (vibração, conexão) são ignorados.
    pub fn remember(&mut self, id: &DeviceId, value: &CapabilityValue) -> Result<(), ConfigError> {
        let mut settings = self.data.devices.get(id).cloned().unwrap_or_default();
        if !settings.remember(value) {
            return Ok(());
        }
        self.data.devices.insert(id.clone(), settings);
        self.save()
    }

    /// Grava de forma atômica (arquivo temporário + `rename`).
    fn save(&self) -> Result<(), ConfigError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let io_err = |source| ConfigError::Io {
            path: path.clone(),
            source,
        };
        let json = serde_json::to_string_pretty(&self.data)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(io_err)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json + "\n").map_err(io_err)?;
        std::fs::rename(&tmp, path).map_err(io_err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> DeviceId {
        DeviceId::usb(0x24ae, 0x1423, "0")
    }

    fn cor(s: &str) -> Color {
        s.parse().unwrap()
    }

    #[test]
    fn grava_e_le_de_volta() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("raiza").join("config.json");
        let mut store = ConfigStore::load(&path);
        assert!(store.saved(&id()).is_empty());
        store.remember(&id(), &CapabilityValue::Dpi(800)).unwrap();
        store.remember(&id(), &CapabilityValue::Dpi(1600)).unwrap();
        store.remember(&id(), &CapabilityValue::Mute(true)).unwrap();
        store
            .remember(
                &id(),
                &CapabilityValue::Rumble {
                    strength: 1,
                    duration_ms: 1,
                },
            )
            .unwrap();

        let de_novo = ConfigStore::load(&path);
        assert_eq!(
            de_novo.saved(&id()),
            [CapabilityValue::Dpi(1600), CapabilityValue::Mute(true)]
        );
        let texto = std::fs::read_to_string(&path).unwrap();
        assert!(texto.contains("\"usb:24ae:1423:0\""), "{texto}");
    }

    #[test]
    fn rgb_normalizado() {
        let mut s = DeviceSettings::default();
        let rgb = |zone, mode: Option<&str>, color: Option<&str>| {
            CapabilityValue::Rgb(RgbSetting {
                zone,
                mode: mode.map(String::from),
                color: color.map(cor),
            })
        };
        s.remember(&rgb(Some(1), None, Some("#00ff00")));
        s.remember(&rgb(None, Some("static"), Some("#ff0000")));
        s.remember(&rgb(Some(2), None, Some("#0000ff")));
        s.remember(&rgb(None, Some("wave"), None));
        assert_eq!(
            s.values(),
            [
                rgb(None, Some("wave"), Some("#ff0000")),
                rgb(Some(2), None, Some("#0000ff")),
            ]
        );
    }

    #[test]
    fn arquivo_ilegivel_e_preservado() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, "{lixo").unwrap();
        let mut store = ConfigStore::load(&path);
        assert!(store.saved(&id()).is_empty());
        assert!(dir.path().join("config.json.corrompido").exists());
        store.remember(&id(), &CapabilityValue::Volume(10)).unwrap();
        assert_eq!(
            ConfigStore::load(&path).saved(&id()),
            [CapabilityValue::Volume(10)]
        );
    }
}
