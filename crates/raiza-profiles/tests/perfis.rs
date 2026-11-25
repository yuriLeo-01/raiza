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
