# Roadmap e status das tarefas

Legenda de status: ⬜ a fazer · 🟦 em andamento · 🟨 em revisão · ✅ na `main` · ⏸ bloqueada

O agente é o sugerido. O líder pode trocar ao despachar.

## F0: Fundação (contratos; o líder faz)

| Id | Tarefa | Agente | Depende | Status |
|---|---|---|---|---|
| T01 | [Contratos do `raiza-core`](T01-contratos-core.md) | claude (líder) | — | ✅ |
| T02 | [Especificação da API D-Bus](T02-api-dbus.md) | claude (líder) | T01 | 🟦 |
| T03 | [CI no GitHub Actions](T03-ci.md) | hydrogen | — | ✅ |

## F1: Daemon base

| Id | Tarefa | Agente | Depende | Status |
|---|---|---|---|---|
| T04 | [Esqueleto do `raizad`](T04-daemon-base.md) | claude | T01, T02 | ⬜ |
| T05 | [Provedor udev: descoberta e hotplug](T05-provider-udev.md) | hydrogen | T01 | 🟦 |
| T06 | [`raiza-ctl`: CLI cliente](T06-raiza-ctl.md) | hydrogen | T02 | ⬜ |
| T07 | [Regra udev e unidade systemd do usuário](T07-udev-systemd.md) | hydrogen | — | ✅ |

## F2: Provedores genéricos (aproveitar o que já existe)

| Id | Tarefa | Agente | Depende | Status |
|---|---|---|---|---|
| T08 | [Provedor BlueZ + UPower](T08-provider-bluez-upower.md) | hydrogen | T01 | 🟦 |
| T09 | [Provedor PipeWire](T09-provider-pipewire.md) | hydrogen | T01 | 🟦 |
| T10 | [Provedor DDC/CI (monitores)](T10-provider-ddc.md) | hydrogen | T01 | 🟦 |
| T11 | [Provedor evdev (controles)](T11-provider-evdev.md) | hydrogen | T01 | 🟦 |
| T12 | [Provedor OpenRGB](T12-provider-openrgb.md) | hydrogen | T01 | 🟦 |

## F3: Interface GTK4/libadwaita

| Id | Tarefa | Agente | Depende | Status |
|---|---|---|---|---|
| T13 | [App base: lista de dispositivos](T13-gtk-base.md) | claude | T02, T04 | ⬜ |
| T14 | [Páginas por capacidade](T14-gtk-capacidades.md) | hydrogen | T13 | ⬜ |

## F4: Perfis HID declarativos (dispositivos sem suporte)

| Id | Tarefa | Agente | Depende | Status |
|---|---|---|---|---|
| T15 | [Formato e validação de perfis](T15-perfis-formato.md) | claude (líder) | T01 | 🟦 |
| T16 | [Executor de perfis via hidraw](T16-perfis-executor.md) | claude | T15, T04 | ⬜ |
| T17 | [Protocolo do mouse Rapoo 24ae:1423](T17-protocolo-rapoo.md) | humano + hydrogen | T15 | ⬜ |
| T18 | [Protocolo do teclado Akko 3151:502f / 36eb:1087](T18-protocolo-akko.md) | humano + hydrogen | T15 | ⬜ |

## F5: Distribuição

| Id | Tarefa | Agente | Depende | Status |
|---|---|---|---|---|
| T19 | [PKGBUILD para o AUR e guia de instalação](T19-aur.md) | hydrogen | T04, T07, T13 | ⬜ |

## Ordem sugerida das levas

1. **Leva 1:** T01 (líder) em paralelo com T03 e T07.
2. **Leva 2:** T02 (líder) em paralelo com T05, T08, T09, T10, T11, T12 e T15 (líder).
3. **Leva 3:** T04, T06, T16, T17, T18.
4. **Leva 4:** T13, depois T14.
5. **Leva 5:** T19.
