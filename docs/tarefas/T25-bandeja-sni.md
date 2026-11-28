# T25: Ícone de bandeja (StatusNotifierItem) para KDE e outros ambientes

- **Fase:** F6
- **Agente sugerido:** claude
- **Depende de:** T04
- **Escopo (arquivos que você pode criar ou alterar):**
  - `crates/raiza-tray/**` (crate nova)
  - `data/io.github.yurileo01.Raiza.Tray.desktop` (autostart)
  - `packaging/aur/PKGBUILD` (só o binário e o autostart novos)

## Objetivo
Um binário `raiza-tray` que mostra a bateria dos periféricos num ícone de bandeja SNI, para
KDE, XFCE, Cinnamon e GNOME com a extensão AppIndicator.

## Contexto
- API: `docs/DBUS.md`; use `raiza_core::dbus::RaizaClient` (feature `dbus`), que já converte
  JSON e sinais. O daemon ativa por D-Bus.
- SNI com a crate `ksni` (versão atual, assíncrona sobre zbus 5 + tokio, sem GTK). Confirme a
  versão com `cargo search ksni` e use uma publicada há pelo menos duas semanas.
- Leia `AGENTS.md`, "Regras de código": sem `unwrap()` em caminhos de E/S, `tracing`,
  comentários em português, identificadores em inglês.
- No GNOME a extensão da T24 já cobre o painel: o `.desktop` de autostart usa `NotShowIn=GNOME;`.
- Textos em português num módulo `text.rs` (a T26 traduz depois).

## O que fazer
1. Ícone com o nível da bateria mais baixa (`battery-level-N0[-charging]-symbolic`); tooltip
   com a lista; oculto/passivo (`Status::Passive`) sem baterias.
2. Menu: uma linha por aparelho com bateria, separador, "Abrir o Raiza" (lança o app id
   `io.github.yurileo01.Raiza` via `gtk-launch` ou `gio launch` do `.desktop`), "Sair".
3. Reage a `DeviceAdded`, `DeviceRemoved`, `CapabilityChanged`; reconecta se o daemon reiniciar.
4. Lógica (montar a lista e escolher o ícone) testável sem D-Bus.

## Fora de escopo
- Escritas. Extensão do GNOME (T24).

## Critérios de aceite
- [ ] `cargo run -p raiza-tray` com `raizad --mock` mostra o ícone num ambiente com SNI (teste
      com `busctl --user list | grep StatusNotifierWatcher` presente) ou, sem watcher, registra
      um aviso no log e espera, sem sair com erro.
- [ ] Testes da lógica de ícone e menu.
- [ ] `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings` e `cargo test --all-features` passam.
