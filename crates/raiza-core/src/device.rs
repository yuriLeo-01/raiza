//! Dispositivo: identidade, tipo e capacidades.

use serde::{Deserialize, Serialize};

use crate::{Capability, CapabilityKind, CapabilityValue, DeviceId, RaizaError};

/// Id de um provedor (ex.: `udev`, `bluez`, `pipewire`). Veja [`crate::Provider::id`].
pub type ProviderId = String;

/// Tipo de dispositivo, usado pela interface para ícones e agrupamento.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    /// Mouse.
    Mouse,
    /// Teclado.
    Keyboard,
    /// Headset ou fone.
    Headset,
    /// Controle de jogo.
    Gamepad,
    /// Microfone.
    Microphone,
    /// Monitor.
    Monitor,
    /// Controlador de iluminação RGB.
    RgbController,
    /// Qualquer outro.
    Other,
}

/// Um dispositivo como um provedor (ou a fusão de vários) o enxerga.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Id estável do dispositivo físico.
    pub id: DeviceId,
    /// Nome legível (ex.: `Rapoo VT9`).
    pub name: String,
    /// Fabricante, se conhecido.
    pub vendor: Option<String>,
    /// Tipo do dispositivo.
    pub kind: DeviceKind,
    /// Provedores que atendem o dispositivo, em ordem de prioridade.
    /// Um provedor sempre preenche só com o próprio id; a fusão junta os demais.
    pub providers: Vec<ProviderId>,
    /// Capacidades, no máximo uma de cada [`CapabilityKind`].
    pub capabilities: Vec<Capability>,
}

impl DeviceInfo {
    /// Capacidade do tipo `kind`, se o dispositivo a tiver.
    pub fn capability(&self, kind: CapabilityKind) -> Option<&Capability> {
        self.capabilities.iter().find(|c| c.kind() == kind)
    }

    /// Versão mutável de [`DeviceInfo::capability`].
    pub fn capability_mut(&mut self, kind: CapabilityKind) -> Option<&mut Capability> {
        self.capabilities.iter_mut().find(|c| c.kind() == kind)
    }

    /// Valida `value` contra a capacidade correspondente deste dispositivo.
    /// Sem a capacidade: [`RaizaError::Unsupported`]. Veja [`CapabilityValue::validate`].
    pub fn validate(&self, value: &CapabilityValue) -> Result<(), RaizaError> {
        let capability = self
            .capability(value.kind())
            .ok_or(RaizaError::Unsupported(value.kind()))?;
        value.validate(capability)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::tests::exemplos;

    fn mouse() -> DeviceInfo {
        DeviceInfo {
            id: DeviceId::usb(0x24ae, 0x1423, "0"),
            name: "Rapoo".into(),
            vendor: Some("Rapoo".into()),
            kind: DeviceKind::Mouse,
            providers: vec!["hid-perfis".into()],
            capabilities: exemplos(),
        }
    }

    #[test]
    fn serde_ida_e_volta() {
        let d = mouse();
        let json = serde_json::to_string(&d).unwrap();
        assert_eq!(serde_json::from_str::<DeviceInfo>(&json).unwrap(), d);

        for k in [
            DeviceKind::Mouse,
            DeviceKind::Keyboard,
            DeviceKind::Headset,
            DeviceKind::Gamepad,
            DeviceKind::Microphone,
            DeviceKind::Monitor,
            DeviceKind::RgbController,
            DeviceKind::Other,
        ] {
            let json = serde_json::to_string(&k).unwrap();
            assert_eq!(serde_json::from_str::<DeviceKind>(&json).unwrap(), k);
        }
        assert_eq!(
            serde_json::to_string(&DeviceKind::RgbController).unwrap(),
            "\"rgb_controller\""
        );
    }

    #[test]
    fn validate_pelo_dispositivo() {
        let mut d = mouse();
        assert_eq!(d.validate(&CapabilityValue::Dpi(800)), Ok(()));
        d.capabilities.retain(|c| c.kind() != CapabilityKind::Dpi);
        assert_eq!(
            d.validate(&CapabilityValue::Dpi(800)),
            Err(RaizaError::Unsupported(CapabilityKind::Dpi))
        );
    }
}
