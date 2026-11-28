#!/bin/sh
# Lista literais de texto fora do gettext no código das interfaces (sem os módulos de
# teste, logs e comentários). O que sobra deve ser nome de ícone, id, formato ou marca.
# Também ficam de fora as linhas que são só um literal seguido de vírgula (a continuação
# de uma chamada `gettext(` quebrada pelo rustfmt) e os pares `("nome", valor)` do `fill`.
# Atenção: o texto precisa ser um literal DENTRO de `gettext(...)`; passado por variável,
# o xgettext não o extrai.
# Uso, na raiz do repositório: po/conferir.sh
cd "$(dirname "$0")/.." || exit 1
for f in $(find crates/raiza-gtk/src crates/raiza-tray/src crates/raiza-ctl/src -name '*.rs') \
    crates/raizad/src/battery.rs; do
    awk -v f="$f" '/^#\[cfg\(test\)\]/ { exit } { print f ":" FNR ": " $0 }' "$f"
done |
    grep -E '"[^"]*[[:alpha:]]{2,}[^"]*"' |
    grep -vE '^[^:]+:[0-9]+: *//' |
    grep -vE '(n?gettext)\(|(trace|debug|info|warn|error)!|\.context\(|unreachable!|panic!|css_class|-symbolic|#\[(arg|command)' |
    grep -vE '^[^:]+:[0-9]+: *"[^"]*",$' |
    grep -vE '\("[a-z_]+", '

# `gettext`/`ngettext` com algo que não é literal: o xgettext não extrai.
grep -rnE '\bn?gettext\( *[^ ")]' crates/raiza-gtk/src crates/raiza-tray/src crates/raiza-ctl/src \
    crates/raizad/src/battery.rs | sed 's/^/NÃO EXTRAÍDO: /'
