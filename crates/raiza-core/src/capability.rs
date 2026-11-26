//! Capacidades padronizadas: o que um dispositivo sabe fazer.
//!
//! As interfaces desenham as telas a partir das capacidades, não da marca (pilar 3).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::{CapabilityValue, RaizaError};

/// Valor numérico com limites: volume, DPI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Level {
    /// Valor atual.
    pub current: u32,
    /// Menor valor aceito (inclusive).
    pub min: u32,
    /// Maior valor aceito (inclusive).
    pub max: u32,
    /// Passo entre valores aceitos, a partir de `min`. `0` ou `1` aceitam qualquer valor no intervalo.
    pub step: u32,
}

impl Level {
    /// Verifica se `value` respeita `min`, `max` e `step`. Devolve a explicação do problema.
    pub fn check(&self, value: u32) -> Result<(), String> {
        if value < self.min || value > self.max {
            return Err(format!(
                "{value} fora do intervalo {}..={}",
                self.min, self.max
            ));
        }
        if self.step > 1 && !(value - self.min).is_multiple_of(self.step) {
            return Err(format!(
                "{value} não é múltiplo do passo {} a partir de {}",
                self.step, self.min
            ));
        }
        Ok(())
    }
}

/// Cor RGB de 24 bits. Em texto e em serde vira `#rrggbb`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct Color {
    /// Vermelho.
    pub r: u8,
    /// Verde.
    pub g: u8,
    /// Azul.
    pub b: u8,
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

impl FromStr for Color {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let hex = s
            .strip_prefix('#')
            .filter(|h| h.len() == 6 && h.is_ascii())
            .ok_or_else(|| format!("cor inválida (use #rrggbb): {s}"))?;
        let byte = |i: usize| {
            u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| format!("cor inválida: {s}"))
        };
        Ok(Color {
            r: byte(0)?,
            g: byte(2)?,
            b: byte(4)?,
        })
    }
}

impl From<Color> for String {
    fn from(c: Color) -> Self {
        c.to_string()
    }
}

impl TryFrom<String> for Color {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

/// Uma zona de iluminação (ex.: logo, roda, faixa de LEDs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RgbZone {
    /// Nome legível da zona.
    pub name: String,
    /// Cor atual da zona.
    pub color: Color,
}

/// Como o dispositivo está conectado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionKind {
    /// Cabo USB.
    Wired,
    /// Dongle sem fio (2.4 GHz).
    Wireless,
    /// Bluetooth.
    Bluetooth,
    /// Outro meio.
    Other,
}

/// Uma capacidade do dispositivo, com valor atual e limites.
///
/// Capacidades que podem ser alteradas têm a flag `writable`. Ela é `false` quando o
/// hardware não aceita escrita **ou** quando o perfil ainda não foi verificado (pilar 1).
/// Bateria é sempre somente leitura.
///
/// Em serde, o campo `kind` traz o [`CapabilityKind`] em `snake_case`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Capability {
    /// Nível de bateria.
    Battery {
        /// Carga de 0 a 100.
        percent: u8,
        /// Se está carregando.
        charging: bool,
    },
    /// Resolução do sensor de um mouse.
    Dpi {
        /// DPI atual e limites.
        level: Level,
        /// Estágios configurados no dispositivo (o botão de DPI alterna entre eles).
        stages: Vec<u32>,
        /// Índice (a partir de 0) do estágio em uso, quando o dispositivo informa. Escrever
        /// o DPI muda o valor deste estágio.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        active_stage: Option<u32>,
        /// Se aceita escrita.
        writable: bool,
    },
    /// Taxa de atualização (polling) em Hz.
    PollingRate {
        /// Taxa atual em Hz.
        current: u32,
        /// Taxas aceitas em Hz.
        supported: Vec<u32>,
        /// Se aceita escrita.
        writable: bool,
    },
    /// Iluminação RGB.
    Rgb {
        /// Zonas de iluminação, na ordem do dispositivo.
        zones: Vec<RgbZone>,
        /// Modos aceitos (ex.: `static`, `breathing`, `wave`).
        modes: Vec<String>,
        /// Modo atual (um dos `modes`).
        current_mode: String,
        /// Se aceita escrita.
        writable: bool,
    },
    /// Volume, em porcentagem (pode passar de 100 com amplificação).
    Volume {
        /// Volume atual e limites.
        level: Level,
        /// Se aceita escrita.
        writable: bool,
    },
    /// Mudo.
    Mute {
        /// Se está mudo.
        muted: bool,
        /// Se aceita escrita.
        writable: bool,
    },
    /// Vibração (controles). Escrever dispara um teste de vibração.
    Rumble {
        /// Maior duração aceita para um pulso, em milissegundos.
        max_duration_ms: u32,
        /// Se aceita escrita.
        writable: bool,
    },
    /// Estado da conexão. Quando gravável, permite conectar e desconectar
    /// (ex.: Bluetooth via BlueZ).
    Connection {
        /// Meio de conexão.
        medium: ConnectionKind,
        /// Se está conectado agora.
        connected: bool,
        /// Se aceita escrita (conectar/desconectar).
        writable: bool,
    },
}

impl Capability {
    /// Tipo desta capacidade.
    pub fn kind(&self) -> CapabilityKind {
        match self {
            Capability::Battery { .. } => CapabilityKind::Battery,
            Capability::Dpi { .. } => CapabilityKind::Dpi,
            Capability::PollingRate { .. } => CapabilityKind::PollingRate,
            Capability::Rgb { .. } => CapabilityKind::Rgb,
            Capability::Volume { .. } => CapabilityKind::Volume,
            Capability::Mute { .. } => CapabilityKind::Mute,
            Capability::Rumble { .. } => CapabilityKind::Rumble,
            Capability::Connection { .. } => CapabilityKind::Connection,
        }
    }

    /// Se a capacidade aceita escrita.
    pub fn is_writable(&self) -> bool {
        match self {
            Capability::Battery { .. } => false,
            Capability::Dpi { writable, .. }
            | Capability::PollingRate { writable, .. }
            | Capability::Rgb { writable, .. }
            | Capability::Volume { writable, .. }
            | Capability::Mute { writable, .. }
            | Capability::Rumble { writable, .. }
            | Capability::Connection { writable, .. } => *writable,
        }
    }

    /// Marca a capacidade como somente leitura (ex.: perfil não verificado, pilar 1).
    pub fn set_read_only(&mut self) {
        match self {
            Capability::Battery { .. } => {}
            Capability::Dpi { writable, .. }
            | Capability::PollingRate { writable, .. }
            | Capability::Rgb { writable, .. }
            | Capability::Volume { writable, .. }
            | Capability::Mute { writable, .. }
            | Capability::Rumble { writable, .. }
            | Capability::Connection { writable, .. } => *writable = false,
        }
    }

    /// Valida `value` (veja [`CapabilityValue::validate`]) e, se válido, atualiza o
    /// estado desta capacidade para refletir a escrita.
    ///
    /// Útil para o daemon manter o cache depois de uma escrita bem-sucedida e para
    /// provedores falsos. Um pedido de vibração não muda o estado.
    pub fn apply(&mut self, value: &CapabilityValue) -> Result<(), RaizaError> {
        value.validate(self)?;
        match (self, value) {
            (Capability::Dpi { level, .. }, CapabilityValue::Dpi(v))
            | (Capability::Volume { level, .. }, CapabilityValue::Volume(v)) => {
                level.current = *v;
            }
            (Capability::PollingRate { current, .. }, CapabilityValue::PollingRate(v)) => {
                *current = *v;
            }
            (Capability::Mute { muted, .. }, CapabilityValue::Mute(v)) => *muted = *v,
            (Capability::Connection { connected, .. }, CapabilityValue::Connection(v)) => {
                *connected = *v;
            }
            (
                Capability::Rgb {
                    zones,
                    current_mode,
                    ..
                },
                CapabilityValue::Rgb(setting),
            ) => {
                if let Some(mode) = &setting.mode {
                    current_mode.clone_from(mode);
                }
                if let Some(color) = setting.color {
                    match setting.zone {
                        Some(i) => {
                            if let Some(zone) = zones.get_mut(i as usize) {
                                zone.color = color;
                            }
                        }
                        None => zones.iter_mut().for_each(|z| z.color = color),
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

/// Tipo de capacidade, sem dados. Em texto e em serde vira `snake_case` (ex.: `polling_rate`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityKind {
    /// Veja [`Capability::Battery`].
    Battery,
    /// Veja [`Capability::Dpi`].
    Dpi,
    /// Veja [`Capability::PollingRate`].
    PollingRate,
    /// Veja [`Capability::Rgb`].
    Rgb,
    /// Veja [`Capability::Volume`].
    Volume,
    /// Veja [`Capability::Mute`].
    Mute,
    /// Veja [`Capability::Rumble`].
    Rumble,
    /// Veja [`Capability::Connection`].
    Connection,
}

impl CapabilityKind {
    /// Todos os tipos, na ordem de declaração.
    pub const ALL: [CapabilityKind; 8] = [
        CapabilityKind::Battery,
        CapabilityKind::Dpi,
        CapabilityKind::PollingRate,
        CapabilityKind::Rgb,
        CapabilityKind::Volume,
        CapabilityKind::Mute,
        CapabilityKind::Rumble,
        CapabilityKind::Connection,
    ];

    /// Nome em `snake_case`, o mesmo usado em serde e no D-Bus.
    pub fn as_str(self) -> &'static str {
        match self {
            CapabilityKind::Battery => "battery",
            CapabilityKind::Dpi => "dpi",
            CapabilityKind::PollingRate => "polling_rate",
            CapabilityKind::Rgb => "rgb",
            CapabilityKind::Volume => "volume",
            CapabilityKind::Mute => "mute",
            CapabilityKind::Rumble => "rumble",
            CapabilityKind::Connection => "connection",
        }
    }
}

impl fmt::Display for CapabilityKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CapabilityKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        CapabilityKind::ALL
            .into_iter()
            .find(|k| k.as_str() == s)
            .ok_or_else(|| format!("capacidade desconhecida: {s}"))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Uma capacidade de cada tipo, todas graváveis quando possível.
    pub(crate) fn exemplos() -> Vec<Capability> {
        vec![
            Capability::Battery {
                percent: 80,
                charging: false,
            },
            Capability::Dpi {
                level: Level {
                    current: 1600,
                    min: 400,
                    max: 16000,
                    step: 50,
                },
                stages: vec![800, 1600, 3200],
                active_stage: None,
                writable: true,
            },
            Capability::PollingRate {
                current: 1000,
                supported: vec![125, 500, 1000],
                writable: true,
            },
            Capability::Rgb {
                zones: vec![
                    RgbZone {
                        name: "logo".into(),
                        color: Color { r: 255, g: 0, b: 0 },
                    },
                    RgbZone {
                        name: "roda".into(),
                        color: Color { r: 0, g: 0, b: 255 },
                    },
                ],
                modes: vec!["static".into(), "breathing".into()],
                current_mode: "static".into(),
                writable: true,
            },
            Capability::Volume {
                level: Level {
                    current: 50,
                    min: 0,
                    max: 150,
                    step: 1,
                },
                writable: true,
            },
            Capability::Mute {
                muted: false,
                writable: true,
            },
            Capability::Rumble {
                max_duration_ms: 2000,
                writable: true,
            },
            Capability::Connection {
                medium: ConnectionKind::Bluetooth,
                connected: true,
                writable: true,
            },
        ]
    }

    #[test]
    fn um_exemplo_por_tipo() {
        let kinds: Vec<_> = exemplos().iter().map(Capability::kind).collect();
        assert_eq!(kinds, CapabilityKind::ALL);
    }

    #[test]
    fn capability_serde_ida_e_volta() {
        for cap in exemplos() {
            let json = serde_json::to_value(&cap).unwrap();
            // O campo `kind` usa o mesmo nome que CapabilityKind.
            assert_eq!(json["kind"], cap.kind().as_str());
            assert_eq!(serde_json::from_value::<Capability>(json).unwrap(), cap);
        }
    }

    #[test]
    fn kind_texto_e_serde_ida_e_volta() {
        for kind in CapabilityKind::ALL {
            assert_eq!(kind.to_string().parse::<CapabilityKind>().unwrap(), kind);
            let json = serde_json::to_string(&kind).unwrap();
            assert_eq!(json, format!("\"{kind}\""));
            assert_eq!(serde_json::from_str::<CapabilityKind>(&json).unwrap(), kind);
        }
        assert!("desconhecida".parse::<CapabilityKind>().is_err());
    }

    #[test]
    fn color_texto_e_serde() {
        let c = Color {
            r: 0x12,
            g: 0xab,
            b: 0xff,
        };
        assert_eq!(c.to_string(), "#12abff");
        assert_eq!("#12ABFF".parse::<Color>().unwrap(), c);
        assert_eq!(serde_json::to_string(&c).unwrap(), "\"#12abff\"");
        assert_eq!(serde_json::from_str::<Color>("\"#12abff\"").unwrap(), c);
        for s in ["12abff", "#12abf", "#12abfg", "#12abffe", "#ééé"] {
            assert!(s.parse::<Color>().is_err(), "{s}");
        }
    }

    #[test]
    fn auxiliares_serde() {
        let zona = RgbZone {
            name: "faixa".into(),
            color: Color { r: 1, g: 2, b: 3 },
        };
        let json = serde_json::to_string(&zona).unwrap();
        assert_eq!(serde_json::from_str::<RgbZone>(&json).unwrap(), zona);
        for k in [
            ConnectionKind::Wired,
            ConnectionKind::Wireless,
            ConnectionKind::Bluetooth,
            ConnectionKind::Other,
        ] {
            let json = serde_json::to_string(&k).unwrap();
            assert_eq!(serde_json::from_str::<ConnectionKind>(&json).unwrap(), k);
        }
    }

    #[test]
    fn somente_leitura() {
        for mut cap in exemplos() {
            cap.set_read_only();
            assert!(!cap.is_writable(), "{}", cap.kind());
        }
    }

    #[test]
    fn level_passo() {
        let l = Level {
            current: 400,
            min: 400,
            max: 1000,
            step: 50,
        };
        assert!(l.check(400).is_ok());
        assert!(l.check(1000).is_ok());
        assert!(l.check(450).is_ok());
        assert!(l.check(425).is_err());
        assert!(l.check(350).is_err());
        assert!(l.check(1050).is_err());
    }
}
