//! Utilitários compartilhados pelos testes de integração.
#![allow(dead_code)]

use std::time::Duration;

use raiza_core::{Capability, DeviceId, DeviceInfo, DeviceKind, Level};
use raizad::Signal;
use tokio::sync::mpsc::UnboundedReceiver;

pub fn mouse_id() -> DeviceId {
    DeviceId::usb(0x24ae, 0x1423, "0")
}

pub fn dpi(current: u32) -> Capability {
    Capability::Dpi {
        level: Level {
            current,
            min: 400,
            max: 3200,
            step: 400,
        },
        stages: vec![],
        active_stage: None,
        writable: true,
    }
}

pub fn mouse(provider: &str, dpi_atual: u32) -> DeviceInfo {
    DeviceInfo {
        id: mouse_id(),
        name: "Rapoo".into(),
        vendor: None,
        kind: DeviceKind::Mouse,
        providers: vec![provider.into()],
        capabilities: vec![dpi(dpi_atual)],
    }
}

/// Próximo sinal, com limite de tempo para o teste não travar.
pub async fn next(rx: &mut UnboundedReceiver<Signal>) -> Signal {
    tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("nenhum sinal em 5 s")
        .expect("canal de sinais fechado")
}

/// Garante que não há mais sinais depois de deixar o daemon trabalhar um pouco.
pub async fn quiet(rx: &mut UnboundedReceiver<Signal>) {
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(rx.try_recv().ok(), None, "sinal inesperado");
}
