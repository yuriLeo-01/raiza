//! Carga de perfis do disco: inválidos ficam de fora, os outros continuam.

use std::time::Duration;

use futures_util::StreamExt;
use raiza_provider_hid::{DirProfiles, ProfileSource, load_dir};

const MOUSE_JSON: &str = include_str!("fixtures/mouse.json");

#[test]
fn invalido_nao_derruba_os_outros() {
    let sistema = tempfile::tempdir().unwrap();
    let usuario = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(sistema.path().join("ficticia")).unwrap();
    std::fs::write(sistema.path().join("ficticia/mouse.json"), MOUSE_JSON).unwrap();
    std::fs::write(sistema.path().join("ficticia/quebrado.json"), "{ nada").unwrap();
    // `max - min` fora do passo: a validação da T15 recusa.
    let invalido = MOUSE_JSON.replace(r#""max": 16000"#, r#""max": 16010"#);
    assert_ne!(invalido, MOUSE_JSON);
    std::fs::write(sistema.path().join("ficticia/invalido.json"), invalido).unwrap();
    // O JSON Schema não é perfil.
    std::fs::write(sistema.path().join("schema.json"), "{}").unwrap();
    std::fs::write(sistema.path().join("leia-me.txt"), "x").unwrap();
    std::fs::write(
        usuario.path().join("meu.json"),
        MOUSE_JSON.replace("Mouse de Teste T16", "Meu Mouse"),
    )
    .unwrap();

    let lidos = load_dir(sistema.path());
    assert_eq!(lidos.len(), 3, "{lidos:?}");
    assert_eq!(lidos.iter().filter(|(_, r)| r.is_ok()).count(), 1);

    let ausente = sistema.path().join("nao-existe");
    let fonte = DirProfiles::new([
        usuario.path().to_path_buf(),
        ausente,
        sistema.path().to_path_buf(),
    ]);
    let nomes: Vec<String> = fonte.load().into_iter().map(|p| p.name).collect();
    // O diretório do usuário vem primeiro (tem prioridade).
    assert_eq!(nomes, ["Meu Mouse", "Mouse de Teste T16"]);
}

#[tokio::test]
async fn vigia_mudancas() {
    let dir = tempfile::tempdir().unwrap();
    let fonte = DirProfiles::new([dir.path().to_path_buf()]);
    let mut mudancas = fonte.watch().unwrap();
    std::fs::write(dir.path().join("novo.json"), MOUSE_JSON).unwrap();
    let veio = tokio::time::timeout(Duration::from_secs(5), mudancas.next()).await;
    assert_eq!(veio, Ok(Some(())));
    assert_eq!(fonte.load().len(), 1);
}

#[test]
fn diretorios_padrao() {
    let dirs = DirProfiles::default_dirs();
    assert!(
        dirs.iter().all(|d| d.ends_with("raiza/profiles")),
        "{dirs:?}"
    );
}
