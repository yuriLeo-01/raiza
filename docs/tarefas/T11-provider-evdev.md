# T11: Provedor evdev (controles)

- **Fase:** F2
- **Agente sugerido:** hydrogen
- **Depende de:** T01
- **Escopo:** `crates/raiza-provider-evdev/**`

## Objetivo
Controles (Xbox, DualShock/DualSense, genéricos): bateria pelo `power_supply` do kernel e teste de vibração (`Rumble`).

## Contexto
Crate `evdev`. A bateria de controles com driver no kernel (`hid-playstation`, `xpadneo`, `hid-nintendo`) aparece em `/sys/class/power_supply/*` ligada ao dispositivo HID. A vibração usa force feedback (`FF_RUMBLE`). O mantenedor não tem controle conectado agora: use testes e simulação; o teste com hardware fica para depois.

## O que fazer
1. Descobrir joysticks (`ID_INPUT_JOYSTICK`) e associar a bateria em `power_supply`.
2. `Rumble` como ação: vibrar X ms com intensidade Y, com limites seguros (no máximo 2 s).
3. Eventos de bateria via udev (subsistema `power_supply`).

## Critérios de aceite
- [ ] Testes com sysfs falso (fixtures em diretório temporário)
- [ ] Limite de duração e intensidade do rumble testado
- [ ] `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings` e `cargo test` passam
