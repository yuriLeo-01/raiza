//! Validação estrita de perfis no carregamento (pilar 1).
//!
//! A primeira defesa é estrutural: o formato só expressa capacidades conhecidas, sem
//! comando livre. Esta etapa confere o resto: tamanhos, posições, limites representáveis,
//! tipos de relatório e padrões conhecidos de bootloader/DFU.

use std::collections::BTreeSet;

use crate::format::{
    ChecksumKind, Encoding, Field, FieldName, IdentifyResponse, Profile, ReadSpec, ReportTemplate,
    ReportType, ResponseSpec,
};

/// Maior relatório aceito, em bytes (sem o report ID). É o tamanho de pacote de um
/// dispositivo HID full-speed.
pub const MAX_REPORT_LEN: usize = 64;
/// Maior número de estágios de DPI aceito em `capabilities.dpi.stages`.
pub const MAX_DPI_STAGES: u32 = 16;

/// Sequências ASCII que indicam bootloader, DFU ou gravação de firmware. Um perfil que
/// as contenha em qualquer relatório enviado é rejeitado, sem diferenciar maiúsculas.
pub const FORBIDDEN_ASCII: &[&str] = &["DFU", "BOOT", "BTLD", "IAP", "FLASH", "ERASE", "FIRMWARE"];

/// Perfil rejeitado: onde e por quê.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProfileError {
    /// JSON malformado, campo desconhecido ou tipo errado.
    #[error("JSON inválido: {0}")]
    Json(String),
    /// O JSON é bem formado, mas viola uma regra do formato.
    #[error("{path}: {reason}")]
    Invalid {
        /// Caminho do item no JSON (ex.: `capabilities.dpi.write.fields.value`).
        path: String,
        /// O que está errado.
        reason: String,
    },
}

fn invalid(path: &str, reason: impl Into<String>) -> ProfileError {
    ProfileError::Invalid {
        path: path.to_owned(),
        reason: reason.into(),
    }
}

fn ensure(cond: bool, path: &str, reason: impl FnOnce() -> String) -> Result<(), ProfileError> {
    if cond {
        Ok(())
    } else {
        Err(invalid(path, reason()))
    }
}

/// Valida um perfil inteiro.
pub fn validate(p: &Profile) -> Result<(), ProfileError> {
    ensure(p.schema == 1, "schema", || {
        format!("versão {} não suportada (use 1)", p.schema)
    })?;
    ensure(!p.name.trim().is_empty(), "name", || "vazio".into())?;
    ensure(!p.vendor.trim().is_empty(), "vendor", || "vazio".into())?;
    ensure(
        !p.authors.is_empty() && p.authors.iter().all(|a| !a.trim().is_empty()),
        "authors",
        || "informe pelo menos um autor".into(),
    )?;
    ensure(!p.matcher.0.is_empty(), "match", || "lista vazia".into())?;
    for (i, m) in p.matcher.0.iter().enumerate() {
        ensure(m.vendor_id.0 != 0, &format!("match[{i}].vendor_id"), || {
            "não pode ser 0000".into()
        })?;
        ensure(
            m.product_id.0 != 0,
            &format!("match[{i}].product_id"),
            || "não pode ser 0000".into(),
        )?;
        ensure(
            !p.matcher.0[..i].contains(m),
            &format!("match[{i}]"),
            || "repetido".into(),
        )?;
    }

    let c = &p.capabilities;
    ensure(
        c.dpi.is_some()
            || c.polling_rate.is_some()
            || c.scan_rate.is_some()
            || c.rgb.is_some()
            || c.battery.is_some(),
        "capabilities",
        || "nenhuma capacidade descrita".into(),
    )?;

    if let Some(dpi) = &c.dpi {
        let path = "capabilities.dpi";
        ensure(dpi.min >= 1 && dpi.min <= dpi.max, path, || {
            format!("limites inválidos: min {} max {}", dpi.min, dpi.max)
        })?;
        ensure(
            dpi.step >= 1 && (dpi.max - dpi.min) % dpi.step == 0,
            path,
            || format!("max - min precisa ser múltiplo de step ({})", dpi.step),
        )?;
        ensure(dpi.read.is_some() || dpi.write.is_some(), path, || {
            "precisa de read ou write".into()
        })?;
        if let Some(n) = dpi.stages {
            ensure(
                (1..=MAX_DPI_STAGES).contains(&n),
                &format!("{path}.stages"),
                || format!("precisa estar em 1..={MAX_DPI_STAGES}"),
            )?;
        }
        let reads_stage = dpi
            .read
            .as_ref()
            .is_some_and(|r| r.response.fields.contains_key(&FieldName::Stage));
        if let Some(read) = &dpi.read {
            let rpath = format!("{path}.read");
            check_read(
                read,
                &rpath,
                &[FieldName::Value],
                &[FieldName::Value, FieldName::Stage],
            )?;
            ensure(!reads_stage || dpi.stages.is_some(), &rpath, || {
                "o campo stage exige capabilities.dpi.stages".into()
            })?;
        }
        if let Some(table) = &dpi.table {
            let tpath = format!("{path}.table");
            check_read(table, &tpath, &[FieldName::Value], &[FieldName::Value])?;
            ensure(table.request.is_some(), &tpath, || {
                "a tabela é lida por consulta: falta request".into()
            })?;
            let n = dpi
                .stages
                .ok_or_else(|| invalid(&tpath, "exige capabilities.dpi.stages"))?;
            let f = table.response.fields[&FieldName::Value];
            let end = f.at + f.encoding.width() * n as usize;
            ensure(end <= table.response.length, &tpath, || {
                format!(
                    "{n} estágios a partir do byte {} passam do tamanho {}",
                    f.at, table.response.length
                )
            })?;
        }
        if let Some(write) = &dpi.write {
            let many = write.0.len() > 1;
            ensure(!write.0.is_empty(), &format!("{path}.write"), || {
                "lista vazia".into()
            })?;
            for (i, t) in write.0.iter().enumerate() {
                let wpath = if many {
                    format!("{path}.write[{i}]")
                } else {
                    format!("{path}.write")
                };
                check_template(
                    t,
                    &wpath,
                    &[FieldName::Value],
                    &[FieldName::Value, FieldName::Stage],
                )?;
                let f = t.fields[&FieldName::Value];
                let fpath = format!("{wpath}.fields.value");
                ensure(dpi.min >= f.offset, &fpath, || {
                    format!("offset {} maior que o DPI mínimo {}", f.offset, dpi.min)
                })?;
                ensure(
                    (dpi.min - f.offset) % f.scale == 0 && dpi.step % f.scale == 0,
                    &fpath,
                    || {
                        format!(
                            "com scale {} nem todo DPI entre min e max vira um número inteiro",
                            f.scale
                        )
                    },
                )?;
                check_fits(raw_of(dpi.max, &f), &f, &fpath, "DPI máximo")?;
                if let Some(sf) = t.fields.get(&FieldName::Stage) {
                    let spath = format!("{wpath}.fields.stage");
                    // Sem ler o estágio do aparelho não há o que pôr neste campo.
                    ensure(reads_stage, &spath, || {
                        "exige ler o estágio (campo stage em capabilities.dpi.read)".into()
                    })?;
                    ensure(sf.mask.is_none(), &spath, || {
                        "mask só vale em respostas".into()
                    })?;
                    let last = dpi.stages.unwrap_or(1).saturating_sub(1);
                    let raw = last.saturating_mul(sf.scale).saturating_add(sf.offset);
                    check_fits(raw, sf, &spath, "o último estágio")?;
                }
            }
        }
    }

    for (path, pr) in [
        ("capabilities.polling_rate", &c.polling_rate),
        ("capabilities.scan_rate", &c.scan_rate),
    ] {
        let Some(pr) = pr else { continue };
        ensure(!pr.rates.is_empty(), &format!("{path}.rates"), || {
            "vazio".into()
        })?;
        for key in pr.rates.keys() {
            ensure(
                key.parse::<u32>().is_ok_and(|hz| hz > 0),
                &format!("{path}.rates"),
                || format!("taxa {key:?} não é um número de Hz"),
            )?;
        }
        ensure(pr.read.is_some() || pr.write.is_some(), path, || {
            "precisa de read ou write".into()
        })?;
        if let Some(read) = &pr.read {
            let rpath = format!("{path}.read");
            check_read(read, &rpath, &[FieldName::Value], &[FieldName::Value])?;
            check_plain(&read.response.fields[&FieldName::Value], &rpath)?;
        }
        if let Some(write) = &pr.write {
            let wpath = format!("{path}.write");
            check_template(write, &wpath, &[FieldName::Value], &[FieldName::Value])?;
            let f = write.fields[&FieldName::Value];
            check_plain(&f, &wpath)?;
            for (hz, code) in &pr.rates {
                check_fits(*code, &f, &wpath, &format!("código da taxa {hz}"))?;
            }
        }
    }

    if let Some(rgb) = &c.rgb {
        let path = "capabilities.rgb";
        ensure(!rgb.zones.is_empty(), &format!("{path}.zones"), || {
            "vazio".into()
        })?;
        ensure(!rgb.modes.is_empty(), &format!("{path}.modes"), || {
            "vazio".into()
        })?;
        let wpath = format!("{path}.write");
        use FieldName::{Blue, Green, Mode, Red, Zone};
        check_template(&rgb.write, &wpath, &[], &[Red, Green, Blue, Mode, Zone])?;
        let fields = &rgb.write.fields;
        let colors = [Red, Green, Blue]
            .iter()
            .filter(|n| fields.contains_key(n))
            .count();
        ensure(colors == 0 || colors == 3, &wpath, || {
            "red, green e blue vêm juntos".into()
        })?;
        ensure(colors == 3 || fields.contains_key(&Mode), &wpath, || {
            "precisa de cor (red, green, blue) ou de mode".into()
        })?;
        for (name, f) in fields {
            check_plain(f, &format!("{wpath}.fields.{name}"))?;
        }
        for name in [Red, Green, Blue] {
            if let Some(f) = fields.get(&name) {
                check_fits(255, f, &format!("{wpath}.fields.{name}"), "cor")?;
            }
        }
        if let Some(f) = fields.get(&Mode) {
            for (mode, code) in &rgb.modes {
                check_fits(
                    *code,
                    f,
                    &format!("{wpath}.fields.mode"),
                    &format!("modo {mode}"),
                )?;
            }
        }
        match fields.get(&Zone) {
            Some(f) => {
                let last = u32::try_from(rgb.zones.len() - 1).unwrap_or(u32::MAX);
                check_fits(last, f, &format!("{wpath}.fields.zone"), "índice de zona")?;
                if let Some(all) = rgb.all_zones {
                    check_fits(all, f, &format!("{path}.all_zones"), "all_zones")?;
                }
            }
            None => {
                ensure(rgb.zones.len() == 1, &wpath, || {
                    "com mais de uma zona, o relatório precisa do campo zone".into()
                })?;
                ensure(
                    rgb.all_zones.is_none(),
                    &format!("{path}.all_zones"),
                    || "só faz sentido com o campo zone".into(),
                )?;
            }
        }
    }

    if let Some(battery) = &c.battery {
        check_read(
            &battery.read,
            "capabilities.battery.read",
            &[FieldName::Percent],
            &[FieldName::Percent, FieldName::Charging],
        )?;
    }

    if let Some(id) = &p.identify {
        check_template(&id.request, "identify.request", &[], &[])?;
        check_fixed_response(
            &id.response,
            "identify.response",
            "informe os bytes que identificam o aparelho",
        )?;
    }
    if let Some(ack) = &p.write_ack {
        check_fixed_response(ack, "write_ack", "informe os bytes que confirmam a escrita")?;
    }

    check_opcodes(p)
}

/// Resposta fixa (identificação ou confirmação de escrita): tipo, tamanho e bytes esperados.
fn check_fixed_response(
    r: &IdentifyResponse,
    path: &str,
    missing_expect: &str,
) -> Result<(), ProfileError> {
    ensure(
        matches!(r.report_type, ReportType::Feature | ReportType::Input),
        &format!("{path}.type"),
        || "respostas só podem ser feature ou input".into(),
    )?;
    ensure(
        (1..=MAX_REPORT_LEN).contains(&r.length),
        &format!("{path}.length"),
        || format!("tamanho {} fora de 1..={MAX_REPORT_LEN}", r.length),
    )?;
    let epath = format!("{path}.expect");
    ensure(!r.expect.is_empty(), &epath, || missing_expect.into())?;
    for e in &r.expect {
        ensure(e.at < r.length, &epath, || {
            format!("posição {} passa do fim da resposta", e.at)
        })?;
    }
    Ok(())
}

fn raw_of(value: u32, f: &Field) -> u32 {
    (value - f.offset) / f.scale
}

fn check_fits(raw: u32, f: &Field, path: &str, what: &str) -> Result<(), ProfileError> {
    ensure(raw <= f.encoding.max(), path, || {
        format!(
            "{what} vira {raw}, que não cabe em {:?} (máximo {})",
            f.encoding,
            f.encoding.max()
        )
    })
}

/// Campos que levam códigos de tabela (taxas, modos, cores) não usam escala.
fn check_plain(f: &Field, path: &str) -> Result<(), ProfileError> {
    ensure(f.scale == 1 && f.offset == 0, path, || {
        "este campo não aceita scale/offset".into()
    })
}

fn check_names(
    names: impl Iterator<Item = FieldName> + Clone,
    path: &str,
    required: &[FieldName],
    allowed: &[FieldName],
) -> Result<(), ProfileError> {
    for n in names.clone() {
        ensure(allowed.contains(&n), path, || {
            format!("campo {n} não é aceito aqui (aceitos: {allowed:?})")
        })?;
    }
    for r in required {
        ensure(names.clone().any(|n| n == *r), path, || {
            format!("falta o campo {r}")
        })?;
    }
    Ok(())
}

fn check_template(
    t: &ReportTemplate,
    path: &str,
    required: &[FieldName],
    allowed: &[FieldName],
) -> Result<(), ProfileError> {
    ensure(
        matches!(t.report_type, ReportType::Feature | ReportType::Output),
        &format!("{path}.type"),
        || "relatórios enviados só podem ser feature ou output".into(),
    )?;
    let len = t.data.0.len();
    ensure(
        (1..=MAX_REPORT_LEN).contains(&len),
        &format!("{path}.data"),
        || format!("tamanho {len} fora de 1..={MAX_REPORT_LEN}"),
    )?;
    check_forbidden(&t.data.0, &format!("{path}.data"))?;
    check_names(
        t.fields.keys().copied(),
        &format!("{path}.fields"),
        required,
        allowed,
    )?;

    let mut used = BTreeSet::new();
    for (name, f) in &t.fields {
        let fpath = format!("{path}.fields.{name}");
        ensure(f.scale >= 1, &fpath, || "scale precisa ser >= 1".into())?;
        ensure(f.mask.is_none(), &fpath, || {
            "mask só vale em respostas".into()
        })?;
        ensure(f.at + f.encoding.width() <= len, &fpath, || {
            format!("passa do fim do relatório ({len} bytes)")
        })?;
        for b in f.at..f.at + f.encoding.width() {
            ensure(used.insert(b), &fpath, || {
                format!("byte {b} usado por mais de um campo")
            })?;
        }
    }
    if let Some(c) = &t.checksum {
        let cpath = format!("{path}.checksum");
        ensure(c.from < c.to && c.to <= len, &cpath, || {
            format!("intervalo {}..{} inválido para {len} bytes", c.from, c.to)
        })?;
        let bytes = c.at..c.at + c.width();
        ensure(
            bytes.end <= len && bytes.clone().all(|b| !(c.from..c.to).contains(&b)),
            &cpath,
            || "o checksum precisa ficar dentro do relatório e fora do intervalo somado".into(),
        )?;
        for b in bytes {
            ensure(!used.contains(&b), &cpath, || {
                format!("byte {b} já é de um campo")
            })?;
        }
        let crc_params = c.poly.is_some() || c.init.is_some() || c.encoding.is_some();
        if c.kind == ChecksumKind::Crc16 {
            ensure(
                c.poly.is_some()
                    && c.init.is_some()
                    && matches!(c.encoding, Some(Encoding::U16le | Encoding::U16be)),
                &cpath,
                || "crc16 precisa de poly, init e encoding (u16le ou u16be)".into(),
            )?;
        } else {
            ensure(!crc_params, &cpath, || {
                "poly, init e encoding só valem para crc16".into()
            })?;
        }
    }
    Ok(())
}

/// Opcodes perigosos por fabricante de controladora, no primeiro byte do relatório.
/// VID `0x3151` (ROYUAN, teclados Akko/Ajazz/AttackShark): `0xAC` apaga o chip de flash,
/// `0x2C` apaga a tela, `0x7F` e `0x30` entram no bootloader (documentado pelo Sharkfin).
pub const FORBIDDEN_OPCODES: &[(u16, &[u8])] = &[(0x3151, &[0xAC, 0x2C, 0x7F, 0x30])];

/// Gatilho de bootloader que não pode aparecer em nenhum relatório enviado.
pub const FORBIDDEN_SEQUENCES: &[&[u8]] = &[&[0x55, 0xAA, 0x55, 0xAA]];

/// Todo relatório que o perfil pode mandar ao dispositivo, com o caminho no JSON.
fn sent_templates(p: &Profile) -> Vec<(String, &ReportTemplate)> {
    let mut out = Vec::new();
    let c = &p.capabilities;
    let reads: [(&str, Option<&ReadSpec>); 5] = [
        ("dpi", c.dpi.as_ref().and_then(|d| d.read.as_ref())),
        ("dpi.table", c.dpi.as_ref().and_then(|d| d.table.as_ref())),
        (
            "polling_rate",
            c.polling_rate.as_ref().and_then(|d| d.read.as_ref()),
        ),
        (
            "scan_rate",
            c.scan_rate.as_ref().and_then(|d| d.read.as_ref()),
        ),
        ("battery", c.battery.as_ref().map(|b| &b.read)),
    ];
    for (name, r) in reads {
        if let Some(req) = r.and_then(|r| r.request.as_ref()) {
            out.push((format!("capabilities.{name}.read.request"), req));
        }
    }
    if let Some(w) = c.dpi.as_ref().and_then(|d| d.write.as_ref()) {
        for (i, t) in w.0.iter().enumerate() {
            out.push((format!("capabilities.dpi.write[{i}]"), t));
        }
    }
    if let Some(w) = c.polling_rate.as_ref().and_then(|d| d.write.as_ref()) {
        out.push(("capabilities.polling_rate.write".into(), w));
    }
    if let Some(w) = c.scan_rate.as_ref().and_then(|d| d.write.as_ref()) {
        out.push(("capabilities.scan_rate.write".into(), w));
    }
    if let Some(rgb) = &c.rgb {
        out.push(("capabilities.rgb.write".into(), &rgb.write));
    }
    if let Some(id) = &p.identify {
        out.push(("identify.request".into(), &id.request));
    }
    out
}

fn check_opcodes(p: &Profile) -> Result<(), ProfileError> {
    let vids: Vec<u16> = p.matcher.0.iter().map(|m| m.vendor_id.0).collect();
    for (path, t) in sent_templates(p) {
        let data = &t.data.0;
        for seq in FORBIDDEN_SEQUENCES {
            ensure(!data.windows(seq.len()).any(|w| w == *seq), &path, || {
                format!("contém {seq:02x?}: gatilho de bootloader não é permitido (pilar 1)")
            })?;
        }
        for (vid, ops) in FORBIDDEN_OPCODES {
            if let Some(first) = data.first() {
                ensure(!(vids.contains(vid) && ops.contains(first)), &path, || {
                    format!(
                        "opcode {first:#04x} é destrutivo nos aparelhos {vid:04x} (apaga flash ou entra no bootloader)"
                    )
                })?;
            }
        }
    }
    Ok(())
}

fn check_read(
    r: &ReadSpec,
    path: &str,
    required: &[FieldName],
    allowed: &[FieldName],
) -> Result<(), ProfileError> {
    if let Some(req) = &r.request {
        let rpath = format!("{path}.request");
        // Um pedido de leitura é uma consulta fixa: sem campos variáveis.
        check_template(req, &rpath, &[], &[])?;
    }
    check_response(&r.response, &format!("{path}.response"), required, allowed)
}

fn check_response(
    r: &ResponseSpec,
    path: &str,
    required: &[FieldName],
    allowed: &[FieldName],
) -> Result<(), ProfileError> {
    ensure(
        matches!(r.report_type, ReportType::Feature | ReportType::Input),
        &format!("{path}.type"),
        || "respostas só podem ser feature ou input".into(),
    )?;
    ensure(
        (1..=MAX_REPORT_LEN).contains(&r.length),
        &format!("{path}.length"),
        || format!("tamanho {} fora de 1..={MAX_REPORT_LEN}", r.length),
    )?;
    for e in &r.expect {
        ensure(e.at < r.length, &format!("{path}.expect"), || {
            format!("posição {} passa do fim da resposta", e.at)
        })?;
    }
    check_names(
        r.fields.keys().copied(),
        &format!("{path}.fields"),
        required,
        allowed,
    )?;
    for (name, f) in &r.fields {
        let fpath = format!("{path}.fields.{name}");
        ensure(f.scale >= 1, &fpath, || "scale precisa ser >= 1".into())?;
        ensure(f.at + f.encoding.width() <= r.length, &fpath, || {
            format!("passa do fim da resposta ({} bytes)", r.length)
        })?;
        if let Some(mask) = f.mask {
            ensure(mask != 0 && mask <= f.encoding.max(), &fpath, || {
                "mask precisa ser não nula e caber na codificação".into()
            })?;
        }
    }
    Ok(())
}

fn check_forbidden(data: &[u8], path: &str) -> Result<(), ProfileError> {
    let upper: Vec<u8> = data.iter().map(u8::to_ascii_uppercase).collect();
    for pat in FORBIDDEN_ASCII {
        let found = upper.windows(pat.len()).any(|w| w == pat.as_bytes());
        ensure(!found, path, || {
            format!("contém {pat:?}: padrão de bootloader/firmware não é permitido (pilar 1)")
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::Encoding;

    #[test]
    fn largura_das_codificacoes() {
        assert_eq!(Encoding::U8.width(), 1);
        assert_eq!(Encoding::U16be.max(), 65535);
    }

    #[test]
    fn padroes_proibidos() {
        assert!(check_forbidden(b"\x01\x02dfu\x00", "x").is_err());
        assert!(check_forbidden(b"xxBootyy", "x").is_err());
        assert!(check_forbidden(&[0x08, 0x03, 0x00, 0x40], "x").is_ok());
    }
}
