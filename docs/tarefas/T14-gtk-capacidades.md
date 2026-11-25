# T14: Páginas por capacidade (GTK)

- **Fase:** F3
- **Agente sugerido:** claude
- **Depende de:** T13
- **Escopo:** `crates/raiza-gtk/src/capabilities/**` (e o registro dos widgets em um único ponto do app)

## Objetivo
Um widget interativo por `CapabilityKind`, escolhido automaticamente (pilar 3: a tela vem das capacidades, não da marca).

## O que fazer
1. Battery: nível e estado de carga. Dpi: `AdwSpinRow` e estágios. PollingRate: `AdwComboRow`. Rgb: seletor de cor e modo por zona. Volume: slider com debounce. Mute: switch. Rumble: botão "testar". Connection: switch conectar/desconectar.
2. Capacidade somente leitura aparece desabilitada, com tooltip explicando o motivo (por exemplo, "perfil não verificado").
3. Erros do daemon viram `AdwToast`.

## Critérios de aceite
- [ ] Todas as capacidades do `MockProvider` têm widget e funcionam com `raizad --mock`
- [ ] Debounce dos sliders testado (lógica separada da UI)
- [ ] `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings` e `cargo test --all-features` passam
