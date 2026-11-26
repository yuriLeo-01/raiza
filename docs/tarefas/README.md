# Roadmap e status das tarefas

Legenda de status: ⬜ a fazer · 🟦 em andamento · 🟨 em revisão · ✅ na `main` · ⏸ bloqueada ou adiada · ❌ cancelada

**Escopo (2026-11-25):** foco em mouse, teclado, headset e microfone. Controles com prioridade
menor. Monitores fora do escopo. OpenRGB adiado. Veja `docs/PILARES.md`.

O agente é o sugerido. O líder pode trocar ao despachar. Subagentes: só Claude (o hydrogen foi
descartado em 2026-11-25); `sonnet` para tarefas simples.

## F0: Fundação (contratos; o líder faz)

| Id | Tarefa | Agente | Depende | Status |
|---|---|---|---|---|
| T01 | [Contratos do `raiza-core`](T01-contratos-core.md) | claude (líder) | — | ✅ |
| T02 | [Especificação da API D-Bus](T02-api-dbus.md) | claude (líder) | T01 | ✅ |
| T03 | [CI no GitHub Actions](T03-ci.md) | hydrogen | — | ✅ |

## F1: Daemon base

| Id | Tarefa | Agente | Depende | Status |
|---|---|---|---|---|
| T04 | [Esqueleto do `raizad`](T04-daemon-base.md) | claude | T01, T02 | ✅ |
| T05 | [Provedor udev: descoberta e hotplug](T05-provider-udev.md) | claude | T01 | ✅ |
| T06 | [`raiza-ctl`: CLI cliente](T06-raiza-ctl.md) | sonnet | T02 | ✅ |
| T07 | [Regra udev e unidade systemd do usuário](T07-udev-systemd.md) | hydrogen | — | ✅ |

## F2: Provedores genéricos (aproveitar o que já existe)

| Id | Tarefa | Agente | Depende | Status |
|---|---|---|---|---|
| T08 | [Provedor BlueZ + UPower](T08-provider-bluez-upower.md) | claude | T01 | ✅ |
| T09 | [Provedor PipeWire](T09-provider-pipewire.md) | claude | T01 | ✅ |
| T10 | [Provedor DDC/CI (monitores)](T10-provider-ddc.md) | — | T01 | ❌ cancelada: monitores fora do escopo |
| T11 | [Provedor evdev (controles)](T11-provider-evdev.md) | claude | T01 | ⬜ prioridade menor |
| T12 | [Provedor OpenRGB](T12-provider-openrgb.md) | claude | T01 | ⏸ adiada |

## F3: Interface GTK4/libadwaita

| Id | Tarefa | Agente | Depende | Status |
|---|---|---|---|---|
| T13 | [App base: lista de dispositivos](T13-gtk-base.md) | claude | T02, T04 | 🟦 retomada |
| T14 | [Páginas por capacidade](T14-gtk-capacidades.md) | claude | T13 | ⬜ |

## F4: Perfis HID declarativos (dispositivos sem suporte)

| Id | Tarefa | Agente | Depende | Status |
|---|---|---|---|---|
| T15 | [Formato e validação de perfis](T15-perfis-formato.md) | claude (líder) | T01 | ✅ |
| T16 | [Executor de perfis via hidraw](T16-perfis-executor.md) | claude | T15, T04 | 🟦 |
| T17 | [Protocolo do mouse Rapoo 24ae:1423](T17-protocolo-rapoo.md) | humano + claude | T15 | ⏸ perfil de leitura na `main` (pesquisa); escrita e DPI aguardam teste no aparelho |
| T18 | [Protocolo do teclado Akko (3151:502f com fio + dongle próprio)](T18-protocolo-akko.md) | humano + claude | T15 | ⏸ aguardando capturas (pesquisa nas notas) |
| T20 | [Headset Akko S9 Ultra 36eb:1087: protocolo, bateria, sidetone e EQ](T20-headset-akko-s9.md) | humano + claude | T15 | ⏸ perfil de bateria na `main` (pesquisa); sidetone e EQ aguardam capturas |
| T21 | [Capacidades `Sidetone` e `Equalizer` (contrato)](T21-sidetone-eq.md) | claude (líder) | T20 | ⏸ aguardando T20 |

## F5: Distribuição

| Id | Tarefa | Agente | Depende | Status |
|---|---|---|---|---|
| T19 | [PKGBUILD para o AUR e guia de instalação](T19-aur.md) | claude | T04, T07, T13 | ⬜ |

## Ordem sugerida das levas

1. **Leva 1:** T01 (líder) em paralelo com T03 e T07.
2. **Leva 2:** T02 e T15 (líder), T05, T08 e T09. No máximo 2 subagentes por vez (cota).
   T10 cancelada e T12 adiada (escopo). T11 passou para depois da leva 3 (prioridade menor).
3. **Leva 3:** T04 e T06. Depois o app gráfico: T13, depois T14. Depois T16 e T11.
   T17, T18 e T20 aguardam capturas (o mantenedor não vai capturar por enquanto). A pesquisa em
   `notas/T17.md`, `notas/T18.md` e `notas/T20.md` mostra que parte delas pode sair sem capturas.
4. **Leva 4:** T19.
