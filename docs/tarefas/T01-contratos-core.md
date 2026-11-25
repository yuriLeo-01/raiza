# T01: Contratos do `raiza-core`

- **Fase:** F0
- **Agente sugerido:** claude (líder)
- **Depende de:** —
- **Escopo:** `crates/raiza-core/**`

## Objetivo
Definir os tipos e traits que todos os provedores, o daemon e as interfaces vão usar. Depois desta tarefa, um provedor pode ser escrito sem conhecer o daemon.

## Contexto
Leia `docs/ARQUITETURA.md` (modelo de dados). O core **não faz I/O** e deve compilar rápido: dependências só `serde`, `thiserror`, `async-trait` (ou traits async nativas) e `futures`/`tokio-stream` para o fluxo de eventos.

## O que fazer
1. `DeviceId`: id estável e serializável (barramento + vendor:product + serial ou caminho físico), com `Display` legível.
2. `DeviceKind`: Mouse, Keyboard, Headset, Gamepad, Microphone, Monitor, RgbController, Other.
3. `DeviceInfo`: id, nome, fabricante, kind, provedor de origem e `Vec<Capability>`.
4. `Capability` (enum com dados) e `CapabilityKind` (enum sem dados): Battery, Dpi, PollingRate, Rgb, Volume, Mute, Brightness, Contrast, Rumble, Connection. Cada uma com valor atual, limites e a flag `writable`.
5. `CapabilityValue`: o que se envia em um `set`, validável contra a `Capability` (fora dos limites dá erro, sem chegar ao provedor).
6. trait `Provider`: `id()`, `discover() -> Vec<DeviceInfo>`, `read(&DeviceId, CapabilityKind)`, `write(&DeviceId, CapabilityValue)`, `events() -> Stream<ProviderEvent>`.
7. `ProviderEvent`: DeviceAdded, DeviceRemoved, CapabilityChanged.
8. `RaizaError` com `thiserror`: NotFound, Unsupported, ReadOnly, OutOfRange, Io, Permission.
9. Função de **fusão**: combina `DeviceInfo`s do mesmo dispositivo físico vindos de provedores diferentes.
10. Um `MockProvider` atrás da feature `mock`, para os testes de outros crates.

## Fora de escopo
D-Bus, I/O real e daemon.

## Critérios de aceite
- [x] Tudo serializa e desserializa com serde (teste de ida e volta para cada tipo)
- [x] Validação de `CapabilityValue` testada (dentro e fora dos limites, escrita em capacidade somente leitura)
- [x] Fusão testada (headset Bluetooth = bluez + pipewire vira um dispositivo)
- [x] Toda API pública tem doc comment
- [x] `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings` e `cargo test` passam
