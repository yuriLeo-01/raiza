# T05: Provedor udev (descoberta e hotplug)

- **Fase:** F1
- **Agente sugerido:** hydrogen
- **Depende de:** T01
- **Escopo:** `crates/raiza-provider-udev/**`

## Objetivo
Listar os periféricos USB/HID/Bluetooth conectados e avisar quando entram ou saem, já com `DeviceKind` inferido.

## Contexto
Crate `udev` (sem root). Subsistemas relevantes: `hidraw`, `input`, `usb`. Para inferir o tipo, use `ID_INPUT_MOUSE`, `ID_INPUT_KEYBOARD`, `ID_INPUT_JOYSTICK` e a classe de áudio USB. Agrupe as várias interfaces do mesmo aparelho (um teclado costuma criar 3 hidraw) pelo dispositivo USB pai. Um dongle sem fio deve aparecer como um dispositivo só.

Dispositivos reais do mantenedor, bons para testar: Rapoo 24ae:1423 (mouse), Akko 3151:502f (teclado com fio), Akko S9 Ultra dongle 36eb:1087, fifine 3142:a010 (microfone), LG 27UP850 043e:9a39 (monitor) e JIGUANG ARGB 1a86:e30b.

## O que fazer
1. Implementar `Provider`. Este provedor só reporta a capacidade `Connection`, porque as outras vêm de outros provedores.
2. `DeviceId` estável entre reinícios e entre portas USB diferentes (prefira o serial; senão, vendor:product + índice).
3. Monitor de hotplug com `udev::MonitorBuilder`, integrado ao tokio.
4. Ignorar hubs, root hubs e pendrives (massa de armazenamento).
5. Camada de I/O injetável: testes com uma árvore udev falsa (fixtures em JSON).

## Critérios de aceite
- [ ] Teste: 3 interfaces hidraw do mesmo teclado viram 1 dispositivo Keyboard
- [ ] Teste: hub e pendrive são ignorados
- [ ] Exemplo `cargo run -p raiza-provider-udev --example listar` imprime os dispositivos reais
- [ ] `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings` e `cargo test` passam
