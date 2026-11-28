# Traduções do Raiza

Catálogos gettext do app GTK (`raiza-gtk`), da bandeja (`raiza-tray`), do CLI (`raiza-ctl`)
e dos avisos de bateria do daemon (`raizad`). O domínio é `raiza`. A extensão do GNOME
Shell tem o próprio `po/`, em `extensions/gnome/po/`.

- Os textos do código são os msgids, em **inglês**, sempre literais dentro de
  `raiza_i18n::gettext("…")` ou `raiza_i18n::ngettext("…", "…", n)`. Valores entram por
  nome, com `raiza_i18n::fill(texto, &[("nome", &valor)])`, para o tradutor poder mudar a
  ordem (`{name}: {percent}%`).
- Um comentário `// Translators: …` logo antes da chamada vai para o tradutor no `.pot`.
- `POTFILES`: arquivos com textos traduzíveis. `LINGUAS`: línguas com tradução.

## Fluxo

```sh
po/atualizar.sh          # regera raiza.pot e junta as novidades nos .po (gettext ≥ 0.24)
po/compilar.sh           # compila em target/locale/<língua>/LC_MESSAGES/raiza.mo
po/conferir.sh           # lista literais fora do gettext, para revisar
RAIZA_LOCALEDIR=target/locale LANGUAGE=pt_BR cargo run -p raiza-gtk
```

O pacote instala os `.mo` em `/usr/share/locale/<língua>/LC_MESSAGES/raiza.mo`.

## Como a língua é escolhida (`crates/raiza-i18n`)

Vale a primeira variável não vazia entre `LANGUAGE`, `LC_ALL`, `LC_MESSAGES` e `LANG`
(`LANGUAGE` aceita lista: `pt_BR:en`). `pt_BR.UTF-8` tenta `pt_BR` e depois `pt`.
`C`, `POSIX` e `en*` dão inglês. Os catálogos são procurados primeiro em `RAIZA_LOCALEDIR`
e depois em `/usr/share/locale`.

## Nova língua

1. `msginit -l <língua> -i po/raiza.pot -o po/<língua>.po` e traduza.
2. Acrescente `<língua>` em `po/LINGUAS`.
3. `po/compilar.sh` precisa passar (o `msgfmt --check` confere os `{nomes}`).
