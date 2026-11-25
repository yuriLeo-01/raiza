# T21: Capacidades `Sidetone` e `Equalizer` (contrato)

- **Fase:** F4
- **Agente sugerido:** claude (líder)
- **Depende de:** T20
- **Escopo:** `crates/raiza-core/**`, `crates/raiza-profiles/**`, `profiles/schema.json`, `docs/DBUS.md`, `docs/PERFIS.md`

## Objetivo
Acrescentar ao contrato as capacidades de headset que a T20 documentou: sidetone e equalização.

## O que fazer
1. A partir de `docs/protocolos/akko-s9-ultra-36eb-1087.md`, definir `Capability::Sidetone` e `Capability::Equalizer` (bandas, faixa de ganho com sinal, presets) e os `CapabilityValue` correspondentes, com validação.
2. Estender o formato de perfil (campos e codificação com sinal, se necessário) e o `schema.json`.
3. Atualizar `docs/DBUS.md` e `docs/PERFIS.md`. Acrescentar sidetone e EQ ao perfil do S9 Ultra.

## Critérios de aceite
- [ ] Ida e volta serde e validação testadas para as capacidades novas
- [ ] Bytes de sidetone e EQ do S9 Ultra testados byte a byte contra as capturas
- [ ] `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings` e `cargo test --all-features` passam
