# T19: PKGBUILD para o AUR e guia de instalação

- **Fase:** F5
- **Agente sugerido:** hydrogen
- **Depende de:** T04, T07, T13
- **Escopo:** `packaging/aur/**`, `docs/INSTALACAO.md`, `README.md` (seção de instalação)

## Objetivo
`raiza-git` instalável no Arch e derivados (o mantenedor usa CachyOS), com daemon, CLI, app, regra udev, unidade systemd e perfis.

## O que fazer
1. `packaging/aur/PKGBUILD` (`raiza-git`): build com `cargo build --release --locked` e install dos binários, `70-raiza.rules`, unidade de usuário, arquivo de ativação D-Bus, `.desktop`, ícone, metainfo e `profiles/` em `/usr/share/raiza/profiles`.
2. `.install` com a mensagem pós-instalação (`systemctl --user enable --now raizad`).
3. Atualizar `docs/INSTALACAO.md` e a seção do README.

## Critérios de aceite
- [ ] `namcap PKGBUILD` sem erros
- [ ] `makepkg -si` funciona numa cópia limpa (descreva nas notas)
