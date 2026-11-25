# T06: `raiza-ctl` (CLI cliente)

- **Fase:** F1
- **Agente sugerido:** sonnet
- **Depende de:** T02
- **Escopo:** `crates/raiza-ctl/**`

## Objetivo
CLI para usar e depurar o daemon: `raiza-ctl list`, `raiza-ctl get <id>`, `raiza-ctl set <id> <capacidade> <valor…>`, `raiza-ctl watch`.

## Contexto
- A API está em `docs/DBUS.md`. Use o cliente tipado `raiza_core::dbus::RaizaClient` (feature `dbus` do `raiza-core`): `connect_session`, `list_devices`, `get_device`, `set_capability`, `version` e `events`. Erros chegam como `ClientError::Raiza(RaizaError)` já tipados.
- O daemon (`raizad`, T04) está sendo escrito **em paralelo**: não mexa em `crates/raizad`. Para testar sem daemon, monte um servidor falso por conexão D-Bus ponto a ponto, como em `raiza_core::dbus::tests::cliente_e_servidor_pelo_fio` (`crates/raiza-core/src/dbus.rs`). Para isso, o `zbus` com a feature `p2p` vai em `[dev-dependencies]` do seu crate.
- Ids de dispositivo são strings como `usb:24ae:1423:0` e `bt:DE:1E:59:99:7C:AE` (`DeviceId` implementa `FromStr`).

## O que fazer
1. `clap` (derive) + `tokio` + `anyhow` (é um binário).
2. Sintaxe do `set`, um subcomando por capacidade, gerando o `CapabilityValue` correspondente:
   - `set <id> dpi 1600`
   - `set <id> polling-rate 1000`
   - `set <id> volume 40`
   - `set <id> mute on|off`
   - `set <id> connection on|off` (conectar/desconectar)
   - `set <id> rgb [--zone N] [--mode NOME] [--color '#rrggbb']` (pelo menos `--mode` ou `--color`)
   - `set <id> rumble <intensidade 0-100> <duração-ms>`
3. `list`: tabela legível (id, tipo, nome, provedores e um resumo das capacidades, ex.: `bateria 80%`, `DPI 1600`). `get <id>`: todas as capacidades com valor, limites e se aceitam escrita. `--json` em `list`, `get` e `watch` imprime o JSON do daemon, para scripts.
4. `watch`: um evento por linha em tempo real (`+ id nome`, `- id`, `~ id capacidade valor`).
5. Erros claros: daemon fora do ar → "o daemon não está rodando; inicie com: systemctl --user start raizad"; `ReadOnly`, `OutOfRange` e `NotFound` com mensagens em português. Código de saída ≠ 0 em erro.
6. `raiza-ctl completions <bash|zsh|fish>` com `clap_complete`.
7. Separe a lógica (parsing → `CapabilityValue`, formatação) do I/O, para testar sem D-Bus.

## Critérios de aceite
- [ ] Testes de parsing de cada forma do `set` (válidas e inválidas) e da formatação de `list`/`get`/`watch`
- [ ] Teste ponta a ponta com servidor D-Bus falso (p2p): `list`, `get`, `set` (sucesso e `OutOfRange`) e um evento de `watch`
- [ ] `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings` e `cargo test --all-features` passam

## Como testar com hardware real (para o mantenedor)
Depois da T04: `systemctl --user start raizad` (ou `cargo run -p raizad`), `cargo run -p raiza-ctl -- list`, e `set` de volume no fifine: `cargo run -p raiza-ctl -- set usb:3142:a010:0 volume 50`.
