# T16: Executor de perfis via hidraw

- **Fase:** F4
- **Agente sugerido:** claude
- **Depende de:** T15, T04
- **Escopo:** `crates/raiza-provider-hid/**`

## Objetivo
Um provedor que carrega os perfis de `profiles/` e de `~/.local/share/raiza/profiles/`, encontra os dispositivos correspondentes em `/dev/hidraw*` e executa as leituras e escritas descritas.

## Contexto
- Formato e API em `docs/PERFIS.md` e no crate `raiza-profiles` (puro, sem I/O): `load`, `Profile::match_index`, `capabilities`, `identify_request`, `check_identity`, `read_request`, `response_spec`, `parse_response` e `build_write`. Este provedor faz o I/O.
- Id do provedor: `"hid-perfis"`, o de **maior prioridade** (`docs/ARQUITETURA.md`). Ids pela regra de `docs/ARQUITETURA.md`: `DeviceId::usb_instance` a partir do dispositivo USB pai do hidraw.
- **Um aparelho, duas conexões:** um perfil pode ter `match` em lista (ex.: mouse no dongle e no cabo). Cada conexão tem o próprio `DeviceId`. Se duas conexões do **mesmo perfil** estão presentes ao mesmo tempo, anuncie as capacidades só na de menor `match_index` (a preferida). A outra não aparece por este provedor (continua aparecendo pelo `udev`).
- **Identificação (pilar 1):** se o perfil tem `identify`, mande `identify_request()` ao abrir o aparelho e passe a resposta a `check_identity()`. **Sem identificação confirmada, nenhuma escrita sai**: anuncie todas as capacidades com `writable: false` e recuse `write` com `RaizaError::ReadOnly`. Repita a identificação a cada reconexão.
- **Perfil `"verified": false`:** só leitura (o `build_write` já recusa; mesmo assim, não chame).
- **Tipos de relatório** (`HidReport.report_type`): `feature` → SET_FEATURE (`send_feature_report`) ao enviar e GET_FEATURE (`get_feature_report`, com o report ID no primeiro byte) ao ler; `output` → `write`; resposta `input` → ler o próximo relatório de entrada com aquele report ID.
- **Respostas que não batem:** `parse_response` devolve `RaizaError::Io` quando o report ID, o tamanho ou os bytes de `expect` não conferem. Trate como "ainda não pronto": repita a leitura algumas vezes, com timeout total curto (ex.: até 500 ms). Exemplo real: o Rapoo VT2 responde `02` = ocupado antes de `01` = pronto (`docs/tarefas/notas/T17.md`).
- **Relatórios espontâneos:** uma capacidade cujo `read` não tem `request` e cuja resposta é `input` é um relatório que o aparelho manda sozinho (ex.: status `0x07` do Rapoo, com bateria, a cada ~3 s). Escute-os sem polling e emita `CapabilityChanged` quando o valor mudar.
- **Permissão:** o acesso a `/dev/hidraw*` vem da regra udev do projeto (`packaging/udev/70-raiza.rules`). Sem permissão, `RaizaError::Permission` com a dica de instalar a regra, e o provedor continua com os outros aparelhos.
- Ainda **não há perfis reais** em `profiles/` (as tarefas de protocolo aguardam capturas). Teste com perfis fictícios, como `crates/raiza-profiles/tests/fixtures/ficticio.json`.

## O que fazer
1. `hidapi` (backend hidraw). Abrir a interface certa (número de interface, usage page/usage do `match`) entre as várias do aparelho.
2. Toda escrita passa pela validação da T15 imediatamente antes de enviar (pilar 1): `build_write` com o estado mais recente.
3. Timeout e repetição limitada nas leituras. Falha deixa o dispositivo em estado de erro, sem travar o daemon.
4. Hotplug: acompanhar `/dev/hidraw*` (monitor do udev, sem polling).
5. Recarregar perfis quando os arquivos mudam (`notify`). Perfil inválido: log com o erro de validação, e os outros continuam.
6. Backend HID injetável e testes com um dispositivo falso que responde como o perfil de exemplo.

## Fora de escopo
Escrever perfis reais (T17, T18, T20). Registrar o provedor no `raizad` (o líder faz na integração).

## Critérios de aceite
- [x] Testes ponta a ponta com o HID falso: ler DPI, escrever DPI, recusar escrita com perfil não verificado
- [x] Teste: perfil com `identify` e resposta errada → nenhum relatório de escrita chega ao HID falso; resposta certa → escrita sai
- [x] Teste: resposta "ocupado" seguida de "pronto" é lida com sucesso; só "ocupado" dá erro depois do timeout
- [x] Teste: relatório espontâneo de bateria gera `CapabilityChanged`
- [x] Teste: cabo e dongle do mesmo perfil presentes juntos → capacidades só no cabo; desplugar o cabo → passam para o dongle
- [x] Sem `unsafe`
- [x] `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings` e `cargo test --all-features` passam
