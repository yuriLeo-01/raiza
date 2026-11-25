//! Lógica pura: do [`Graph`] para os periféricos de áudio, com o [`DeviceId`] de cada um.
//!
//! Regras:
//! - cada `Audio/Device` do PipeWire é um aparelho; os nós `Audio/Sink` e `Audio/Source`
//!   dele apontam para ele por `device.id`;
//! - Bluetooth (`device.api = bluez5`): id por `api.bluez5.address`;
//! - ALSA: só se a placa (`api.alsa.card`) pertence a um dispositivo USB, pelo
//!   [`CardResolver`]. Onboard, PCI e HDMI ficam de fora;
//! - o contrato tem **um** `Volume` e **um** `Mute` por aparelho: se há saída (sink), eles
//!   representam a saída e o tipo é `Headset`; se só há entrada, a entrada e `Microphone`.
//!   Exceção: aparelhos com `device.form-factor = microphone` (ex.: o fifine, que também
//!   tem saída para fone de retorno) ficam com a entrada e o tipo `Microphone`.

use std::collections::BTreeMap;

use raiza_core::{Capability, DeviceId, DeviceInfo, DeviceKind, Level};
use tracing::debug;

use crate::PROVIDER_ID;
use crate::curve::linear_to_percent;
use crate::graph::{Graph, Object, ObjectKind};
use crate::sysfs::CardResolver;

/// Volume máximo anunciado (porcentagem na curva cúbica).
pub const MAX_VOLUME: u32 = 100;

/// Direção do nó escolhido.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Saída (`Audio/Sink`).
    Output,
    /// Entrada (`Audio/Source`).
    Input,
}

/// Um periférico de áudio e o nó do PipeWire que recebe volume e mudo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDevice {
    /// Como o provedor anuncia o aparelho.
    pub info: DeviceInfo,
    /// Id do nó (sink ou source) cujo volume e mudo as capacidades representam.
    pub node: u32,
    /// Se o nó é a saída ou a entrada.
    pub direction: Direction,
}

/// Um nó de áudio do grafo, com o id calculado (para diagnóstico, no exemplo `listar`).
#[derive(Debug, Clone, PartialEq)]
pub struct NodeReport {
    /// Id do nó.
    pub node: u32,
    /// `node.name`.
    pub name: String,
    /// `media.class`.
    pub media_class: String,
    /// Id do aparelho, ou `None` se ele não é um periférico USB/Bluetooth.
    pub device: Option<DeviceId>,
    /// Volume na curva cúbica (pode passar de 100) e mudo.
    pub volume: Option<(u32, bool)>,
}

/// Lista os periféricos de áudio do grafo, em ordem de id.
pub fn scan(graph: &Graph, resolver: &dyn CardResolver) -> Vec<AudioDevice> {
    let mut out: BTreeMap<DeviceId, AudioDevice> = BTreeMap::new();
    for device in audio_devices(graph) {
        let Some(id) = identity(device, resolver) else {
            continue;
        };
        let Some(audio) = audio_device(graph, device, id) else {
            continue;
        };
        if out.contains_key(&audio.info.id) {
            debug!(device = device.id, id = %audio.info.id, "id repetido, mantido o primeiro");
            continue;
        }
        out.insert(audio.info.id.clone(), audio);
    }
    out.into_values().collect()
}

/// Todos os nós `Audio/Sink` e `Audio/Source`, com o id calculado do aparelho.
pub fn node_reports(graph: &Graph, resolver: &dyn CardResolver) -> Vec<NodeReport> {
    audio_nodes(graph)
        .map(|node| {
            let device = node
                .prop_u32("device.id")
                .and_then(|d| graph.get(d))
                .and_then(|d| identity(d, resolver));
            NodeReport {
                node: node.id,
                name: node.prop_str("node.name").unwrap_or_default(),
                media_class: node.media_class().unwrap_or_default(),
                device,
                volume: node.volume().map(|(v, m)| (linear_to_percent(v), m)),
            }
        })
        .collect()
}

fn audio_devices(graph: &Graph) -> impl Iterator<Item = &Object> {
    graph.objects().filter(|o| {
        o.kind == ObjectKind::Device && o.media_class().as_deref() == Some("Audio/Device")
    })
}

fn audio_nodes(graph: &Graph) -> impl Iterator<Item = &Object> {
    graph.objects().filter(|o| {
        o.kind == ObjectKind::Node
            && matches!(
                o.media_class().as_deref(),
                Some("Audio/Sink" | "Audio/Source")
            )
    })
}

/// Id do aparelho físico, ou `None` se não for periférico USB/Bluetooth.
fn identity(device: &Object, resolver: &dyn CardResolver) -> Option<DeviceId> {
    match device.prop_str("device.api").as_deref() {
        Some("bluez5") => {
            let address = device.prop_str("api.bluez5.address").unwrap_or_default();
            match DeviceId::bluetooth(&address) {
                Ok(id) => Some(id),
                Err(_) => {
                    debug!(device = device.id, address, "Bluetooth sem endereço válido");
                    None
                }
            }
        }
        Some("alsa") => {
            let Some(card) = device.prop_u32("api.alsa.card") else {
                debug!(device = device.id, "placa ALSA sem api.alsa.card");
                return None;
            };
            let id = resolver.usb_id(card);
            if id.is_none() {
                debug!(
                    device = device.id,
                    card, "placa não USB (onboard/PCI/HDMI), ignorada"
                );
            }
            id
        }
        api => {
            debug!(device = device.id, ?api, "API de aparelho não suportada");
            None
        }
    }
}

fn audio_device(graph: &Graph, device: &Object, id: DeviceId) -> Option<AudioDevice> {
    // Nós deste aparelho que têm volume, do mais prioritário ao menos.
    let mut nodes: Vec<(&Object, Direction, f64, bool)> = audio_nodes(graph)
        .filter(|n| n.prop_u32("device.id") == Some(device.id))
        .filter_map(|n| {
            let direction = match n.media_class().as_deref() {
                Some("Audio/Sink") => Direction::Output,
                _ => Direction::Input,
            };
            let (linear, muted) = n.volume()?;
            Some((n, direction, linear, muted))
        })
        .collect();
    nodes.sort_by_key(|(n, ..)| {
        (
            std::cmp::Reverse(n.prop_u32("priority.session").unwrap_or(0)),
            n.id,
        )
    });
    let first = |dir| nodes.iter().find(|(_, d, ..)| *d == dir);

    let prefers_input = device.prop_str("device.form-factor").as_deref() == Some("microphone");
    let chosen = if prefers_input {
        first(Direction::Input).or_else(|| first(Direction::Output))
    } else {
        first(Direction::Output).or_else(|| first(Direction::Input))
    };
    let Some(&(node, direction, linear, muted)) = chosen else {
        debug!(device = device.id, %id, "aparelho sem sink/source com volume, ignorado");
        return None;
    };

    let mut current = linear_to_percent(linear);
    if current > MAX_VOLUME {
        debug!(%id, current, "volume acima de 100 %, anunciado como 100");
        current = MAX_VOLUME;
    }
    let kind = match direction {
        Direction::Output => DeviceKind::Headset,
        Direction::Input => DeviceKind::Microphone,
    };
    let name = ["device.description", "device.nick", "device.alias"]
        .iter()
        .find_map(|k| device.prop_str(k))
        .or_else(|| node.prop_str("node.description"))
        .unwrap_or_else(|| id.to_string());
    // Sem nome de fabricante, o PipeWire repete o VID aqui (ex.: `3142` no fifine).
    let vid = device
        .prop_str("device.vendor.id")
        .map(|v| v.trim_start_matches("0x").to_ascii_lowercase());
    let vendor = device
        .prop_str("device.vendor.name")
        .filter(|v| Some(v.to_ascii_lowercase()) != vid);

    Some(AudioDevice {
        info: DeviceInfo {
            id,
            name,
            vendor,
            kind,
            providers: vec![PROVIDER_ID.into()],
            capabilities: vec![
                Capability::Volume {
                    level: Level {
                        current,
                        min: 0,
                        max: MAX_VOLUME,
                        step: 1,
                    },
                    writable: true,
                },
                Capability::Mute {
                    muted,
                    writable: true,
                },
            ],
        },
        node: node.id,
        direction,
    })
}
