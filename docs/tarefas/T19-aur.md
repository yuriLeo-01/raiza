# T19: PKGBUILD para o AUR e guia de instalação

- **Fase:** F5
- **Agente sugerido:** sonnet
- **Depende de:** T04, T07, T13
- **Escopo:** `packaging/aur/**`, `docs/INSTALACAO.md`, `README.md` (seção de instalação)

## Objetivo
`raiza-git` instalável no Arch e derivados (o mantenedor usa CachyOS), com daemon, CLI, app, regra udev, unidade systemd e perfis.

## Contexto
- Repositório: `https://github.com/yuriLeo-01/raiza` (público). Licença `GPL-3.0-or-later`.
- Binários: `raizad` (daemon), `raiza-ctl` (CLI), `raiza-gtk` (app GTK4/libadwaita).
- Arquivos e destinos:

| Origem | Destino |
|---|---|
| `packaging/udev/70-raiza.rules` | `/usr/lib/udev/rules.d/70-raiza.rules` |
| `packaging/systemd/raizad.service` | `/usr/lib/systemd/user/raizad.service` |
| `packaging/systemd/io.github.yurileo01.Raiza.Daemon.service` | `/usr/share/dbus-1/services/io.github.yurileo01.Raiza.Daemon.service` |
| `data/io.github.yurileo01.Raiza.desktop` | `/usr/share/applications/` |
| `data/io.github.yurileo01.Raiza.metainfo.xml` | `/usr/share/metainfo/` |
| `data/icons/hicolor/**` | `/usr/share/icons/hicolor/**` |
| `profiles/**` | `/usr/share/raiza/profiles/**` (o executor lê daí; o usuário pode pôr os seus em `~/.local/share/raiza/profiles`) |
| `raiza-ctl completions bash\|zsh\|fish` (gerado no build) | `/usr/share/bash-completion/completions/raiza-ctl`, `/usr/share/zsh/site-functions/_raiza-ctl`, `/usr/share/fish/vendor_completions.d/raiza-ctl.fish` |

- O nome D-Bus do daemon é `io.github.yurileo01.Raiza.Daemon`. O app id do GTK é `io.github.yurileo01.Raiza`.
- Dependências: `gtk4`, `libadwaita`, `systemd-libs` (libudev), `dbus`. Opcionais: `pipewire` + `wireplumber` (volume e mudo, usa `pw-dump` e `wpctl`), `bluez` (Bluetooth), `upower` (bateria). Build: `cargo`, `git`. O HID usa o backend nativo do `hidapi` (sem a biblioteca C).
- Siga as diretrizes de Rust do Arch: `cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"` no `prepare`, `cargo build --frozen --release` no `build`, `cargo test --frozen` no `check`.
- **Não instale nada no sistema** (sem `sudo`, sem `pacman -U`, sem `makepkg -i`). Instalar é passo do mantenedor.
- O pacote `-git` baixa do GitHub. Para testar o build com o código local, use uma cópia do PKGBUILD com a `source` apontando para o repositório local (`git+file://…`) num diretório temporário, e descreva nas notas.

## O que fazer
1. `packaging/aur/PKGBUILD` (`raiza-git`) com `pkgver()` a partir do git, build e `package()` instalando tudo da tabela.
2. `packaging/aur/raiza-git.install` com a mensagem pós-instalação: `systemctl --user enable --now raizad` e "desplugue e plugue de novo os aparelhos para a regra udev valer" (o pacman do Arch já recarrega as regras udev).
3. `packaging/aur/.SRCINFO` gerado com `makepkg --printsrcinfo`.
4. Atualizar `docs/INSTALACAO.md` (seção "Arch e derivados" antes da instalação manual) e a seção de instalação do `README.md`.

## Critérios de aceite
- [x] `makepkg` (sem `-i`) constrói o pacote numa cópia limpa com a fonte local; o conteúdo (`tar -tf` do `.pkg.tar.zst`) tem todos os arquivos da tabela (lista nas notas)
- [x] `namcap` sem erros, se estiver disponível (`pacman -Q namcap`); se não estiver, diga nas notas
- [x] `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings` e `cargo test --all-features` passam

## Como testar com hardware real (para o mantenedor)
`cd packaging/aur && makepkg -si`, depois `systemctl --user enable --now raizad`, desplugar e plugar o mouse e o headset, e `raiza-ctl list`: a bateria do Rapoo e do S9 Ultra deve aparecer.
