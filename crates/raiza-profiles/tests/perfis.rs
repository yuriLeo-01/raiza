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
/// bateria 0x53 = 83 %, `01` = na bateria, polling `84` = 2000 Hz.
#[test]
fn rapoo_vt2_le_o_status_documentado() {
    let p = perfil_do_repositorio("rapoo/rapoo-vt2-max-24ae-1423.json");
    assert!(!p.verified);
    assert!(p.capabilities().iter().all(|c| !c.is_writable()));
    assert_eq!(p.match_index(0x24ae, 0x1423, Some(1), None, None), Some(0));
    assert_eq!(p.match_index(0x24ae, 0x4623, Some(1), None, None), Some(1));
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
    assert!(matches!(
        p.parse_response(CapabilityKind::PollingRate, &status)
            .unwrap(),
        Capability::PollingRate { current: 2000, .. }
    ));
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
}
