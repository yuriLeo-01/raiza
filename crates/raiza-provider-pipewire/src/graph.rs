//! Retrato do grafo do PipeWire, montado a partir da saída JSON do `pw-dump`.
//!
//! O `pw-dump` imprime um vetor de objetos `{id, type, info: {props, params}}`. No modo
//! `--monitor`, cada mudança chega como outro vetor com os objetos alterados (inteiros) e
//! remoções como `{"id": N, "info": null}`. [`Graph::apply`] aplica essas mudanças.

use std::collections::BTreeMap;

use serde_json::{Map, Value};
use tracing::debug;

/// Tipo de objeto do PipeWire que interessa ao provedor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    /// `PipeWire:Interface:Device` (placa ALSA, aparelho Bluetooth).
    Device,
    /// `PipeWire:Interface:Node` (sink, source, stream).
    Node,
    /// Qualquer outro (cliente, porta, link...).
    Other,
}

/// Um objeto do grafo.
#[derive(Debug, Clone, PartialEq)]
pub struct Object {
    /// Id global do objeto no PipeWire (muda a cada reconexão do aparelho).
    pub id: u32,
    /// Tipo do objeto.
    pub kind: ObjectKind,
    /// `info.props`.
    pub props: Map<String, Value>,
    /// `info.params` (ex.: `Props`, `Route`).
    pub params: Map<String, Value>,
}

impl Object {
    /// Propriedade como texto. Números viram texto (o PipeWire mistura os dois).
    pub fn prop_str(&self, key: &str) -> Option<String> {
        match self.props.get(key)? {
            Value::String(s) => Some(s.trim().to_owned()).filter(|s| !s.is_empty()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        }
    }

    /// Propriedade como inteiro sem sinal, aceitando número ou texto.
    pub fn prop_u32(&self, key: &str) -> Option<u32> {
        match self.props.get(key)? {
            Value::Number(n) => n.as_u64().and_then(|n| u32::try_from(n).ok()),
            Value::String(s) => s.trim().parse().ok(),
            _ => None,
        }
    }

    /// `media.class`, ex.: `Audio/Sink`.
    pub fn media_class(&self) -> Option<String> {
        self.prop_str("media.class")
    }

    /// Volume e mudo do nó, do parâmetro `Props`: (volume linear médio dos canais, mudo).
    ///
    /// O volume é a média de `channelVolumes`, como faz o WirePlumber.
    pub fn volume(&self) -> Option<(f64, bool)> {
        let props = self.params.get("Props")?.as_array()?;
        props.iter().find_map(|p| {
            let channels: Vec<f64> = p
                .get("channelVolumes")?
                .as_array()?
                .iter()
                .filter_map(Value::as_f64)
                .collect();
            if channels.is_empty() {
                return None;
            }
            let linear = channels.iter().sum::<f64>() / channels.len() as f64;
            let muted = p.get("mute").and_then(Value::as_bool).unwrap_or(false);
            Some((linear, muted))
        })
    }

    fn from_json(item: &Value) -> Option<Self> {
        let id = item
            .get("id")?
            .as_u64()
            .and_then(|id| u32::try_from(id).ok())?;
        let kind = match item.get("type").and_then(Value::as_str) {
            Some("PipeWire:Interface:Device") => ObjectKind::Device,
            Some("PipeWire:Interface:Node") => ObjectKind::Node,
            _ => ObjectKind::Other,
        };
        let info = item.get("info");
        let object = |key| {
            info.and_then(|i| i.get(key))
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default()
        };
        Some(Object {
            id,
            kind,
            props: object("props"),
            params: object("params"),
        })
    }
}

/// Os objetos do PipeWire, por id.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Graph {
    objects: BTreeMap<u32, Object>,
}

impl Graph {
    /// Monta o grafo a partir de um `pw-dump` completo.
    pub fn from_dump(items: &[Value]) -> Self {
        let mut graph = Graph::default();
        graph.apply(items);
        graph
    }

    /// Aplica uma leva do `pw-dump --monitor`. Devolve `true` se algum aparelho ou nó
    /// mudou (o resto, como clientes e portas, não afeta o provedor).
    pub fn apply(&mut self, items: &[Value]) -> bool {
        let mut relevant = false;
        for item in items {
            let Some(id) = item
                .get("id")
                .and_then(Value::as_u64)
                .and_then(|id| u32::try_from(id).ok())
            else {
                debug!("objeto do pw-dump sem id, ignorado");
                continue;
            };
            if matches!(item.get("info"), Some(Value::Null)) {
                if let Some(old) = self.objects.remove(&id) {
                    relevant |= old.kind != ObjectKind::Other;
                }
                continue;
            }
            let Some(mut new) = Object::from_json(item) else {
                continue;
            };
            if let Some(old) = self.objects.get(&id) {
                // Mudanças parciais: o que não veio fica como estava.
                if new.props.is_empty() {
                    new.props = old.props.clone();
                }
                for (key, value) in &old.params {
                    new.params
                        .entry(key.clone())
                        .or_insert_with(|| value.clone());
                }
                if new.kind == ObjectKind::Other {
                    new.kind = old.kind;
                }
            }
            relevant |= new.kind != ObjectKind::Other;
            self.objects.insert(id, new);
        }
        relevant
    }

    /// Objeto pelo id.
    pub fn get(&self, id: u32) -> Option<&Object> {
        self.objects.get(&id)
    }

    /// Todos os objetos, em ordem de id.
    pub fn objects(&self) -> impl Iterator<Item = &Object> {
        self.objects.values()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn node(id: u32, volume: f64, mute: bool) -> Value {
        json!({
            "id": id,
            "type": "PipeWire:Interface:Node",
            "info": {
                "props": { "media.class": "Audio/Source", "device.id": 55 },
                "params": { "Props": [ { "mute": mute, "channelVolumes": [volume, volume] } ] }
            }
        })
    }

    #[test]
    fn le_volume_e_props() {
        let g = Graph::from_dump(&[node(45, 0.1905, false)]);
        let n = g.get(45).unwrap();
        assert_eq!(n.kind, ObjectKind::Node);
        assert_eq!(n.prop_u32("device.id"), Some(55));
        assert_eq!(n.prop_str("device.id").as_deref(), Some("55"));
        assert_eq!(n.media_class().as_deref(), Some("Audio/Source"));
        assert_eq!(n.volume(), Some((0.1905, false)));
    }

    #[test]
    fn aplica_mudancas_e_remocoes() {
        let mut g = Graph::from_dump(&[node(45, 0.1905, false)]);
        assert!(g.apply(&[node(45, 0.5, true)]));
        assert_eq!(g.get(45).unwrap().volume(), Some((0.5, true)));

        // Cliente novo: não interessa.
        let cliente = json!({"id": 90, "type": "PipeWire:Interface:Client", "info": {"props": {}}});
        assert!(!g.apply(&[cliente]));
        assert!(!g.apply(&[json!({"id": 90, "info": null})]));

        // Mudança parcial só de params mantém as props.
        let parcial = json!({"id": 45, "type": "PipeWire:Interface:Node",
            "info": {"params": {"Props": [{"mute": false, "channelVolumes": [1.0]}]}}});
        assert!(g.apply(&[parcial]));
        assert_eq!(g.get(45).unwrap().prop_u32("device.id"), Some(55));
        assert_eq!(g.get(45).unwrap().volume(), Some((1.0, false)));

        assert!(g.apply(&[json!({"id": 45, "info": null})]));
        assert!(g.get(45).is_none());
    }
}
