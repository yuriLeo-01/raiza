# T08: Provedor BlueZ + UPower

- **Fase:** F2
- **Agente sugerido:** hydrogen
- **Depende de:** T01
- **Escopo:** `crates/raiza-provider-bluez/**`

## Objetivo
Dispositivos Bluetooth (headsets, controles, mouses) com conexão, bateria e conectar/desconectar. Bateria de periféricos sem fio não Bluetooth vinda do UPower.

## Contexto
Tudo via D-Bus de **sistema** com `zbus`: `org.bluez` (`Device1`: Address, Name, Alias, Connected, Paired, Icon, Connect/Disconnect; `Battery1`: Percentage) e `org.freedesktop.UPower` (`Device`: Type, PowerSupply, Percentage, State, Model, NativePath). Dispositivo real do mantenedor: headset Thinkplus K3 Pro (Bluetooth). Use o `Icon` do BlueZ (`audio-headset`, `audio-headphones`, `input-gaming`, `input-mouse`, `input-keyboard`…) para o `DeviceKind`.

## Contrato com o `raiza-core` (vale para todo provedor)
- Crate novo em `crates/raiza-provider-<nome>/`, com `raiza-core = { workspace = true }`. Para testes, use `raiza-core = { workspace = true, features = ["mock"] }` em `[dev-dependencies]` se precisar do `MockProvider`.
- Implemente `raiza_core::Provider` (com `#[async_trait::async_trait]`). Runtime: `tokio`. Logs: `tracing`. Erros: `RaizaError`.
- **Identidade**: siga a tabela "Identidade" de `docs/ARQUITETURA.md`. Para USB, use **sempre** `DeviceId::usb_instance(serial, porta, portas_do_mesmo_modelo_sem_serial)`. Para Bluetooth, use `DeviceId::bluetooth(mac)`. Sem isso a fusão com os outros provedores falha.
- `DeviceInfo.providers` = `vec!["<id do provedor>".into()]`.
- Em `write`, valide de novo o valor com `CapabilityValue::validate` contra o estado mais recente antes de tocar no hardware (pilar 1).
- Um dispositivo com problema vira `Err` só para ele: o provedor continua atendendo os outros. Serviço do sistema ausente → lista vazia e log em nível `info`, nunca pânico.
- Commite o `Cargo.lock` normalmente. Conflitos nele o líder resolve no merge.

**Específico deste crate** — ele exporta **dois** provedores:
- `BluezProvider`, id `"bluez"`: só dispositivos **pareados**. Id `DeviceId::bluetooth(Address)`. Capacidades: `Connection { medium: Bluetooth, connected, writable: true }` (escrever `CapabilityValue::Connection(bool)` chama `Connect`/`Disconnect`) e `Battery { percent, charging: false }` quando houver `Battery1`.
- `UpowerProvider`, id `"upower"`: só `Battery` (somente leitura, `charging` a partir de `State`). Ignore a bateria do próprio computador e a fonte (`PowerSupply == true`, `Type` 1/2) e o `DisplayDevice`. Para o id: se o `NativePath` aponta para o BlueZ (contém o MAC), use `DeviceId::bluetooth`; senão resolva `/sys/class/power_supply/<NativePath>/device` subindo até o dispositivo USB pai e use `DeviceId::usb_instance`. Sem como identificar: ignore o dispositivo e logue em `debug`.

## O que fazer
1. Implementar os dois provedores conforme acima.
2. Eventos via `PropertiesChanged` e `InterfacesAdded/Removed` (BlueZ) e `DeviceAdded/DeviceRemoved` + `PropertiesChanged` (UPower), sem polling.
3. Camada D-Bus injetável (trait) e testes com respostas simuladas.

## Fora de escopo
Parear dispositivos novos, perfis de áudio (A2DP/HFP), volume.

## Critérios de aceite
- [ ] Testes com BlueZ e UPower simulados (conectado, desconectado, bateria mudando, bateria do notebook ignorada)
- [ ] Teste: `write(Connection(false))` chama `Disconnect` no backend simulado
- [ ] Exemplo `cargo run -p raiza-provider-bluez --example listar` mostra o headset real
- [ ] `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings` e `cargo test` passam

## Como testar com hardware real (para o mantenedor)
Ligar o headset Thinkplus K3 Pro, rodar o exemplo e conferir bateria e conexão.
