# T10: Provedor DDC/CI (monitores)

- **Fase:** F2
- **Agente sugerido:** hydrogen
- **Depende de:** T01
- **Escopo:** `crates/raiza-provider-ddc/**`

## Objetivo
Brilho e contraste de monitores externos: as capacidades `Brightness` e `Contrast`.

## Contexto
Crate `ddc-hi` (ou `ddc-i2c`) sobre `/dev/i2c-*`. Precisa do módulo `i2c-dev` e de permissão (T07). Monitor real: LG 27UP850 (que também expõe "LG Monitor Controls" via HID, mas use DDC). DDC é lento (cerca de 50 ms por comando): aplique debounce nas escritas e use cache nas leituras.

## O que fazer
1. Descobrir monitores e ler o VCP 0x10 (brilho) e o 0x12 (contraste) com máximo e atual.
2. Escrita com debounce de 200 ms, para um slider da interface não inundar o barramento.
3. Erro claro quando faltam `i2c-dev` ou permissão (`RaizaError::Permission`, com dica).

## Critérios de aceite
- [ ] Testes do debounce e do cache com backend simulado
- [ ] Exemplo que lê o brilho do monitor real
- [ ] `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings` e `cargo test` passam
