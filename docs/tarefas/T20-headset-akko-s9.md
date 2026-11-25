# T20: Headset Akko S9 Ultra (36eb:1087): protocolo, bateria, sidetone e EQ

- **Fase:** F4
- **Agente sugerido:** humano + claude
- **Depende de:** T15
- **Escopo:** `docs/protocolos/akko-s9-ultra-36eb-1087.md`, `profiles/akko/akko-s9-ultra-36eb-1087.json`, `docs/protocolos/capturas/akko-s9-ultra-36eb-1087/**` (só leitura), `docs/tarefas/notas/T20.md`

## Objetivo
Documentar o protocolo HID do headset sem fio Akko S9 Ultra e escrever o perfil dele (bateria), inicialmente com `"verified": false`. Descrever sidetone e equalização com precisão suficiente para o líder estender o contrato (T21).

## Contexto
- O headset conversa pelo dongle USB `36eb:1087` ("Akko S9 Ultra_Dongle", fabricante "GTAI"). Interfaces: 0–2 de áudio USB (controle, saída e entrada, atendidas pelo PipeWire na T09) e **3 HID sem protocolo de boot**, que é a candidata a receber os comandos do fabricante.
- O udev (T05) já o lista como `Headset` com id `usb:36eb:1087:00000000000000` (o serial do dongle é todo zero).
- O fabricante não tem software para Linux. Capacidades desejadas: **bateria** (nível e carregando), **sidetone** (retorno da própria voz no fone, liga/desliga e nível) e **EQ** (presets e, se houver, bandas com ganho).
- **Contrato:** hoje o `raiza-core` e o formato de perfil (T15) só têm `battery` entre as capacidades desta tarefa. `Sidetone` e `Equalizer` ainda **não existem**. Não os invente no perfil, porque a validação rejeitaria. Documente o protocolo deles; o líder cria as capacidades na T21 a partir do seu documento.
- **Esta tarefa depende de capturas feitas pelo mantenedor**, numa VM Windows com o dongle repassado e usbmon no Linux, com `scripts/capturar-usb.sh`. O roteiro está em `docs/tarefas/notas/T20.md`, que também traz o descritor HID já decodificado: o canal de comandos provável é o report ID 2, com 63 bytes, pelos endpoints de interrupção `0x02`/`0x82`. Os `.pcapng`, o `anotacoes.md` (valores que o software mostrava em cada passo), o `lsusb-v.txt` e o `descritor-hid-if3.bin` ficam em `docs/protocolos/capturas/akko-s9-ultra-36eb-1087/`. Para analisar: `tshark` (pacote `wireshark-cli`).

## O que fazer
1. Sem capturas na pasta: **pare** e avise nas notas. Com capturas: siga o roteiro de `docs/tarefas/notas/T20.md` para saber o que cada arquivo contém (o nome do arquivo e a linha correspondente em `anotacoes.md`).
2. Com capturas: extraia os relatórios HID da interface 3, compare e deduza report ID, comando, posição dos valores, codificação (atenção a ganhos de EQ **com sinal**) e checksum.
3. Leia o descritor HID (`/sys/class/hidraw/hidrawN/device/report_descriptor` da interface 3) e documente as usage pages.
4. Escreva `docs/protocolos/akko-s9-ultra-36eb-1087.md` com: bateria, sidetone (faixa, passo, liga/desliga separado ou nível 0), EQ (número de bandas, frequências, faixa e passo do ganho em dB, presets e seus códigos, se a escrita é por banda ou do EQ inteiro) e, para cada item, a captura de origem.
5. Escreva o perfil `profiles/akko/akko-s9-ultra-36eb-1087.json` com `kind: "headset"` e **só `battery`**, que já cabe no formato.
6. Pesquise suporte existente (HeadsetControl, OpenRGB, GitHub). Se existir, documente e reaproveite.

## Fora de escopo
Enviar qualquer comando ao dispositivo real. Mudar `crates/raiza-core` ou o formato de perfil (é a T21, do líder). Volume e mudo (vêm do PipeWire).

## Critérios de aceite
- [ ] O perfil passa em `cargo test -p raiza-profiles`
- [ ] O documento do protocolo cobre bateria, sidetone e EQ, cada comando com a captura de origem
- [ ] `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings` e `cargo test --all-features` passam

## Como testar com hardware real (para o mantenedor)
Com o executor de perfis (T16): ligar o headset, conferir a bateria no `raiza-ctl` e comparar com o software oficial.
