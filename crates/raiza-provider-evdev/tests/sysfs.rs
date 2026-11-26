//! Leitura do sysfs falso: identidade, bateria e suporte a vibração.

mod common;

use common::*;
use raiza_core::{Capability, CapabilityKind, DeviceId, DeviceKind};

#[test]
fn usb_com_serial_bateria_e_rumble() {
    let f = Fake::new();
    let usb = f.usb("1-2", "054c", "0ce6", Some("ABC123"));
    let (node, hid) = f.pad(
        &usb,
        "0003:054C:0CE6.0005",
        23,
        "0003",
        "",
        Some(FF_RUMBLE_CAPS),
    );
    f.battery(
        "ps-controller-battery-aa",
        &hid,
        &[
            ("capacity", "73"),
            ("status", "Charging"),
            ("type", "Battery"),
        ],
    );

    let pad = f.sysfs().read_pad(&node).unwrap();
    assert_eq!(pad.info.id, DeviceId::usb(0x054c, 0x0ce6, "ABC123"));
    assert_eq!(pad.info.kind, DeviceKind::Gamepad);
    assert_eq!(pad.info.providers, ["evdev"]);
    assert_eq!(pad.info.vendor.as_deref(), Some("Sony"));
    assert_eq!(
        pad.info.capability(CapabilityKind::Battery),
        Some(&Capability::Battery {
            percent: 73,
            charging: true
        })
    );
    assert_eq!(
        pad.info.capability(CapabilityKind::Rumble),
        Some(&Capability::Rumble {
            max_duration_ms: 2000,
            writable: true
        })
    );
    assert_eq!(pad.devnode.to_str(), Some("/dev/input/event23"));
}

#[test]
fn usb_sem_serial_usa_instancia_por_porta() {
    let f = Fake::new();
    let a = f.usb("1-2", "054c", "0ce6", None);
    let _b = f.usb("1-3", "054c", "0ce6", None);
    let (node, _) = f.pad(&a, "0003:054C:0CE6.0005", 23, "0003", "", None);
    let pad = f.sysfs().read_pad(&node).unwrap();
    // Duas portas sem serial do mesmo modelo: "1-2" é a primeira.
    assert_eq!(pad.info.id, DeviceId::usb(0x054c, 0x0ce6, "0"));

    let (node_b, _) = f.pad(&_b, "0003:054C:0CE6.0006", 24, "0003", "", None);
    let pad_b = f.sysfs().read_pad(&node_b).unwrap();
    assert_eq!(pad_b.info.id, DeviceId::usb(0x054c, 0x0ce6, "1"));
}

#[test]
fn bluetooth_usa_uniq() {
    let f = Fake::new();
    let parent = f.root().join("devices/virtual/misc/uhid");
    let (node, hid) = f.pad(
        &parent,
        "0005:054C:0CE6.0007",
        30,
        "0005",
        "aa:bb:cc:dd:ee:ff",
        Some(FF_RUMBLE_CAPS),
    );
    f.battery(
        "ps-controller-battery-aa:bb",
        &hid,
        &[("capacity", "40"), ("status", "Discharging")],
    );
    let pad = f.sysfs().read_pad(&node).unwrap();
    assert_eq!(
        pad.info.id,
        DeviceId::bluetooth("AA:BB:CC:DD:EE:FF").unwrap()
    );
    assert_eq!(
        pad.info.capability(CapabilityKind::Battery),
        Some(&Capability::Battery {
            percent: 40,
            charging: false
        })
    );
}

#[test]
fn so_capacity_level_vira_percentual() {
    let casos = [
        ("Critical", 5),
        ("Low", 20),
        ("Normal", 50),
        ("High", 80),
        ("Full", 100),
    ];
    for (nivel, esperado) in casos {
        let f = Fake::new();
        let usb = f.usb("1-2", "045e", "0b12", Some("X"));
        let (node, hid) = f.pad(&usb, "0003:045E:0B12.0001", 5, "0003", "", None);
        f.battery(
            "xpad",
            &hid,
            &[("capacity_level", nivel), ("status", "Discharging")],
        );
        let pad = f.sysfs().read_pad(&node).unwrap();
        assert_eq!(
            pad.info.capability(CapabilityKind::Battery),
            Some(&Capability::Battery {
                percent: esperado,
                charging: false
            }),
            "nível {nivel}"
        );
    }
}

#[test]
fn bateria_de_outro_dispositivo_nao_conta() {
    let f = Fake::new();
    let usb = f.usb("1-2", "054c", "0ce6", Some("A"));
    let (node, _) = f.pad(&usb, "0003:054C:0CE6.0005", 23, "0003", "", None);
    // Bateria de um notebook, sem relação com o controle.
    let acpi = f.root().join("devices/LNXSYSTM:00/PNP0C0A:00");
    std::fs::create_dir_all(&acpi).unwrap();
    f.battery("BAT0", &acpi, &[("capacity", "99"), ("status", "Full")]);
    // Bateria que não é do tipo Battery, ligada ao controle.
    let hid = usb.join("0003:054C:0CE6.0005");
    f.battery("ac", &hid, &[("capacity", "1"), ("type", "Mains")]);

    let pad = f.sysfs().read_pad(&node).unwrap();
    assert!(pad.info.capability(CapabilityKind::Battery).is_none());
}

#[test]
fn sem_ff_rumble_nao_anuncia_vibracao() {
    for ff in [None, Some(FF_SEM_RUMBLE)] {
        let f = Fake::new();
        let usb = f.usb("1-2", "054c", "0ce6", Some("A"));
        let (node, _) = f.pad(&usb, "0003:054C:0CE6.0005", 23, "0003", "", ff);
        let pad = f.sysfs().read_pad(&node).unwrap();
        assert!(
            pad.info.capability(CapabilityKind::Rumble).is_none(),
            "{ff:?}"
        );
    }
}

#[test]
fn barramento_desconhecido_vira_other() {
    let f = Fake::new();
    let parent = f.root().join("devices/platform/pad");
    let (node, _) = f.pad(&parent, "0019:0001:0001.0001", 9, "0019", "", None);
    let pad = f.sysfs().read_pad(&node).unwrap();
    assert!(matches!(pad.info.id, DeviceId::Other { ref provider, .. } if provider == "evdev"));
}
