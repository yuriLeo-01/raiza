# T15: Formato e validação de perfis HID

- **Fase:** F4
- **Agente sugerido:** claude (líder)
- **Depende de:** T01
- **Escopo:** `crates/raiza-profiles/**` (formato e validação), `profiles/schema.json`, `docs/PERFIS.md`

## Objetivo
Um formato JSON declarativo que descreve como ler e escrever capacidades de um dispositivo HID sem suporte, para a comunidade contribuir sem escrever Rust (pilar 4), com segurança acima de tudo (pilar 1).

## O que fazer
1. Esquema: identificação (vendor:product, interface/usage page), `"verificado": bool`, autores e lista de capacidades, cada uma com templates de relatório HID (feature/output report, report id, bytes fixos, posição e codificação do valor, checksum) e a forma de ler a resposta.
2. Validação estrita: limites de valor, tamanho de relatório e apenas tipos de relatório permitidos. Sem comandos de firmware ou bootloader; padrões conhecidos de DFU são rejeitados.
3. `"verificado": false` força somente leitura.
4. `profiles/schema.json` (JSON Schema) para validação em editores e no CI.
5. `docs/PERFIS.md`: como escrever um perfil e como capturar o protocolo (Wireshark + usbmon no Linux, ou USBPcap no Windows com o software oficial).

## Critérios de aceite
- [ ] Perfil de exemplo fictício passa na validação. Perfis com valor fora do limite, relatório grande demais ou padrão de DFU são rejeitados (testes)
- [ ] A geração de bytes a partir de um valor é testada byte a byte
- [ ] `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings` e `cargo test` passam
