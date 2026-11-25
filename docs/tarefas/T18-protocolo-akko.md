# T18: Protocolo do teclado Akko (3151:502f com fio e dongle próprio)

- **Fase:** F4
- **Agente sugerido:** humano + claude
- **Depende de:** T15
- **Escopo:** `docs/protocolos/akko-3151-502f.md`, `profiles/akko/akko-3151-502f.json`, `docs/protocolos/capturas/akko-3151-502f/**` (só leitura), `docs/tarefas/notas/T18.md`

## Objetivo
Documentar o protocolo HID do teclado Akko e escrever o perfil dele, inicialmente com `"verified": false`.

## Contexto
O teclado se conecta de dois jeitos:
- **com fio**: `3151:502f` (o udev já o reconhece como `Keyboard`, com 3 interfaces hidraw);
- **sem fio, pelo dongle próprio do teclado**: VID:PID **ainda desconhecido**. O mantenedor vai ligar o dongle e informar.

Atenção: o dongle `36eb:1087` ("Akko S9 Ultra") **não** é deste teclado: é de um headset (T20).

O fabricante não tem software para Linux. Capacidades desejadas: RGB (modo, cor) e, sem fio, bateria.
**Esta tarefa depende de capturas feitas pelo mantenedor**: tráfego USB do software oficial no Windows (USBPcap + Wireshark), mudando uma configuração por vez e anotando o valor. Os arquivos `.pcapng` ficam em `docs/protocolos/capturas/akko-3151-502f/`. O guia de captura está em `docs/PERFIS.md`.

**O mesmo teclado com dois ids.** Cada conexão tem o próprio `DeviceId` (`usb:3151:502f:…` e `usb:<dongle>:…`). Regra do projeto (`docs/ARQUITETURA.md`, "Um aparelho, duas conexões"):
- se o protocolo é **o mesmo** nas duas conexões, escreva **um** perfil com `match` em lista: primeiro o cabo (conexão preferida), depois o dongle;
- se o protocolo é **diferente** (outros report IDs, comandos ou tamanhos), **pare**: descreva as diferenças em `docs/tarefas/notas/T18.md` e faça commit. O líder decide como representar isso no formato.

## O que fazer
1. Sem capturas na pasta: **pare** e escreva em `docs/tarefas/notas/T18.md` o roteiro exato de captura que o mantenedor deve seguir (quais configurações mudar, em que ordem, como nomear os arquivos), **para o cabo e para o dongle**, incluindo os comandos para descobrir o VID:PID do dongle (`lsusb` antes e depois de plugar).
2. Com capturas: extraia os relatórios HID (`tshark -Y usbhid -T fields -e usbhid.data` ou `usb.data_fragment`), compare e deduza report ID, comando, posição do valor e checksum. Compare cabo e dongle.
3. Leia o descritor HID no Linux (`/sys/class/hidraw/hidrawN/device/report_descriptor`) e documente as usage pages e qual interface recebe os comandos.
4. Escreva o protocolo em `docs/protocolos/akko-3151-502f.md` e o perfil em `profiles/akko/akko-3151-502f.json`. O teste `cargo test -p raiza-profiles` valida todo perfil em `profiles/`.
5. Pesquise se já existe suporte em outro projeto (OpenRGB, projetos no GitHub). Se existir, documente e reaproveite.
6. Informe o VID:PID do dongle nas notas: o líder acrescenta a regra udev (`packaging/udev/70-raiza.rules` está fora do seu escopo).

## Fora de escopo
Enviar qualquer comando ao dispositivo real. Quem valida no hardware é o mantenedor, que depois marca `"verified": true`.

## Critérios de aceite
- [ ] O perfil passa em `cargo test -p raiza-profiles`
- [ ] Cada comando do perfil aponta, no documento do protocolo, para a captura que o originou
- [ ] Cabo e dongle comparados no documento (mesmo protocolo ou diferenças listadas)
- [ ] `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings` e `cargo test --all-features` passam
