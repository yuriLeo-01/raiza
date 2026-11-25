# T09: Provedor PipeWire

- **Fase:** F2
- **Agente sugerido:** hydrogen
- **Depende de:** T01
- **Escopo:** `crates/raiza-provider-pipewire/**`

## Objetivo
Volume e mudo de microfones e headsets: as capacidades `Volume` e `Mute`.

## Contexto
Opções: crate `pipewire` (bindings da libpipewire) ou o comando `wpctl`/`pw-dump` do WirePlumber. Decida e justifique nas notas. Prefira a lib se ela permitir eventos sem polling (`pw-dump --monitor` também é aceitável). Dispositivos reais: microfone fifine 3142:a010 (USB), o dongle 36eb:1087 (tem interfaces de áudio USB de saída e entrada) e o headset Bluetooth Thinkplus K3 Pro (id `bt:DE:1E:59:99:7C:AE`).

## Contrato com o `raiza-core` (vale para todo provedor)
- Crate novo em `crates/raiza-provider-<nome>/`, com `raiza-core = { workspace = true }`. Para testes, use `raiza-core = { workspace = true, features = ["mock"] }` em `[dev-dependencies]` se precisar do `MockProvider`.
- Implemente `raiza_core::Provider` (com `#[async_trait::async_trait]`). Runtime: `tokio`. Logs: `tracing`. Erros: `RaizaError`.
- **Identidade**: siga a tabela "Identidade" de `docs/ARQUITETURA.md`. Para USB, use **sempre** `DeviceId::usb_instance(serial, porta, portas_do_mesmo_modelo_sem_serial)`. Para Bluetooth, use `DeviceId::bluetooth(mac)`. Sem isso a fusão com os outros provedores falha.
- `DeviceInfo.providers` = `vec!["<id do provedor>".into()]`.
- Em `write`, valide de novo o valor com `CapabilityValue::validate` contra o estado mais recente antes de tocar no hardware (pilar 1).
- Um dispositivo com problema vira `Err` só para ele: o provedor continua atendendo os outros. Serviço do sistema ausente → lista vazia e log em nível `info`, nunca pânico.
- Commite o `Cargo.lock` normalmente. Conflitos nele o líder resolve no merge.

**Específico deste provedor:**
- Id do provedor: `"pipewire"`.
- Só periféricos: dispositivos de áudio Bluetooth e USB. Ignore placas de som onboard/PCI e HDMI.
- Id: Bluetooth por `api.bluez5.address` → `DeviceId::bluetooth`. USB pela placa ALSA: `api.alsa.card` = N → `/sys/class/sound/cardN/device` → suba até o dispositivo USB pai (`idVendor`, `idProduct`, `serial`) → `DeviceId::usb_instance`.
- Limite do contrato: **um** `Volume` e **um** `Mute` por dispositivo. Se o aparelho tem saída (sink), eles representam a saída e `kind = Headset`. Se só tem entrada (source), representam a entrada e `kind = Microphone`. O volume do microfone de um headset fica de fora (anote nas notas).
- `Volume { level: Level { current, min: 0, max: 100, step: 1 }, writable: true }`, em porcentagem na curva cúbica, igual à do `wpctl`.

## O que fazer
1. Mapear cada nó de áudio para o dispositivo físico, como descrito acima.
2. Ler e escrever volume e mudo.
3. Eventos `CapabilityChanged` quando o volume ou o mudo mudam por fora (por exemplo, pelo GNOME), e `DeviceAdded`/`DeviceRemoved`.
4. Backend injetável (trait) para testar sem PipeWire.

## Critérios de aceite
- [ ] Testes com backend simulado (USB, Bluetooth, onboard ignorado, curva cúbica ida e volta)
- [ ] Exemplo `cargo run -p raiza-provider-pipewire --example listar-pipewire` lista os nós com o `DeviceId` calculado
- [ ] `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings` e `cargo test` passam

## Como testar com hardware real (para o mantenedor)
Rodar o exemplo com o fifine plugado. O id deve ser igual ao do exemplo do `raiza-provider-udev` (T05).
