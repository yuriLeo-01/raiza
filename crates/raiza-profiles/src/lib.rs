//! Perfis HID declarativos do Raiza: formato, validação e codificação.
//!
//! Um perfil é um JSON (veja `docs/PERFIS.md` e `profiles/schema.json`) que descreve como
//! ler e escrever capacidades de um dispositivo HID sem suporte oficial. Este crate **não
//! faz I/O**: ele carrega e valida perfis ([`load`]), gera os bytes de um comando a partir
//! de um valor ([`Profile::build_write`]) e interpreta respostas ([`Profile::parse_response`]).
//! Quem fala com `/dev/hidraw*` é o executor de perfis (T16).
//!
//! Pilar 1:
//! - o formato só expressa capacidades conhecidas, sem comando livre;
//! - a validação rejeita relatórios grandes, campos fora do lugar, limites que não cabem
//!   na codificação e padrões de bootloader/DFU;
//! - um perfil com `"verified": false` nunca gera escrita.

#![warn(missing_docs)]

pub mod format;
mod validate;

use raiza_core::{
    Capability, CapabilityKind, CapabilityValue, Color, ConnectionKind, Level, RaizaError, RgbZone,
};

pub use format::Profile;
use format::{Field, FieldName, ReportTemplate, ReportType, ResponseSpec};
pub use validate::{FORBIDDEN_ASCII, MAX_REPORT_LEN, ProfileError, validate};

/// Lê e valida um perfil a partir do JSON.
pub fn load(json: &str) -> Result<Profile, ProfileError> {
    let profile: Profile =
        serde_json::from_str(json).map_err(|e| ProfileError::Json(e.to_string()))?;
    validate(&profile)?;
    Ok(profile)
}

/// Um relatório pronto para enviar: `bytes[0]` é o report ID, como o hidraw espera.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HidReport {
    /// `Feature` (SET_FEATURE) ou `Output` (write no hidraw).
    pub report_type: ReportType,
    /// Report ID seguido do conteúdo.
    pub bytes: Vec<u8>,
}

impl Profile {
    /// Se o perfil serve para este dispositivo/interface. Critérios ausentes no perfil
    /// (interface, usage page, usage) não restringem.
    pub fn matches(
        &self,
        vendor_id: u16,
        product_id: u16,
        interface: Option<u8>,
        usage_page: Option<u16>,
        usage: Option<u16>,
    ) -> bool {
        self.match_index(vendor_id, product_id, interface, usage_page, usage)
            .is_some()
    }

    /// Qual entrada de `match` serve para este dispositivo/interface (`0` = conexão
    /// preferida). Com duas conexões do mesmo aparelho presentes ao mesmo tempo, o
    /// executor anuncia as capacidades só na de menor índice.
    pub fn match_index(
        &self,
        vendor_id: u16,
        product_id: u16,
        interface: Option<u8>,
        usage_page: Option<u16>,
        usage: Option<u16>,
    ) -> Option<usize> {
        let opt = |want: Option<u16>, got: Option<u16>| want.is_none() || want == got;
        self.matcher.0.iter().position(|m| {
            m.vendor_id.0 == vendor_id
                && m.product_id.0 == product_id
                && (m.interface.is_none() || m.interface == interface)
                && opt(m.usage_page.map(|h| h.0), usage_page)
                && opt(m.usage.map(|h| h.0), usage)
        })
    }

    /// Meio da conexão `index` do `match`, se o perfil o declara.
    pub fn connection(&self, index: usize) -> Option<ConnectionKind> {
        self.matcher.0.get(index).and_then(|m| m.connection)
    }

    /// Capacidades descritas, com estado inicial (antes de qualquer leitura): DPI no
    /// mínimo, primeira taxa, primeiro modo e zonas apagadas, bateria em 0.
    /// O executor deve trocá-las pelo resultado de uma leitura assim que possível.
    ///
    /// `writable` é `true` só se o perfil é verificado **e** descreve a escrita.
    pub fn capabilities(&self) -> Vec<Capability> {
        let c = &self.capabilities;
        let mut out = Vec::new();
        if let Some(dpi) = &c.dpi {
            out.push(self.dpi_capability(dpi.min));
        }
        if c.polling_rate.is_some() {
            let first = self.supported_rates().first().copied().unwrap_or(0);
            out.push(self.polling_capability(first));
        }
        if let Some(rgb) = &c.rgb {
            out.push(Capability::Rgb {
                zones: rgb
                    .zones
                    .iter()
                    .map(|name| RgbZone {
                        name: name.clone(),
                        color: Color { r: 0, g: 0, b: 0 },
                    })
                    .collect(),
                modes: rgb.modes.keys().cloned().collect(),
                current_mode: rgb.modes.keys().next().cloned().unwrap_or_default(),
                writable: self.verified,
            });
        }
        if c.battery.is_some() {
            out.push(Capability::Battery {
                percent: 0,
                charging: false,
            });
        }
        out
    }

    /// Pedido de identificação, se o perfil tiver um. O executor deve mandá-lo e passar a
    /// resposta para [`Profile::check_identity`] **antes de qualquer escrita** (pilar 1).
    pub fn identify_request(&self) -> Option<HidReport> {
        self.identify.as_ref().map(|i| encode(&i.request, &[]))
    }

    /// Confere a resposta da identificação (`bytes[0]` é o report ID). Sem `identify` no
    /// perfil, sempre aceita. Resposta diferente vira erro, e o executor deve tratar o
    /// aparelho como somente leitura: ele não é o que o perfil descreve.
    pub fn check_identity(&self, bytes: &[u8]) -> Result<(), RaizaError> {
        let Some(id) = &self.identify else {
            return Ok(());
        };
        let r = &id.response;
        let matches = bytes.split_first().is_some_and(|(&rid, data)| {
            rid == r.report_id
                && data.len() >= r.length
                && r.expect.iter().all(|e| data[e.at] == e.value)
        });
        if matches {
            Ok(())
        } else {
            Err(RaizaError::Io(format!(
                "identificação não confere com o perfil {:?}: o aparelho fica somente leitura",
                self.name
            )))
        }
    }

    /// Relatório a enviar antes de ler `kind`, se o perfil pedir um.
    pub fn read_request(&self, kind: CapabilityKind) -> Option<HidReport> {
        let req = self.read_spec(kind)?.request.as_ref()?;
        Some(encode(req, &[]))
    }

    /// Resposta esperada ao ler `kind`. `None` se o perfil não sabe ler essa capacidade.
    pub fn response_spec(&self, kind: CapabilityKind) -> Option<&ResponseSpec> {
        self.read_spec(kind).map(|r| &r.response)
    }

    /// Interpreta a resposta de uma leitura. `bytes[0]` é o report ID.
    ///
    /// Resposta com report ID, tamanho ou bytes de `expect` diferentes vira
    /// [`RaizaError::Io`]: o executor deve ignorá-la e tentar de novo.
    pub fn parse_response(
        &self,
        kind: CapabilityKind,
        bytes: &[u8],
    ) -> Result<Capability, RaizaError> {
        let spec = self
            .response_spec(kind)
            .ok_or(RaizaError::Unsupported(kind))?;
        let (&id, data) = bytes
            .split_first()
            .ok_or_else(|| RaizaError::Io("resposta vazia".into()))?;
        if id != spec.report_id || data.len() < spec.length {
            return Err(RaizaError::Io(format!(
                "resposta inesperada: report ID {id}, {} bytes (esperado ID {}, {} bytes)",
                data.len(),
                spec.report_id,
                spec.length
            )));
        }
        if let Some(e) = spec.expect.iter().find(|e| data[e.at] != e.value) {
            return Err(RaizaError::Io(format!(
                "resposta inesperada: byte {} = {:#04x}, esperado {:#04x}",
                e.at, data[e.at], e.value
            )));
        }
        let read = |name: FieldName| spec.fields.get(&name).map(|f| decode(f, data));
        match kind {
            CapabilityKind::Dpi => {
                let stage = read(FieldName::Stage);
                if let Some(s) = stage {
                    let n = self.dpi_stages();
                    if s >= n {
                        return Err(RaizaError::Io(format!("estágio de DPI {s} fora de 0..{n}")));
                    }
                }
                let mut cap = self.dpi_capability(read(FieldName::Value).unwrap_or(0));
                if let Capability::Dpi { active_stage, .. } = &mut cap {
                    *active_stage = stage;
                }
                Ok(cap)
            }
            CapabilityKind::PollingRate => {
                let code = read(FieldName::Value).unwrap_or(0);
                let hz = self
                    .capabilities
                    .polling_rate
                    .as_ref()
                    .and_then(|pr| pr.rates.iter().find(|(_, c)| **c == code))
                    .and_then(|(hz, _)| hz.parse().ok())
                    .ok_or_else(|| {
                        RaizaError::Io(format!("código de taxa desconhecido: {code}"))
                    })?;
                Ok(self.polling_capability(hz))
            }
            CapabilityKind::Battery => Ok(Capability::Battery {
                percent: u8::try_from(read(FieldName::Percent).unwrap_or(0).min(100))
                    .unwrap_or(100),
                charging: read(FieldName::Charging).is_some_and(|v| v != 0),
            }),
            other => Err(RaizaError::Unsupported(other)),
        }
    }

    /// Gera os relatórios que aplicam `value`, dado o estado atual `current` da capacidade.
    ///
    /// Recusa com [`RaizaError::ReadOnly`] se o perfil não é verificado (pilar 1) e valida
    /// `value` contra `current` antes de gerar qualquer byte. RGB pode gerar um relatório
    /// por zona quando o dispositivo não tem código de "todas as zonas".
    pub fn build_write(
        &self,
        current: &Capability,
        value: &CapabilityValue,
    ) -> Result<Vec<HidReport>, RaizaError> {
        let kind = value.kind();
        if !self.verified {
            return Err(RaizaError::ReadOnly(kind));
        }
        value.validate(current)?;
        let c = &self.capabilities;
        match value {
            CapabilityValue::Dpi(dpi) => {
                let templates = c
                    .dpi
                    .as_ref()
                    .and_then(|d| d.write.as_ref())
                    .ok_or(RaizaError::ReadOnly(kind))?;
                // Estágio em uso, só se veio de uma leitura e está dentro do limite: um
                // estágio desconhecido nunca vira endereço (pilar 1).
                let stage = match current {
                    Capability::Dpi {
                        active_stage: Some(s),
                        ..
                    } if *s < self.dpi_stages() => Some(*s),
                    _ => None,
                };
                templates
                    .0
                    .iter()
                    .map(|t| {
                        let f = t.fields[&FieldName::Value];
                        let mut values = vec![(FieldName::Value, (dpi - f.offset) / f.scale)];
                        if let Some(sf) = t.fields.get(&FieldName::Stage) {
                            let s = stage.ok_or_else(|| {
                                RaizaError::Io(
                                    "estágio de DPI em uso desconhecido: leia o DPI antes".into(),
                                )
                            })?;
                            values.push((FieldName::Stage, s * sf.scale + sf.offset));
                        }
                        Ok(encode(t, &values))
                    })
                    .collect()
            }
            CapabilityValue::PollingRate(hz) => {
                let pr = c
                    .polling_rate
                    .as_ref()
                    .ok_or(RaizaError::Unsupported(kind))?;
                let t = pr.write.as_ref().ok_or(RaizaError::ReadOnly(kind))?;
                let code = pr.rates.get(&hz.to_string()).copied().ok_or_else(|| {
                    RaizaError::OutOfRange {
                        kind,
                        detail: format!("{hz} Hz não está no perfil"),
                    }
                })?;
                Ok(vec![encode(t, &[(FieldName::Value, code)])])
            }
            CapabilityValue::Rgb(setting) => {
                let rgb = c.rgb.as_ref().ok_or(RaizaError::Unsupported(kind))?;
                let Capability::Rgb {
                    zones,
                    current_mode,
                    ..
                } = current
                else {
                    return Err(RaizaError::Unsupported(kind));
                };
                let mode = setting.mode.as_ref().unwrap_or(current_mode);
                let mode_code = rgb.modes.get(mode).copied();
                let color_of = |zone: usize| {
                    setting
                        .color
                        .or_else(|| zones.get(zone).map(|z| z.color))
                        .unwrap_or(Color { r: 0, g: 0, b: 0 })
                };
                let values = |zone: usize, zone_code: Option<u32>| {
                    let color = color_of(zone);
                    let mut v = vec![
                        (FieldName::Red, u32::from(color.r)),
                        (FieldName::Green, u32::from(color.g)),
                        (FieldName::Blue, u32::from(color.b)),
                    ];
                    if let Some(m) = mode_code {
                        v.push((FieldName::Mode, m));
                    }
                    if let Some(z) = zone_code {
                        v.push((FieldName::Zone, z));
                    }
                    v
                };
                let has_zone = rgb.write.fields.contains_key(&FieldName::Zone);
                let reports = match (setting.zone, has_zone, rgb.all_zones) {
                    (Some(z), true, _) => vec![encode(&rgb.write, &values(z as usize, Some(z)))],
                    (None, true, Some(all)) => vec![encode(&rgb.write, &values(0, Some(all)))],
                    (None, true, None) => (0..rgb.zones.len())
                        .map(|z| encode(&rgb.write, &values(z, u32::try_from(z).ok())))
                        .collect(),
                    // Uma zona só, sem campo de zona.
                    (_, false, _) => vec![encode(&rgb.write, &values(0, None))],
                };
                Ok(reports)
            }
            _ => Err(RaizaError::Unsupported(kind)),
        }
    }

    fn read_spec(&self, kind: CapabilityKind) -> Option<&format::ReadSpec> {
        let c = &self.capabilities;
        match kind {
            CapabilityKind::Dpi => c.dpi.as_ref()?.read.as_ref(),
            CapabilityKind::PollingRate => c.polling_rate.as_ref()?.read.as_ref(),
            CapabilityKind::Battery => Some(&c.battery.as_ref()?.read),
            _ => None,
        }
    }

    fn supported_rates(&self) -> Vec<u32> {
        let mut rates: Vec<u32> = self
            .capabilities
            .polling_rate
            .iter()
            .flat_map(|pr| pr.rates.keys().filter_map(|k| k.parse().ok()))
            .collect();
        rates.sort_unstable();
        rates
    }

    /// Número de estágios de DPI declarado (0 se o perfil não declara).
    fn dpi_stages(&self) -> u32 {
        self.capabilities
            .dpi
            .as_ref()
            .and_then(|d| d.stages)
            .unwrap_or(0)
    }

    fn dpi_capability(&self, current: u32) -> Capability {
        let dpi = self.capabilities.dpi.as_ref();
        Capability::Dpi {
            level: Level {
                current,
                min: dpi.map_or(0, |d| d.min),
                max: dpi.map_or(0, |d| d.max),
                step: dpi.map_or(1, |d| d.step),
            },
            stages: Vec::new(),
            active_stage: None,
            writable: self.verified && dpi.is_some_and(|d| d.write.is_some()),
        }
    }

    fn polling_capability(&self, current: u32) -> Capability {
        Capability::PollingRate {
            current,
            supported: self.supported_rates(),
            writable: self.verified
                && self
                    .capabilities
                    .polling_rate
                    .as_ref()
                    .is_some_and(|p| p.write.is_some()),
        }
    }
}

/// Monta o relatório: report ID, bytes fixos, campos e checksum.
/// O perfil já foi validado, então os campos cabem no relatório.
fn encode(t: &ReportTemplate, values: &[(FieldName, u32)]) -> HidReport {
    let mut data = t.data.0.clone();
    for (name, raw) in values {
        let Some(f) = t.fields.get(name) else {
            continue;
        };
        let raw = (*raw).min(f.encoding.max());
        match f.encoding {
            format::Encoding::U8 => data[f.at] = raw as u8,
            format::Encoding::U16le => {
                data[f.at..f.at + 2].copy_from_slice(&(raw as u16).to_le_bytes());
            }
            format::Encoding::U16be => {
                data[f.at..f.at + 2].copy_from_slice(&(raw as u16).to_be_bytes());
            }
        }
    }
    if let Some(c) = &t.checksum {
        let id = c.include_report_id.then_some(t.report_id);
        let covered: Vec<u8> = id
            .into_iter()
            .chain(data[c.from..c.to].iter().copied())
            .collect();
        let sum = covered.iter().fold(0u8, |a, b| a.wrapping_add(*b));
        match c.kind {
            format::ChecksumKind::Sum8 => data[c.at] = sum,
            format::ChecksumKind::Sum8Neg => data[c.at] = sum.wrapping_neg(),
            format::ChecksumKind::Sum8Inv => data[c.at] = 0xFF - sum,
            format::ChecksumKind::Xor8 => data[c.at] = covered.iter().fold(0u8, |a, b| a ^ b),
            format::ChecksumKind::Crc16 => {
                let poly = c.poly.map_or(0, |h| h.0);
                let init = c.init.map_or(0, |h| h.0);
                let crc = crc16(&covered, poly, init);
                let bytes = match c.encoding {
                    Some(format::Encoding::U16le) => crc.to_le_bytes(),
                    _ => crc.to_be_bytes(),
                };
                data[c.at..c.at + 2].copy_from_slice(&bytes);
            }
        }
    }
    let mut bytes = Vec::with_capacity(data.len() + 1);
    bytes.push(t.report_id);
    bytes.extend(data);
    HidReport {
        report_type: t.report_type,
        bytes,
    }
}

/// CRC de 16 bits deslocando à direita (veja [`format::ChecksumKind::Crc16`]).
pub fn crc16(bytes: &[u8], poly: u16, init: u16) -> u16 {
    let mut crc = init;
    for &b in bytes {
        crc ^= u16::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ poly
            } else {
                crc >> 1
            };
        }
    }
    crc
}

/// Lê um campo da resposta (já validado: cabe em `data`).
fn decode(f: &Field, data: &[u8]) -> u32 {
    let mut raw = match f.encoding {
        format::Encoding::U8 => u32::from(data[f.at]),
        format::Encoding::U16le => u32::from(u16::from_le_bytes([data[f.at], data[f.at + 1]])),
        format::Encoding::U16be => u32::from(u16::from_be_bytes([data[f.at], data[f.at + 1]])),
    };
    if let Some(mask) = f.mask {
        raw = (raw & mask) >> mask.trailing_zeros();
    }
    raw.saturating_mul(f.scale).saturating_add(f.offset)
}
