# T07: Regra udev e unidade systemd do usuário

- **Fase:** F1
- **Agente sugerido:** hydrogen
- **Depende de:** —
- **Escopo:** `packaging/udev/**`, `packaging/systemd/**`, `docs/INSTALACAO.md`

## Objetivo
O usuário acessa os dispositivos sem root, e o daemon sobe sozinho com a sessão.

## Contexto
Dispositivos HID iniciais (os do mantenedor, que terão perfis nas T17/T18 ou passam pelo OpenRGB):

| vendor:product | Dispositivo |
|---|---|
| `24ae:1423` | Rapoo, mouse |
| `3151:502f` | Akko, teclado com fio |
| `36eb:1087` | Akko S9 Ultra, dongle 2.4 GHz |
| `1a86:e30b` | JIGUANG, controlador ARGB |

O microfone fifine (`3142:a010`) e o monitor LG (`043e:9a39`) **não** entram na regra hidraw: são atendidos por PipeWire e DDC/CI.
O nome do serviço D-Bus é `io.github.yurileo01.Raiza`, e o binário do daemon será instalado em `/usr/bin/raizad`.

## O que fazer
1. `packaging/udev/70-raiza.rules`: `TAG+="uaccess"` em `hidraw` **apenas** para os vendor:product da tabela acima, com um comentário por dispositivo. Use `SUBSYSTEM=="hidraw"` + `ATTRS{idVendor}`/`ATTRS{idProduct}` (minúsculas). Nunca libere todo hidraw (pilar 1).
2. Regra para `/dev/i2c-*` (DDC/CI) **restrita aos barramentos de controladoras de vídeo** (`ATTRS{class}` 0x030000/0x038000); nunca o SMBus da placa-mãe (pilar 1). Documentar a alternativa do grupo `i2c` e o módulo `i2c-dev`.
3. `packaging/systemd/raizad.service` (unidade de usuário, `Type=dbus`, `BusName=io.github.yurileo01.Raiza`) e o arquivo de ativação D-Bus `io.github.yurileo01.Raiza.service`.
4. `docs/INSTALACAO.md`: onde cada arquivo vai, `udevadm control --reload && udevadm trigger` e `systemctl --user enable --now raizad`.

## Critérios de aceite
- [x] `udevadm verify packaging/udev/70-raiza.rules` passa (se a versão do systemd tiver o comando)
- [x] `systemd-analyze --user verify packaging/systemd/raizad.service` sem erros (pode avisar que o binário não existe)
- [x] `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings` e `cargo test` passam
