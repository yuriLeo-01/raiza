//! Retrato da árvore udev: o que a camada de I/O entrega para a lógica de descoberta.
//!
//! A lógica de [`crate::scan()`] só enxerga um [`Tree`]. Em produção ele vem do udev
//! ([`crate::SystemUdev`]); nos testes, de um arquivo JSON ([`Tree::from_json`]).

use std::collections::BTreeMap;
use std::path::Path;

use raiza_core::RaizaError;
use serde::{Deserialize, Serialize};

/// Um nó da árvore udev (um diretório em `/sys/devices`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    /// Caminho completo no sysfs, ex.: `/sys/devices/.../usb3/3-2/3-2.2`.
    pub syspath: String,
    /// Subsistema: `usb`, `input`, `hidraw`, `sound`, `hid`...
    pub subsystem: String,
    /// Tipo dentro do subsistema, ex.: `usb_device` ou `usb_interface`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub devtype: Option<String>,
    /// Propriedades do udev (`ID_INPUT_MOUSE`, `ID_BUS`...).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, String>,
    /// Atributos do sysfs (`idVendor`, `serial`, `removable`...).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attributes: BTreeMap<String, String>,
}

impl Node {
    /// Último componente do caminho, ex.: `3-2.2`, `3-2.2:1.0`, `input12`.
    pub fn sysname(&self) -> &str {
        self.syspath.rsplit('/').next().unwrap_or_default()
    }

    /// Valor de uma propriedade udev.
    pub fn property(&self, name: &str) -> Option<&str> {
        self.properties.get(name).map(String::as_str)
    }

    /// Valor de um atributo sysfs, sem espaços nas pontas.
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes.get(name).map(|v| v.trim())
    }

    /// Se a propriedade vale `1` (como as `ID_INPUT_*`).
    pub fn flag(&self, name: &str) -> bool {
        self.property(name) == Some("1")
    }

    /// Dispositivo USB (não interface).
    pub fn is_usb_device(&self) -> bool {
        self.subsystem == "usb" && self.devtype.as_deref() == Some("usb_device")
    }

    /// Interface de um dispositivo USB.
    pub fn is_usb_interface(&self) -> bool {
        self.subsystem == "usb" && self.devtype.as_deref() == Some("usb_interface")
    }

    /// Nó `inputN` (ignora os nós `eventN`/`mouseN`, que repetem as mesmas propriedades).
    pub fn is_input(&self) -> bool {
        self.subsystem == "input" && self.sysname().starts_with("input")
    }
}

/// Conjunto de nós, indexado pelo `syspath`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tree {
    nodes: BTreeMap<String, Node>,
}

impl Tree {
    /// Monta a árvore. Nós repetidos (mesmo `syspath`) ficam com a última versão.
    pub fn new(nodes: impl IntoIterator<Item = Node>) -> Self {
        Tree {
            nodes: nodes.into_iter().map(|n| (n.syspath.clone(), n)).collect(),
        }
    }

    /// Lê uma árvore de um JSON com uma lista de [`Node`] (formato das fixtures de teste
    /// e da saída de `--example listar-udev -- --json`).
    pub fn from_json(json: &str) -> Result<Self, RaizaError> {
        let nodes: Vec<Node> = serde_json::from_str(json)
            .map_err(|e| RaizaError::Io(format!("árvore udev inválida: {e}")))?;
        Ok(Tree::new(nodes))
    }

    /// Serializa a árvore como JSON (lista de nós, em ordem de `syspath`).
    pub fn to_json(&self) -> String {
        let nodes: Vec<&Node> = self.nodes.values().collect();
        serde_json::to_string_pretty(&nodes).unwrap_or_else(|_| "[]".to_owned())
    }

    /// Todos os nós, em ordem de `syspath`.
    pub fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.nodes.values()
    }

    /// Nó com esse `syspath`.
    pub fn get(&self, syspath: &str) -> Option<&Node> {
        self.nodes.get(syspath)
    }

    /// Primeiro ancestral (sem contar o próprio nó) que satisfaz `pred`.
    pub fn ancestor(&self, node: &Node, pred: impl Fn(&Node) -> bool) -> Option<&Node> {
        Path::new(&node.syspath)
            .ancestors()
            .skip(1)
            .filter_map(|p| p.to_str())
            .filter_map(|p| self.nodes.get(p))
            .find(|n| pred(n))
    }

    /// Dispositivo USB ao qual o nó pertence (o ancestral `usb_device` mais próximo).
    pub fn owner_usb_device(&self, node: &Node) -> Option<&Node> {
        self.ancestor(node, Node::is_usb_device)
    }

    /// Todos os nós abaixo de `node` na árvore.
    pub fn descendants<'a>(&'a self, node: &'a Node) -> impl Iterator<Item = &'a Node> + 'a {
        let prefix = format!("{}/", node.syspath);
        self.nodes
            .range(prefix.clone()..)
            .take_while(move |(k, _)| k.starts_with(&prefix))
            .map(|(_, n)| n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(syspath: &str, subsystem: &str, devtype: Option<&str>) -> Node {
        Node {
            syspath: syspath.into(),
            subsystem: subsystem.into(),
            devtype: devtype.map(Into::into),
            properties: BTreeMap::new(),
            attributes: BTreeMap::new(),
        }
    }

    #[test]
    fn ancestrais_e_descendentes() {
        let tree = Tree::new([
            node("/sys/devices/usb3/3-2", "usb", Some("usb_device")),
            node("/sys/devices/usb3/3-2/3-2.2", "usb", Some("usb_device")),
            node(
                "/sys/devices/usb3/3-2/3-2.2/3-2.2:1.0",
                "usb",
                Some("usb_interface"),
            ),
            node(
                "/sys/devices/usb3/3-2/3-2.2/3-2.2:1.0/x/input/input8",
                "input",
                None,
            ),
            node("/sys/devices/usb3/3-2/3-2.20", "usb", Some("usb_device")),
        ]);
        let input = tree
            .get("/sys/devices/usb3/3-2/3-2.2/3-2.2:1.0/x/input/input8")
            .unwrap();
        assert_eq!(input.sysname(), "input8");
        assert!(input.is_input());
        assert_eq!(tree.owner_usb_device(input).unwrap().sysname(), "3-2.2");

        let teclado = tree.get("/sys/devices/usb3/3-2/3-2.2").unwrap();
        let filhos: Vec<_> = tree.descendants(teclado).map(Node::sysname).collect();
        // `3-2.20` não é filho de `3-2.2`, apesar do prefixo de texto.
        assert_eq!(filhos, ["3-2.2:1.0", "input8"]);
    }

    #[test]
    fn json_ida_e_volta() {
        let mut n = node("/sys/devices/usb3/3-2", "usb", Some("usb_device"));
        n.attributes.insert("idVendor".into(), "1a86\n".into());
        let tree = Tree::new([n]);
        let de_novo = Tree::from_json(&tree.to_json()).unwrap();
        assert_eq!(de_novo, tree);
        assert_eq!(
            de_novo
                .get("/sys/devices/usb3/3-2")
                .unwrap()
                .attribute("idVendor"),
            Some("1a86")
        );
        assert!(Tree::from_json("{").is_err());
    }
}
