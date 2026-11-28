//! Testes do formato de perfil com o perfil fictício `tests/fixtures/ficticio.json`.

use raiza_core::{Capability, CapabilityKind, CapabilityValue, Color, RaizaError, RgbSetting};
use raiza_profiles::format::ReportType;
use raiza_profiles::{HidReport, Profile, ProfileError, load};
use serde_json::{Value, json};

const FICTICIO: &str = include_str!("fixtures/ficticio.json");

fn perfil() -> Profile {
    load(FICTICIO).expect("o perfil fictício é válido")
}

fn json_base() -> Value {
    serde_json::from_str(FICTICIO).unwrap()
}

/// Aplica `muda` ao JSON do perfil fictício e tenta carregar.
fn carregar_com(muda: impl FnOnce(&mut Value)) -> Result<Profile, ProfileError> {
    let mut v = json_base();
    muda(&mut v);
    load(&v.to_string())
}

fn rejeitado(resultado: Result<Profile, ProfileError>, trecho: &str) {
    match resultado {
        Ok(_) => panic!("deveria rejeitar ({trecho})"),
        Err(e) => assert!(
            e.to_string().contains(trecho),
            "erro {e:?} não menciona {trecho:?}"
        ),
    }
}

fn cap(p: &Profile, kind: CapabilityKind) -> Capability {
    p.capabilities()
        .into_iter()
        .find(|c| c.kind() == kind)
        .unwrap()
}

#[test]
fn ficticio_carrega_e_faz_ida_e_volta() {
    let p = perfil();
    assert!(p.matches(0x1234, 0x5678, Some(1), Some(0xff00), Some(1)));
    assert!(!p.matches(0x1234, 0x5678, Some(0), Some(0xff00), None));
    assert!(!p.matches(0x1234, 0x9999, Some(1), Some(0xff00), None));
    let de_novo = serde_json::to_string(&p).unwrap();
    assert_eq!(load(&de_novo).unwrap(), p);

    let kinds: Vec<_> = p.capabilities().iter().map(Capability::kind).collect();
    assert_eq!(
        kinds,
        [
            CapabilityKind::Dpi,
            CapabilityKind::PollingRate,
            CapabilityKind::Rgb,
            CapabilityKind::Battery
        ]
    );
    assert!(cap(&p, CapabilityKind::Dpi).is_writable());
    assert!(
        matches!(cap(&p, CapabilityKind::PollingRate), Capability::PollingRate { supported, .. } if supported == [125, 250, 500, 1000])
    );
}

#[test]
fn match_aceita_lista_com_conexao_preferida() {
    let dongle = json!({"vendor_id": "1234", "product_id": "9999"});
    let p = carregar_com(|v| {
        let cabo = v["match"].clone();
        v["match"] = json!([cabo, dongle]);
    })
    .unwrap();
    assert_eq!(
        p.match_index(0x1234, 0x5678, Some(1), Some(0xff00), None),
        Some(0)
    );
    assert_eq!(p.match_index(0x1234, 0x9999, None, None, None), Some(1));
    assert_eq!(p.match_index(0x1234, 0x0001, None, None, None), None);
    // Lista volta como lista; um só volta como objeto.
    let json = serde_json::to_value(&p).unwrap();
    assert!(json["match"].is_array());
    assert!(serde_json::to_value(perfil()).unwrap()["match"].is_object());
    assert_eq!(load(&json.to_string()).unwrap(), p);

    rejeitado(carregar_com(|v| v["match"] = json!([])), "lista vazia");
    rejeitado(
        carregar_com(|v| {
            let cabo = v["match"].clone();
            v["match"] = json!([cabo.clone(), cabo]);
        }),
        "repetido",
    );
}

#[test]
fn bytes_do_dpi() {
    let p = perfil();
    let atual = cap(&p, CapabilityKind::Dpi);
    let r = p.build_write(&atual, &CapabilityValue::Dpi(1600)).unwrap();
    // 1600 / 50 = 32 = 0x0020 em u16le no byte 4; checksum = -(0x04 + 0x20) = 0xdc.
    assert_eq!(
        r,
        [HidReport {
            report_type: ReportType::Output,
            bytes: vec![
                0x08, 0x04, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0xdc
            ],
        }]
    );
    // 16000 / 50 = 320 = 0x0140.
    let r = p.build_write(&atual, &CapabilityValue::Dpi(16000)).unwrap();
    assert_eq!(&r[0].bytes[5..7], &[0x40, 0x01]);
    assert_eq!(r[0].bytes[16], 0u8.wrapping_sub(0x04 + 0x40 + 0x01));
}

#[test]
fn bytes_da_taxa() {
    let p = perfil();
    let atual = cap(&p, CapabilityKind::PollingRate);
    let r = p
        .build_write(&atual, &CapabilityValue::PollingRate(500))
        .unwrap();
    // código 2 no byte 2; xor = 0x09 ^ 0x01 ^ 0x02 = 0x0a.
    assert_eq!(
        r,
        [HidReport {
            report_type: ReportType::Feature,
            bytes: vec![0x06, 0x09, 0x01, 0x02, 0x00, 0x00, 0x00, 0x00, 0x0a],
        }]
    );
}

#[test]
fn bytes_do_rgb() {
    let p = perfil();
    let atual = cap(&p, CapabilityKind::Rgb);
    let cor = Color {
        r: 0x10,
        g: 0x20,
        b: 0x30,
    };
    let uma_zona = CapabilityValue::Rgb(RgbSetting {
        zone: Some(1),
        mode: Some("breathing".into()),
        color: Some(cor),
    });
    // soma = 0x0a + 0x01 + 0x02 + 0x10 + 0x20 + 0x30 = 0x6d.
    assert_eq!(
        p.build_write(&atual, &uma_zona).unwrap(),
        [HidReport {
            report_type: ReportType::Feature,
            bytes: vec![0x06, 0x0a, 0x01, 0x02, 0x10, 0x20, 0x30, 0x00, 0x6d],
        }]
    );

    // Sem código de "todas": um relatório por zona, mantendo o modo atual ("breathing" é
    // o primeiro em ordem alfabética, então é o modo inicial).
    let todas = CapabilityValue::Rgb(RgbSetting {
        zone: None,
        mode: None,
        color: Some(cor),
    });
    let r = p.build_write(&atual, &todas).unwrap();
    assert_eq!(r.len(), 2);
    assert_eq!(r[0].bytes[1..7], [0x0a, 0x00, 0x02, 0x10, 0x20, 0x30]);
    assert_eq!(r[1].bytes[1..7], [0x0a, 0x01, 0x02, 0x10, 0x20, 0x30]);
}

#[test]
fn le_respostas() {
    let p = perfil();
    let mut resposta = vec![0x08, 0x03, 0, 0, 0, 0x20, 0x00];
    resposta.resize(17, 0);
    assert!(matches!(
        p.parse_response(CapabilityKind::Dpi, &resposta).unwrap(),
        Capability::Dpi { level, .. } if level.current == 1600
    ));
    assert_eq!(
        p.read_request(CapabilityKind::Dpi).unwrap().bytes[..2],
        [0x08, 0x03]
    );

    assert_eq!(
        p.parse_response(CapabilityKind::Battery, &[0x07, 0x05, 87, 0x80, 0])
            .unwrap(),
        Capability::Battery {
            percent: 87,
            charging: true
        }
    );
    assert_eq!(
        p.parse_response(CapabilityKind::Battery, &[0x07, 0x05, 250, 0x01, 0])
            .unwrap(),
        Capability::Battery {
            percent: 100,
            charging: false
        }
    );
    // Report ID, eco ou tamanho errados: a resposta é descartada.
    for ruim in [
        &[0x06, 0x05, 87, 0, 0][..],
        &[0x07, 0x04, 87, 0, 0][..],
        &[0x07, 0x05][..],
        &[][..],
    ] {
        assert!(matches!(
            p.parse_response(CapabilityKind::Battery, ruim),
            Err(RaizaError::Io(_))
        ));
    }
    assert_eq!(
        p.parse_response(CapabilityKind::Rgb, &[0x06]),
        Err(RaizaError::Unsupported(CapabilityKind::Rgb))
    );
}

#[test]
fn nao_verificado_e_somente_leitura() {
    let p = carregar_com(|v| v["verified"] = json!(false)).unwrap();
    assert!(p.capabilities().iter().all(|c| !c.is_writable()));
    let atual = cap(&perfil(), CapabilityKind::Dpi); // mesmo com um estado "gravável"
    assert_eq!(
        p.build_write(&atual, &CapabilityValue::Dpi(1600)),
        Err(RaizaError::ReadOnly(CapabilityKind::Dpi))
    );
}

#[test]
fn valor_fora_do_limite_nao_gera_bytes() {
    let p = perfil();
    let atual = cap(&p, CapabilityKind::Dpi);
    for v in [100, 1625, 20000] {
        assert!(matches!(
            p.build_write(&atual, &CapabilityValue::Dpi(v)),
            Err(RaizaError::OutOfRange { .. })
        ));
    }
    let taxa = cap(&p, CapabilityKind::PollingRate);
    assert!(
        p.build_write(&taxa, &CapabilityValue::PollingRate(333))
            .is_err()
    );
}

#[test]
fn rejeita_limite_que_nao_cabe() {
    // DPI 16000 / 50 = 320 não cabe em u8.
    rejeitado(
        carregar_com(|v| {
            v["capabilities"]["dpi"]["write"]["fields"]["value"]["encoding"] = json!("u8");
        }),
        "não cabe",
    );
    // Código de taxa 300 não cabe em u8.
    rejeitado(
        carregar_com(|v| v["capabilities"]["polling_rate"]["rates"]["8000"] = json!(300)),
        "não cabe",
    );
    // Scale que não divide o passo.
    rejeitado(
        carregar_com(|v| {
            v["capabilities"]["dpi"]["write"]["fields"]["value"]["scale"] = json!(100);
        }),
        "inteiro",
    );
    rejeitado(
        carregar_com(|v| v["capabilities"]["dpi"]["max"] = json!(16010)),
        "múltiplo",
    );
}

#[test]
fn rejeita_relatorio_grande_demais() {
    rejeitado(
        carregar_com(|v| {
            v["capabilities"]["polling_rate"]["write"]["data"] = json!("00".repeat(65));
        }),
        "fora de 1..=64",
    );
    rejeitado(
        carregar_com(|v| {
            v["capabilities"]["battery"]["read"]["response"]["length"] = json!(100);
        }),
        "fora de 1..=64",
    );
}

#[test]
fn rejeita_padrao_de_dfu() {
    // "DFU" em ASCII = 44 46 55.
    rejeitado(
        carregar_com(|v| {
            v["capabilities"]["polling_rate"]["write"]["data"] = json!("09 44 46 55 00 00 00 00");
        }),
        "bootloader",
    );
    // "boot" no pedido de leitura também conta.
    rejeitado(
        carregar_com(|v| {
            v["capabilities"]["dpi"]["read"]["request"]["data"] = json!("62 6f 6f 74");
        }),
        "bootloader",
    );
}

#[test]
fn rejeita_comando_livre_e_campos_desconhecidos() {
    rejeitado(
        carregar_com(|v| v["capabilities"]["firmware_update"] = json!({})),
        "unknown field",
    );
    rejeitado(
        carregar_com(|v| v["capabilities"]["dpi"]["write"]["raw"] = json!("00")),
        "unknown field",
    );
    rejeitado(
        carregar_com(|v| {
            v["capabilities"]["dpi"]["write"]["fields"]["zone"] = json!({"at": 8});
        }),
        "não é aceito",
    );
}

#[test]
fn rejeita_estrutura_invalida() {
    rejeitado(
        carregar_com(|v| {
            v["capabilities"]["dpi"]["write"]["fields"]["value"]["at"] = json!(15);
        }),
        "passa do fim",
    );
    rejeitado(
        carregar_com(|v| {
            v["capabilities"]["rgb"]["write"]["fields"]["red"]["at"] = json!(2);
        }),
        "mais de um campo",
    );
    rejeitado(
        carregar_com(|v| {
            v["capabilities"]["dpi"]["write"]["checksum"]["at"] = json!(3);
        }),
        "fora do intervalo",
    );
    rejeitado(
        carregar_com(|v| {
            v["capabilities"]["dpi"]["read"]["request"]["fields"] = json!({"value": {"at": 1}});
        }),
        "não é aceito",
    );
    rejeitado(
        carregar_com(|v| {
            v["capabilities"]["battery"]["read"]["response"]["type"] = json!("output");
        }),
        "feature ou input",
    );
    rejeitado(
        carregar_com(|v| {
            v["capabilities"]["polling_rate"]["write"]["type"] = json!("input");
        }),
        "feature ou output",
    );
    rejeitado(
        carregar_com(|v| {
            v["capabilities"]["rgb"]["write"]["fields"]
                .as_object_mut()
                .unwrap()
                .remove("zone");
        }),
        "campo zone",
    );
    rejeitado(carregar_com(|v| v["authors"] = json!([])), "autor");
    rejeitado(carregar_com(|v| v["schema"] = json!(2)), "versão");
    rejeitado(
        carregar_com(|v| v["match"]["vendor_id"] = json!("24AEX")),
        "hex",
    );
    rejeitado(
        carregar_com(|v| v["capabilities"] = json!({})),
        "nenhuma capacidade",
    );
}

/// `profiles/schema.json` precisa aceitar o que o Rust aceita e recusar o que o formato proíbe
/// estruturalmente.
#[test]
fn schema_json_concorda_com_o_formato() {
    let schema: Value =
        serde_json::from_str(include_str!("../../../profiles/schema.json")).unwrap();
    let validador = jsonschema::draft202012::new(&schema).expect("schema.json é válido");
    assert!(validador.is_valid(&json_base()));
    // As extensões (CRC-16 e identify) também passam no schema.
    assert!(validador.is_valid(&perfil_crc16()));
    let mut com_id = json_base();
    com_identify(&mut com_id);
    assert!(validador.is_valid(&com_id));
    let mut crc_sem_poly = perfil_crc16();
    crc_sem_poly["capabilities"]["battery"]["read"]["request"]["checksum"]
        .as_object_mut()
        .unwrap()
        .remove("poly");
    assert!(!validador.is_valid(&crc_sem_poly));
    // O que a Rust reserializa também passa no schema.
    assert!(validador.is_valid(&serde_json::to_value(perfil()).unwrap()));

    let mut ruins = Vec::new();
    for muda in [
        (|v: &mut Value| v["capabilities"]["firmware_update"] = json!({})) as fn(&mut Value),
        |v| v["capabilities"]["dpi"]["write"]["raw"] = json!("00"),
        |v| v["capabilities"]["dpi"]["write"]["type"] = json!("input"),
        |v| v["capabilities"]["rgb"]["write"]["fields"]["bogus"] = json!({"at": 1}),
        |v| v["match"]["vendor_id"] = json!("24AEX"),
        |v| v["authors"] = json!([]),
        |v| v["schema"] = json!(2),
    ] {
        let mut v = json_base();
        muda(&mut v);
        ruins.push(v);
    }
    for (i, v) in ruins.iter().enumerate() {
        assert!(
            !validador.is_valid(v),
            "o schema deveria recusar o caso {i}"
        );
        assert!(
            load(&v.to_string()).is_err(),
            "a Rust deveria recusar o caso {i}"
        );
    }
}

/// Todo perfil em `profiles/` (menos o `schema.json`) precisa carregar sem erro.
#[test]
fn perfis_do_repositorio_sao_validos() {
    fn visitar(dir: &std::path::Path, achados: &mut Vec<std::path::PathBuf>) {
        let Ok(entradas) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entradas.flatten() {
            let p = e.path();
            if p.is_dir() {
                visitar(&p, achados);
            } else if p.extension().is_some_and(|x| x == "json")
                && p.file_name().is_some_and(|n| n != "schema.json")
            {
                achados.push(p);
            }
        }
    }
    let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles");
    let mut perfis = Vec::new();
    visitar(&raiz, &mut perfis);
    for p in perfis {
        let json = std::fs::read_to_string(&p).unwrap();
        if let Err(e) = load(&json) {
            panic!("{}: {e}", p.display());
        }
    }
}

/// Perfil mínimo no formato do headset Akko S9 Ultra (pesquisa da T20): consulta de
/// bateria `02 12 01 00`, com CRC-16 de 2 bytes que cobre o report ID.
fn perfil_crc16() -> Value {
    let mut pedido = vec!["12", "01", "00"];
    pedido.resize(63, "00");
    json!({
        "schema": 1, "name": "Headset de teste", "vendor": "Teste", "kind": "headset",
        "verified": false, "authors": ["teste"],
        "match": {"vendor_id": "36eb", "product_id": "1087"},
        "capabilities": {"battery": {"read": {
            "request": {
                "type": "output", "report_id": 2, "data": pedido.join(" "),
                "checksum": {"kind": "crc16", "at": 61, "from": 0, "to": 61,
                             "include_report_id": true, "poly": "8005", "init": "ffff",
                             "encoding": "u16be"}
            },
            "response": {"type": "input", "report_id": 2, "length": 63,
                         "expect": [{"at": 0, "value": 18}],
                         "fields": {"percent": {"at": 3}}}
        }}}
    })
}

#[test]
fn crc16_bate_com_a_captura_do_s9_ultra() {
    let p = load(&perfil_crc16().to_string()).unwrap();
    let pedido = p.read_request(CapabilityKind::Battery).unwrap();
    assert_eq!(pedido.bytes.len(), 64);
    assert_eq!(&pedido.bytes[..4], &[0x02, 0x12, 0x01, 0x00]);
    // Valor citado no protocolo documentado pelo HaloBattery para esta consulta.
    assert_eq!(&pedido.bytes[62..], &[0x71, 0x12]);
    assert_eq!(
        raiza_profiles::crc16(&pedido.bytes[..62], 0x8005, 0xffff),
        0x7112
    );
}

#[test]
fn crc16_exige_parametros() {
    let mut v = perfil_crc16();
    v["capabilities"]["battery"]["read"]["request"]["checksum"]
        .as_object_mut()
        .unwrap()
        .remove("poly");
    rejeitado(load(&v.to_string()), "crc16 precisa");
    let mut v = perfil_crc16();
    let c = &mut v["capabilities"]["battery"]["read"]["request"]["checksum"];
    c["kind"] = json!("sum8");
    c["at"] = json!(62);
    rejeitado(load(&v.to_string()), "só valem para crc16");
}

/// Identificação no estilo ROYUAN (Akko 5087 V3 HE, id 2829): `8F` com checksum
/// `0xFF - soma` no byte 7.
fn com_identify(v: &mut Value) {
    v["identify"] = json!({
        "request": {"type": "feature", "report_id": 0, "data": "8f 00 00 00 00 00 00 00",
                    "checksum": {"kind": "sum8_inv", "at": 7, "from": 0, "to": 7}},
        "response": {"type": "feature", "report_id": 0, "length": 8,
                     "expect": [{"at": 0, "value": 143}, {"at": 1, "value": 13},
                                {"at": 2, "value": 11}, {"at": 3, "value": 0},
                                {"at": 4, "value": 0}]}
    });
}

#[test]
fn identify_confere_o_aparelho() {
    let p = carregar_com(com_identify).unwrap();
    let pedido = p.identify_request().unwrap();
    // 0xFF - 0x8F = 0x70, o mesmo checksum registrado pelo Sharkfin para o `8F`.
    assert_eq!(pedido.bytes, [0x00, 0x8f, 0, 0, 0, 0, 0, 0, 0x70]);
    assert_eq!(pedido.report_type, ReportType::Feature);
    let certo = [0x00, 0x8f, 0x0d, 0x0b, 0x00, 0x00, 0, 0, 0];
    assert!(p.check_identity(&certo).is_ok());
    let outro = [0x00, 0x8f, 0x0f, 0x0b, 0x00, 0x00, 0, 0, 0];
    assert!(p.check_identity(&outro).is_err());
    assert!(p.check_identity(&[]).is_err());
    // Sem `identify`, nada a conferir.
    assert!(perfil().check_identity(&[]).is_ok());

    rejeitado(
        carregar_com(|v| {
            com_identify(v);
            v["identify"]["response"]["expect"] = json!([]);
        }),
        "identificam",
    );
}

#[test]
fn rejeita_opcodes_destrutivos_e_gatilho_de_bootloader() {
    let apaga_flash = |v: &mut Value| {
        v["capabilities"]["polling_rate"]["write"]["data"] = json!("ac 01 00 00 00 00 00 00");
    };
    // No VID 3151 (ROYUAN), 0xAC apaga o chip de flash.
    rejeitado(
        carregar_com(|v| {
            apaga_flash(v);
            v["match"]["vendor_id"] = json!("3151");
        }),
        "destrutivo",
    );
    // Em outro fabricante, o mesmo byte não tem esse significado.
    assert!(carregar_com(apaga_flash).is_ok());
    // O trailer de entrada no bootloader é proibido em qualquer aparelho.
    rejeitado(
        carregar_com(|v| {
            v["capabilities"]["polling_rate"]["write"]["data"] = json!("09 01 00 55 aa 55 aa 00");
        }),
        "bootloader",
    );
    // Vale também para o pedido de identificação.
    rejeitado(
        carregar_com(|v| {
            com_identify(v);
            v["match"]["vendor_id"] = json!("3151");
            v["identify"]["request"]["data"] = json!("7f 00 00 00 00 00 00 00");
        }),
        "destrutivo",
    );
}

fn perfil_do_repositorio(caminho: &str) -> Profile {
    let arquivo = format!("{}/../../profiles/{caminho}", env!("CARGO_MANIFEST_DIR"));
    load(&std::fs::read_to_string(&arquivo).unwrap()).unwrap()
}

/// Bytes do status espontâneo 0x07 registrados pelo periph (`docs/descriptors.md`):
/// bateria 0x53 = 83 %, `01` = na bateria, report rate `84` = 2000 Hz.
#[test]
fn rapoo_vt2_le_o_status_documentado() {
    let p = perfil_do_repositorio("rapoo/rapoo-vt2-max-24ae-1423.json");
    // Verificado em 2026-11-26: só o polling é gravável (1 byte em 0x0880, testado no
    // aparelho do mantenedor e conferido no app oficial); o resto continua só leitura.
    assert!(p.verified);
    let writable: Vec<CapabilityKind> = p
        .capabilities()
        .iter()
        .filter(|c| c.is_writable())
        .map(|c| c.kind())
        .collect();
    assert_eq!(
        writable,
        [
            CapabilityKind::Dpi,
            CapabilityKind::PollingRate,
            CapabilityKind::ScanRate
        ]
    );
    let atual = p
        .capabilities()
        .into_iter()
        .find(|c| c.kind() == CapabilityKind::PollingRate)
        .unwrap();
    let reports = p
        .build_write(&atual, &CapabilityValue::PollingRate(2000))
        .unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].bytes.len(), 33);
    assert_eq!(
        &reports[0].bytes[..9],
        &[0x06, 0xA5, 0xA5, 0x01, 0x80, 0x08, 0x00, 0x00, 0x84]
    );
    assert!(reports[0].bytes[9..].iter().all(|&b| b == 0));
    assert!(
        p.build_write(&atual, &CapabilityValue::PollingRate(300))
            .is_err()
    );
    // Cabo primeiro: com o cabo plugado o mouse só fala por ele e o dongle fica mudo
    // (testado no aparelho do mantenedor).
    assert_eq!(p.match_index(0x24ae, 0x4623, Some(1), None, None), Some(0));
    assert_eq!(p.match_index(0x24ae, 0x1423, Some(1), None, None), Some(1));
    assert_eq!(p.connection(0), Some(raiza_core::ConnectionKind::Wired));
    assert_eq!(p.connection(1), Some(raiza_core::ConnectionKind::Wireless));
    let status = [
        0x07, 0x20, 0x02, 0xB0, 0x04, 0xB0, 0x04, 0x01, 0x53, 0x00, 0x84, 0x84, 0x00, 0x00, 0x00,
        0x00, 0x01, 0x05, 0x00,
    ];
    assert_eq!(
        p.parse_response(CapabilityKind::Battery, &status).unwrap(),
        Capability::Battery {
            percent: 83,
            charging: false
        }
    );
    let mut carregando = status;
    carregando[7] = 0x02;
    assert!(matches!(
        p.parse_response(CapabilityKind::Battery, &carregando)
            .unwrap(),
        Capability::Battery { charging: true, .. }
    ));
    // Status real lido do mouse do mantenedor em 2026-11-26: bateria 0x5a = 90 %.
    let real = [
        0x07, 0x20, 0x03, 0x40, 0x06, 0x40, 0x06, 0x01, 0x5a, 0x00, 0x05, 0x02, 0x00, 0x23, 0x46,
        0x00, 0x02, 0x01, 0xb3,
    ];
    assert_eq!(
        p.parse_response(CapabilityKind::Battery, &real).unwrap(),
        Capability::Battery {
            percent: 90,
            charging: false
        }
    );
    // Byte 10 = key scan rate (expoente, fora do perfil); byte 11 = report rate (polling),
    // na codificação da memória. No mouse do mantenedor: `02` = 500 Hz e depois `84` = 2000 Hz,
    // conferido com o app oficial.
    let polling = |status: &[u8]| match p.parse_response(CapabilityKind::PollingRate, status) {
        Ok(Capability::PollingRate { current, .. }) => current,
        outro => panic!("{outro:?}"),
    };
    assert_eq!(polling(&real), 500);
    let mut agora = real;
    agora[11] = 0x84;
    assert_eq!(polling(&agora), 2000);
    assert_eq!(polling(&status), 2000);
    // DPI X do estágio ativo: `40 06` = 1600 no estágio 3, igual ao app oficial.
    let dpi_real = p.parse_response(CapabilityKind::Dpi, &real).unwrap();
    assert!(matches!(
        &dpi_real,
        Capability::Dpi { level, active_stage: Some(3), .. } if level.current == 1600
    ));
    // Escrita: X em 0x0888 + 2 × 3 = 0x088E e Y em 0x08C8 + 6 = 0x08CE, 1700 = `a4 06`.
    let reports = p
        .build_write(&dpi_real, &CapabilityValue::Dpi(1700))
        .unwrap();
    assert_eq!(reports.len(), 2);
    assert_eq!(
        &reports[0].bytes[..10],
        &[0x06, 0xA5, 0xA5, 0x02, 0x8E, 0x08, 0x00, 0x00, 0xA4, 0x06]
    );
    assert_eq!(
        &reports[1].bytes[..10],
        &[0x06, 0xA5, 0xA5, 0x02, 0xCE, 0x08, 0x00, 0x00, 0xA4, 0x06]
    );
    assert!(reports.iter().all(|r| r.bytes.len() == 33));
    // Confirmação de escrita: resposta `06 01 …` (32 bytes) lida do aparelho; `02` (ocupado)
    // e `00` (mouse dormindo) não confirmam.
    let mut ack = [0u8; 32];
    ack[0] = 0x06;
    ack[1] = 0x01;
    assert!(p.check_write_ack(&ack).is_ok());
    for status in [0x00, 0x02] {
        ack[1] = status;
        assert!(matches!(p.check_write_ack(&ack), Err(RaizaError::Io(_))));
    }
    // Sem estágio lido do aparelho, nada é gerado.
    let inicial = p
        .capabilities()
        .into_iter()
        .find(|c| c.kind() == CapabilityKind::Dpi)
        .unwrap();
    assert!(matches!(
        p.build_write(&inicial, &CapabilityValue::Dpi(1700)),
        Err(RaizaError::Io(_))
    ));
    // Estágio fora de 0..7 no status: a resposta é descartada.
    let mut estagio_invalido = real;
    estagio_invalido[2] = 7;
    assert!(
        p.parse_response(CapabilityKind::Dpi, &estagio_invalido)
            .is_err()
    );
    // Só escuta: nada é enviado para ler.
    assert!(p.read_request(CapabilityKind::Battery).is_none());
}

/// Resposta `02 12 01 00 64` (100 %) documentada pelo HaloBattery a partir das capturas.
#[test]
fn akko_s9_le_a_bateria_documentada() {
    let p = perfil_do_repositorio("akko/akko-s9-ultra-36eb-1087.json");
    assert!(!p.verified);
    let pedido = p.read_request(CapabilityKind::Battery).unwrap();
    assert_eq!(&pedido.bytes[..4], &[0x02, 0x12, 0x01, 0x00]);
    assert_eq!(&pedido.bytes[62..], &[0x71, 0x12]);
    let mut resposta = vec![0x02, 0x12, 0x01, 0x00, 0x64];
    resposta.resize(64, 0);
    assert_eq!(
        p.parse_response(CapabilityKind::Battery, &resposta)
            .unwrap(),
        Capability::Battery {
            percent: 100,
            charging: false
        }
    );
    // Byte 3 = 1: o headset não informou o nível (desligado ou longe). A resposta é descartada.
    resposta[3] = 0x01;
    assert!(
        p.parse_response(CapabilityKind::Battery, &resposta)
            .is_err()
    );
    // No cabo (`36eb:0087`, lido do aparelho do mantenedor em 2026-11-28): `bc` = bit 7
    // (carregando) + `3c` (60 %), enquanto o dongle respondia `01` (sem nível).
    resposta[3] = 0x00;
    resposta[4] = 0xbc;
    assert_eq!(
        p.parse_response(CapabilityKind::Battery, &resposta)
            .unwrap(),
        Capability::Battery {
            percent: 60,
            charging: true
        }
    );
    // Cabo é a conexão preferida: com os dois presentes, só ele informa a bateria.
    assert_eq!(p.match_index(0x36eb, 0x0087, Some(3), None, None), Some(0));
    assert_eq!(p.match_index(0x36eb, 0x1087, Some(3), None, None), Some(1));
}

/// Status `0xF7` do receptor 2.4 GHz do teclado, lido do dongle do mantenedor em 2026-11-26:
/// bateria `64` = 100 %, byte 3 `00` = teclado online.
#[test]
fn akko_5087_dongle_le_a_bateria_pelo_receptor() {
    let p = perfil_do_repositorio("akko/akko-5087-v3-he-dongle-3151-5038.json");
    assert!(!p.verified);
    assert_eq!(
        p.match_index(0x3151, 0x5038, Some(2), Some(0xffff), Some(2)),
        Some(0)
    );
    assert_eq!(p.connection(0), Some(raiza_core::ConnectionKind::Wireless));
    // Pedido: `F7`, checksum `0xFF - 0xF7` = `08` no byte 7, 64 bytes, report ID 0.
    let pedido = p.read_request(CapabilityKind::Battery).unwrap();
    assert_eq!(pedido.bytes.len(), 65);
    assert_eq!(&pedido.bytes[..9], &[0x00, 0xF7, 0, 0, 0, 0, 0, 0, 0x08]);
    let mut resposta = [0u8; 65];
    resposta[1..10].copy_from_slice(&[0x00, 0x64, 0x00, 0x00, 0x01, 0x01, 0x01, 0x00, 0x00]);
    assert_eq!(
        p.parse_response(CapabilityKind::Battery, &resposta)
            .unwrap(),
        Capability::Battery {
            percent: 100,
            charging: false
        }
    );
    // Teclado desligado (byte 3 ≠ 0): a resposta é descartada.
    let mut offline = resposta;
    offline[4] = 0x01;
    assert!(p.parse_response(CapabilityKind::Battery, &offline).is_err());
}

/// O campo `stage` só entra na escrita se o estágio é lido do aparelho e `stages` é
/// declarado; o último estágio precisa caber na codificação.
#[test]
fn stage_exige_leitura_e_limite() {
    let arquivo = format!(
        "{}/../../profiles/rapoo/rapoo-vt2-max-24ae-1423.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let base: Value = serde_json::from_str(&std::fs::read_to_string(&arquivo).unwrap()).unwrap();
    let erro = |ajuste: &dyn Fn(&mut Value)| {
        let mut v = base.clone();
        ajuste(&mut v);
        load(&v.to_string()).unwrap_err().to_string()
    };
    let e = erro(&|v| {
        v["capabilities"]["dpi"]["read"]["response"]["fields"]
            .as_object_mut()
            .unwrap()
            .remove("stage");
    });
    assert!(e.contains("stage"), "{e}");
    let e = erro(&|v| {
        v["capabilities"]["dpi"]
            .as_object_mut()
            .unwrap()
            .remove("stages");
    });
    assert!(e.contains("stages"), "{e}");
    let e = erro(&|v| v["capabilities"]["dpi"]["stages"] = json!(0));
    assert!(e.contains("stages"), "{e}");
    let e = erro(&|v| {
        v["capabilities"]["dpi"]["write"][1]["fields"]["stage"]["encoding"] = json!("u8");
    });
    assert!(e.contains("último estágio"), "{e}");
}

/// `write_ack` usa as mesmas regras da resposta do `identify`.
#[test]
fn write_ack_validado() {
    let ok = carregar_com(|v| {
        v["write_ack"] = json!({"type": "input", "report_id": 6, "length": 31,
                                "expect": [{"at": 0, "value": 1}]});
    });
    assert!(ok.is_ok(), "{ok:?}");
    rejeitado(
        carregar_com(|v| {
            v["write_ack"] = json!({"type": "input", "report_id": 6, "length": 31, "expect": []});
        }),
        "confirmam a escrita",
    );
    rejeitado(
        carregar_com(|v| {
            v["write_ack"] = json!({"type": "output", "report_id": 6, "length": 31,
                                    "expect": [{"at": 0, "value": 1}]});
        }),
        "feature ou input",
    );
    rejeitado(
        carregar_com(|v| {
            v["write_ack"] = json!({"type": "input", "report_id": 6, "length": 4,
                                    "expect": [{"at": 9, "value": 1}]});
        }),
        "passa do fim",
    );
}

/// Tabela de estágios e Key Scan Rate (T22). Bytes lidos do mouse do mantenedor em
/// 2026-11-28: `06 A5 A4 0E 88 08 00 00` → `06 01 00 00 00` + 7 estágios u16 LE
/// (400, 800, 950, 1600, 950, 6400, 30000), iguais ao app oficial.
#[test]
fn rapoo_vt2_tabela_de_dpi_e_key_scan() {
    let p = perfil_do_repositorio("rapoo/rapoo-vt2-max-24ae-1423.json");
    let pedido = p.dpi_table_request().unwrap();
    assert_eq!(pedido.report_type, ReportType::Output);
    assert_eq!(pedido.bytes.len(), 33);
    assert_eq!(
        &pedido.bytes[..8],
        &[0x06, 0xA5, 0xA4, 0x0E, 0x88, 0x08, 0x00, 0x00]
    );
    assert!(pedido.bytes[8..].iter().all(|&b| b == 0));

    let mut resposta = [0u8; 32];
    resposta[..21].copy_from_slice(&[
        0x06, 0x01, 0x00, 0x00, 0x00, 0x90, 0x01, 0x20, 0x03, 0xb6, 0x03, 0x40, 0x06, 0xb6, 0x03,
        0x00, 0x19, 0x30, 0x75, 0x06, 0x00,
    ]);
    assert_eq!(
        p.parse_dpi_table(&resposta).unwrap(),
        [400, 800, 950, 1600, 950, 6400, 30000]
    );
    // Ocupado, ou uma confirmação de escrita atrasada (zeros): descartadas.
    let mut ocupado = resposta;
    ocupado[1] = 0x02;
    assert!(p.parse_dpi_table(&ocupado).is_err());
    let mut ack = [0u8; 32];
    ack[0] = 0x06;
    ack[1] = 0x01;
    assert!(p.parse_dpi_table(&ack).is_err());

    // Gravar o estágio 5 (não o em uso): X em 0x0888 + 10 = 0x0892, Y em 0x08D2.
    let mut dpi = p
        .capabilities()
        .into_iter()
        .find(|c| c.kind() == CapabilityKind::Dpi)
        .unwrap();
    let vazio = CapabilityValue::DpiStage {
        stage: 5,
        value: 3200,
    };
    // Tabela desconhecida: nada é gerado.
    assert!(matches!(
        p.build_write(&dpi, &vazio),
        Err(RaizaError::OutOfRange { .. })
    ));
    if let Capability::Dpi {
        stages,
        active_stage,
        ..
    } = &mut dpi
    {
        *stages = p.parse_dpi_table(&resposta).unwrap();
        *active_stage = Some(3);
    }
    let reports = p.build_write(&dpi, &vazio).unwrap();
    assert_eq!(reports.len(), 2);
    assert_eq!(
        &reports[0].bytes[..10],
        &[0x06, 0xA5, 0xA5, 0x02, 0x92, 0x08, 0x00, 0x00, 0x80, 0x0C]
    );
    assert_eq!(
        &reports[1].bytes[..10],
        &[0x06, 0xA5, 0xA5, 0x02, 0xD2, 0x08, 0x00, 0x00, 0x80, 0x0C]
    );
    for ruim in [(7, 800), (0, 20), (0, 805)] {
        let v = CapabilityValue::DpiStage {
            stage: ruim.0,
            value: ruim.1,
        };
        assert!(p.build_write(&dpi, &v).is_err(), "{ruim:?}");
    }

    // Key Scan Rate no byte 10 do status (com o ID): `05` = 4000 Hz, igual ao bloco 0x0881.
    let real = [
        0x07, 0x20, 0x03, 0x40, 0x06, 0x40, 0x06, 0x01, 0x5a, 0x00, 0x05, 0x02, 0x00, 0x23, 0x46,
        0x00, 0x02, 0x01, 0xb3,
    ];
    let scan = p.parse_response(CapabilityKind::ScanRate, &real).unwrap();
    assert_eq!(
        scan,
        Capability::ScanRate {
            current: 4000,
            supported: vec![125, 250, 500, 1000, 2000, 4000, 8000],
            writable: true,
        }
    );
    // 1 byte em 0x0881, testado no aparelho em 2026-11-28 (status e memória conferidos).
    let reports = p
        .build_write(&scan, &CapabilityValue::ScanRate(1000))
        .unwrap();
    assert_eq!(
        &reports[0].bytes[..9],
        &[0x06, 0xA5, 0xA5, 0x01, 0x81, 0x08, 0x00, 0x00, 0x03]
    );
    assert!(reports[0].bytes[9..].iter().all(|&b| b == 0));

    // O perfil do repositório também passa no schema.
    let schema: Value =
        serde_json::from_str(include_str!("../../../profiles/schema.json")).unwrap();
    let validador = jsonschema::draft202012::new(&schema).unwrap();
    let json: Value = serde_json::from_str(include_str!(
        "../../../profiles/rapoo/rapoo-vt2-max-24ae-1423.json"
    ))
    .unwrap();
    assert!(validador.is_valid(&json));
}

#[test]
fn tabela_de_dpi_validada() {
    let rapoo = || -> Value {
        serde_json::from_str(include_str!(
            "../../../profiles/rapoo/rapoo-vt2-max-24ae-1423.json"
        ))
        .unwrap()
    };
    let com = |muda: fn(&mut Value)| {
        let mut v = rapoo();
        muda(&mut v);
        load(&v.to_string())
    };
    let erro = |r: Result<Profile, ProfileError>, trecho: &str| match r {
        Err(e) => assert!(e.to_string().contains(trecho), "{e}"),
        Ok(_) => panic!("deveria recusar ({trecho})"),
    };
    erro(
        com(|v| {
            v["capabilities"]["dpi"]["table"]["response"]["fields"]["value"]["at"] = json!(20);
        }),
        "passam do tamanho",
    );
    erro(
        com(|v| {
            v["capabilities"]["dpi"]["table"]
                .as_object_mut()
                .unwrap()
                .remove("request");
        }),
        "falta request",
    );
    erro(
        com(|v| {
            v["capabilities"]["dpi"]["table"]["response"]["fields"]["stage"] = json!({"at": 1});
        }),
        "stage",
    );
    erro(
        com(|v| {
            v["capabilities"]["dpi"]["table"]["request"]["fields"] = json!({"value": {"at": 3}});
        }),
        "capabilities.dpi.table.request",
    );
}
