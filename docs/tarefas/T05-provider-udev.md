# T05: Provedor udev (descoberta e hotplug)

- **Fase:** F1
- **Agente sugerido:** hydrogen
- **Depende de:** T01
- **Escopo:** `crates/raiza-provider-udev/**`

## Objetivo
Listar os periféricos USB/HID/Bluetooth conectados e avisar quando entram ou saem, já com `DeviceKind` inferido.

## Contexto
Crate `udev` (sem root). Subsistemas relevantes: `hidraw`, `input`, `usb`. Para inferir o tipo, use `ID_INPUT_MOUSE`, `ID_INPUT_KEYBOARD`, `ID_INPUT_JOYSTICK` e a classe de áudio USB (`bInterfaceClass` 01). Agrupe as várias interfaces do mesmo aparelho (um teclado costuma criar 3 hidraw) pelo dispositivo USB pai. Um dongle sem fio deve aparecer como um dispositivo só.

Este é o provedor **genérico e de menor prioridade**: garante que todo periférico apareça na lista. As outras capacidades vêm de provedores específicos, que se fundem com este pelo `DeviceId`.

Dispositivos reais do mantenedor, bons para testar: Rapoo 24ae:1423 (mouse), Akko 3151:502f (teclado com fio), dongle do headset Akko S9 Ultra 36eb:1087, fifine 3142:a010 (microfone), LG 27UP850 043e:9a39 (porta USB de controle do monitor: **ignorada**, monitores fora do escopo; lista `IGNORED_USB`) e JIGUANG ARGB 1a86:e30b.

## Contrato com o `raiza-core` (vale para todo provedor)
- Crate novo em `crates/raiza-provider-<nome>/`, com `raiza-core = { workspace = true }`. Para testes, use `raiza-core = { workspace = true, features = ["mock"] }` em `[dev-dependencies]` se precisar do `MockProvider`.
- Implemente `raiza_core::Provider` (com `#[async_trait::async_trait]`). Runtime: `tokio`. Logs: `tracing`. Erros: `RaizaError`.
- **Identidade**: siga a tabela "Identidade" de `docs/ARQUITETURA.md`. Para USB, use **sempre** `DeviceId::usb_instance(serial, porta, portas_do_mesmo_modelo_sem_serial)`. Para Bluetooth, use `DeviceId::bluetooth(mac)`. Sem isso a fusão com os outros provedores falha.
- `DeviceInfo.providers` = `vec!["<id do provedor>".into()]`.
- Em `write`, valide de novo o valor com `CapabilityValue::validate` contra o estado mais recente antes de tocar no hardware (pilar 1).
- Um dispositivo com problema vira `Err` só para ele: o provedor continua atendendo os outros. Serviço do sistema ausente → lista vazia e log em nível `info`, nunca pânico.
- Commite o `Cargo.lock` normalmente. Conflitos nele o líder resolve no merge.

**Específico deste provedor:**
- Id do provedor: `"udev"`.
- Única capacidade: `Capability::Connection { medium, connected: true, writable: false }`. `medium` = `Wired` para USB (inclusive dongles, já que não dá para distinguir com segurança) e `Bluetooth` para dispositivos de entrada no barramento Bluetooth.
- USB: `serial` = atributo `serial` do dispositivo USB pai; porta = nome dele em `/sys/bus/usb/devices/` (ex.: `1-3.2`).
- Bluetooth (input com `ID_BUS=bluetooth`): o MAC vem do atributo `uniq` do dispositivo de input.
- `write` sempre devolve `RaizaError::ReadOnly(CapabilityKind::Connection)`.

## O que fazer
1. Implementar `Provider` conforme o contrato acima.
2. `DeviceId` estável entre reinícios e entre portas USB diferentes, via `DeviceId::usb_instance`.
3. Monitor de hotplug com `udev::MonitorBuilder`, integrado ao tokio (`AsyncFd`), emitindo `DeviceAdded`/`DeviceRemoved`. Sem polling.
4. Ignorar hubs (classe 09), root hubs, massa de armazenamento (classe 08) e dispositivos internos que não sejam periféricos (ex.: webcam embutida, leitor de digital e Bluetooth interno, se identificáveis por `removable` = `fixed`).
5. Camada de I/O injetável: testes com uma árvore udev falsa (fixtures em JSON).

## Fora de escopo
Bateria, DPI e qualquer outra capacidade. Escrever em dispositivos.

## Critérios de aceite
- [x] Teste: 3 interfaces hidraw do mesmo teclado viram 1 dispositivo Keyboard
- [x] Teste: hub e pendrive são ignorados
- [x] Teste: dois aparelhos iguais sem serial ganham instâncias `0` e `1`, independente da ordem de enumeração
- [x] Exemplo `cargo run -p raiza-provider-udev --example listar-udev` imprime os dispositivos reais com o `DeviceId`
- [x] `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings` e `cargo test` passam

## Como testar com hardware real (para o mantenedor)
`cargo run -p raiza-provider-udev --example listar-udev`, plugar e desplugar o mouse e conferir os eventos.
