> ⏸ **Adiada em 2026-11-25** por decisão de escopo do mantenedor.

# T12: Provedor OpenRGB

- **Fase:** F2
- **Agente sugerido:** hydrogen
- **Depende de:** T01
- **Escopo:** `crates/raiza-provider-openrgb/**`

## Objetivo
Controlar o RGB de tudo que o OpenRGB já suporta, conectando ao servidor SDK dele (TCP em localhost:6742), se estiver rodando.

## Contexto
Crate `openrgb` (ou implementar o protocolo SDK, que é simples e documentado). Se o OpenRGB não estiver rodando, o provedor fica inativo e sem erro barulhento (pilar 5: aproveitar o que existe, nunca exigir). Dispositivo do mantenedor que pode ser suportado: controlador JIGUANG ARGB 1a86:e30b.

## Contrato com o `raiza-core` (vale para todo provedor)
- Crate novo em `crates/raiza-provider-<nome>/`, com `raiza-core = { workspace = true }`. Para testes, use `raiza-core = { workspace = true, features = ["mock"] }` em `[dev-dependencies]` se precisar do `MockProvider`.
- Implemente `raiza_core::Provider` (com `#[async_trait::async_trait]`). Runtime: `tokio`. Logs: `tracing`. Erros: `RaizaError`.
- **Identidade**: siga a tabela "Identidade" de `docs/ARQUITETURA.md`. Para USB, use **sempre** `DeviceId::usb_instance(serial, porta, portas_do_mesmo_modelo_sem_serial)`. Para Bluetooth, use `DeviceId::bluetooth(mac)`. Sem isso a fusão com os outros provedores falha.
- `DeviceInfo.providers` = `vec!["<id do provedor>".into()]`.
- Em `write`, valide de novo o valor com `CapabilityValue::validate` contra o estado mais recente antes de tocar no hardware (pilar 1).
- Um dispositivo com problema vira `Err` só para ele: o provedor continua atendendo os outros. Serviço do sistema ausente → lista vazia e log em nível `info`, nunca pânico.
- Commite o `Cargo.lock` normalmente. Conflitos nele o líder resolve no merge.

**Específico deste provedor:**
- Id do provedor: `"openrgb"`.
- Id: se o `location` do controlador for `HID: /dev/hidrawN`, resolva `/sys/class/hidraw/hidrawN/device` até o dispositivo USB pai e use `DeviceId::usb_instance`, para fundir com o udev. Nos outros casos, `DeviceId::Other { provider: "openrgb", key }` com `key` estável (nome + serial do controlador ou, sem serial, nome + índice).
- `kind` a partir do tipo do controlador no OpenRGB (mouse, teclado, headset…), `RgbController` no resto.
- `Capability::Rgb`: uma `RgbZone` por zona do OpenRGB (cor = a do primeiro LED da zona), `modes` com os nomes em minúsculas, `current_mode` o ativo.
- **Pilar 1**: use só `UpdateLEDs`/`UpdateZoneLEDs`/`UpdateMode`. **Nunca** `SaveMode` (grava na flash do dispositivo), `ResizeZone`, nem comandos de perfil.

## O que fazer
1. Conectar, listar controladores, zonas e modos e mapear para `Capability::Rgb`.
2. Definir uma cor por zona (todos os LEDs da zona) ou para todas, e trocar de modo.
3. Reconectar com backoff (até 60 s) se o OpenRGB reiniciar, emitindo `DeviceRemoved`/`DeviceAdded`.

## Critérios de aceite
- [ ] Testes com um servidor SDK falso (TCP local)
- [ ] Teste: o servidor falso nunca recebe `SaveMode`
- [ ] OpenRGB ausente: o provedor só loga em nível `info` e devolve lista vazia
- [ ] `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings` e `cargo test` passam

## Como testar com hardware real (para o mantenedor)
Abrir o OpenRGB com o servidor SDK ligado e rodar `cargo run -p raiza-provider-openrgb --example listar`.
