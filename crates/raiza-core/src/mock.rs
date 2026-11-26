//! Provedor falso para testes (feature `mock`).
//!
//! Guarda os dispositivos em memória, aplica escritas validadas, registra o que foi
//! escrito e emite eventos como um provedor real.
//!
//! ```
//! # futures::executor::block_on(async {
//! use raiza_core::mock::MockProvider;
//! use raiza_core::{Capability, CapabilityValue, DeviceId, DeviceInfo, DeviceKind, Provider};
//!
//! let id = DeviceId::usb(0x046d, 0xc077, "0");
//! let mock = MockProvider::new("mock").with_device(DeviceInfo {
//!     id: id.clone(),
//!     name: "Mouse".into(),
//!     vendor: None,
//!     kind: DeviceKind::Mouse,
//!     providers: vec!["mock".into()],
//!     capabilities: vec![Capability::Mute { muted: false, writable: true }],
//! });
//! mock.write(&id, CapabilityValue::Mute(true)).await.unwrap();
//! assert_eq!(mock.writes(), [(id, CapabilityValue::Mute(true))]);
//! # });
//! ```

use std::sync::{Mutex, MutexGuard, PoisonError};

use async_trait::async_trait;
use futures_channel::mpsc;

use crate::{
    Capability, CapabilityKind, CapabilityValue, DeviceId, DeviceInfo, EventStream, Provider,
    ProviderEvent, RaizaError,
};

/// Provedor em memória para testes.
#[derive(Debug)]
pub struct MockProvider {
    id: String,
    state: Mutex<State>,
}

#[derive(Debug, Default)]
struct State {
    devices: Vec<DeviceInfo>,
    writes: Vec<(DeviceId, CapabilityValue)>,
    subscribers: Vec<mpsc::UnboundedSender<ProviderEvent>>,
    next_error: Option<RaizaError>,
}

impl State {
    fn emit(&mut self, event: ProviderEvent) {
        self.subscribers
            .retain(|tx| tx.unbounded_send(event.clone()).is_ok());
    }

    fn device_mut(&mut self, id: &DeviceId) -> Result<&mut DeviceInfo, RaizaError> {
        self.devices
            .iter_mut()
            .find(|d| &d.id == id)
            .ok_or_else(|| RaizaError::NotFound(id.clone()))
    }
}

impl MockProvider {
    /// Cria um provedor vazio com o id dado.
    pub fn new(id: impl Into<String>) -> Self {
        MockProvider {
            id: id.into(),
            state: Mutex::default(),
        }
    }

    /// Adiciona um dispositivo sem emitir evento (estado inicial, visto por `discover`).
    pub fn with_device(self, device: DeviceInfo) -> Self {
        self.lock().devices.push(device);
        self
    }

    /// Pluga um dispositivo e emite [`ProviderEvent::DeviceAdded`].
    pub fn add_device(&self, device: DeviceInfo) {
        let mut state = self.lock();
        state.devices.retain(|d| d.id != device.id);
        state.devices.push(device.clone());
        state.emit(ProviderEvent::DeviceAdded { device });
    }

    /// Desconecta um dispositivo e emite [`ProviderEvent::DeviceRemoved`] se ele existia.
    pub fn remove_device(&self, id: &DeviceId) {
        let mut state = self.lock();
        let before = state.devices.len();
        state.devices.retain(|d| &d.id != id);
        if state.devices.len() != before {
            state.emit(ProviderEvent::DeviceRemoved { id: id.clone() });
        }
    }

    /// Muda uma capacidade "por fora" (ex.: a bateria caiu) e emite
    /// [`ProviderEvent::CapabilityChanged`].
    pub fn change_capability(&self, id: &DeviceId, capability: Capability) {
        let mut state = self.lock();
        let Ok(device) = state.device_mut(id) else {
            return;
        };
        device
            .capabilities
            .retain(|c| c.kind() != capability.kind());
        device.capabilities.push(capability.clone());
        state.emit(ProviderEvent::CapabilityChanged {
            id: id.clone(),
            capability,
        });
    }

    /// Faz a próxima chamada de `discover`, `read` ou `write` falhar com `error`.
    pub fn fail_next(&self, error: RaizaError) {
        self.lock().next_error = Some(error);
    }

    /// Escritas aceitas até agora, em ordem.
    pub fn writes(&self) -> Vec<(DeviceId, CapabilityValue)> {
        self.lock().writes.clone()
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        // Num mock, um pânico em outro teste não deve envenenar este.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn take_error(state: &mut State) -> Result<(), RaizaError> {
        state.next_error.take().map_or(Ok(()), Err)
    }
}

#[async_trait]
impl Provider for MockProvider {
    fn id(&self) -> &str {
        &self.id
    }

    async fn discover(&self) -> Result<Vec<DeviceInfo>, RaizaError> {
        let mut state = self.lock();
        Self::take_error(&mut state)?;
        Ok(state.devices.clone())
    }

    async fn read(&self, id: &DeviceId, kind: CapabilityKind) -> Result<Capability, RaizaError> {
        let mut state = self.lock();
        Self::take_error(&mut state)?;
        state
            .device_mut(id)?
            .capability(kind)
            .cloned()
            .ok_or(RaizaError::Unsupported(kind))
    }

    async fn write(&self, id: &DeviceId, value: CapabilityValue) -> Result<(), RaizaError> {
        let mut state = self.lock();
        Self::take_error(&mut state)?;
        let capability = state
            .device_mut(id)?
            .capability_mut(value.kind())
            .ok_or(RaizaError::Unsupported(value.kind()))?;
        capability.apply(&value)?;
        let capability = capability.clone();
        state.writes.push((id.clone(), value));
        state.emit(ProviderEvent::CapabilityChanged {
            id: id.clone(),
            capability,
        });
        Ok(())
    }

    fn events(&self) -> EventStream {
        let (tx, rx) = mpsc::unbounded();
        self.lock().subscribers.push(tx);
        Box::pin(rx)
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;
    use futures::{FutureExt, StreamExt};

    use super::*;
    use crate::{DeviceKind, Level};

    fn id() -> DeviceId {
        DeviceId::usb(0x24ae, 0x1423, "0")
    }

    fn dpi(current: u32, writable: bool) -> Capability {
        Capability::Dpi {
            level: Level {
                current,
                min: 400,
                max: 3200,
                step: 400,
            },
            stages: vec![],
            active_stage: None,
            writable,
        }
    }

    fn mouse(writable: bool) -> DeviceInfo {
        DeviceInfo {
            id: id(),
            name: "Rapoo".into(),
            vendor: None,
            kind: DeviceKind::Mouse,
            providers: vec!["mock".into()],
            capabilities: vec![dpi(800, writable)],
        }
    }

    #[test]
    fn descobre_le_e_escreve() {
        block_on(async {
            let p = MockProvider::new("mock").with_device(mouse(true));
            let mut eventos = p.events();
            assert_eq!(p.id(), "mock");
            assert_eq!(p.discover().await.unwrap(), [mouse(true)]);

            p.write(&id(), CapabilityValue::Dpi(1600)).await.unwrap();
            assert_eq!(
                p.read(&id(), CapabilityKind::Dpi).await.unwrap(),
                dpi(1600, true)
            );
            assert_eq!(p.writes(), [(id(), CapabilityValue::Dpi(1600))]);
            assert_eq!(
                eventos.next().await,
                Some(ProviderEvent::CapabilityChanged {
                    id: id(),
                    capability: dpi(1600, true)
                })
            );
        });
    }

    #[test]
    fn recusa_invalidos_sem_registrar() {
        block_on(async {
            let p = MockProvider::new("mock").with_device(mouse(false));
            assert_eq!(
                p.write(&id(), CapabilityValue::Dpi(1600)).await,
                Err(RaizaError::ReadOnly(CapabilityKind::Dpi))
            );
            assert_eq!(
                p.write(&id(), CapabilityValue::Mute(true)).await,
                Err(RaizaError::Unsupported(CapabilityKind::Mute))
            );
            let outro = DeviceId::usb(1, 2, "x");
            assert_eq!(
                p.read(&outro, CapabilityKind::Dpi).await,
                Err(RaizaError::NotFound(outro))
            );
            assert!(p.writes().is_empty());
        });
    }

    #[test]
    fn hotplug_e_falha_simulada() {
        block_on(async {
            let p = MockProvider::new("mock");
            let mut eventos = p.events();
            p.add_device(mouse(true));
            p.change_capability(&id(), dpi(400, true));
            p.remove_device(&id());
            p.remove_device(&id()); // já removido: sem evento

            let recebidos: Vec<_> = (0..3).map(|_| eventos.next().now_or_never()).collect();
            assert!(matches!(
                recebidos[0],
                Some(Some(ProviderEvent::DeviceAdded { .. }))
            ));
            assert!(matches!(
                recebidos[1],
                Some(Some(ProviderEvent::CapabilityChanged { .. }))
            ));
            assert_eq!(
                recebidos[2],
                Some(Some(ProviderEvent::DeviceRemoved { id: id() }))
            );
            assert_eq!(eventos.next().now_or_never(), None);

            p.fail_next(RaizaError::Io("cabo solto".into()));
            assert!(p.discover().await.is_err());
            assert!(p.discover().await.is_ok());
        });
    }
}
