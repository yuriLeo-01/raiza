# Perfis HID

Cada arquivo descreve como o Raiza lê e escreve as capacidades de um modelo pelo `/dev/hidraw`.
O formato está em [`docs/PERFIS.md`](../docs/PERFIS.md) e o schema em [`schema.json`](schema.json).
Como contribuir um perfil: [`CONTRIBUTING.pt-BR.md`](../CONTRIBUTING.pt-BR.md#2-contribuir-um-perfil) ([English](../CONTRIBUTING.md#2-contributing-a-profile)).

O pacote instala esta pasta em `/usr/share/raiza/profiles/`. Perfis próprios vão em
`~/.local/share/raiza/profiles/`, e o daemon os recarrega sozinho quando o arquivo muda.

| Perfil | Aparelho | Conexões | Verificado | Lê | Grava | Protocolo |
|---|---|---|---|---|---|---|
| [`rapoo/rapoo-vt2-max-24ae-1423.json`](rapoo/rapoo-vt2-max-24ae-1423.json) | Rapoo VT2 Max / VT2 Pro Max | cabo `24ae:4623`, dongle `24ae:1423` | ✅ | bateria, carregando, DPI, polling | DPI (estágio em uso), polling | [doc](../docs/protocolos/rapoo-24ae-1423.md) |
| [`akko/akko-s9-ultra-36eb-1087.json`](akko/akko-s9-ultra-36eb-1087.json) | Akko Verge S9 Ultra (headset) | dongle `36eb:1087` | — | bateria | — | [doc](../docs/protocolos/akko-s9-ultra-36eb-1087.md) |
| [`akko/akko-5087-v3-he-dongle-3151-5038.json`](akko/akko-5087-v3-he-dongle-3151-5038.json) | Akko 5087 V3 HE (teclado) | dongle `3151:5038` | — | bateria | — | [notas](../docs/tarefas/notas/T18.md) |

"Verificado" (✅) significa que as escritas foram testadas num aparelho real e conferidas com o
software oficial. Sem isso, o perfil é somente leitura.
