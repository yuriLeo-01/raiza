//! Testes ponta a ponta do executor de perfis com o HID falso.

mod common;

use std::time::{Duration, Instant};

use common::*;
use raiza_core::{Capability, CapabilityKind, CapabilityValue, DeviceId, Provider, RaizaError};
use raiza_profiles::Profile;
use raiza_profiles::format::ReportType;
use raiza_provider_hid::{HidProfilesProvider, PROVIDER_ID, Timeouts};

type Prov = HidProfilesProvider<FakeBackend, FakeProfiles>;

const RESPOSTA: Duration = Duration::from_millis(300);

fn provedor(backend: &FakeBackend, perfis: Vec<Profile>) -> (Prov, FakeProfiles) {
    let fonte = FakeProfiles::new(perfis);
    let timeouts = Timeouts {
        response: RESPOSTA,
        spontaneous: Duration::from_millis(500),
        debounce: Duration::from_millis(20),
    };
    (
        HidProfilesProvider::with_parts(backend.clone(), fonte.clone(), timeouts),
        fonte,
    )
}

fn id_cabo() -> DeviceId {
    DeviceId::usb(VID, PID_CABO, "0")
}

fn id_dongle() -> DeviceId {
    DeviceId::usb(VID, PID_DONGLE, "0")
}

fn dpi(cap: &Capability) -> (u32, bool) {
    match cap {
        Capability::Dpi {
            level, writable, ..
        } => (level.current, *writable),
        outra => panic!("esperava DPI, veio {outra:?}"),
    }
}

/// Um mouse no cabo, perfil verificado, e o provedor já com `discover` feito.
async fn mouse_no_cabo(perfil: Profile) -> (Prov, FakeBackend, std::sync::Arc<FakeMouse>) {
    let backend = FakeBackend::default();
    let mouse = FakeMouse::new(PID_CABO, "1-1");
    backend.plug(&mouse);
    let (p, _) = provedor(&backend, vec![perfil]);
    let lista = p.discover().await.unwrap();
    assert_eq!(lista.len(), 1, "{lista:?}");
    (p, backend, mouse)
}

#[tokio::test]
async fn le_dpi() {
    let (p, backend, mouse) = mouse_no_cabo(perfil(|_| {})).await;
    let lista = p.discover().await.unwrap();
    let d = &lista[0];
    assert_eq!(d.id, id_cabo());
    assert_eq!(d.name, "Mouse de Teste T16");
    assert_eq!(d.providers, [PROVIDER_ID]);
    // A leitura inicial já trouxe o DPI do aparelho (32 × 50).
    assert_eq!(
        dpi(d.capability(CapabilityKind::Dpi).unwrap()),
        (1600, true)
    );

    mouse.with_state(|st| st.dpi_raw = 64);
    let cap = p.read(&id_cabo(), CapabilityKind::Dpi).await.unwrap();
    assert_eq!(dpi(&cap), (3200, true));

    // Só a interface de comandos (1, ff00) foi aberta, nunca a do mouse comum.
    assert!(backend.opened().iter().all(|path| path == &mouse.path(1)));
    // O pedido de leitura é a consulta fixa do perfil.
    let pedido = &mouse.sent()[0];
    assert_eq!(pedido.0, ReportType::Output);
    assert_eq!(&pedido.1[..2], &[8, 3]);
}

#[tokio::test]
async fn escreve_dpi() {
    let perfil = perfil(|_| {});
    let (p, _backend, mouse) = mouse_no_cabo(perfil.clone()).await;
    let mut eventos = p.events();

    p.write(&id_cabo(), CapabilityValue::Dpi(800))
        .await
        .unwrap();

    // Exatamente o relatório que a T15 gera, com checksum.
    let atual = perfil
        .capabilities()
        .into_iter()
        .find(|c| c.kind() == CapabilityKind::Dpi)
        .unwrap();
    let esperado = perfil
        .build_write(&atual, &CapabilityValue::Dpi(800))
        .unwrap();
    assert_eq!(mouse.dpi_writes(), [esperado[0].bytes.clone()]);
    assert_eq!(mouse.with_state(|st| st.dpi_raw), 16);

    let e = next_event(&mut eventos, 1000)
        .await
        .expect("CapabilityChanged");
    assert_eq!(resumo(&e), format!("~{}:dpi", id_cabo()));
    let cap = p.read(&id_cabo(), CapabilityKind::Dpi).await.unwrap();
    assert_eq!(dpi(&cap), (800, true));

    // Valor fora do passo: recusado antes de sair qualquer byte.
    let r = p.write(&id_cabo(), CapabilityValue::Dpi(825)).await;
    assert!(matches!(r, Err(RaizaError::OutOfRange { .. })), "{r:?}");
    assert_eq!(mouse.dpi_writes().len(), 1);
}

#[tokio::test]
async fn perfil_nao_verificado_recusa_escrita() {
    let (p, _backend, mouse) = mouse_no_cabo(perfil(|v| v["verified"] = false.into())).await;
    let d = &p.discover().await.unwrap()[0];
    assert!(d.capabilities.iter().all(|c| !c.is_writable()), "{d:?}");
    // A leitura funciona.
    let cap = p.read(&id_cabo(), CapabilityKind::Dpi).await.unwrap();
    assert_eq!(dpi(&cap), (1600, false));

    let r = p.write(&id_cabo(), CapabilityValue::Dpi(800)).await;
    assert_eq!(r, Err(RaizaError::ReadOnly(CapabilityKind::Dpi)));
    let r = p.write(&id_cabo(), CapabilityValue::PollingRate(500)).await;
    assert_eq!(r, Err(RaizaError::ReadOnly(CapabilityKind::PollingRate)));
    assert!(mouse.dpi_writes().is_empty());
    assert!(
        mouse.sent().iter().all(|(_, b)| b[..2] == [8, 3]),
        "só consultas de leitura: {:?}",
        mouse.sent()
    );
}

#[tokio::test]
async fn identificacao_errada_bloqueia_escrita() {
    let backend = FakeBackend::default();
    let mouse = FakeMouse::new(PID_CABO, "1-1");
    mouse.with_state(|st| st.identity = IDENTIDADE_ERRADA.to_vec());
    backend.plug(&mouse);
    let (p, _) = provedor(&backend, vec![perfil(com_identify)]);
    let lista = p.discover().await.unwrap();

    // Anunciado, mas somente leitura.
    assert_eq!(lista.len(), 1);
    assert!(lista[0].capabilities.iter().all(|c| !c.is_writable()));
    let r = p.write(&id_cabo(), CapabilityValue::Dpi(800)).await;
    assert_eq!(r, Err(RaizaError::ReadOnly(CapabilityKind::Dpi)));
    let r = p.write(&id_cabo(), CapabilityValue::PollingRate(500)).await;
    assert_eq!(r, Err(RaizaError::ReadOnly(CapabilityKind::PollingRate)));

    // A consulta de identificação saiu (com o checksum sum8_inv); nenhuma escrita.
    let enviados = mouse.sent();
    assert_eq!(enviados[0].0, ReportType::Feature);
    assert_eq!(enviados[0].1, [0, 0x8f, 0, 0, 0, 0, 0, 0, 0x70]);
    assert!(mouse.dpi_writes().is_empty());
    assert!(
        !enviados
            .iter()
            .any(|(t, b)| *t == ReportType::Feature && b[0] == 6),
        "nenhum feature de escrita: {enviados:?}"
    );
}

#[tokio::test]
async fn identificacao_certa_libera_escrita() {
    let backend = FakeBackend::default();
    let mouse = FakeMouse::new(PID_CABO, "1-1");
    backend.plug(&mouse);
    let (p, _) = provedor(&backend, vec![perfil(com_identify)]);
    let lista = p.discover().await.unwrap();
    assert_eq!(
        dpi(lista[0].capability(CapabilityKind::Dpi).unwrap()),
        (1600, true)
    );

    p.write(&id_cabo(), CapabilityValue::Dpi(800))
        .await
        .unwrap();
    assert_eq!(mouse.dpi_writes().len(), 1);
    // Polling vai como SET_FEATURE.
    p.write(&id_cabo(), CapabilityValue::PollingRate(500))
        .await
        .unwrap();
    assert!(
        mouse
            .sent()
            .iter()
            .any(|(t, b)| *t == ReportType::Feature && b[..4] == [6, 9, 1, 2])
    );
}

#[tokio::test]
async fn identificacao_se_repete_na_reconexao() {
    let backend = FakeBackend::default();
    let mouse = FakeMouse::new(PID_CABO, "1-1");
    backend.plug(&mouse);
    let (p, _) = provedor(&backend, vec![perfil(com_identify)]);
    p.discover().await.unwrap();
    let mut eventos = p.events();

    backend.unplug(&mouse);
    assert_eq!(
        next_event(&mut eventos, 2000).await.map(|e| resumo(&e)),
        Some(format!("-{}", id_cabo()))
    );
    // Volta, mas agora responde como outro aparelho.
    let outro = FakeMouse::new(PID_CABO, "1-1");
    outro.with_state(|st| st.identity = IDENTIDADE_ERRADA.to_vec());
    backend.plug(&outro);
    assert_eq!(
        next_event(&mut eventos, 2000).await.map(|e| resumo(&e)),
        Some(format!("+{}", id_cabo()))
    );
    let r = p.write(&id_cabo(), CapabilityValue::Dpi(800)).await;
    assert_eq!(r, Err(RaizaError::ReadOnly(CapabilityKind::Dpi)));
    assert!(outro.dpi_writes().is_empty());
}

#[tokio::test]
async fn ocupado_depois_pronto() {
    let backend = FakeBackend::default();
    let mouse = FakeMouse::new(PID_CABO, "1-1");
    mouse.with_state(|st| st.busy = 3);
    backend.plug(&mouse);
    let (p, _) = provedor(&backend, vec![perfil(|_| {})]);
    p.discover().await.unwrap();

    mouse.with_state(|st| st.dpi_raw = 20);
    let cap = p.read(&id_cabo(), CapabilityKind::Dpi).await.unwrap();
    assert_eq!(dpi(&cap), (1000, true));
}

#[tokio::test]
async fn so_ocupado_da_erro_depois_do_timeout() {
    let backend = FakeBackend::default();
    let mouse = FakeMouse::new(PID_CABO, "1-1");
    mouse.with_state(|st| st.busy = SEMPRE_OCUPADO);
    backend.plug(&mouse);
    let (p, _) = provedor(&backend, vec![perfil(|_| {})]);
    // A leitura inicial falha, mas o aparelho continua listado.
    assert_eq!(p.discover().await.unwrap().len(), 1);

    let inicio = Instant::now();
    let r = p.read(&id_cabo(), CapabilityKind::Dpi).await;
    let passou = inicio.elapsed();
    assert!(matches!(r, Err(RaizaError::Io(_))), "{r:?}");
    assert!(passou >= RESPOSTA, "desistiu cedo demais: {passou:?}");
    assert!(passou < RESPOSTA * 3, "demorou demais: {passou:?}");

    // O aparelho se recupera: a próxima leitura funciona.
    mouse.with_state(|st| st.busy = 1);
    let cap = p.read(&id_cabo(), CapabilityKind::Dpi).await.unwrap();
    assert_eq!(dpi(&cap).0, 1600);
}

#[tokio::test]
async fn bateria_espontanea_gera_capability_changed() {
    let (p, _backend, mouse) = mouse_no_cabo(perfil(|_| {})).await;
    let mut eventos = p.events();

    mouse.push_input(&[7, 0x20, 85, 0x80, 0, 0, 0, 0, 0]);
    let e = next_event(&mut eventos, 2000).await.expect("evento");
    let raiza_core::ProviderEvent::CapabilityChanged { id, capability } = e else {
        panic!("esperava CapabilityChanged: {e:?}");
    };
    assert_eq!(id, id_cabo());
    assert_eq!(
        capability,
        Capability::Battery {
            percent: 85,
            charging: true
        }
    );

    // O mesmo valor de novo não gera evento; outro relatório (DPI) também não.
    mouse.push_input(&[7, 0x20, 85, 0x80, 0, 0, 0, 0, 0]);
    mouse.push_input(&[9, 1, 2, 3]);
    assert!(next_event(&mut eventos, 200).await.is_none());

    // Nova leitura: o último valor espontâneo, sem esperar outro relatório.
    let cap = p.read(&id_cabo(), CapabilityKind::Battery).await.unwrap();
    assert_eq!(
        cap,
        Capability::Battery {
            percent: 85,
            charging: true
        }
    );

    mouse.push_input(&[7, 0x20, 84, 0, 0, 0, 0, 0, 0]);
    let e = next_event(&mut eventos, 2000).await.expect("evento");
    assert!(
        matches!(
            e,
            raiza_core::ProviderEvent::CapabilityChanged {
                capability: Capability::Battery {
                    percent: 84,
                    charging: false
                },
                ..
            }
        ),
        "{e:?}"
    );
}

#[tokio::test]
async fn cabo_e_dongle_juntos() {
    let backend = FakeBackend::default();
    let cabo = FakeMouse::new(PID_CABO, "1-1");
    let dongle = FakeMouse::new(PID_DONGLE, "1-2");
    backend.plug(&dongle);
    backend.plug(&cabo);
    let (p, _) = provedor(&backend, vec![perfil(|_| {})]);

    // Só o cabo (primeira entrada do `match`) aparece.
    let ids: Vec<DeviceId> = p
        .discover()
        .await
        .unwrap()
        .into_iter()
        .map(|d| d.id)
        .collect();
    assert_eq!(ids, [id_cabo()]);
    assert!(dongle.sent().is_empty(), "o dongle nem foi consultado");
    let r = p.read(&id_dongle(), CapabilityKind::Dpi).await;
    assert_eq!(r, Err(RaizaError::NotFound(id_dongle())));

    // Desplugar o cabo: as capacidades passam para o dongle.
    let mut eventos = p.events();
    backend.unplug(&cabo);
    let a = next_event(&mut eventos, 2000).await.map(|e| resumo(&e));
    let b = next_event(&mut eventos, 2000).await.map(|e| resumo(&e));
    assert_eq!(a, Some(format!("-{}", id_cabo())));
    assert_eq!(b, Some(format!("+{}", id_dongle())));
    let cap = p.read(&id_dongle(), CapabilityKind::Dpi).await.unwrap();
    assert_eq!(dpi(&cap).0, 1600);

    // Plugar o cabo de novo: voltam para ele.
    let cabo = FakeMouse::new(PID_CABO, "1-1");
    backend.plug(&cabo);
    let a = next_event(&mut eventos, 2000).await.map(|e| resumo(&e));
    let b = next_event(&mut eventos, 2000).await.map(|e| resumo(&e));
    assert_eq!(a, Some(format!("-{}", id_dongle())));
    assert_eq!(b, Some(format!("+{}", id_cabo())));
}

#[tokio::test]
async fn sem_permissao_continua_com_os_outros() {
    let backend = FakeBackend::default();
    let cabo = FakeMouse::new(PID_CABO, "1-1");
    let dongle = FakeMouse::new(PID_DONGLE, "1-2");
    backend.plug(&cabo);
    backend.plug(&dongle);
    backend.deny(&cabo.path(1));
    let (p, _) = provedor(&backend, vec![perfil(|_| {})]);

    // O cabo não abre; o dongle assume.
    let ids: Vec<DeviceId> = p
        .discover()
        .await
        .unwrap()
        .into_iter()
        .map(|d| d.id)
        .collect();
    assert_eq!(ids, [id_dongle()]);
    p.read(&id_dongle(), CapabilityKind::Dpi).await.unwrap();
}

#[tokio::test]
async fn recarrega_perfis() {
    let backend = FakeBackend::default();
    let mouse = FakeMouse::new(PID_CABO, "1-1");
    backend.plug(&mouse);
    let (p, fonte) = provedor(&backend, vec![]);
    assert!(p.discover().await.unwrap().is_empty());
    let mut eventos = p.events();

    fonte.set(vec![perfil(|v| v["verified"] = false.into())]);
    let e = next_event(&mut eventos, 2000).await.expect("DeviceAdded");
    assert_eq!(resumo(&e), format!("+{}", id_cabo()));

    // Perfil alterado (agora verificado): o aparelho é reanunciado.
    fonte.set(vec![perfil(|_| {})]);
    let a = next_event(&mut eventos, 2000).await.map(|e| resumo(&e));
    let b = next_event(&mut eventos, 2000).await;
    assert_eq!(a, Some(format!("-{}", id_cabo())));
    let Some(raiza_core::ProviderEvent::DeviceAdded { device }) = b else {
        panic!("esperava DeviceAdded: {b:?}");
    };
    assert!(
        device
            .capability(CapabilityKind::Dpi)
            .unwrap()
            .is_writable()
    );
}

#[tokio::test]
async fn aparelho_desconhecido() {
    let (p, _backend, _mouse) = mouse_no_cabo(perfil(|_| {})).await;
    let outro = DeviceId::usb(0xdead, 0xbeef, "0");
    assert_eq!(
        p.read(&outro, CapabilityKind::Dpi).await,
        Err(RaizaError::NotFound(outro.clone()))
    );
    assert_eq!(
        p.write(&outro, CapabilityValue::Dpi(800)).await,
        Err(RaizaError::NotFound(outro))
    );
    assert_eq!(
        p.read(&id_cabo(), CapabilityKind::Volume).await,
        Err(RaizaError::Unsupported(CapabilityKind::Volume))
    );
}
