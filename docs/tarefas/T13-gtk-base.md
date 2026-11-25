# T13: App GTK4/libadwaita base

- **Fase:** F3
- **Agente sugerido:** claude
- **Depende de:** T02, T04
- **Escopo:** `crates/raiza-gtk/**`, `data/**` (ícones, .desktop, metainfo)

## Objetivo
Janela libadwaita no estilo do app Configurações do GNOME: barra lateral com os dispositivos agrupados por tipo e uma página de detalhes.

## Contexto
- `gtk4`, `libadwaita` e `relm4` (GTK 4.24 e libadwaita 1.10 instalados na máquina do mantenedor). Siga as GNOME HIG: `AdwNavigationSplitView`, `AdwPreferencesPage` e `AdwStatusPage` para o estado vazio ou sem daemon. App id: `io.github.yurileo01.Raiza`.
- Cliente do daemon: `raiza_core::dbus::RaizaClient` (feature `dbus`): `connect_session`, `list_devices`, `get_device`, `set_capability`, `version`, `events` (fluxo de `ProviderEvent`). Contrato em `docs/DBUS.md`. Erros chegam como `ClientError::Raiza(RaizaError)`.
- O `zbus` usa **tokio**: faça as chamadas D-Bus nos *commands* do relm4 (que rodam num runtime tokio), nunca na thread da interface.
- `DeviceRemoved` seguido de `DeviceAdded` com o mesmo id é **atualização** (`docs/DBUS.md`): mantenha a página do dispositivo aberta, sem piscar.
- Para testar, rode o daemon de demonstração: `cargo run -p raizad -- --mock` (dois dispositivos fictícios) ou o daemon real `cargo run -p raizad`. Com o mock, `raiza-ctl` (`cargo run -p raiza-ctl -- set …`) muda valores por fora, para ver a atualização ao vivo.
- **A T14 vem depois** e acrescenta um widget interativo por capacidade em `crates/raiza-gtk/src/capabilities/`. Deixe nesta tarefa um **ponto único** onde a página de detalhes pede "a linha para esta capacidade", hoje devolvendo a linha somente leitura, para a T14 só trocar a implementação.

## O que fazer
1. Lista lateral com ícone por `DeviceKind`, nome e bateria (quando houver).
2. Atualização ao vivo pelos sinais D-Bus.
3. Página de detalhes genérica que mostra as capacidades como linhas somente leitura (os controles interativos ficam na T14).
4. Estado "daemon não está rodando" com botão para tentar de novo.
5. `.desktop`, ícone simbólico e `metainfo.xml`.
6. Separe a lógica (agrupamento por tipo, ordenação, texto de cada capacidade, aplicação dos eventos ao modelo) da interface, para testá-la sem GTK.

## Critérios de aceite
- [ ] Com `raizad --mock`, o app lista e atualiza os dispositivos (descreva nas notas como testou; screenshot se conseguir)
- [ ] Testes da lógica sem GTK (agrupamento, texto das capacidades, aplicação de `ProviderEvent` ao modelo, inclusive o par Removed+Added)
- [ ] `desktop-file-validate` e `appstreamcli validate` passam
- [ ] `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings` e `cargo test --all-features` passam
