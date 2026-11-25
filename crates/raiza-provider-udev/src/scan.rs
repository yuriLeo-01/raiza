//! Lógica pura de descoberta: de um [`Tree`] para a lista de [`DeviceInfo`].
//!
//! Regras:
//! - um aparelho USB é o dispositivo USB pai (`usb_device`); as várias interfaces,
//!   `hidraw` e `input` dele viram um dispositivo só;
//! - ficam de fora hubs e root hubs (classe 09), armazenamento em massa (08), adaptadores
//!   Bluetooth (e0) e aparelhos internos sem HID (porta `fixed` até o controlador);
//! - aparelhos Bluetooth são os nós `input` com `ID_BUS=bluetooth`, agrupados pelo MAC (`uniq`).

use std::collections::{BTreeMap, BTreeSet};

use raiza_core::{Capability, ConnectionKind, DeviceId, DeviceInfo, DeviceKind};
use tracing::debug;

use crate::PROVIDER_ID;
use crate::tree::{Node, Tree};

/// Classes USB (de dispositivo ou de interface) que não são periféricos.
const CLASS_AUDIO: &str = "01";
const CLASS_HID: &str = "03";
const CLASS_MASS_STORAGE: &str = "08";
const CLASS_HUB: &str = "09";
const CLASS_WIRELESS: &str = "e0";

/// Lista os periféricos da árvore, em ordem de id.
pub fn scan(tree: &Tree) -> Vec<DeviceInfo> {
    let usb_devices: Vec<&Node> = tree.nodes().filter(|n| n.is_usb_device()).collect();

    // Portas dos aparelhos sem serial, por vendor:product, para `DeviceId::usb_instance`.
    // Conta todos os dispositivos USB (mesmo os ignorados), como manda o contrato.
    let mut no_serial_ports: BTreeMap<(u16, u16), Vec<&str>> = BTreeMap::new();
    for dev in &usb_devices {
        if let Some(ids) = usb_ids(dev)
            && serial(dev).is_none()
        {
            no_serial_ports.entry(ids).or_default().push(dev.sysname());
        }
    }

    let mut devices: Vec<DeviceInfo> = usb_devices
        .iter()
        .filter(|dev| is_peripheral(tree, dev))
        .filter_map(|dev| {
            let (vid, pid) = usb_ids(dev)?;
            let ports = no_serial_ports
                .get(&(vid, pid))
                .map_or(&[][..], Vec::as_slice);
            let instance = DeviceId::usb_instance(serial(dev), dev.sysname(), ports);
            Some(usb_device_info(
                tree,
                dev,
                DeviceId::usb(vid, pid, instance),
            ))
        })
        .collect();

    devices.extend(bluetooth_devices(tree));
    devices.sort_by(|a, b| a.id.cmp(&b.id));
    devices
}

/// `idVendor` e `idProduct` em hexadecimal.
fn usb_ids(dev: &Node) -> Option<(u16, u16)> {
    let hex = |name| u16::from_str_radix(dev.attribute(name)?, 16).ok();
    match (hex("idVendor"), hex("idProduct")) {
        (Some(vid), Some(pid)) => Some((vid, pid)),
        _ => {
            debug!(syspath = %dev.syspath, "dispositivo USB sem idVendor/idProduct válidos");
            None
        }
    }
}

fn serial(dev: &Node) -> Option<&str> {
    dev.attribute("serial").filter(|s| !s.is_empty())
}

/// Interfaces do próprio dispositivo USB (não as de aparelhos atrás dele, se for hub).
fn interfaces<'a>(tree: &'a Tree, dev: &'a Node) -> Vec<&'a Node> {
    let mut list: Vec<&Node> = tree
        .descendants(dev)
        .filter(|n| n.is_usb_interface())
        .filter(|n| tree.owner_usb_device(n).map(|o| o.syspath.as_str()) == Some(&dev.syspath))
        .collect();
    list.sort_by_key(|i| interface_number(i));
    list
}

fn interface_number(interface: &Node) -> u8 {
    interface
        .attribute("bInterfaceNumber")
        .and_then(|n| u8::from_str_radix(n, 16).ok())
        .unwrap_or(u8::MAX)
}

fn class_of(interface: &Node) -> (&str, &str, &str) {
    (
        interface.attribute("bInterfaceClass").unwrap_or_default(),
        interface
            .attribute("bInterfaceSubClass")
            .unwrap_or_default(),
        interface
            .attribute("bInterfaceProtocol")
            .unwrap_or_default(),
    )
}

/// Decide se o dispositivo USB é um periférico que deve aparecer na lista.
fn is_peripheral(tree: &Tree, dev: &Node) -> bool {
    let skip = |reason: &str| {
        debug!(port = dev.sysname(), reason, "dispositivo USB ignorado");
        false
    };
    // Root hub: `usbN`, sem hífen no nome.
    if !dev.sysname().contains('-') {
        return skip("root hub");
    }
    let class = dev.attribute("bDeviceClass").unwrap_or_default();
    if [CLASS_HUB, CLASS_MASS_STORAGE, CLASS_WIRELESS].contains(&class) {
        return skip("classe de dispositivo não periférica");
    }
    let ifaces = interfaces(tree, dev);
    if !ifaces.is_empty()
        && ifaces
            .iter()
            .all(|i| [CLASS_HUB, CLASS_MASS_STORAGE, CLASS_WIRELESS].contains(&class_of(i).0))
    {
        return skip("só interfaces de hub, armazenamento ou Bluetooth");
    }
    let has_hid = ifaces.iter().any(|i| class_of(i).0 == CLASS_HID);
    if is_internal(tree, dev) && !has_hid {
        return skip("interno (porta fixa) e sem HID");
    }
    true
}

/// Interno = porta `fixed` em todo o caminho até o controlador. Um aparelho preso por
/// dentro de um hub externo (ex.: o controle USB de um monitor) não é interno.
fn is_internal(tree: &Tree, dev: &Node) -> bool {
    let mut current = dev;
    loop {
        if current.attribute("removable") != Some("fixed") {
            return false;
        }
        match tree.owner_usb_device(current) {
            // Chegou ao root hub: tudo no caminho era fixo.
            Some(parent) if !parent.sysname().contains('-') => return true,
            Some(parent) => current = parent,
            None => return true,
        }
    }
}

fn usb_device_info(tree: &Tree, dev: &Node, id: DeviceId) -> DeviceInfo {
    let name = dev
        .attribute("product")
        .or_else(|| dev.property("ID_MODEL_FROM_DATABASE"))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| match &id {
            DeviceId::Usb {
                vendor_id,
                product_id,
                ..
            } => format!("Dispositivo USB {vendor_id:04x}:{product_id:04x}"),
            other => other.to_string(),
        });
    let vendor = dev
        .attribute("manufacturer")
        .or_else(|| dev.property("ID_VENDOR_FROM_DATABASE"))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let kind = usb_kind(tree, dev, &name);
    device_info(id, name, vendor, kind, ConnectionKind::Wired)
}

fn device_info(
    id: DeviceId,
    name: String,
    vendor: Option<String>,
    kind: DeviceKind,
    medium: ConnectionKind,
) -> DeviceInfo {
    DeviceInfo {
        id,
        name,
        vendor,
        kind,
        providers: vec![PROVIDER_ID.into()],
        capabilities: vec![Capability::Connection {
            medium,
            connected: true,
            writable: false,
        }],
    }
}

/// Infere o tipo de um aparelho USB, nesta ordem:
/// 1. protocolo de boot HID da interface de menor número (teclado 01, mouse 02): é o que
///    separa um mouse com teclas extras de um teclado com interface de mouse;
/// 2. classe de áudio (01): microfone ou headset;
/// 3. propriedades `ID_INPUT_*` dos nós `input`;
/// 4. `Other`.
fn usb_kind(tree: &Tree, dev: &Node, name: &str) -> DeviceKind {
    let ifaces = interfaces(tree, dev);

    for iface in &ifaces {
        match class_of(iface) {
            (CLASS_HID, "01", "01") => return DeviceKind::Keyboard,
            (CLASS_HID, "01", "02") => return DeviceKind::Mouse,
            _ => {}
        }
    }

    if ifaces.iter().any(|i| class_of(i).0 == CLASS_AUDIO) {
        return audio_kind(tree, dev, name);
    }

    let inputs: Vec<&Node> = tree
        .descendants(dev)
        .filter(|n| n.is_input() && n.property("ID_BUS") != Some("bluetooth"))
        .filter(|n| tree.owner_usb_device(n).map(|o| o.syspath.as_str()) == Some(&dev.syspath))
        .collect();
    let order = |n: &Node| {
        tree.ancestor(n, Node::is_usb_interface)
            .map_or(u8::MAX, interface_number)
    };
    input_kind(&inputs, order)
}

/// Microfone ou headset. O nome manda quando é explícito; senão, só captura é microfone
/// e qualquer saída de som é headset. Um microfone com saída para fone de retorno
/// (ex.: fifine) depende do nome.
fn audio_kind(tree: &Tree, dev: &Node, name: &str) -> DeviceKind {
    let lower = name.to_lowercase();
    if lower.contains("headset") || lower.contains("headphone") {
        return DeviceKind::Headset;
    }
    if lower.contains("mic") {
        return DeviceKind::Microphone;
    }
    let pcms: Vec<&str> = tree
        .descendants(dev)
        .filter(|n| n.subsystem == "sound" && n.sysname().starts_with("pcmC"))
        .map(Node::sysname)
        .collect();
    let capture = pcms.iter().any(|p| p.ends_with('c'));
    let playback = pcms.iter().any(|p| p.ends_with('p'));
    if capture && !playback {
        DeviceKind::Microphone
    } else {
        DeviceKind::Headset
    }
}

/// Tipo pelas propriedades `ID_INPUT_*`. `order` dá a posição do nó (número da interface
/// USB); vence o teclado ou mouse que aparecer primeiro.
fn input_kind(inputs: &[&Node], order: impl Fn(&Node) -> u8) -> DeviceKind {
    let first = |flag| {
        inputs
            .iter()
            .filter(|n| n.flag(flag))
            .map(|n| order(n))
            .min()
    };
    let keyboard = first("ID_INPUT_KEYBOARD");
    let mouse = first("ID_INPUT_MOUSE");
    match (keyboard, mouse) {
        (Some(k), Some(m)) if m < k => DeviceKind::Mouse,
        (Some(_), _) => DeviceKind::Keyboard,
        (None, Some(_)) => DeviceKind::Mouse,
        (None, None) => {
            if inputs.iter().any(|n| n.flag("ID_INPUT_JOYSTICK")) {
                DeviceKind::Gamepad
            } else if inputs.iter().any(|n| n.flag("ID_INPUT_TOUCHPAD")) {
                DeviceKind::Mouse
            } else {
                DeviceKind::Other
            }
        }
    }
}

/// Sufixo que o BlueZ põe no nó `input` dos controles de mídia de fones (AVRCP).
const AVRCP_SUFFIX: &str = " (AVRCP)";

/// Aparelhos de entrada Bluetooth, agrupados pelo MAC (`uniq`).
fn bluetooth_devices(tree: &Tree) -> Vec<DeviceInfo> {
    let mut groups: BTreeMap<DeviceId, Vec<&Node>> = BTreeMap::new();
    for node in tree
        .nodes()
        .filter(|n| n.is_input() && n.property("ID_BUS") == Some("bluetooth"))
    {
        let uniq = node.attribute("uniq").unwrap_or_default();
        match DeviceId::bluetooth(uniq) {
            Ok(id) => groups.entry(id).or_default().push(node),
            Err(_) => {
                debug!(syspath = %node.syspath, uniq, "input Bluetooth sem MAC válido em uniq")
            }
        }
    }

    groups
        .into_iter()
        .map(|(id, inputs)| {
            let names: BTreeSet<&str> = inputs
                .iter()
                .filter_map(|n| n.attribute("name"))
                .filter(|s| !s.is_empty())
                .collect();
            let avrcp = !names.is_empty() && names.iter().all(|n| n.ends_with(AVRCP_SUFFIX));
            // Nome do dispositivo HID pai (sem sufixos como "Mouse"/"Keyboard"); senão o
            // menor nome entre os nós input.
            let hid_name = inputs
                .iter()
                .filter_map(|n| tree.ancestor(n, |a| a.subsystem == "hid"))
                .filter_map(|h| h.property("HID_NAME"))
                .map(str::trim)
                .find(|s| !s.is_empty());
            let name = hid_name
                .or_else(|| names.iter().copied().min_by_key(|n| n.len()))
                .map(|n| n.trim_end_matches(AVRCP_SUFFIX).to_owned())
                .unwrap_or_else(|| id.to_string());
            // Sem interfaces USB para desempatar teclado e mouse, o nome ajuda.
            let lower = name.to_lowercase();
            let kind = if avrcp {
                DeviceKind::Headset
            } else if lower.contains("mouse") {
                DeviceKind::Mouse
            } else if lower.contains("keyboard") {
                DeviceKind::Keyboard
            } else {
                input_kind(&inputs, |_| 0)
            };
            device_info(id, name, None, kind, ConnectionKind::Bluetooth)
        })
        .collect()
}
