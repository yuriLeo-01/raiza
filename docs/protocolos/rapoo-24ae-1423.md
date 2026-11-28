# Rapoo VT2 Max / VT2 Pro Max (24ae:1423 dongle, 24ae:4623 cabo)

Resumo, com crédito, do protocolo descoberto e testado pelo projeto
[periph](https://github.com/arthur-ba1/periph) (MIT, `docs/protocol.md`, commit `d0ca959`). Detalhes
e lacunas em [`docs/tarefas/notas/T17.md`](../tarefas/notas/T17.md).

- Interface 1. Comandos: output report `0x06` (33 bytes) `06 A5 <op> <len> <addr_lo> <addr_hi> 00 00 [dados]`,
  `op` `A4` lê, `A5` escreve. Resposta: input `0x06`, `<status> 00 00 00 [dados]`, com `01` pronto,
  `02` ocupado e `00` mouse dormindo.
- Status espontâneo: input `0x07` (19 bytes), ~a cada 3 s: byte 2 estágio de DPI, 3–6 DPI X/Y (u16 LE),
  7 `01` bateria / `02` carregando, 8 bateria %, 10 key scan rate (125 × 2ⁿ), 11 report rate (polling, tabela abaixo).
- Tabela de DPI: `06 A5 A4 0E 88 08 00 00` lê os 14 bytes de `0x0888` (7 estágios X, u16 LE);
  resposta `06 01 00 00 00 <dados>`, ou seja, dados a partir do byte 4 do conteúdo. Lida do
  aparelho em 2026-11-28: 400, 800, 950, 1600, 950, 6400, 30000 (igual ao app oficial).
- Key Scan Rate: `0x0881` e byte 10 do status, mesmo expoente (`05` = 4000 Hz). Escrita de 1 byte
  testada em 2026-11-28: `06 A5 A5 01 81 08 00 00 03` → bloco `0x0880` relido `84 03 00 00` (só o key
  scan mudou) e status com byte 10 = `03` (1000 Hz); desfeito para `05`.
- Polling: 125=`08`, 250=`04`, 500=`02`, 1000=`01`, 2000=`84`, 4000=`82`, 8000=`81`.
- **Nunca** usar a interface 2 (relatórios de 512 bytes, provavelmente firmware/macros).

Perfil: [`profiles/rapoo/rapoo-vt2-max-24ae-1423.json`](../../profiles/rapoo/rapoo-vt2-max-24ae-1423.json)
(verificado: leitura de bateria, carga, DPI e report rate pelo status `0x07`; escrita do report rate
com 1 byte em `0x0880` e do DPI do estágio em uso (X em `0x0888 + 2s`, Y em `0x08C8 + 2s`), testadas no aparelho. **Não** grave o bloco de 4 bytes do periph: `0x0881` é o key scan rate).
