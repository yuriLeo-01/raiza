# Extensão do GNOME Shell do Raiza

Mostra no painel a bateria dos periféricos que o daemon `raizad` conhece (mouse, teclado, headset),
sem abrir o app. O ícone mostra a bateria mais baixa. O menu tem uma linha por aparelho e o item
"Abrir o Raiza". A extensão só lê: não muda DPI nem nada nos aparelhos.

- GNOME Shell 46 a 51.
- Atualiza pelos sinais D-Bus do daemon (`DeviceAdded`, `DeviceRemoved`, `CapabilityChanged`), sem polling.
- Sem o daemon no barramento, o ícone some. Ele volta quando o daemon aparecer.
- UUID: `raiza@yurileo01.github.io`.

## Instalar

Pelo pacote do AUR (`raiza-git`), a extensão já vai para `/usr/share/gnome-shell/extensions/`.

A partir do código, só para o seu usuário:

```sh
cd extensions/gnome
make            # gera raiza@yurileo01.github.io.shell-extension.zip (gnome-extensions pack)
make install    # gnome-extensions install --force do zip
```

No Wayland, faça logout e login para o Shell enxergar a extensão nova. Depois ative:

```sh
gnome-extensions enable raiza@yurileo01.github.io
```

## Testar sem hardware

Numa sessão aninhada, com o daemon de demonstração no mesmo barramento:

```sh
make install
dbus-run-session -- bash -c '
  cargo run -p raizad -- --mock &
  gnome-shell --devkit      # GNOME 49+; nas versões anteriores, gnome-shell --nested --wayland
'
```

Dentro da sessão aninhada, ative a extensão (`gnome-extensions enable raiza@yurileo01.github.io`
num terminal dela, ou pelo app Extensões). O mock traz um mouse em 80 % e um headset em 45 %,
carregando. Pare o daemon e o ícone some; suba de novo e ele volta.

Logs da extensão: `journalctl --user -f -o cat /usr/bin/gnome-shell` (ou a saída do terminal da
sessão aninhada). Avisos começam com `Raiza:`.

## Tradução

Os textos ficam em inglês no código e são traduzidos por `po/*.po` (domínio
`raiza-shell-extension`). `make pot` atualiza o modelo `po/raiza-shell-extension.pot`.
