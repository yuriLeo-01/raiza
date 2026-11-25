# T04: Esqueleto do `raizad`

- **Fase:** F1
- **Agente sugerido:** claude
- **Depende de:** T01, T02
- **Escopo:** `crates/raizad/**`

## Objetivo
Um daemon que sobe no barramento de sessão, registra provedores, funde dispositivos e expõe a API de `docs/DBUS.md`.

## Contexto
- Contrato da API em `docs/DBUS.md`. Use `raiza_core::dbus` (feature `dbus`): `DbusError`, `parse_id`, `from_wire`, `to_wire`. O teste `dbus::tests::cliente_e_servidor_pelo_fio` do `raiza-core` mostra um servidor mínimo.
- Fusão e roteamento com `raiza_core::fuse` e `raiza_core::capability_owner`.

## O que fazer
1. `tokio` + `zbus` + `tracing-subscriber` (nível por `RAIZA_LOG`).
2. Registro de provedores (`Vec<Box<dyn Provider>>`), cada um isolado: erro ou pânico em um provedor é logado e não derruba os outros (pilar 1).
3. Estado em memória dos dispositivos, fundidos com a função do core, e repasse de `ProviderEvent` como sinais D-Bus.
4. `SetCapability` valida com o core antes de chamar o provedor.
5. Persistência simples em `~/.config/raiza/config.json` (último valor escrito por dispositivo) e reaplicação quando o dispositivo reaparece.
6. Registrar os provedores reais já integrados, na ordem de prioridade de `docs/ARQUITETURA.md`: `raiza_provider_bluez::BluezProvider` (`bluez`), `raiza_provider_bluez::UpowerProvider` (`upower`) e `raiza_provider_udev::UdevProvider` (`udev`, o último). Com a flag `--mock`, registrar só o `MockProvider` (feature `mock` do `raiza-core`).
7. Desligamento limpo com SIGTERM/SIGINT.
8. **Atualização de dispositivo:** um provedor pode mandar `DeviceRemoved` seguido de `DeviceAdded` com o mesmo id (o `udev` faz isso quando o tipo de um aparelho muda). Trate o par como atualização do `DeviceInfo` daquele provedor: não apague a configuração salva e não reaplique valores por causa disso. Veja `docs/ARQUITETURA.md`, "Atualização de um dispositivo".

## Critérios de aceite
- [x] `cargo run -p raizad -- --mock` e `busctl --user call io.github.yurileo01.Raiza /io/github/yurileo01/Raiza io.github.yurileo01.Raiza1 ListDevices` devolvem os dispositivos mock
- [x] Teste de integração com um barramento D-Bus privado (`dbus-daemon --session` ou `zbus` p2p)
- [x] Teste: provedor que entra em pânico não derruba o daemon
- [x] Teste: `DeviceRemoved` + `DeviceAdded` do mesmo id preserva a configuração salva
- [x] Teste: `DeviceAdded` sozinho para um id já anunciado pelo mesmo provedor substitui o `DeviceInfo` (sem duplicar) (ajuste enviado em 2026-11-25)
- [x] `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings` e `cargo test --all-features` passam
