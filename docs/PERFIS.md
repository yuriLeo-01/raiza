# Perfis HID: como escrever um

Um **perfil** é um arquivo JSON que ensina o Raiza a falar com um dispositivo HID que não tem
suporte no Linux: ler a bateria, trocar o DPI, mudar a cor. Ele não exige saber Rust (pilar 4).

Os perfis ficam em `profiles/<fabricante>/<modelo>.json` e são validados pelo crate
`raiza-profiles` e por [`profiles/schema.json`](../profiles/schema.json).

## Segurança primeiro (pilar 1)

- **Um perfil só descreve capacidades conhecidas** (`dpi`, `polling_rate`, `rgb`, `battery`).
  Não existe "comando livre": um perfil não consegue mandar ao dispositivo nada além dos
  relatórios dessas capacidades.
- **Todo perfil novo começa com `"verified": false`**, o que deixa tudo somente leitura: o
  Raiza não envia nenhuma escrita. Ele só vira `true` depois de testado no hardware real e
  revisado num pull request.
- A validação rejeita o perfil inteiro se encontrar:
  - relatório maior que 64 bytes;
  - campo fora do relatório ou dois campos no mesmo byte;
  - limite que a codificação não representa (ex.: DPI 16000 num campo `u8`);
  - relatório enviado do tipo `input`, ou resposta do tipo `output`;
  - campo ou capacidade desconhecidos;
  - as sequências ASCII `DFU`, `BOOT`, `BTLD`, `IAP`, `FLASH`, `ERASE` ou `FIRMWARE` em qualquer
    relatório enviado. São padrões de bootloader e gravação de firmware;
  - a sequência `55 AA 55 AA` (gatilho de bootloader) em qualquer relatório enviado;
  - opcodes destrutivos conhecidos de um fabricante, no primeiro byte do relatório. Hoje: VID
    `3151` (ROYUAN) com `AC` (apaga a flash), `2C` (apaga a tela), `7F` ou `30` (bootloader).
- **Pedidos de leitura (`read.request`) são consultas fixas**: não têm campos variáveis. Mesmo
  com `"verified": false`, o Raiza os envia para ler. Por isso, na revisão confira que o pedido
  só **consulta** e não altera nada no dispositivo.
- Nunca capture nem descreva comandos de atualização de firmware, reset de fábrica ou
  gravação de macros na memória do dispositivo.

## Estrutura

```json
{
  "$schema": "../schema.json",
  "schema": 1,
  "name": "Mouse Fictício X1",
  "vendor": "Fictícia",
  "kind": "mouse",
  "verified": false,
  "authors": ["Seu Nome"],
  "match": { "vendor_id": "1234", "product_id": "5678", "interface": 1, "usage_page": "ff00" },
  "capabilities": { "dpi": { ... }, "polling_rate": { ... }, "rgb": { ... }, "battery": { ... } }
}
```

| Campo | Significado |
|---|---|
| `schema` | Versão do formato: `1` |
| `kind` | `mouse`, `keyboard`, `headset`, `gamepad`, `microphone`, `rgb_controller`, `other` |
| `match` | Um objeto, ou uma **lista** quando o mesmo aparelho se conecta de mais de um jeito com o mesmo protocolo (cabo e dongle). A primeira entrada é a conexão preferida |
| `match.vendor_id`, `match.product_id` | IDs USB em hex (veja `lsusb`) |
| `match.interface` | `bInterfaceNumber` da interface que recebe os comandos (veja `lsusb -v`) |
| `match.usage_page`, `match.usage` | Opcionais, para desempatar interfaces. `ff00` costuma ser a de comandos do fabricante |

Um exemplo completo e testado está em
[`crates/raiza-profiles/tests/fixtures/ficticio.json`](../crates/raiza-profiles/tests/fixtures/ficticio.json).

### Relatório enviado (`write`, `read.request`)

```json
{
  "type": "output",
  "report_id": 8,
  "data": "04 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00",
  "fields": { "value": { "at": 4, "encoding": "u16le", "scale": 50 } },
  "checksum": { "kind": "sum8_neg", "at": 15, "from": 0, "to": 15 }
}
```

- `type`: `feature` (SET_FEATURE) ou `output`.
- `report_id`: o primeiro byte que o hidraw recebe. `0` se o dispositivo não numera relatórios.
- `data`: os bytes fixos **depois** do report ID. O tamanho de `data` é o tamanho do relatório.
- `fields`: onde o valor entra. `at` conta a partir do início de `data`.
- `checksum`: calculado depois de aplicar os campos, sobre `data[from..to]` (o byte `to` fica
  de fora). Com `"include_report_id": true`, o report ID entra antes desses bytes.
  - `sum8` = soma módulo 256; `sum8_neg` = `0x100 - soma`; `sum8_inv` = `0xFF - soma` (teclados
    ROYUAN, como os Akko); `xor8` = XOR. Todos ocupam 1 byte.
  - `crc16`: 2 bytes, com `poly`, `init` (hex de 4 dígitos) e `encoding` (`u16le` ou `u16be`).
    Algoritmo: `crc ^= byte` e, em cada um dos 8 bits, `crc = (crc >> 1) ^ poly` se o bit 0 for 1,
    senão `crc >>= 1`. Exemplo, o headset Akko S9 Ultra:
    `{"kind": "crc16", "at": 61, "from": 0, "to": 61, "include_report_id": true, "poly": "8005", "init": "ffff", "encoding": "u16be"}`.

### Identificação (`identify`)

Quando o mesmo VID:PID é usado por aparelhos com protocolos diferentes (caso dos teclados ROYUAN,
VID `3151`), o perfil **deve** ter `identify`: uma consulta fixa e os bytes que a resposta precisa
ter. O Raiza manda a consulta ao conectar e só permite escritas se a resposta bater. Se não bater,
o aparelho fica somente leitura.

```json
"identify": {
  "request": { "type": "feature", "report_id": 0, "data": "8f 00 00 00 00 00 00 00",
               "checksum": { "kind": "sum8_inv", "at": 7, "from": 0, "to": 7 } },
  "response": { "type": "feature", "report_id": 0, "length": 8,
                "expect": [{ "at": 0, "value": 143 }, { "at": 1, "value": 13 }, { "at": 2, "value": 11 }] }
}
```

### Campo (`fields.<nome>`)

| Chave | Padrão | Significado |
|---|---|---|
| `at` | — | Posição do primeiro byte |
| `encoding` | `u8` | `u8`, `u16le` (little-endian) ou `u16be` |
| `scale` | `1` | Escrita: `bruto = (valor - offset) / scale`. Leitura: `valor = bruto * scale + offset` |
| `offset` | `0` | Veja `scale` |
| `mask` | — | Só em respostas: pega alguns bits (ex.: `128` = bit 7) e desloca para o bit 0 |

Campos aceitos por capacidade:

| Capacidade | Escrita | Leitura |
|---|---|---|
| `dpi` | `value` | `value` |
| `polling_rate` | `value` (código da tabela `rates`) | `value` (código, convertido de volta em Hz) |
| `rgb` | `red`, `green`, `blue` (juntos), `mode`, `zone` | — (não lido na versão 1) |
| `battery` | — | `percent` e, opcional, `charging` (≠ 0 = carregando) |

### Resposta (`read.response`)

```json
{
  "type": "feature",
  "report_id": 7,
  "length": 4,
  "expect": [{ "at": 0, "value": 5 }],
  "fields": { "percent": { "at": 1 }, "charging": { "at": 2, "mask": 128 } }
}
```

- `type`: `feature` (GET_FEATURE) ou `input` (o Raiza espera o próximo relatório de entrada
  com esse report ID).
- `expect`: bytes que precisam bater (ex.: o eco do comando). Uma resposta diferente é
  descartada.

### Capacidades

- `dpi`: `min`, `max`, `step` e `read`/`write`. `max - min` precisa ser múltiplo de `step`, e
  todo DPI permitido precisa virar um número inteiro com o `scale`/`offset` do campo.
- `polling_rate`: `rates` mapeia Hz para o código do dispositivo: `{"125": 8, "1000": 1}`.
- `rgb`: `zones` (nomes; o índice é o código de zona), `modes` (nome → código) e `write`.
  `all_zones` é o código de "todas as zonas"; sem ele, o Raiza manda um relatório por zona.
  Com mais de uma zona, o relatório precisa do campo `zone`.
- `battery`: só `read`.

## Como descobrir o protocolo

O caminho mais seguro é **observar** o software oficial conversando com o dispositivo, sem
enviar nada por conta própria.

### No Linux, com usbmon e Wireshark

O jeito mais prático é `scripts/capturar-usb.sh <vid:pid> <pasta> <passo>`: ele grava um passo por
arquivo e guarda **só** o tráfego do dispositivo (o resto do barramento, como o seu teclado, é
descartado antes de salvar). O roteiro da T20 (`docs/tarefas/notas/T20.md`) é um exemplo completo.
À mão:


1. `sudo modprobe usbmon` e abra o Wireshark com permissão para capturar em `usbmonN`. O `N` é o
   barramento do dispositivo; veja `lsusb`: `Bus 003 Device 007` → `usbmon3`.
2. Filtre pelo dispositivo: `usb.device_address == 7`.
3. Se o software oficial só roda no Windows, use uma VM com o dispositivo repassado (USB
   passthrough). O tráfego passa pelo usbmon do Linux.
4. Faça **uma ação por vez** no software (ex.: DPI 800 → 1600) e anote os pacotes
   `SET_REPORT`/`URB_INTERRUPT out` que aparecem. Compare capturas de ações parecidas para
   achar o byte que muda.

### No Windows, com USBPcap

1. Instale o [USBPcap](https://desowin.org/usbpcap/) junto com o Wireshark.
2. Capture no root hub do dispositivo enquanto usa o software oficial, como no item 4 acima.

### Dicas

- `data` no perfil **não inclui** o report ID: no Wireshark, o primeiro byte do relatório é o ID.
- Procure checksums: um byte no fim que muda junto com qualquer outro byte.
- Leia a bateria em níveis diferentes (cheia, depois de usar) para achar o byte de `percent`.
- Documente o protocolo descoberto em `docs/protocolos/<fabricante>-<modelo>.md` (pilar 6).

## Do rascunho ao `verified: true`

1. Escreva o perfil com `"verified": false` e confira no editor com o `$schema`.
2. `cargo test -p raiza-profiles` valida os perfis do repositório.
3. Com o executor (T16), teste a **leitura** no hardware.
4. Num pull request, mostre as capturas e o teste de cada escrita feito **com o software
   oficial como referência**. O mantenedor revisa e só então troca para `"verified": true`.
