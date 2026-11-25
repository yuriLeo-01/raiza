# T16: Executor de perfis via hidraw

- **Fase:** F4
- **Agente sugerido:** claude
- **Depende de:** T15, T04
- **Escopo:** `crates/raiza-provider-hid/**`

## Objetivo
Um provedor que carrega os perfis de `profiles/` e de `~/.local/share/raiza/profiles/`, encontra os dispositivos correspondentes em `/dev/hidraw*` e executa as leituras e escritas descritas.

## O que fazer
1. `hidapi` (backend hidraw). Abrir a interface certa (usage page) entre as várias do aparelho.
2. Toda escrita passa pela validação da T15 imediatamente antes de enviar (pilar 1).
3. Timeout e repetição limitada nas leituras. Falha deixa o dispositivo em estado de erro, sem travar o daemon.
4. Recarregar perfis quando os arquivos mudam (`notify`).
5. Backend HID injetável e testes com um dispositivo falso que responde como o perfil de exemplo.

## Critérios de aceite
- [ ] Testes ponta a ponta com o HID falso (ler DPI, escrever DPI, recusar escrita com perfil não verificado)
- [ ] Sem `unsafe`
- [ ] `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings` e `cargo test` passam
