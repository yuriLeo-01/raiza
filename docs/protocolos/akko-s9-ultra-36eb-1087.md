# Akko Verge S9 Ultra (36eb:1087 dongle, 36eb:0087 cabo)

Resumo, com crédito, do protocolo documentado pelo projeto
[HaloBattery](https://github.com/HeyOkay/HaloBattery) (MIT, `docs/protocols.md` na tag `akko-s9-test2`),
a partir de capturas USBPcap do app oficial ([issue #505](https://github.com/HeyOkay/HaloBattery/issues/505)).
Detalhes em [`docs/tarefas/notas/T20.md`](../tarefas/notas/T20.md).

- Interface 3, report ID `0x02`, 64 bytes com o ID, nos dois sentidos (usage page `0x8C`).
- Todo quadro termina com um CRC-16 nos bytes 62–63 (big-endian), sobre os bytes 0–61, incluindo o
  report ID: init `0xFFFF`, deslocando à direita com polinômio `0x8005`. Conferido pelo Raiza contra
  três exemplos.
- Conexão: `02 10 01 00 …` → `02 10 01 00 01` ligado, `… 00` desligado.
- Bateria: `02 12 01 00 …` → `02 12 01 00 <nível %>`. Byte 3 = `01` significa sem nível (confirmado no aparelho em 2026-11-26: com o headset desligado vem `01`, e a bateria aparece pela releitura assim que ele liga).
- Carregando: só pelo cabo, quando o headset aparece como `36eb:0087`.
- Sidetone e EQ: **não documentados** (aguardam capturas, T20).

Perfil: [`profiles/akko/akko-s9-ultra-36eb-1087.json`](../../profiles/akko/akko-s9-ultra-36eb-1087.json)
(só leitura: bateria).
