//! Validação estrita de perfis no carregamento (pilar 1).
//!
//! A primeira defesa é estrutural: o formato só expressa capacidades conhecidas, sem
//! comando livre. Esta etapa confere o resto: tamanhos, posições, limites representáveis,
//! tipos de relatório e padrões conhecidos de bootloader/DFU.

use std::collections::BTreeSet;

use crate::format::{
    Field, FieldName, Profile, ReadSpec, ReportTemplate, ReportType, ResponseSpec,
};

/// Maior relatório aceito, em bytes (sem o report ID). É o tamanho de pacote de um
/// dispositivo HID full-speed.
pub const MAX_REPORT_LEN: usize = 64;

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
        c.dpi.is_some() || c.polling_rate.is_some() || c.rgb.is_some() || c.battery.is_some(),
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
        if let Some(read) = &dpi.read {
            check_read(
                read,
                &format!("{path}.read"),
                &[FieldName::Value],
                &[FieldName::Value],
            )?;
        }
        if let Some(write) = &dpi.write {
            let wpath = format!("{path}.write");
            check_template(write, &wpath, &[FieldName::Value], &[FieldName::Value])?;
            let f = write.fields[&FieldName::Value];
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
        }
    }

    if let Some(pr) = &c.polling_rate {
        let path = "capabilities.polling_rate";
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
        ensure(
            c.at < len && !(c.from..c.to).contains(&c.at),
            &cpath,
            || "o checksum precisa ficar dentro do relatório e fora do intervalo somado".into(),
        )?;
        ensure(!used.contains(&c.at), &cpath, || {
            format!("byte {} já é de um campo", c.at)
        })?;
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
