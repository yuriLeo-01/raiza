# T14: Controles por capacidade (GTK)

- **Fase:** F3
- **Agente sugerido:** claude
- **Depende de:** T13
- **Escopo:** `crates/raiza-gtk/**` (o trabalho principal fica em `crates/raiza-gtk/src/capabilities/`)

## Objetivo
Um widget interativo por `CapabilityKind`, escolhido automaticamente (pilar 3: a tela vem das capacidades, não da marca).

## Contexto
- A T13 deixou o **ponto único** `capabilities::row_for(&Capability, &RowContext) -> Box<dyn CapabilityRow>` (`crates/raiza-gtk/src/capabilities/mod.rs`), que hoje devolve uma linha somente leitura. `RowContext` já leva o `DeviceId` e o `RaizaClient` (com `#[expect(dead_code)]`, que você deve tirar). `CapabilityRow::update` recebe o valor novo sem recriar o widget. Leia `docs/tarefas/notas/T13.md`.
- Escritas: `RaizaClient::set_capability(&DeviceId, &CapabilityValue)`. O `zbus` roda em **tokio**: dispare as escritas fora da thread da interface, do mesmo jeito que a T13 faz as chamadas D-Bus, e traga o resultado de volta para a thread do GTK.
- **Sem laço de realimentação:** depois de uma escrita, o daemon emite `CapabilityChanged`, e a página chama `update()`. Mudar o valor de um widget por `update()` **não pode** disparar uma nova escrita (bloqueie o sinal do widget durante o `update`, ou compare com o último valor enviado).
- Toda escrita é validada pelo daemon. Um erro (`OutOfRange`, `ReadOnly`, `Io`, `Permission`) volta como `ClientError::Raiza(RaizaError)`: mostre um `AdwToast` em português e volte o widget ao último valor confirmado. Para isso, acrescente ao `RowContext` um jeito de a linha reportar erros à janela, e um `AdwToastOverlay` na janela (`app.rs`).
- Para testar: `cargo run -p raizad -- --mock` traz um mouse (Dpi, PollingRate, Rgb, Battery, Connection) e um headset (Volume, Mute, Battery). `cargo run -p raiza-ctl -- set …` muda valores por fora, para conferir o `update()`. `cargo run -p raizad` usa os aparelhos reais do mantenedor (volume e mudo do microfone fifine pelo PipeWire). **Não** teste escrita em aparelho real além de volume e mudo.

## O que fazer
1. Battery: nível e estado de carga (somente leitura). Dpi: `AdwSpinRow` com min/max/passo e os estágios como sugestões. PollingRate: `AdwComboRow` com as taxas aceitas. Rgb: modo (`AdwComboRow`) e cor por zona (`GtkColorDialogButton`). Volume: slider com debounce. Mute: switch. Rumble: botão "testar" (pulso curto e seguro, dentro de `max_duration_ms`). Connection: switch conectar/desconectar quando `writable`, senão só o estado.
2. Capacidade somente leitura (`writable: false`) aparece desabilitada, com tooltip explicando o motivo: "somente leitura: perfil não verificado ou o aparelho não aceita mudança".
3. Erros do daemon viram `AdwToast`.
4. Debounce: no máximo uma escrita a cada ~150 ms enquanto o usuário arrasta, e sempre o último valor no fim. Lógica separada da interface, testável sem GTK.

## Critérios de aceite
- [x] Todas as capacidades do `MockProvider` têm widget e funcionam com `raizad --mock` (descreva nas notas como testou)
- [x] Debounce testado sem GTK (rajada → um envio por janela, último valor sempre enviado)
- [x] Teste: aplicar um valor vindo do daemon não gera escrita (sem laço)
- [x] `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings` e `cargo test --all-features` passam
