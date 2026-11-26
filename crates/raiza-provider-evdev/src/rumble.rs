//! Backend de vibração: um pulso de `FF_RUMBLE` num nó `/dev/input/eventN`.

use std::path::Path;
use std::time::Duration;

use evdev::{Device, FFEffectCode, FFEffectData, FFEffectKind, FFReplay, FFTrigger};
use raiza_core::{CapabilityKind, RaizaError};

/// Maior duração de um pulso, em milissegundos. É o `max_duration_ms` anunciado.
pub const MAX_RUMBLE_MS: u32 = 2000;

/// Um pulso de vibração já dentro dos limites. Só se constrói por [`Pulse::new`],
/// então todo backend recebe valores válidos (pilar 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pulse {
    magnitude: u16,
    duration_ms: u32,
}

impl Pulse {
    /// Cria um pulso. `strength` vai de 0 a 100 (vira 0–0xFFFF); `duration_ms`, até
    /// [`MAX_RUMBLE_MS`]. Fora disso, devolve [`RaizaError::OutOfRange`].
    pub fn new(strength: u8, duration_ms: u32) -> Result<Self, RaizaError> {
        let out_of_range = |detail: String| RaizaError::OutOfRange {
            kind: CapabilityKind::Rumble,
            detail,
        };
        if strength > 100 {
            return Err(out_of_range(format!("intensidade {strength} acima de 100")));
        }
        if duration_ms > MAX_RUMBLE_MS {
            return Err(out_of_range(format!(
                "duração {duration_ms} ms acima de {MAX_RUMBLE_MS} ms"
            )));
        }
        let magnitude = (u32::from(strength) * u32::from(u16::MAX) / 100) as u16;
        Ok(Pulse {
            magnitude,
            duration_ms,
        })
    }

    /// Magnitude de 0 a 0xFFFF.
    pub fn magnitude(&self) -> u16 {
        self.magnitude
    }

    /// Duração em milissegundos (no máximo [`MAX_RUMBLE_MS`]).
    pub fn duration_ms(&self) -> u32 {
        self.duration_ms
    }
}

/// Toca pulsos de vibração. Injetável para testar sem controle.
pub trait RumbleBackend: Send + Sync + 'static {
    /// Toca o pulso e só volta depois que ele acabou e o efeito foi removido do
    /// dispositivo. Bloqueante: o provedor chama de `spawn_blocking`.
    fn play(&self, devnode: &Path, pulse: Pulse) -> Result<(), RaizaError>;
}

/// O backend real: abre o nó evdev e usa force feedback.
#[derive(Debug, Default, Clone, Copy)]
pub struct EvdevRumble;

impl RumbleBackend for EvdevRumble {
    fn play(&self, devnode: &Path, pulse: Pulse) -> Result<(), RaizaError> {
        let mut device = Device::open(devnode)?;
        let supports = device
            .supported_ff()
            .is_some_and(|ff| ff.contains(FFEffectCode::FF_RUMBLE));
        if !supports {
            return Err(RaizaError::Unsupported(CapabilityKind::Rumble));
        }
        let mut effect = device.upload_ff_effect(FFEffectData {
            direction: 0,
            trigger: FFTrigger::default(),
            replay: FFReplay {
                // `Pulse` garante `duration_ms <= MAX_RUMBLE_MS`, que cabe em `u16`.
                length: u16::try_from(pulse.duration_ms).unwrap_or(u16::MAX),
                delay: 0,
            },
            kind: FFEffectKind::Rumble {
                strong_magnitude: pulse.magnitude,
                weak_magnitude: pulse.magnitude,
            },
        })?;
        effect.play(1)?;
        std::thread::sleep(Duration::from_millis(u64::from(pulse.duration_ms)));
        let stopped = effect.stop();
        // Ao sair de escopo, `effect` é removido do dispositivo (EVIOCRMFF).
        drop(effect);
        Ok(stopped?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intensidade_vira_magnitude() {
        assert_eq!(Pulse::new(0, 100).unwrap().magnitude(), 0);
        assert_eq!(Pulse::new(100, 100).unwrap().magnitude(), 0xFFFF);
        assert_eq!(Pulse::new(50, 100).unwrap().magnitude(), 0x7FFF);
    }

    #[test]
    fn limites_do_pulso() {
        assert!(Pulse::new(101, 100).is_err());
        assert!(Pulse::new(50, MAX_RUMBLE_MS + 1).is_err());
        assert!(Pulse::new(100, MAX_RUMBLE_MS).is_ok());
    }
}
