# T18: Protocolo do teclado Akko 3151:502f (com fio) e dongle Akko S9 Ultra 36eb:1087

- **Fase:** F4
- **Agente sugerido:** humano + hydrogen
- **Depende de:** T15
- **Escopo:** `docs/protocolos/akko-3151-502f.md`, `profiles/akko-3151-502f.json`

## Objetivo
Documentar o protocolo HID do teclado Akko 3151:502f (com fio) e dongle Akko S9 Ultra 36eb:1087 e escrever o perfil dele, inicialmente com `"verified": false`.

## Contexto
O fabricante não tem software para Linux. Capacidades desejadas: RGB (modo, cor, brilho) e, no dongle, bateria.
**Esta tarefa depende de capturas feitas pelo mantenedor**: tráfego USB do software oficial no Windows (USBPcap + Wireshark), mudando uma configuração por vez e anotando o valor. Os arquivos `.pcapng` ficam em `docs/protocolos/capturas/akko-3151-502f/`.

## O que fazer
1. Sem capturas na pasta: **pare** e escreva em `docs/tarefas/notas/T18.md` o roteiro exato de captura que o mantenedor deve seguir (quais configurações mudar, em que ordem, e como nomear os arquivos).
2. Com capturas: extrair os relatórios HID (`tshark -Y usbhid -T fields -e usbhid.data` ou `usb.data_fragment`), comparar e deduzir report id, comando, posição do valor e checksum.
3. Ler o descritor HID no Linux (`/sys/class/hidraw/hidrawN/device/report_descriptor`) e documentar as usage pages.
4. Escrever o protocolo em `docs/protocolos/akko-3151-502f.md` e o perfil em `profiles/akko-3151-502f.json`, validado contra `profiles/schema.json`.
5. Pesquisar se já existe suporte em outro projeto (OpenRGB, libratbag, projetos no GitHub). Se existir, documentar e reaproveitar.

## Fora de escopo
Enviar qualquer comando ao dispositivo real. Quem valida no hardware é o mantenedor, que depois marca `"verified": true`.

## Critérios de aceite
- [ ] Perfil passa na validação da T15 (há teste ou comando de validação)
- [ ] Cada comando do perfil aponta para a captura que o originou no documento do protocolo
