# Raiza

**Um hub único para gerenciar seus periféricos no Linux, de qualquer marca, mesmo sem suporte do fabricante.**

Mouse, teclado, headset, controle, microfone, monitor e RGB, tudo num lugar só, com
interface nativa do GNOME (e, no futuro, do KDE).

> 🚧 **Em planejamento.** Nada funciona ainda. Acompanhe o roadmap em [`docs/tarefas/README.md`](docs/tarefas/README.md).

## Por quê?

Os fabricantes de periféricos quase nunca lançam software para Linux. Hoje, quem usa
Linux precisa juntar várias ferramentas diferentes, uma por marca, quando ela existe. O Raiza
junta tudo num hub e, para os dispositivos que ninguém suporta, permite descrever o
protocolo em **perfis JSON** que qualquer pessoa pode contribuir.

## Como funciona

```
raizad (daemon Rust) ── D-Bus ──► raiza-gtk (GTK4/libadwaita) · raiza-ctl (CLI)
   ├── BlueZ / UPower   → Bluetooth e bateria
   ├── PipeWire         → volume e mudo
   ├── DDC/CI           → brilho do monitor
   ├── evdev            → controles
   ├── OpenRGB          → RGB
   └── perfis HID       → dispositivos sem suporte, via /dev/hidraw
```

- [Pilares do projeto](docs/PILARES.md)
- [Arquitetura](docs/ARQUITETURA.md)
- [Como o projeto é desenvolvido (Claude líder + subagentes)](docs/ORQUESTRACAO.md)

## Compilar

```sh
cargo build
```

Dependências de sistema (Arch): `rust gtk4 libadwaita hidapi systemd-libs dbus`.

## Licença

GPL-3.0-or-later.
