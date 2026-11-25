# T07: Regra udev e unidade systemd do usuário

- **Fase:** F1
- **Agente sugerido:** hydrogen
- **Depende de:** —
- **Escopo:** `packaging/udev/**`, `packaging/systemd/**`, `docs/INSTALACAO.md`

## Objetivo
O usuário acessa os dispositivos sem root, e o daemon sobe sozinho com a sessão.

## O que fazer
1. `packaging/udev/70-raiza.rules`: `TAG+="uaccess"` em `hidraw` **apenas** para vendor:product listados (comece pelos da T05), com um comentário por dispositivo. Nunca libere todo hidraw (pilar 1).
2. Regra para `/dev/i2c-*` (DDC/CI). Documentar a alternativa do grupo `i2c` e o módulo `i2c-dev`.
3. `packaging/systemd/raizad.service` (unidade de usuário, `Type=dbus`, `BusName=io.github.yurileo01.Raiza`) e o arquivo de ativação D-Bus `io.github.yurileo01.Raiza.service`.
4. `docs/INSTALACAO.md`: onde cada arquivo vai, `udevadm control --reload && udevadm trigger` e `systemctl --user enable --now raizad`.

## Critérios de aceite
- [ ] `udevadm verify packaging/udev/70-raiza.rules` passa (se a versão do systemd tiver o comando)
- [ ] `systemd-analyze --user verify packaging/systemd/raizad.service` sem erros (pode avisar que o binário não existe)
- [ ] `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings` e `cargo test` passam
