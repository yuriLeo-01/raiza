# T10: Provedor DDC/CI (monitores)

- **Fase:** F2
- **Agente sugerido:** hydrogen
- **Depende de:** T01
- **Escopo:** `crates/raiza-provider-ddc/**`

## Objetivo
Brilho e contraste de monitores externos: as capacidades `Brightness` e `Contrast`.

## Contexto
Crate `ddc-hi` (ou `ddc-i2c`) sobre `/dev/i2c-*`. Precisa do módulo `i2c-dev` e de permissão: a regra udev do projeto libera só os barramentos de controladoras de vídeo. Monitor real: LG 27UP850 (que também expõe "LG Monitor Controls" via USB/HID, mas use DDC). DDC é lento (cerca de 50 ms por comando, às vezes falha e precisa de nova tentativa): aplique debounce nas escritas e use cache nas leituras.

## Contrato com o `raiza-core` (vale para todo provedor)
- Crate novo em `crates/raiza-provider-<nome>/`, com `raiza-core = { workspace = true }`. Para testes, use `raiza-core = { workspace = true, features = ["mock"] }` em `[dev-dependencies]` se precisar do `MockProvider`.
- Implemente `raiza_core::Provider` (com `#[async_trait::async_trait]`). Runtime: `tokio`. Logs: `tracing`. Erros: `RaizaError`.
- **Identidade**: siga a tabela "Identidade" de `docs/ARQUITETURA.md`. Para USB, use **sempre** `DeviceId::usb_instance(serial, porta, portas_do_mesmo_modelo_sem_serial)`. Para Bluetooth, use `DeviceId::bluetooth(mac)`. Sem isso a fusão com os outros provedores falha.
- `DeviceInfo.providers` = `vec!["<id do provedor>".into()]`.
- Em `write`, valide de novo o valor com `CapabilityValue::validate` contra o estado mais recente antes de tocar no hardware (pilar 1).
- Um dispositivo com problema vira `Err` só para ele: o provedor continua atendendo os outros. Serviço do sistema ausente → lista vazia e log em nível `info`, nunca pânico.
- Commite o `Cargo.lock` normalmente. Conflitos nele o líder resolve no merge.

**Específico deste provedor:**
- Id do provedor: `"ddc"`. `kind = Monitor`.
- Id: `DeviceId::display(fabricante_pnp, código_produto, serial)` a partir do EDID.
- `Brightness`/`Contrast { level: Level { current, min: 0, max: <máximo do VCP>, step: 1 }, writable: true }`.
- **Pilar 1**: o provedor só escreve os VCP 0x10 (brilho) e 0x12 (contraste). Nenhum outro código VCP, nunca o 0x04 (reset de fábrica) nem comandos de capacidades/salvar.
- Só abra barramentos que respondem a DDC (monitores). Nunca escreva em barramento que não identificou como monitor.

## O que fazer
1. Descobrir monitores e ler os VCP 0x10 e 0x12, com valor atual e máximo.
2. Escrita com debounce de 200 ms. Semântica: um `write` superado por outro dentro da janela devolve `Ok(())` sem enviar nada; o último espera o envio e devolve o resultado real. Depois de escrever, emitir `CapabilityChanged`.
3. Cache de leitura (o valor muda raramente por fora). Sem polling periódico.
4. Erro claro quando faltam `i2c-dev` ou permissão (`RaizaError::Permission`, com dica de `modprobe i2c-dev` e da regra udev).
5. Backend injetável (trait) para testar sem monitor.

## Critérios de aceite
- [ ] Testes do debounce (três escritas rápidas → um envio, com o último valor) e do cache, com backend simulado
- [ ] Teste: o backend simulado nunca recebe código VCP diferente de 0x10/0x12
- [ ] Exemplo `cargo run -p raiza-provider-ddc --example listar` lê o brilho do monitor real
- [ ] `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings` e `cargo test` passam

## Como testar com hardware real (para o mantenedor)
`sudo modprobe i2c-dev`, instalar a regra udev (`docs/INSTALACAO.md`) e rodar o exemplo.
