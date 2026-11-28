//! Valores que se pedem a um dispositivo, e a validação deles (pilar 1).

use serde::{Deserialize, Serialize};

use crate::{Capability, CapabilityKind, Color, RaizaError};

/// Alteração de iluminação. Precisa trazer pelo menos `mode` ou `color`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RgbSetting {
    /// Índice da zona em [`Capability::Rgb::zones`]; `None` aplica a todas.
    pub zone: Option<u32>,
    /// Novo modo (precisa estar em `modes`); `None` mantém o atual.
    pub mode: Option<String>,
    /// Nova cor; `None` mantém a atual.
    pub color: Option<Color>,
}

/// O que se envia num `set`: o novo valor de uma capacidade.
///
/// Antes de chegar ao provedor, todo valor passa por [`CapabilityValue::validate`]
/// contra a capacidade anunciada pelo dispositivo.
///
/// Em serde: `{"kind": "dpi", "value": 1600}`, com `kind` igual ao [`CapabilityKind`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum CapabilityValue {
    /// Novo DPI.
    Dpi(u32),
    /// Novo valor de um estágio da tabela de DPI (o em uso ou outro). Em serde:
    /// `{"kind": "dpi_stage", "value": {"stage": 2, "value": 950}}`.
    DpiStage {
        /// Índice do estágio, a partir de 0 (precisa existir em `stages`).
        stage: u32,
        /// Novo DPI do estágio.
        value: u32,
    },
    /// Nova taxa de atualização em Hz.
    PollingRate(u32),
    /// Nova taxa de varredura (Key Scan Rate) em Hz.
    ScanRate(u32),
    /// Nova iluminação.
    Rgb(RgbSetting),
    /// Novo volume em porcentagem.
    Volume(u32),
    /// Liga ou desliga o mudo.
    Mute(bool),
    /// Conecta (`true`) ou desconecta (`false`).
    Connection(bool),
    /// Dispara um pulso de vibração.
    Rumble {
        /// Intensidade de 0 a 100.
        strength: u8,
        /// Duração em milissegundos (até `max_duration_ms`).
        duration_ms: u32,
    },
}

impl CapabilityValue {
    /// Tipo de capacidade que este valor altera. `DpiStage` altera [`CapabilityKind::Dpi`].
    pub fn kind(&self) -> CapabilityKind {
        match self {
            CapabilityValue::Dpi(_) | CapabilityValue::DpiStage { .. } => CapabilityKind::Dpi,
            CapabilityValue::PollingRate(_) => CapabilityKind::PollingRate,
            CapabilityValue::ScanRate(_) => CapabilityKind::ScanRate,
            CapabilityValue::Rgb(_) => CapabilityKind::Rgb,
            CapabilityValue::Volume(_) => CapabilityKind::Volume,
            CapabilityValue::Mute(_) => CapabilityKind::Mute,
            CapabilityValue::Rumble { .. } => CapabilityKind::Rumble,
            CapabilityValue::Connection(_) => CapabilityKind::Connection,
        }
    }

    /// Valida este valor contra a capacidade que o dispositivo anunciou.
    ///
    /// - tipo diferente: [`RaizaError::Unsupported`];
    /// - capacidade somente leitura: [`RaizaError::ReadOnly`];
    /// - fora dos limites: [`RaizaError::OutOfRange`].
    pub fn validate(&self, capability: &Capability) -> Result<(), RaizaError> {
        let kind = self.kind();
        if capability.kind() != kind {
            return Err(RaizaError::Unsupported(kind));
        }
        if !capability.is_writable() {
            return Err(RaizaError::ReadOnly(kind));
        }
        let out_of_range = |detail: String| RaizaError::OutOfRange { kind, detail };
        match (self, capability) {
            (CapabilityValue::Dpi(v), Capability::Dpi { level, .. })
            | (CapabilityValue::Volume(v), Capability::Volume { level, .. }) => {
                level.check(*v).map_err(out_of_range)
            }
            (CapabilityValue::DpiStage { stage, value }, Capability::Dpi { level, stages, .. }) => {
                if stages.is_empty() {
                    Err(out_of_range(
                        "a tabela de estágios deste dispositivo é desconhecida".into(),
                    ))
                } else if *stage as usize >= stages.len() {
                    Err(out_of_range(format!(
                        "estágio {stage} não existe (o dispositivo tem {})",
                        stages.len()
                    )))
                } else {
                    level.check(*value).map_err(out_of_range)
                }
            }
            (CapabilityValue::PollingRate(v), Capability::PollingRate { supported, .. })
            | (CapabilityValue::ScanRate(v), Capability::ScanRate { supported, .. }) => {
                if supported.contains(v) {
                    Ok(())
                } else {
                    Err(out_of_range(format!(
                        "{v} Hz não está entre as taxas aceitas {supported:?}"
                    )))
                }
            }
            (CapabilityValue::Rgb(s), Capability::Rgb { zones, modes, .. }) => {
                if s.mode.is_none() && s.color.is_none() {
                    return Err(out_of_range("informe um modo ou uma cor".into()));
                }
                match (s.zone, &s.mode) {
                    (Some(zone), _) if zone as usize >= zones.len() => Err(out_of_range(format!(
                        "zona {zone} não existe (o dispositivo tem {})",
                        zones.len()
                    ))),
                    (_, Some(mode)) if !modes.contains(mode) => Err(out_of_range(format!(
                        "modo {mode:?} não está entre {modes:?}"
                    ))),
                    _ => Ok(()),
                }
            }
            (CapabilityValue::Mute(_), Capability::Mute { .. })
            | (CapabilityValue::Connection(_), Capability::Connection { .. }) => Ok(()),
            (
                CapabilityValue::Rumble {
                    strength,
                    duration_ms,
                },
                Capability::Rumble {
                    max_duration_ms, ..
                },
            ) => {
                if *strength > 100 {
                    Err(out_of_range(format!("intensidade {strength} acima de 100")))
                } else if *duration_ms > *max_duration_ms {
                    Err(out_of_range(format!(
                        "duração {duration_ms} ms acima de {max_duration_ms} ms"
                    )))
                } else {
                    Ok(())
                }
            }
            // Inalcançável: o tipo já foi conferido acima.
            _ => Err(RaizaError::Unsupported(kind)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::tests::exemplos;

    fn cap(kind: CapabilityKind) -> Capability {
        exemplos()
            .into_iter()
            .find(|c| c.kind() == kind)
            .expect("exemplo existe")
    }

    fn rgb(zone: Option<u32>, mode: Option<&str>, color: Option<Color>) -> CapabilityValue {
        CapabilityValue::Rgb(RgbSetting {
            zone,
            mode: mode.map(Into::into),
            color,
        })
    }

    const VERDE: Color = Color { r: 0, g: 255, b: 0 };

    fn validos() -> Vec<CapabilityValue> {
        vec![
            CapabilityValue::Dpi(3200),
            CapabilityValue::DpiStage {
                stage: 2,
                value: 950,
            },
            CapabilityValue::PollingRate(500),
            CapabilityValue::ScanRate(8000),
            rgb(Some(1), Some("breathing"), Some(VERDE)),
            rgb(None, None, Some(VERDE)),
            CapabilityValue::Volume(150),
            CapabilityValue::Mute(true),
            CapabilityValue::Rumble {
                strength: 100,
                duration_ms: 2000,
            },
            CapabilityValue::Connection(false),
        ]
    }

    #[test]
    fn estagio_sem_tabela_e_aplicar() {
        let mut c = cap(CapabilityKind::Dpi);
        if let Capability::Dpi {
            stages,
            active_stage,
            ..
        } = &mut c
        {
            *active_stage = Some(1);
            stages.clear();
        }
        let v = CapabilityValue::DpiStage {
            stage: 0,
            value: 800,
        };
        assert!(matches!(v.validate(&c), Err(RaizaError::OutOfRange { .. })));

        let mut c = cap(CapabilityKind::Dpi);
        if let Capability::Dpi { active_stage, .. } = &mut c {
            *active_stage = Some(1);
        }
        c.apply(&CapabilityValue::DpiStage {
            stage: 0,
            value: 400,
        })
        .unwrap();
        c.apply(&CapabilityValue::Dpi(2000)).unwrap();
        let Capability::Dpi { level, stages, .. } = &c else {
            unreachable!()
        };
        assert_eq!(stages, &[400, 2000, 3200]);
        assert_eq!(level.current, 2000);
        c.apply(&CapabilityValue::DpiStage {
            stage: 1,
            value: 1000,
        })
        .unwrap();
        let Capability::Dpi { level, .. } = &c else {
            unreachable!()
        };
        assert_eq!(level.current, 1000, "estágio em uso muda o DPI atual");
    }

    #[test]
    fn serde_ida_e_volta() {
        for v in validos() {
            let json = serde_json::to_value(&v).unwrap();
            if !matches!(v, CapabilityValue::DpiStage { .. }) {
                assert_eq!(json["kind"], v.kind().as_str());
            }
            assert_eq!(serde_json::from_value::<CapabilityValue>(json).unwrap(), v);
        }
        assert_eq!(
            serde_json::to_string(&CapabilityValue::Dpi(1600)).unwrap(),
            r#"{"kind":"dpi","value":1600}"#
        );
        assert_eq!(
            serde_json::to_string(&CapabilityValue::DpiStage {
                stage: 2,
                value: 950
            })
            .unwrap(),
            r#"{"kind":"dpi_stage","value":{"stage":2,"value":950}}"#
        );
    }

    #[test]
    fn dentro_dos_limites() {
        for v in validos() {
            let c = cap(v.kind());
            assert_eq!(v.validate(&c), Ok(()), "{v:?}");
        }
    }

    #[test]
    fn fora_dos_limites() {
        let invalidos = [
            CapabilityValue::Dpi(100),   // abaixo do mínimo
            CapabilityValue::Dpi(20000), // acima do máximo
            CapabilityValue::Dpi(1625),  // fora do passo
            CapabilityValue::DpiStage {
                stage: 3, // só há 3 estágios
                value: 800,
            },
            CapabilityValue::DpiStage {
                stage: 0,
                value: 1625, // fora do passo
            },
            CapabilityValue::PollingRate(250),
            CapabilityValue::ScanRate(2000),
            rgb(Some(2), None, Some(VERDE)),
            rgb(None, Some("arco-íris"), None),
            rgb(None, None, None),
            CapabilityValue::Volume(151),
            CapabilityValue::Rumble {
                strength: 101,
                duration_ms: 10,
            },
            CapabilityValue::Rumble {
                strength: 50,
                duration_ms: 2001,
            },
        ];
        for v in invalidos {
            let c = cap(v.kind());
            assert!(
                matches!(v.validate(&c), Err(RaizaError::OutOfRange { kind, .. }) if kind == v.kind()),
                "{v:?}"
            );
        }
    }

    #[test]
    fn somente_leitura_recusa() {
        for v in validos() {
            let mut c = cap(v.kind());
            c.set_read_only();
            assert_eq!(v.validate(&c), Err(RaizaError::ReadOnly(v.kind())));
        }
    }

    #[test]
    fn tipo_errado_recusa() {
        let bateria = cap(CapabilityKind::Battery);
        assert_eq!(
            CapabilityValue::Dpi(1600).validate(&bateria),
            Err(RaizaError::Unsupported(CapabilityKind::Dpi))
        );
    }

    #[test]
    fn apply_atualiza_estado() {
        let mut dpi = cap(CapabilityKind::Dpi);
        dpi.apply(&CapabilityValue::Dpi(800)).unwrap();
        assert!(matches!(dpi, Capability::Dpi { level, .. } if level.current == 800));

        // Valor inválido não altera nada.
        let antes = dpi.clone();
        assert!(dpi.apply(&CapabilityValue::Dpi(1)).is_err());
        assert_eq!(dpi, antes);

        let mut luz = cap(CapabilityKind::Rgb);
        luz.apply(&rgb(Some(0), Some("breathing"), Some(VERDE)))
            .unwrap();
        let Capability::Rgb {
            zones,
            current_mode,
            ..
        } = &luz
        else {
            unreachable!()
        };
        assert_eq!(current_mode, "breathing");
        assert_eq!(zones[0].color, VERDE);
        assert_ne!(zones[1].color, VERDE);

        luz.apply(&rgb(None, None, Some(VERDE))).unwrap();
        let Capability::Rgb { zones, .. } = &luz else {
            unreachable!()
        };
        assert!(zones.iter().all(|z| z.color == VERDE));
    }
}
