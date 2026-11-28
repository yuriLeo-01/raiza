# Extensão do GNOME Shell do Raiza.
#   make         gera o zip (raiza@yurileo01.github.io.shell-extension.zip)
#   make install instala o zip na conta do usuário (~/.local/share/gnome-shell/extensions)
#   make pot     atualiza o modelo de tradução em po/

UUID := raiza@yurileo01.github.io
ZIP  := $(UUID).shell-extension.zip

.PHONY: all pack install pot clean

all: pack

pack: $(ZIP)

$(ZIP): metadata.json extension.js $(wildcard po/*.po)
	gnome-extensions pack --force --podir=po --gettext-domain=raiza-shell-extension .

install: $(ZIP)
	gnome-extensions install --force $(ZIP)

pot:
	xgettext --from-code=UTF-8 -L JavaScript -k_ --add-comments=TRANSLATORS \
		--package-name=raiza-shell-extension -o po/raiza-shell-extension.pot extension.js

clean:
	rm -f $(ZIP)
