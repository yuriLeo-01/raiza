# Raiza

**Um hub único para gerenciar seus periféricos no Linux, de qualquer marca, mesmo sem suporte do fabricante.**

Mouse, teclado, headset e microfone (e controles, com prioridade menor) num lugar só: bateria,
DPI, taxa de atualização (polling), volume, mudo e conexão, com um app nativo do GNOME
(GTK4/libadwaita) e uma linha de comando.

> 🧪 **Versão de testes (0.0.1).** Já funciona no dia a dia com os aparelhos da tabela abaixo,
> mas ainda pode mudar bastante. Acompanhe o roadmap em [`docs/tarefas/README.md`](docs/tarefas/README.md).

## Por quê?

Os fabricantes de periféricos quase nunca lançam software para Linux. Hoje, quem usa Linux precisa
juntar várias ferramentas diferentes, uma por marca, quando ela existe. O Raiza junta tudo num hub e,
para os aparelhos que ninguém suporta, permite descrever o protocolo em **perfis JSON**: qualquer
pessoa pode contribuir um perfil, sem escrever código.

## O que funciona

### Com qualquer marca

O Raiza aproveita o que o próprio Linux já sabe sobre o aparelho:

| Recurso | De onde vem | Exemplos |
|---|---|---|
| Bateria | UPower (drivers do kernel) | Logitech sem fio, controles de PS4/PS5, muitos mouses sem fio |
| Bateria, conectar e desconectar | BlueZ (Bluetooth) | fones e mouses com o serviço de bateria padrão |
| Volume e mudo | PipeWire | qualquer headset ou microfone |
| Vibração | evdev | controles de jogo |
| Lista de aparelhos conectados | udev | tudo o que está plugado |

Aparelhos que só dizem "conectado" (hubs, controladoras sem nada ligado) ficam escondidos por padrão.
O botão de olho no app e `raiza-ctl list --all` mostram todos.

### Com perfil (por modelo)

DPI, polling e bateria em protocolo próprio dependem de um perfil para o modelo, em
[`profiles/`](profiles/). Perfis novos começam **somente leitura** e só passam a gravar depois de
testados no aparelho (veja [segurança](#segurança)).

| Aparelho | Conexão | Lê | Grava |
|---|---|---|---|
| Rapoo VT2 Max / VT2 Pro Max | dongle `24ae:1423`, cabo `24ae:4623` | bateria, carregando, DPI, polling | DPI, polling |
| Akko Verge S9 Ultra (headset) | dongle `36eb:1087` | bateria | — |
| Akko 5087 V3 HE (teclado) | dongle `3151:5038` | bateria | — |

Tem um aparelho que não está aqui? [Peça suporte](https://github.com/yuriLeo-01/raiza/issues/new?template=aparelho.yml) ou
[contribua um perfil](CONTRIBUTING.md#2-contribuir-um-perfil).

## Instalar

### Arch e derivados (CachyOS, EndeavourOS, Manjaro…)

```sh
git clone https://github.com/yuriLeo-01/raiza.git
cd raiza/packaging/aur
makepkg -si
systemctl --user enable --now raizad
```

Depois, **desplugue e plugue de novo** os aparelhos para a regra udev valer. Abra o app "Raiza" pelo
menu de aplicativos.

Para outras distribuições, veja a instalação manual em [`docs/INSTALACAO.md`](docs/INSTALACAO.md).

## Usar pelo terminal

```sh
raiza-ctl list                                  # aparelhos e capacidades
raiza-ctl get usb:24ae:1423:2607-54L            # detalhes de um aparelho
raiza-ctl set usb:24ae:1423:2607-54L dpi 1600
raiza-ctl set usb:24ae:1423:2607-54L polling-rate 2000
raiza-ctl watch                                 # acompanha as mudanças ao vivo
```

`--json` em `list`, `get` e `watch` dá uma saída para scripts. `raiza-ctl completions bash|zsh|fish`
gera o autocompletar (o pacote já instala).

## Segurança

Mandar um comando errado para um periférico pode estragar o aparelho. Por isso o Raiza segue
regras rígidas ([pilar 1](docs/PILARES.md)):

- **Nenhum comando sai sem estar descrito e validado** num perfil. Toda escrita é conferida (limites,
  tamanho, checksum) imediatamente antes de ir para o aparelho.
- **Perfis novos são somente leitura** (`"verified": false`) até alguém testar no aparelho de
  verdade e comparar com o software oficial.
- **Comandos conhecidos por apagar firmware ou entrar no modo de gravação são bloqueados** na
  validação, mesmo que um perfil os peça.
- Quando o mesmo id USB é usado por aparelhos diferentes, o perfil exige uma **identificação** do
  aparelho antes de qualquer escrita.
- O acesso aos aparelhos vem de uma regra udev que libera **só os modelos suportados**, para o
  usuário da sessão, sem rodar nada como root.

## Como funciona

```
raizad (daemon, sessão do usuário) ── D-Bus ──► raiza-gtk (app) · raiza-ctl (CLI)
   ├── perfis HID → /dev/hidraw  (DPI, polling, bateria em protocolo próprio)
   ├── BlueZ      → Bluetooth e bateria
   ├── UPower     → bateria
   ├── PipeWire   → volume e mudo
   ├── evdev      → vibração de controles
   └── udev       → lista de aparelhos e hotplug
```

Quando mais de uma fonte conhece o mesmo aparelho, o daemon junta as informações, e a mais
específica (o perfil) tem prioridade.

- [Pilares do projeto](docs/PILARES.md)
- [Arquitetura](docs/ARQUITETURA.md)
- [API D-Bus](docs/DBUS.md), para quem quer fazer outra interface
- [Formato dos perfis](docs/PERFIS.md)
- [Protocolos documentados](docs/protocolos/)
- [Como o projeto é desenvolvido (Claude líder + subagentes)](docs/ORQUESTRACAO.md)

## Desenvolver

Dependências (Arch): `rust gtk4 libadwaita systemd-libs dbus`. O acesso HID usa o backend nativo
do `hidapi`, sem a biblioteca C.

```sh
cargo build
systemctl --user stop raizad      # se o Raiza estiver instalado: libera o nome na D-Bus
cargo run -p raizad -- --mock     # daemon com aparelhos de mentira, sem tocar em hardware
cargo run -p raiza-gtk            # o app, ligado a esse daemon
```

Antes de mandar um pull request:

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

Veja [`CONTRIBUTING.md`](CONTRIBUTING.md).

## Licença

[GPL-3.0-or-later](LICENSE).
