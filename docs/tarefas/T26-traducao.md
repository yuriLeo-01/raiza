# T26: Interface em inglês e português (gettext)

- **Fase:** F6
- **Agente sugerido:** claude
- **Depende de:** T23, T25 (para não conflitar em `raiza-gtk` e `raiza-tray`)
- **Escopo (arquivos que você pode criar ou alterar):**
  - `crates/raiza-gtk/**`, `crates/raiza-tray/**`, `crates/raiza-ctl/**`, `crates/raizad/src/battery.rs`
  - `po/**` (novo), `packaging/aur/PKGBUILD` (compilar e instalar os `.mo`)

## Objetivo
Os textos do app, da bandeja, do CLI e dos avisos de bateria aparecem em inglês por padrão e em
português do Brasil quando o sistema está em `pt_BR`.

## Contexto
- Hoje os textos estão em português, centralizados em `text.rs` (GTK) e espalhados no CLI e em
  `crates/raizad/src/battery.rs`.
- Use `gettext-rs` (domínio `raiza`), msgids em inglês, `po/pt_BR.po` com os textos atuais,
  `po/POTFILES`. Plurais com `ngettext`. Comentários do código continuam em português.
- A extensão do GNOME (T24) tem o próprio `po/` dentro de `extensions/gnome/`; não mexa nela.

## Fora de escopo
- Mensagens de log (`tracing`) e erros internos: ficam em português.
- Outras línguas além de en e pt_BR.

## Critérios de aceite
- [ ] `LANGUAGE=en raiza-gtk` e `LANGUAGE=pt_BR raiza-gtk` mostram cada língua.
- [ ] Nenhum texto visível ao usuário fora do gettext (`grep` documentado no relatório).
- [ ] `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings` e `cargo test --all-features` passam.
