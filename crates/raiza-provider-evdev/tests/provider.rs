//! Provedor completo com fontes falsas: descoberta, leitura, vibração e eventos.

mod common;

use std::time::Duration;

use common::*;
use futures_util::StreamExt;
use raiza_core::{
    Capability, CapabilityKind, CapabilityValue, DeviceId, Provider, ProviderEvent, RaizaError,
};
use raiza_provider_evdev::{EvdevProvider, MAX_RUMBLE_MS};

fn rumble(strength: u8, duration_ms: u32) -> CapabilityValue {
    CapabilityValue::Rumble {
        strength,
        duration_ms,
    }
}

/// Um DualSense USB com bateria. Devolve o fixture, o nó e o dono da bateria.
fn dualsense() -> (
    Fake,
    raiza_provider_evdev::InputNode,
    std::path::PathBuf,
    DeviceId,
) {
    let f = Fake::new();
    let usb = f.usb("1-2", "054c", "0ce6", Some("S1"));
    let (node, hid) = f.pad(
        &usb,
        "0003:054C:0CE6.0005",
        23,
        "0003",
        "",
        Some(FF_RUMBLE_CAPS),
    );
    f.battery("ps", &hid, &[("capacity", "80"), ("status", "Discharging")]);
    (f, node, hid, DeviceId::usb(0x054c, 0x0ce6, "S1"))
}

#[tokio::test]
async fn descobre_le_e_toca() {
    let (f, node, _, id) = dualsense();
    let (fonte, _tx) = Falsa::nova(vec![node]);
    let gravador = Gravador::default();
    let p = EvdevProvider::with_parts(fonte, f.sysfs(), gravador.clone());

    assert_eq!(p.id(), "evdev");
    let achados = p.discover().await.unwrap();
    assert_eq!(achados.len(), 1);
    assert_eq!(achados[0].id, id);

    assert_eq!(
        p.read(&id, CapabilityKind::Battery).await.unwrap(),
        Capability::Battery {
            percent: 80,
            charging: false
        }
    );
    assert_eq!(
        p.read(&id, CapabilityKind::Dpi).await,
        Err(RaizaError::Unsupported(CapabilityKind::Dpi))
    );

    p.write(&id, rumble(50, 500)).await.unwrap();
    let pulsos = gravador.pulsos.lock().unwrap();
    assert_eq!(pulsos.len(), 1);
    assert_eq!(pulsos[0].0.to_str(), Some("/dev/input/event23"));
    assert_eq!(pulsos[0].1.magnitude(), 0x7FFF);
    assert_eq!(pulsos[0].1.duration_ms(), 500);
}

#[tokio::test]
async fn backend_nunca_recebe_valor_fora_do_limite() {
    let (f, node, _, id) = dualsense();
    let (fonte, _tx) = Falsa::nova(vec![node]);
    let gravador = Gravador::default();
    let p = EvdevProvider::with_parts(fonte, f.sysfs(), gravador.clone());
    p.discover().await.unwrap();

    for invalido in [
        rumble(101, 100),
        rumble(255, 100),
        rumble(50, MAX_RUMBLE_MS + 1),
        rumble(50, u32::MAX),
    ] {
        assert!(
            matches!(
                p.write(&id, invalido.clone()).await,
                Err(RaizaError::OutOfRange { .. })
            ),
            "{invalido:?}"
        );
    }
    // Valor de outra capacidade também é recusado.
    assert_eq!(
        p.write(&id, CapabilityValue::Dpi(800)).await,
        Err(RaizaError::Unsupported(CapabilityKind::Dpi))
    );
    assert!(gravador.pulsos.lock().unwrap().is_empty());

    // Nos limites exatos passa, e zero não toca no hardware.
    p.write(&id, rumble(100, MAX_RUMBLE_MS)).await.unwrap();
    p.write(&id, rumble(0, 500)).await.unwrap();
    p.write(&id, rumble(50, 0)).await.unwrap();
    let pulsos = gravador.pulsos.lock().unwrap();
    assert_eq!(pulsos.len(), 1);
    assert_eq!(pulsos[0].1.magnitude(), 0xFFFF);
    assert_eq!(pulsos[0].1.duration_ms(), MAX_RUMBLE_MS);
}

#[tokio::test]
async fn escrita_sem_rumble_ou_sem_controle() {
    let f = Fake::new();
    let usb = f.usb("1-2", "054c", "0ce6", Some("S1"));
    let (node, _) = f.pad(&usb, "0003:054C:0CE6.0005", 23, "0003", "", None);
    let (fonte, _tx) = Falsa::nova(vec![node]);
    let gravador = Gravador::default();
    let p = EvdevProvider::with_parts(fonte, f.sysfs(), gravador.clone());
    let id = DeviceId::usb(0x054c, 0x0ce6, "S1");

    assert_eq!(
        p.write(&id, rumble(50, 100)).await,
        Err(RaizaError::Unsupported(CapabilityKind::Rumble))
    );
    let outro = DeviceId::usb(1, 2, "0");
    assert_eq!(
        p.write(&outro, rumble(50, 100)).await,
        Err(RaizaError::NotFound(outro.clone()))
    );
    assert!(gravador.pulsos.lock().unwrap().is_empty());
}

#[tokio::test]
async fn udev_ausente_devolve_lista_vazia() {
    let f = Fake::new();
    let p = EvdevProvider::with_parts(Quebrada, f.sysfs(), Gravador::default());
    assert_eq!(p.discover().await.unwrap(), vec![]);
    assert!(p.events().next().await.is_none());
}

#[tokio::test]
async fn controle_ruim_nao_atrapalha_os_outros() {
    let (f, node, _, id) = dualsense();
    // Segundo nó cujo sysfs sumiu: ignorado, sem derrubar o primeiro.
    let fantasma = raiza_provider_evdev::InputNode {
        devnode: "/dev/input/event99".into(),
        input_syspath: f.root().join("nao/existe"),
    };
    let (fonte, _tx) = Falsa::nova(vec![fantasma, node]);
    let p = EvdevProvider::with_parts(fonte, f.sysfs(), Gravador::default());
    let achados = p.discover().await.unwrap();
    assert_eq!(achados.len(), 1);
    assert_eq!(achados[0].id, id);
}

#[tokio::test(start_paused = true)]
async fn eventos_de_hotplug_e_bateria() {
    let (f, node, _, id) = dualsense();
    let (fonte, tx) = Falsa::nova(vec![]);
    let nos = fonte.nos.clone();
    let p = EvdevProvider::with_parts(fonte, f.sysfs(), Gravador::default());
    assert!(p.discover().await.unwrap().is_empty());
    let mut eventos = p.events();

    // Plugar o controle.
    nos.lock().unwrap().push(node);
    tx.send(()).unwrap();
    let ProviderEvent::DeviceAdded { device } = eventos.next().await.unwrap() else {
        panic!("esperava DeviceAdded");
    };
    assert_eq!(device.id, id);

    // A bateria mudou: só `CapabilityChanged`.
    std::fs::write(f.root().join("class/power_supply/ps/capacity"), "79\n").unwrap();
    std::fs::write(f.root().join("class/power_supply/ps/status"), "Charging\n").unwrap();
    tx.send(()).unwrap();
    assert_eq!(
        eventos.next().await.unwrap(),
        ProviderEvent::CapabilityChanged {
            id: id.clone(),
            capability: Capability::Battery {
                percent: 79,
                charging: true
            }
        }
    );

    // Desplugar.
    nos.lock().unwrap().clear();
    tx.send(()).unwrap();
    assert_eq!(
        eventos.next().await.unwrap(),
        ProviderEvent::DeviceRemoved { id }
    );

    // O monitor fecha: o fluxo termina.
    drop(tx);
    assert!(
        tokio::time::timeout(Duration::from_secs(5), eventos.next())
            .await
            .unwrap()
            .is_none()
    );
}
