# T13: App GTK4/libadwaita base

- **Fase:** F3
- **Agente sugerido:** claude
- **Depende de:** T02, T04
- **Escopo:** `crates/raiza-gtk/**`, `data/**` (ícones, .desktop, metainfo)

## Objetivo
Janela libadwaita no estilo do app Configurações do GNOME: barra lateral com os dispositivos agrupados por tipo e uma página de detalhes.

## Contexto
`gtk4`, `libadwaita` e `relm4`. Cliente do daemon pelo proxy D-Bus do `raiza-core`. Siga as GNOME HIG: `AdwNavigationSplitView`, `AdwPreferencesPage` e `AdwStatusPage` para o estado vazio ou sem daemon. App id: `io.github.yurileo01.Raiza`.

## O que fazer
1. Lista lateral com ícone por `DeviceKind`, nome e bateria (quando houver).
2. Atualização ao vivo pelos sinais D-Bus.
3. Página de detalhes genérica que mostra as capacidades como linhas somente leitura (os controles interativos ficam na T14).
4. Estado "daemon não está rodando" com botão para tentar de novo.
5. `.desktop`, ícone simbólico e `metainfo.xml`.

## Critérios de aceite
- [ ] Com `raizad --mock`, o app lista e atualiza os dispositivos (screenshot nas notas)
- [ ] `desktop-file-validate` e `appstreamcli validate` passam
- [ ] `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings` e `cargo test` passam
