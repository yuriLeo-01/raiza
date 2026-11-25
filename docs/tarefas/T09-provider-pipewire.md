# T09: Provedor PipeWire

- **Fase:** F2
- **Agente sugerido:** hydrogen
- **Depende de:** T01
- **Escopo:** `crates/raiza-provider-pipewire/**`

## Objetivo
Volume e mudo de microfones e headsets: as capacidades `Volume` e `Mute`.

## Contexto
Opções: crate `pipewire` (bindings da libpipewire) ou o comando `wpctl` do WirePlumber. Decida e justifique nas notas. Prefira a lib se ela permitir eventos sem polling. Dispositivo real: microfone fifine 3142:a010 (USB) e o headset Bluetooth.

## O que fazer
1. Mapear cada nó de áudio para o dispositivo físico (propriedades `device.bus`, `api.alsa.card.*`, `api.bluez5.address`, `device.vendor.id`/`device.product.id`), para fundir com udev e bluez.
2. Ler e escrever volume (0–100%, curva cúbica igual à do `wpctl`) e mudo.
3. Eventos quando o volume ou o mudo mudam por fora (por exemplo, pelo GNOME).

## Critérios de aceite
- [ ] Testes com backend simulado
- [ ] Exemplo que lista os nós com o id de dispositivo calculado
- [ ] `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings` e `cargo test` passam
