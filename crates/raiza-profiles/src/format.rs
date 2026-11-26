//! Tipos do formato JSON de perfil (versão 1). Veja `docs/PERFIS.md`.

use std::collections::BTreeMap;
use std::fmt;

use raiza_core::{ConnectionKind, DeviceKind};
use serde::{Deserialize, Serialize};

/// Perfil HID declarativo de um dispositivo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// Caminho ou URL do `schema.json`, para validação no editor. Ignorado pelo Raiza.
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub json_schema: Option<String>,
    /// Versão do formato. Hoje só `1`.
    pub schema: u32,
    /// Nome legível do dispositivo.
    pub name: String,
    /// Fabricante.
    pub vendor: String,
    /// Tipo do dispositivo.
    pub kind: DeviceKind,
    /// Se o perfil foi testado e revisado. `false` força somente leitura (pilar 1).
    pub verified: bool,
    /// Quem escreveu ou capturou o protocolo. Não pode ser vazio.
    pub authors: Vec<String>,
    /// Como reconhecer o dispositivo. Um objeto, ou uma lista quando o mesmo aparelho
    /// se conecta de mais de um jeito com o mesmo protocolo (ex.: teclado com cabo e com
    /// dongle). A primeira entrada é a conexão preferida.
    #[serde(rename = "match")]
    pub matcher: Matchers,
    /// Consulta de identificação. Quando existe, o executor só escreve no dispositivo
    /// depois que a resposta bater (pilar 1). Use quando o VID:PID é compartilhado por
    /// aparelhos com protocolos diferentes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identify: Option<IdentifySpec>,
    /// O que o perfil sabe ler e escrever.
    pub capabilities: ProfileCapabilities,
}

/// Consulta de identificação: um pedido fixo e os bytes que a resposta precisa ter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentifySpec {
    /// Pedido fixo, sem campos variáveis.
    pub request: ReportTemplate,
    /// Resposta esperada.
    pub response: IdentifyResponse,
}

/// Resposta esperada da identificação.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentifyResponse {
    /// `feature` ou `input`.
    #[serde(rename = "type")]
    pub report_type: ReportType,
    /// Report ID da resposta.
    pub report_id: u8,
    /// Tamanho da resposta, sem o report ID.
    pub length: usize,
    /// Bytes que identificam o aparelho. Não pode ser vazio.
    pub expect: Vec<Expect>,
}

/// Uma ou mais identificações; no JSON, um objeto ou uma lista de objetos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Matchers(pub Vec<DeviceMatch>);

impl Serialize for Matchers {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self.0.as_slice() {
            [one] => one.serialize(s),
            many => many.serialize(s),
        }
    }
}

impl<'de> Deserialize<'de> for Matchers {
    // Manual (e não `untagged`) para preservar a mensagem de erro de cada campo.
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let value = serde_json::Value::deserialize(d)?;
        let list = if value.is_array() {
            serde_json::from_value(value).map_err(D::Error::custom)?
        } else {
            vec![serde_json::from_value(value).map_err(D::Error::custom)?]
        };
        Ok(Matchers(list))
    }
}

/// Identificação do dispositivo e da interface HID que recebe os comandos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceMatch {
    /// Vendor ID USB, em hex (`"24ae"`).
    pub vendor_id: Hex16,
    /// Product ID USB, em hex (`"1423"`).
    pub product_id: Hex16,
    /// Número da interface USB (`bInterfaceNumber`), se o aparelho tiver várias.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interface: Option<u8>,
    /// Usage page HID da interface (`"ff00"` = vendor), se precisar desempatar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_page: Option<Hex16>,
    /// Usage HID da interface.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Hex16>,
    /// Meio desta conexão (`"wired"` no cabo, `"wireless"` no dongle). Quando presente,
    /// o executor anuncia a capacidade de conexão com esse meio.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<ConnectionKind>,
}

/// Número de 16 bits escrito em hex no JSON (`"24ae"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct Hex16(pub u16);

impl fmt::Display for Hex16 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04x}", self.0)
    }
}

impl From<Hex16> for String {
    fn from(h: Hex16) -> Self {
        h.to_string()
    }
}

impl TryFrom<String> for Hex16 {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        if s.len() == 4
            && let Ok(v) = u16::from_str_radix(&s, 16)
        {
            return Ok(Hex16(v));
        }
        Err(format!("esperado 4 dígitos hex, como \"24ae\": {s:?}"))
    }
}

/// Bytes escritos em hex no JSON, separados ou não por espaços (`"08 03 00 ff"`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct HexBytes(pub Vec<u8>);

impl From<HexBytes> for String {
    fn from(h: HexBytes) -> Self {
        h.0.iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl TryFrom<String> for HexBytes {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        let digits: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        if !digits.len().is_multiple_of(2) || !digits.is_ascii() {
            return Err(format!("hex inválido: {s:?}"));
        }
        (0..digits.len())
            .step_by(2)
            .map(|i| {
                u8::from_str_radix(&digits[i..i + 2], 16)
                    .map_err(|_| format!("hex inválido: {s:?}"))
            })
            .collect::<Result<Vec<u8>, String>>()
            .map(HexBytes)
    }
}

/// Capacidades que um perfil pode descrever. Qualquer outro nome é rejeitado: não existe
/// "comando livre" num perfil (pilar 1).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileCapabilities {
    /// Resolução do sensor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dpi: Option<DpiSpec>,
    /// Taxa de atualização.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polling_rate: Option<PollingRateSpec>,
    /// Iluminação.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rgb: Option<RgbSpec>,
    /// Bateria (somente leitura).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub battery: Option<BatterySpec>,
}

/// DPI: limites e relatórios. Campo usado: `value`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DpiSpec {
    /// Menor DPI.
    pub min: u32,
    /// Maior DPI.
    pub max: u32,
    /// Passo entre valores.
    pub step: u32,
    /// Como ler o DPI atual.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read: Option<ReadSpec>,
    /// Quantos estágios de DPI o aparelho tem. Obrigatório quando algum relatório usa o
    /// campo `stage`: um estágio lido fora de `0..stages` é descartado e nunca vira endereço.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stages: Option<u32>,
    /// Como escrever um DPI: um relatório ou uma lista, enviados em ordem (ex.: X e Y).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write: Option<Templates>,
}

/// Um ou mais relatórios; no JSON, um objeto ou uma lista de objetos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Templates(pub Vec<ReportTemplate>);

impl Serialize for Templates {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self.0.as_slice() {
            [one] => one.serialize(s),
            many => many.serialize(s),
        }
    }
}

impl<'de> Deserialize<'de> for Templates {
    // Manual (e não `untagged`) para preservar a mensagem de erro de cada campo.
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let value = serde_json::Value::deserialize(d)?;
        let list = if value.is_array() {
            serde_json::from_value(value).map_err(D::Error::custom)?
        } else {
            vec![serde_json::from_value(value).map_err(D::Error::custom)?]
        };
        Ok(Templates(list))
    }
}

/// Taxa de atualização. Campo usado: `value`, que leva o código bruto de `rates`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PollingRateSpec {
    /// Taxa em Hz (chave, como texto) → código enviado ao dispositivo.
    pub rates: BTreeMap<String, u32>,
    /// Como ler a taxa atual.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read: Option<ReadSpec>,
    /// Como escrever uma taxa.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write: Option<ReportTemplate>,
}

/// Iluminação. Campos usados na escrita: `red`, `green`, `blue`, `mode`, `zone`.
/// O estado atual não é lido na versão 1 do formato.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RgbSpec {
    /// Nomes das zonas, na ordem do dispositivo. O índice é o código enviado em `zone`.
    pub zones: Vec<String>,
    /// Nome do modo → código enviado em `mode`.
    pub modes: BTreeMap<String, u32>,
    /// Código de `zone` que significa "todas". Sem ele, e com o campo `zone` no
    /// relatório, "todas" vira um relatório por zona.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub all_zones: Option<u32>,
    /// Como escrever.
    pub write: ReportTemplate,
}

/// Bateria. Campos usados na resposta: `percent` e, opcional, `charging` (≠ 0 = carregando).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatterySpec {
    /// Como ler.
    pub read: ReadSpec,
}

/// Leitura: um pedido opcional e a resposta esperada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadSpec {
    /// Relatório enviado antes de ler (uma consulta, sem campos variáveis). Sem ele, a
    /// leitura é só um GET_FEATURE ou a espera de um relatório de entrada.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<ReportTemplate>,
    /// Resposta esperada.
    pub response: ResponseSpec,
}

/// Tipo de relatório HID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportType {
    /// Feature report (SET_FEATURE ao enviar, GET_FEATURE ao ler).
    Feature,
    /// Output report (só envio).
    Output,
    /// Input report (só leitura).
    Input,
}

/// Um relatório enviado ao dispositivo: bytes fixos + campos variáveis + checksum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportTemplate {
    /// `feature` ou `output`.
    #[serde(rename = "type")]
    pub report_type: ReportType,
    /// Report ID (`0` = dispositivo sem relatórios numerados).
    pub report_id: u8,
    /// Conteúdo fixo do relatório, sem o report ID. O tamanho dele é o tamanho do relatório.
    pub data: HexBytes,
    /// Campos variáveis, sobrescritos em `data`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<FieldName, Field>,
    /// Checksum calculado depois de aplicar os campos.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksum: Option<Checksum>,
}

/// Resposta esperada de uma leitura.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResponseSpec {
    /// `feature` ou `input`.
    #[serde(rename = "type")]
    pub report_type: ReportType,
    /// Report ID da resposta.
    pub report_id: u8,
    /// Tamanho da resposta, sem o report ID.
    pub length: usize,
    /// Bytes que precisam bater para a resposta ser aceita (ex.: eco do comando).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expect: Vec<Expect>,
    /// Campos lidos da resposta.
    pub fields: BTreeMap<FieldName, Field>,
}

/// Byte que precisa ter um valor certo na resposta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expect {
    /// Posição (sem contar o report ID).
    pub at: usize,
    /// Valor esperado.
    pub value: u8,
}

/// Nome de um campo variável. Cada capacidade aceita só alguns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldName {
    /// O valor da capacidade (DPI, código de taxa).
    Value,
    /// Componente vermelho (0–255).
    Red,
    /// Componente verde (0–255).
    Green,
    /// Componente azul (0–255).
    Blue,
    /// Código do modo RGB.
    Mode,
    /// Código da zona RGB.
    Zone,
    /// Carga da bateria (0–100 depois de `scale`/`offset`).
    Percent,
    /// Carregando (≠ 0).
    Charging,
    /// Estágio de DPI em uso (índice a partir de 0). Na leitura, `bruto × scale + offset`
    /// é o estágio. Na escrita, entra `estágio × scale + offset` (ex.: o endereço do estágio
    /// na memória do aparelho: `scale` = tamanho de cada entrada, `offset` = início da tabela).
    Stage,
}

impl fmt::Display for FieldName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            FieldName::Value => "value",
            FieldName::Red => "red",
            FieldName::Green => "green",
            FieldName::Blue => "blue",
            FieldName::Mode => "mode",
            FieldName::Zone => "zone",
            FieldName::Percent => "percent",
            FieldName::Charging => "charging",
            FieldName::Stage => "stage",
        };
        f.write_str(s)
    }
}

/// Onde e como um número fica no relatório.
///
/// Escrita: `bruto = (valor - offset) / scale`. Leitura: `valor = bruto * scale + offset`,
/// com `bruto` já filtrado por `mask` (e deslocado para o bit 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    /// Posição do primeiro byte (sem contar o report ID).
    pub at: usize,
    /// Codificação.
    #[serde(default)]
    pub encoding: Encoding,
    /// Divisor na escrita, multiplicador na leitura (ex.: DPI guardado em unidades de 50).
    #[serde(default = "um", skip_serializing_if = "e_um")]
    pub scale: u32,
    /// Deslocamento somado na leitura e subtraído na escrita.
    #[serde(default, skip_serializing_if = "e_zero")]
    pub offset: u32,
    /// Máscara de bits, só para leitura.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask: Option<u32>,
}

fn um() -> u32 {
    1
}

fn e_um(v: &u32) -> bool {
    *v == 1
}

fn e_zero(v: &u32) -> bool {
    *v == 0
}

/// Codificação de um número no relatório.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    /// 1 byte.
    #[default]
    U8,
    /// 2 bytes, little-endian.
    U16le,
    /// 2 bytes, big-endian.
    U16be,
}

impl Encoding {
    /// Quantos bytes ocupa.
    pub fn width(self) -> usize {
        match self {
            Encoding::U8 => 1,
            Encoding::U16le | Encoding::U16be => 2,
        }
    }

    /// Maior valor bruto representável.
    pub fn max(self) -> u32 {
        match self {
            Encoding::U8 => u8::MAX.into(),
            Encoding::U16le | Encoding::U16be => u16::MAX.into(),
        }
    }
}

/// Checksum do relatório, calculado depois de aplicar os campos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checksum {
    /// Algoritmo.
    pub kind: ChecksumKind,
    /// Onde gravar o checksum.
    pub at: usize,
    /// Primeiro byte somado (inclusive).
    pub from: usize,
    /// Último byte somado (exclusive).
    pub to: usize,
    /// Se o report ID entra no cálculo, antes de `data[from..to]` (ex.: CRC do Akko S9 Ultra).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub include_report_id: bool,
    /// Só `crc16`: polinômio, aplicado bit a bit deslocando à direita.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poly: Option<Hex16>,
    /// Só `crc16`: valor inicial.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub init: Option<Hex16>,
    /// Só `crc16`: `u16le` ou `u16be`, como os 2 bytes são gravados.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoding: Option<Encoding>,
}

impl Checksum {
    /// Quantos bytes o checksum ocupa.
    pub fn width(&self) -> usize {
        match self.kind {
            ChecksumKind::Crc16 => 2,
            _ => 1,
        }
    }
}

/// Algoritmos de checksum aceitos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChecksumKind {
    /// Soma dos bytes, módulo 256.
    Sum8,
    /// Complemento de dois da soma (`0x100 - soma`), módulo 256.
    Sum8Neg,
    /// Complemento de um da soma (`0xFF - soma`), módulo 256 (teclados ROYUAN, como o Akko).
    Sum8Inv,
    /// XOR dos bytes.
    Xor8,
    /// CRC de 16 bits: `crc ^= byte`, e em cada um dos 8 bits, se o bit 0 é 1,
    /// `crc = (crc >> 1) ^ poly`, senão `crc >>= 1`. Pede `poly`, `init` e `encoding`.
    Crc16,
}
