#!/bin/sh
# Compila os .po em <destino>/<língua>/LC_MESSAGES/raiza.mo.
# Uso: po/compilar.sh [destino]   (padrão: target/locale)
# Em desenvolvimento: po/compilar.sh && RAIZA_LOCALEDIR=target/locale LANGUAGE=pt_BR raiza-gtk
set -eu
cd "$(dirname "$0")/.."
dest="${1:-target/locale}"

while read -r lang; do
    [ -n "$lang" ] || continue
    mkdir -p "$dest/$lang/LC_MESSAGES"
    msgfmt --check --output-file="$dest/$lang/LC_MESSAGES/raiza.mo" "po/$lang.po"
done < po/LINGUAS
