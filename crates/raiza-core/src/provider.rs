//! Contrato dos provedores.

use std::pin::Pin;

use async_trait::async_trait;
use futures_core::Stream;
use serde::{Deserialize, Serialize};

use crate::{Capability, CapabilityKind, CapabilityValue, DeviceId, DeviceInfo, RaizaError};

/// Evento emitido por um provedor.
///
/// Um provedor pode anunciar que um dispositivo já conhecido mudou (outro nome, tipo ou
/// conjunto de capacidades) como `DeviceRemoved` seguido de `DeviceAdded` com o mesmo id.
/// O daemon deve tratar esse par como **atualização**: substituir o `DeviceInfo` daquele
/// provedor, sem perder as configurações salvas do dispositivo. Para só mudar o valor de
/// uma capacidade, use `CapabilityChanged`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum ProviderEvent {
    /// Um dispositivo apareceu (plugado, pareado, ligado).
    DeviceAdded {
        /// O dispositivo, como o provedor o enxerga.
        device: DeviceInfo,
    },
    /// Um dispositivo sumiu.
    DeviceRemoved {
        /// Id do dispositivo.
        id: DeviceId,
    },
    /// O valor de uma capacidade mudou (por escrita ou por fora, ex.: bateria).
    CapabilityChanged {
        /// Id do dispositivo.
        id: DeviceId,
        /// Novo estado da capacidade.
        capability: Capability,
    },
}

/// Fluxo de eventos de um provedor.
pub type EventStream = Pin<Box<dyn Stream<Item = ProviderEvent> + Send>>;

/// Contrato de todo provedor (udev, BlueZ, PipeWire, perfis HID...).
///
/// O daemon guarda os provedores como `Box<dyn Provider>`, em ordem de prioridade.
///
/// Regras para quem implementa:
/// - Erro em um dispositivo vira `Err` para aquele dispositivo. Nunca entre em pânico (pilar 1).
/// - `write` só recebe valores que o daemon já validou com [`DeviceInfo::validate`].
///   Mesmo assim, valide de novo contra o estado mais recente antes de enviar
///   qualquer coisa ao hardware.
/// - Capacidades de perfis não verificados devem ser anunciadas com `writable: false`.
/// - Nada de polling agressivo: prefira eventos do sistema (pilar 2).
#[async_trait]
pub trait Provider: Send + Sync {
    /// Id curto e estável do provedor, ex.: `udev`, `bluez`, `pipewire`.
    fn id(&self) -> &str;

    /// Lista os dispositivos que este provedor atende agora.
    async fn discover(&self) -> Result<Vec<DeviceInfo>, RaizaError>;

    /// Lê o estado atual de uma capacidade, direto da fonte.
    async fn read(&self, id: &DeviceId, kind: CapabilityKind) -> Result<Capability, RaizaError>;

    /// Aplica um novo valor a uma capacidade.
    async fn write(&self, id: &DeviceId, value: CapabilityValue) -> Result<(), RaizaError>;

    /// Fluxo de eventos de hotplug e de mudança de valores. O daemon chama uma vez,
    /// depois de `discover`. O fluxo termina quando o provedor é encerrado.
    fn events(&self) -> EventStream;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DeviceKind, capability::tests::exemplos};

    #[test]
    fn eventos_serde_ida_e_volta() {
        let id = DeviceId::usb(0x3151, 0x502f, "0");
        let eventos = [
            ProviderEvent::DeviceAdded {
                device: DeviceInfo {
                    id: id.clone(),
                    name: "Akko".into(),
                    vendor: None,
                    kind: DeviceKind::Keyboard,
                    providers: vec!["udev".into()],
                    capabilities: vec![],
                },
            },
            ProviderEvent::DeviceRemoved { id: id.clone() },
            ProviderEvent::CapabilityChanged {
                id,
                capability: exemplos().remove(0),
            },
        ];
        for e in eventos {
            let json = serde_json::to_string(&e).unwrap();
            assert_eq!(serde_json::from_str::<ProviderEvent>(&json).unwrap(), e);
        }
    }

    #[test]
    fn provider_e_dyn_compativel() {
        fn aceita(_: Option<Box<dyn Provider>>) {}
        aceita(None);
    }
}
