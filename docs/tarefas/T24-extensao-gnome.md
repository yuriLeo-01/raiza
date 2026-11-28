# T24: Extensão do GNOME Shell: bateria dos periféricos no painel

- **Fase:** F6
- **Agente sugerido:** claude
- **Depende de:** T04 (API D-Bus estável)
- **Escopo (arquivos que você pode criar ou alterar):**
  - `extensions/gnome/**`
  - `packaging/aur/PKGBUILD` (só a instalação da extensão)
  - `README.md` e `README.pt-BR.md` (só uma linha na seção "What works"/"O que funciona")

## Objetivo
Uma extensão do GNOME Shell (GNOME 46 a 51) que mostra, no painel, a bateria dos periféricos
que o `raizad` conhece, sem abrir o app.

## Contexto
- API: `docs/DBUS.md`. Barramento de sessão, serviço `io.github.yurileo01.Raiza.Daemon`,
  objeto `/io/github/yurileo01/Raiza`, interface `io.github.yurileo01.Raiza1`. `ListDevices()`
  devolve JSON de `[DeviceInfo]`; sinais `DeviceAdded(s)`, `DeviceRemoved(s)`,
  `CapabilityChanged(s id, s capability)`. Bateria = capacidade `{"kind":"battery","percent","charging"}`.
- O daemon ativa por D-Bus; se não estiver rodando, a extensão mostra nada (sem erro na tela)
  e volta a funcionar quando o nome aparecer (`Gio.bus_watch_name`).
- O mantenedor usa GNOME Shell 51.1 (gjs 1.90). Use módulos ES (`import ... from 'gi://...'`,
  `resource:///org/gnome/shell/...`), `Extension` de `extension.js` com `enable()`/`disable()`
  que limpam tudo (sinais, timers, widgets), como exige a revisão do extensions.gnome.org.
- UUID: `raiza@yurileo01.github.io`. App id do app GTK, para o item "Abrir o Raiza":
  `io.github.yurileo01.Raiza` (`Gio.DesktopAppInfo`).
- Ícones simbólicos do tema: `battery-level-N0-symbolic`, `battery-level-N0-charging-symbolic`.
- Textos: use o gettext da extensão (`this.gettext`/`_()`), msgids em inglês, com
  `po/pt_BR.po` na pasta da extensão.

## O que fazer
1. Indicador no painel (ou item em Configurações rápidas): ícone com a bateria mais baixa entre
   os aparelhos; oculto quando nenhum aparelho tem bateria.
2. Menu: uma linha por aparelho com bateria (nome, ícone, %, "carregando"), e "Abrir o Raiza".
3. Atualiza por sinais, sem polling.
4. Instalação: `make`/script simples em `extensions/gnome/` que gera o zip
   (`gnome-extensions pack`); PKGBUILD instala em `/usr/share/gnome-shell/extensions/<uuid>`.
5. `extensions/gnome/README.md` curto: instalar, ativar (`gnome-extensions enable <uuid>`), testar
   com `cargo run -p raizad -- --mock` e `dbus-run-session -- gnome-shell --devkit` (GNOME 49+) ou `--nested`.

## Fora de escopo
- Ícone SNI para outros ambientes (T25). Escritas (DPI etc.): a extensão só lê.
- Publicar no extensions.gnome.org (o mantenedor decide depois).

## Critérios de aceite
- [ ] `gnome-extensions pack` gera o zip sem erro; `metadata.json` lista as versões 46–51.
- [ ] Com o daemon `--mock` numa sessão aninhada, o indicador mostra as baterias e reage a sinais.
- [ ] `disable()` desfaz tudo (sem vazamento de sinais).
- [ ] O PKGBUILD continua passando no `makepkg --nobuild` (não instale nada no sistema).

## Como testar com hardware real (para o mantenedor)
Instalar, fazer logout/login (Wayland), ativar a extensão e conferir a bateria do Rapoo, do
teclado e do headset.
