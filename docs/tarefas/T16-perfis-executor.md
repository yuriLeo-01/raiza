# T16: Executor de perfis via hidraw

- **Fase:** F4
- **Agente sugerido:** claude
- **Depende de:** T15, T04
- **Escopo:** `crates/raiza-provider-hid/**`

## Objetivo
Um provedor que carrega os perfis de `profiles/` e de `~/.local/share/raiza/profiles/`, encontra os dispositivos correspondentes em `/dev/hidraw*` e executa as leituras e escritas descritas.

## Contexto
- Formato e API em `docs/PERFIS.md` e no crate `raiza-profiles`: `load`, `Profile::match_index`, `capabilities`, `read_request`, `response_spec`, `parse_response` e `build_write`. O crate não faz I/O; este provedor faz.
- Id do provedor: `"hid-perfis"`. Ids pela regra de `docs/ARQUITETURA.md` (`DeviceId::usb_instance` do dispositivo USB pai).
- **Um aparelho, duas conexões:** um perfil pode ter `match` em lista (ex.: teclado com cabo e com dongle). Cada conexão tem o próprio `DeviceId`. Se duas conexões do **mesmo perfil** estão presentes ao mesmo tempo, anuncie as capacidades só na de menor `match_index` (a preferida). A outra não aparece por este provedor (continua aparecendo pelo `udev`).

## O que fazer
1. `hidapi` (backend hidraw). Abrir a interface certa (usage page) entre as várias do aparelho.
2. Toda escrita passa pela validação da T15 imediatamente antes de enviar (pilar 1).
3. Timeout e repetição limitada nas leituras. Falha deixa o dispositivo em estado de erro, sem travar o daemon.
4. Recarregar perfis quando os arquivos mudam (`notify`).
5. Backend HID injetável e testes com um dispositivo falso que responde como o perfil de exemplo.

## Critérios de aceite
- [ ] Testes ponta a ponta com o HID falso (ler DPI, escrever DPI, recusar escrita com perfil não verificado)
- [ ] Teste: cabo e dongle do mesmo perfil presentes juntos → capacidades só no cabo; desplugar o cabo → passam para o dongle
- [ ] Sem `unsafe`
- [ ] `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings` e `cargo test --all-features` passam
