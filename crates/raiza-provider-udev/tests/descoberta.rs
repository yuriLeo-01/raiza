//! Descoberta a partir de árvores udev falsas (fixtures JSON).

use raiza_core::{Capability, ConnectionKind, DeviceId, DeviceInfo, DeviceKind};
use raiza_provider_udev::{Node, Tree, scan};

fn fixture(nome: &str) -> String {
    let caminho = format!("{}/tests/fixtures/{nome}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&caminho).unwrap_or_else(|e| panic!("{caminho}: {e}"))
}

fn arvore(nome: &str) -> Tree {
    Tree::from_json(&fixture(nome)).unwrap()
}

fn nos(nome: &str) -> Vec<Node> {
    serde_json::from_str(&fixture(nome)).unwrap()
}

fn ids(devices: &[DeviceInfo]) -> Vec<String> {
    devices.iter().map(|d| d.id.to_string()).collect()
}

#[test]
fn tres_hidraw_do_mesmo_teclado_viram_um_keyboard() {
    let devices = scan(&arvore("teclado_tres_hidraw.json"));
    assert_eq!(devices.len(), 1, "{devices:#?}");
    let teclado = &devices[0];
    assert_eq!(teclado.id, DeviceId::usb(0x3151, 0x502f, "0"));
    assert_eq!(teclado.kind, DeviceKind::Keyboard);
    assert_eq!(teclado.name, "Akko Keyboard");
    assert_eq!(teclado.providers, ["udev"]);
    assert_eq!(
        teclado.capabilities,
        [Capability::Connection {
            medium: ConnectionKind::Wired,
            connected: true,
            writable: false,
        }]
    );
}

#[test]
fn hub_pendrive_bluetooth_e_webcam_interna_sao_ignorados() {
    let devices = scan(&arvore("hub_e_pendrive.json"));
    // Fica só o mouse atrás do hub. O controle do monitor LG (043e:9a39) é ignorado:
    // monitores estão fora do escopo (lista IGNORED_USB).
    assert_eq!(ids(&devices), ["usb:24ae:1423:RAPOO-TESTE-1"]);
    // Mouse com interface de teclado: vale o protocolo de boot da primeira interface.
    assert_eq!(devices[0].kind, DeviceKind::Mouse);
}

#[test]
fn iguais_sem_serial_ganham_instancias_pela_porta() {
    let esperado = ["usb:24ae:1423:0", "usb:24ae:1423:1", "usb:24ae:1423:2"];
    let base = nos("dois_iguais_sem_serial.json");

    // Várias ordens de enumeração: sempre o mesmo resultado.
    let mut ordens = vec![base.clone()];
    let mut invertida = base.clone();
    invertida.reverse();
    ordens.push(invertida);
    for r in 1..base.len() {
        let mut girada = base.clone();
        girada.rotate_left(r);
        ordens.push(girada);
    }

    for nodes in ordens {
        let devices = scan(&Tree::new(nodes));
        assert_eq!(ids(&devices), esperado);
        // Só o da porta 6-0 não tem interfaces na fixture (vira `Other`): é a instância 1.
        let kinds: Vec<_> = devices.iter().map(|d| d.kind).collect();
        assert_eq!(
            kinds,
            [DeviceKind::Mouse, DeviceKind::Other, DeviceKind::Mouse]
        );
    }

    // Qual porta ficou com qual instância: ordem lexicográfica 3-4.2 < 6-0 < 6-1
    // (o serial só de espaços conta como ausente). É a regra do core, a mesma que os
    // outros provedores usam.
    let portas = ["6-1", "3-4.2", "6-0"];
    assert_eq!(DeviceId::usb_instance(None, "3-4.2", &portas), "0");
    assert_eq!(DeviceId::usb_instance(None, "6-0", &portas), "1");
    assert_eq!(DeviceId::usb_instance(None, "6-1", &portas), "2");
}

#[test]
fn bluetooth_agrupa_pelo_mac() {
    let devices = scan(&arvore("bluetooth.json"));
    assert_eq!(
        ids(&devices),
        ["bt:AA:BB:CC:00:11:22", "bt:D1:2A:33:44:55:66"]
    );
    let fone = &devices[0];
    assert_eq!(fone.kind, DeviceKind::Headset);
    assert_eq!(fone.name, "WH-1000XM4");
    let mouse = &devices[1];
    assert_eq!(mouse.name, "MX Master 3");
    assert_eq!(
        mouse.capabilities,
        [Capability::Connection {
            medium: ConnectionKind::Bluetooth,
            connected: true,
            writable: false,
        }]
    );
}

#[test]
fn audio_vira_microfone_ou_headset() {
    let json = r#"[
      {"syspath": "/sys/devices/usb1", "subsystem": "usb", "devtype": "usb_device",
       "attributes": {"bDeviceClass": "09", "idVendor": "1d6b", "idProduct": "0002"}},
      {"syspath": "/sys/devices/usb1/1-2", "subsystem": "usb", "devtype": "usb_device",
       "attributes": {"idVendor": "3142", "idProduct": "a010", "product": "fifine Microphone"}},
      {"syspath": "/sys/devices/usb1/1-2/1-2:1.0", "subsystem": "usb", "devtype": "usb_interface",
       "attributes": {"bInterfaceNumber": "00", "bInterfaceClass": "01", "bInterfaceSubClass": "01"}},
      {"syspath": "/sys/devices/usb1/1-2/1-2:1.0/sound/card4/pcmC4D0c", "subsystem": "sound"},
      {"syspath": "/sys/devices/usb1/1-2/1-2:1.0/sound/card4/pcmC4D0p", "subsystem": "sound"},
      {"syspath": "/sys/devices/usb1/1-3", "subsystem": "usb", "devtype": "usb_device",
       "attributes": {"idVendor": "36eb", "idProduct": "1087", "product": "S9 Ultra_Dongle"}},
      {"syspath": "/sys/devices/usb1/1-3/1-3:1.0", "subsystem": "usb", "devtype": "usb_interface",
       "attributes": {"bInterfaceNumber": "00", "bInterfaceClass": "01", "bInterfaceSubClass": "01"}},
      {"syspath": "/sys/devices/usb1/1-3/1-3:1.0/sound/card1/pcmC1D0c", "subsystem": "sound"},
      {"syspath": "/sys/devices/usb1/1-3/1-3:1.0/sound/card1/pcmC1D0p", "subsystem": "sound"},
      {"syspath": "/sys/devices/usb1/1-4", "subsystem": "usb", "devtype": "usb_device",
       "attributes": {"idVendor": "1234", "idProduct": "0001", "product": "USB Audio"}},
      {"syspath": "/sys/devices/usb1/1-4/1-4:1.0", "subsystem": "usb", "devtype": "usb_interface",
       "attributes": {"bInterfaceNumber": "00", "bInterfaceClass": "01", "bInterfaceSubClass": "01"}},
      {"syspath": "/sys/devices/usb1/1-4/1-4:1.0/sound/card5/pcmC5D0c", "subsystem": "sound"}
    ]"#;
    let devices = scan(&Tree::from_json(json).unwrap());
    let kinds: Vec<_> = devices.iter().map(|d| (d.name.as_str(), d.kind)).collect();
    assert_eq!(
        kinds,
        [
            ("USB Audio", DeviceKind::Microphone),
            ("fifine Microphone", DeviceKind::Microphone),
            ("S9 Ultra_Dongle", DeviceKind::Headset),
        ]
    );
}

#[test]
fn arvore_vazia_nao_lista_nada() {
    assert!(scan(&Tree::default()).is_empty());
}
