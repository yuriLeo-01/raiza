# T23: App GTK: editar estágios de DPI e Key Scan Rate

- **Fase:** F6
- **Agente sugerido:** claude
- **Depende de:** T22
- **Escopo (arquivos que você pode criar ou alterar):**
  - `crates/raiza-gtk/**`

## Objetivo
Na página do mouse, o usuário vê e edita cada estágio de DPI, vê qual está em uso e escolhe a
Key Scan Rate, com a mesma experiência das outras linhas.

## Contexto
- Contrato da T22 (leia `docs/DBUS.md`): `Capability::Dpi { level, stages, active_stage, writable }`
  com `stages` = tabela real; `CapabilityValue::DpiStage { stage, value }`;
  `Capability::ScanRate { current, supported, writable }` e `CapabilityValue::ScanRate(u32)`.
- Hoje `crates/raiza-gtk/src/capabilities/level.rs` tem `DpiRow`, com um popover que só
  lista os estágios como atalhos para o DPI atual. `polling.rs` tem a linha de polling rate, que
  serve de molde para a Key Scan. Textos ficam em `text.rs`. Escritas passam pelo debounce de
  `debounce.rs` e pela sincronização de `sync.rs`.
- Teste visual sem hardware: `systemctl --user stop raizad; cargo run -p raizad -- --mock` e
  `cargo run -p raiza-gtk`. Ajuste o mock do `raiza-core` (feature `mock`) **só se** a T22 não
  tiver deixado um mouse com tabela e Key Scan; nesse caso, avise no relatório.

## O que fazer
1. Grupo "Estágios de DPI": uma `AdwSpinRow` por estágio (limites e passo de `level`), o
   estágio em uso marcado (ícone ou sufixo "em uso"); mudar um valor envia `DpiStage` com
   debounce; mudar o estágio em uso também atualiza a linha de DPI.
2. Linha "Key Scan Rate" (combo), como a de polling. Explicar em subtítulo curto o que é.
3. Somente leitura (`writable: false`): valores visíveis, controles desativados, com o
   tooltip já usado nas outras linhas.
4. Sem tabela (lista vazia): manter o comportamento atual.

## Fora de escopo
- Mudanças em `raiza-core` ou na API D-Bus (peça ao líder).
- Tradução (T26): escreva os textos novos em `text.rs` como os demais.

## Critérios de aceite
- [ ] Com o `--mock`, os estágios aparecem, editam e refletem `CapabilityChanged`.
- [ ] Testes de modelo/lógica sem GTK para a montagem dos estágios.
- [ ] `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings` e `cargo test --all-features` passam.

## Como testar com hardware real (para o mantenedor)
Abrir o Rapoo no app, mudar um estágio não ativo e conferir no Rapoo Hub; desfazer.
