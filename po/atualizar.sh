#!/bin/sh
# Regera po/raiza.pot a partir do código (po/POTFILES) e junta as novidades nos .po.
# Uso, na raiz do repositório: po/atualizar.sh
# Requer o gettext 0.24 ou mais novo (xgettext com suporte a Rust).
set -eu
cd "$(dirname "$0")/.."

xgettext --language=Rust --from-code=UTF-8 \
    --keyword=gettext --keyword=ngettext:1,2 \
    --add-comments=Translators \
    --package-name=raiza --msgid-bugs-address=https://github.com/yuriLeo-01/raiza/issues \
    --files-from=po/POTFILES --output=po/raiza.pot --sort-by-file

while read -r lang; do
    [ -n "$lang" ] || continue
    msgmerge --quiet --update --backup=none --no-fuzzy-matching "po/$lang.po" po/raiza.pot
done < po/LINGUAS
