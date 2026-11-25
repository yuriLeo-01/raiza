# T03: CI no GitHub Actions

- **Fase:** F0
- **Agente sugerido:** hydrogen
- **Depende de:** —
- **Escopo:** `.github/workflows/**`, `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`

## Objetivo
Todo push e todo PR rodam formatação, lint e testes.

## O que fazer
1. Workflow `ci.yml` em `ubuntu-latest`: instalar `libudev-dev libhidapi-dev libdbus-1-dev libgtk-4-dev libadwaita-1-dev pkg-config` e rodar `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings` e `cargo test`.
2. Cache com `Swatinem/rust-cache`.
3. `rust-toolchain.toml` em stable com `rustfmt` e `clippy`.
4. Job separado (pode falhar sem bloquear) que roda `cargo deny` ou `cargo audit`.

## Critérios de aceite
- [x] O YAML é válido (`actionlint`, se disponível, ou revisão manual)
- [x] Os comandos do workflow rodam localmente sem erro
- [x] `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings` e `cargo test` passam
