# T02: Especificação da API D-Bus

- **Fase:** F0
- **Agente sugerido:** claude (líder)
- **Depende de:** T01
- **Escopo:** `docs/DBUS.md`, `crates/raiza-core/src/dbus.rs` (proxy/tipos compartilhados)

## Objetivo
Contrato estável entre o daemon e qualquer cliente (GTK, CLI, KDE futuro).

## O que fazer
1. `docs/DBUS.md`: serviço `io.github.yurileo01.Raiza.Daemon`, objeto `/io/github/yurileo01/Raiza` e interface `io.github.yurileo01.Raiza1` (versionada).
2. Métodos `ListDevices`, `GetDevice(id)`, `SetCapability(id, value)` e `Version`. Sinais `DeviceAdded`, `DeviceRemoved`, `CapabilityChanged`.
3. Escolher o formato no fio: tipos D-Bus nativos (`a{sv}`) **ou** JSON em string. Justificar no documento. Recomendação: tipos nativos com `zvariant`.
4. Em `raiza-core`, atrás da feature `dbus`: o `#[zbus::proxy]` do cliente, para o GTK e o CLI reusarem.
5. Mapeamento de `RaizaError` para erros D-Bus nomeados (`io.github.yurileo01.Raiza1.Error.ReadOnly` etc.).

## Critérios de aceite
- [x] `docs/DBUS.md` tem exemplos de chamada com `busctl --user`
- [x] O proxy compila com `--features dbus`
- [x] `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings` e `cargo test` passam
