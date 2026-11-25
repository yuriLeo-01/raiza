# T06: `raiza-ctl` (CLI cliente)

- **Fase:** F1
- **Agente sugerido:** claude
- **Depende de:** T02
- **Escopo:** `crates/raiza-ctl/**`

## Objetivo
CLI para usar e depurar o daemon: `raiza-ctl list`, `raiza-ctl get <id>`, `raiza-ctl set <id> <capacidade> <valor>`, `raiza-ctl watch`.

## O que fazer
1. `clap` (derive), com o proxy D-Bus do `raiza-core` (feature `dbus`).
2. Saída em tabela legível e `--json` para scripts.
3. `watch` mostra os sinais em tempo real.
4. Mensagem clara quando o daemon não está rodando ("inicie com: systemctl --user start raizad").
5. Completions de shell (bash, zsh, fish) com `clap_complete`.

## Critérios de aceite
- [ ] Testes de parsing dos argumentos e da formatação de saída (sem daemon)
- [ ] Com `raizad --mock` rodando, `list`, `get`, `set` e `watch` funcionam (descreva nas notas)
- [ ] `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings` e `cargo test` passam
