# T08: Provedor BlueZ + UPower

- **Fase:** F2
- **Agente sugerido:** hydrogen
- **Depende de:** T01
- **Escopo:** `crates/raiza-provider-bluez/**`

## Objetivo
Dispositivos Bluetooth (headsets, controles, mouses) com conexão, bateria e conectar/desconectar. Bateria de periféricos sem fio vinda do UPower.

## Contexto
Tudo via D-Bus de sistema com `zbus`: `org.bluez` (`Device1`: Connected, Paired, Icon, Connect/Disconnect; `Battery1`: Percentage) e `org.freedesktop.UPower` (`Device`: Percentage, State, Model, NativePath). Dispositivo real do mantenedor: headset Thinkplus K3 Pro (Bluetooth). Use o `Icon` do BlueZ (`audio-headset`, `input-gaming`, `input-mouse`…) para o `DeviceKind`.

## O que fazer
1. Implementar `Provider`: capacidades `Connection` (gravável: conectar/desconectar) e `Battery` (somente leitura).
2. Eventos via `PropertiesChanged` e `InterfacesAdded/Removed`, sem polling.
3. UPower: casar com o dispositivo (BlueZ pelo endereço MAC; USB pelo serial ou caminho) para que a fusão do core funcione.
4. Camada D-Bus injetável e testes com respostas simuladas.

## Critérios de aceite
- [ ] Testes com BlueZ e UPower simulados (conectado, desconectado, bateria mudando)
- [ ] Exemplo `--example listar` mostra o headset real
- [ ] `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings` e `cargo test` passam
