# T22: Tabela de estágios de DPI e Key Scan Rate (contrato + perfil Rapoo)

- **Fase:** F6
- **Agente sugerido:** claude (líder) + humano (testes no aparelho)
- **Depende de:** T16, T17
- **Escopo (arquivos que você pode criar ou alterar):**
  - `crates/raiza-core/**`, `docs/DBUS.md`
  - `crates/raiza-profiles/**`, `profiles/schema.json`, `docs/PERFIS.md`
  - `crates/raiza-provider-hid/**`
  - `crates/raiza-ctl/**` (só o necessário para os novos valores)
  - `profiles/rapoo/rapoo-vt2-max-24ae-1423.json`, `docs/protocolos/rapoo-24ae-1423.md`

## Objetivo
O daemon lê a tabela inteira de estágios de DPI do Rapoo, grava um estágio qualquer (não só o
em uso) e expõe a Key Scan Rate como capacidade própria, lida e gravada.

## Contexto
- Memória do Rapoo (T17): tabela de DPI X em `0x0888` e Y em `0x08C8`, 7 estágios u16le cada;
  estágio ativo em `0x0898`; report rate em `0x0880`; key scan em `0x0881` (125×2ⁿ, igual ao
  byte 10 do status `0x07`).
- Leitura de memória: relatório de saída 6, `06 A5 A4 <len> <addr lo> <addr hi> 00 00`;
  resposta no relatório de entrada 6 com o status em `[0]` e os dados a partir de `[7]`.
- A escrita de DPI de hoje (`dpi.write`, dois modelos X/Y) já endereça o estágio por `stage`.

## O que fazer
1. Contrato (`raiza-core` + `docs/DBUS.md`):
   - `Capability::Dpi.stages` passa a significar a **tabela real** do aparelho quando o perfil
     sabe lê-la (lista vazia = desconhecida).
   - `CapabilityValue::DpiStage { stage, value }` (JSON `{"kind":"dpi_stage","value":{"stage":2,"value":950}}`),
     validado contra `level` e contra `stages.len()`.
   - `Capability::ScanRate { current, supported, writable }` (`kind` `scan_rate`) e
     `CapabilityValue::ScanRate(u32)`, no molde de `PollingRate`.
2. Formato de perfil: `dpi.table` (consulta fixa + campo de lista `values` com `at`, `count`,
   `type`) e `scan_rate` no molde de `polling_rate`. Validação e schema.
3. Executor: ler a tabela junto com o DPI e na releitura; gravar `DpiStage` pelos modelos
   existentes com o estágio pedido; `ScanRate`.
4. Perfil Rapoo: `dpi.table` e `scan_rate` começam **sem** escrita; o mantenedor testa cada
   escrita uma vez e só então ela entra no perfil (pilar 1). Registrar os bytes no doc do protocolo.
5. `raiza-ctl set <id> dpi-stage <n> <valor>` e `set <id> scan-rate <hz>`.

## Fora de escopo
- Interface GTK (T23).
- Adicionar ou remover estágios (o número é fixo no aparelho).

## Critérios de aceite
- [ ] `raiza-ctl get` do Rapoo mostra os 7 estágios iguais aos do Rapoo Hub e a Key Scan Rate.
- [ ] Gravar um estágio não ativo muda só ele (conferido no Rapoo Hub) e é desfeito no teste.
- [ ] Testes de ida e volta JSON, validação e bytes gerados.
- [ ] `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings` e `cargo test --all-features` passam.

## Como testar com hardware real (para o mantenedor)
Com o Rapoo no cabo e no dongle: comparar a tabela com o Rapoo Hub; autorizar cada escrita de teste.
