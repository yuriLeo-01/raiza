# T04: Esqueleto do `raizad`

- **Fase:** F1
- **Agente sugerido:** claude
- **Depende de:** T01, T02
- **Escopo:** `crates/raizad/**`

## Objetivo
Um daemon que sobe no barramento de sessão, registra provedores, funde dispositivos e expõe a API de `docs/DBUS.md`.

## O que fazer
1. `tokio` + `zbus` + `tracing-subscriber` (nível por `RAIZA_LOG`).
2. Registro de provedores (`Vec<Box<dyn Provider>>`), cada um isolado: erro ou pânico em um provedor é logado e não derruba os outros (pilar 1).
3. Estado em memória dos dispositivos, fundidos com a função do core, e repasse de `ProviderEvent` como sinais D-Bus.
4. `SetCapability` valida com o core antes de chamar o provedor.
5. Persistência simples em `~/.config/raiza/config.json` (último valor escrito por dispositivo) e reaplicação quando o dispositivo reaparece.
6. Por enquanto só registra o `MockProvider`, atrás da flag `--mock`. Os provedores reais entram quando forem integrados.
7. Desligamento limpo com SIGTERM/SIGINT.

## Critérios de aceite
- [ ] `cargo run -p raizad -- --mock` e `busctl --user call io.github.yurileo01.Raiza /io/github/yurileo01/Raiza io.github.yurileo01.Raiza1 ListDevices` devolvem os dispositivos mock
- [ ] Teste de integração com um barramento D-Bus privado (`dbus-daemon --session` ou `zbus` p2p)
- [ ] Teste: provedor que entra em pânico não derruba o daemon
- [ ] `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings` e `cargo test` passam
