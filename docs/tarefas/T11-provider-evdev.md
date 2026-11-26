# T11: Provedor evdev (controles)

- **Fase:** F2
- **Agente sugerido:** sonnet
- **Depende de:** T01
- **Escopo:** `crates/raiza-provider-evdev/**`

## Objetivo
Controles (Xbox, DualShock/DualSense, genéricos): bateria pelo `power_supply` do kernel e teste de vibração (`Rumble`).

## Contexto
Crate `evdev`. A bateria de controles com driver no kernel (`hid-playstation`, `xpadneo`, `hid-nintendo`) aparece em `/sys/class/power_supply/*` ligada ao dispositivo HID. A vibração usa force feedback (`FF_RUMBLE`). O systemd já dá acesso (`uaccess`) aos joysticks na sessão local. O mantenedor não tem controle conectado agora: use testes e simulação; o teste com hardware fica para depois.

## Contrato com o `raiza-core` (vale para todo provedor)
- Crate novo em `crates/raiza-provider-<nome>/`, com `raiza-core = { workspace = true }`. Para testes, use `raiza-core = { workspace = true, features = ["mock"] }` em `[dev-dependencies]` se precisar do `MockProvider`.
- Implemente `raiza_core::Provider` (com `#[async_trait::async_trait]`). Runtime: `tokio`. Logs: `tracing`. Erros: `RaizaError`.
- **Identidade**: siga a tabela "Identidade" de `docs/ARQUITETURA.md`. Para USB, use **sempre** `DeviceId::usb_instance(serial, porta, portas_do_mesmo_modelo_sem_serial)`. Para Bluetooth, use `DeviceId::bluetooth(mac)`. Sem isso a fusão com os outros provedores falha.
- `DeviceInfo.providers` = `vec!["<id do provedor>".into()]`.
- Em `write`, valide de novo o valor com `CapabilityValue::validate` contra o estado mais recente antes de tocar no hardware (pilar 1).
- Um dispositivo com problema vira `Err` só para ele: o provedor continua atendendo os outros. Serviço do sistema ausente → lista vazia e log em nível `info`, nunca pânico.
- Commite o `Cargo.lock` normalmente. Conflitos nele o líder resolve no merge.

**Específico deste provedor:**
- Id do provedor: `"evdev"`. `kind = Gamepad`.
- Id: controle USB → `DeviceId::usb_instance` a partir do dispositivo USB pai. Controle Bluetooth → `DeviceId::bluetooth` com o atributo `uniq` do dispositivo de input.
- `Battery { percent, charging }` (somente leitura), de `capacity` e `status` do `power_supply`. Se o driver só tiver `capacity_level`, mapeie Critical/Low/Normal/High/Full para 5/20/50/80/100.
- `Rumble { max_duration_ms: 2000, writable: true }`, só se o dispositivo anunciar `FF_RUMBLE`. Intensidade 0–100 → magnitude 0–0xFFFF. Remova o efeito do dispositivo depois de tocar.

## O que fazer
1. Descobrir joysticks (`ID_INPUT_JOYSTICK`) e associar a bateria em `power_supply`.
2. `Rumble` como ação, com os limites acima validados antes de chegar ao hardware.
3. Eventos de bateria e hotplug via monitor do udev (subsistemas `power_supply` e `input`), integrado ao tokio. Sem polling.
4. Camada de I/O injetável.

## Critérios de aceite
- [x] Testes com sysfs falso (fixtures em diretório temporário), incluindo `capacity_level`
- [x] Limite de duração e intensidade do rumble testado (o backend simulado nunca recebe valor fora do limite)
- [x] `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings` e `cargo test --all-features` passam
