#!/usr/bin/env bash
# Captura o tráfego USB de UM dispositivo, um passo por arquivo, para descobrir protocolos.
#
# Uso:
#   scripts/capturar-usb.sh <vid:pid> <pasta> <nome-do-passo>
#   ex.: scripts/capturar-usb.sh 36eb:1087 docs/protocolos/capturas/akko-s9-ultra-36eb-1087 03-sidetone-liga
#
# Como funciona: grava o barramento inteiro (usbmonN) num arquivo temporário até você
# apertar Enter, e salva em <pasta>/<nome-do-passo>.pcapng SÓ os pacotes do dispositivo.
# O temporário é apagado: o barramento pode ter o seu teclado, e o repositório é público.
#
# Só OBSERVA o tráfego: não envia nada ao dispositivo (pilar 1).
# Requisitos: wireshark-cli (dumpcap e tshark) e o módulo usbmon (o script carrega).
set -euo pipefail

[[ $# -eq 3 ]] || { sed -n '2,13p' "$0"; exit 2; }
vidpid=$1 pasta=$2 passo=$3
[[ $passo =~ ^[A-Za-z0-9._-]+$ ]] || { echo "nome do passo: só letras, números, . _ -" >&2; exit 2; }
for cmd in dumpcap tshark lsusb; do
  command -v "$cmd" >/dev/null || { echo "falta $cmd (Arch/CachyOS: sudo pacman -S wireshark-cli usbutils)" >&2; exit 1; }
done

endereco() { # imprime "barramento dispositivo" (sem zeros à esquerda)
  lsusb -d "$vidpid" | awk 'NR==1 {print $2+0, $4+0}'
}
read -r bus dev < <(endereco) || true
[[ -n ${bus:-} ]] || { echo "$vidpid não está conectado (lsusb -d $vidpid)" >&2; exit 1; }

[[ -e /dev/usbmon$bus ]] || sudo modprobe usbmon
if [[ ! -r /dev/usbmon$bus ]]; then
  echo "dando ao seu usuário leitura em /dev/usbmon$bus (vale até reiniciar)..."
  sudo setfacl -m "u:$USER:r" "/dev/usbmon$bus"
fi

mkdir -p "$pasta"
destino="$pasta/$passo.pcapng"
[[ ! -e $destino ]] || { echo "$destino já existe; use outro nome" >&2; exit 1; }
tmp=$(mktemp --suffix=.pcapng)
trap 'rm -f "$tmp"' EXIT

dumpcap -q -i "usbmon$bus" -w "$tmp" 2>/dev/null &
pid=$!
sleep 1
echo
echo ">>> Gravando $vidpid (barramento $bus, dispositivo $dev): $passo"
echo ">>> Faça UMA ação no software, espere uns 3 segundos e aperte Enter."
read -r _
kill -INT "$pid"; wait "$pid" 2>/dev/null || true

# Se a VM reconectou o aparelho durante o passo, o endereço muda: guarda os dois.
read -r bus2 dev2 < <(endereco) || true
filtro="usb.bus_id == $bus && usb.device_address == $dev"
if [[ -n ${dev2:-} && $dev2 != "$dev" ]]; then
  echo "aviso: o endereço mudou de $dev para $dev2 durante o passo (reconexão)"
  filtro="usb.bus_id == $bus && (usb.device_address == $dev || usb.device_address == $dev2)"
fi
tshark -r "$tmp" -Y "$filtro" -w "$destino" 2>/dev/null
n=$(tshark -r "$destino" 2>/dev/null | wc -l)
echo ">>> $destino: $n pacotes do dispositivo."
[[ $n -gt 0 ]] || echo "aviso: nenhum pacote. A VM está com o dispositivo? A ação chegou a ser aplicada?"

printf 'O que o software mostrava (valor, nome do preset...)? ' ; read -r nota
printf -- '- `%s` (%s, %s pacotes): %s\n' "$passo" "$(date '+%Y-%m-%d %H:%M')" "$n" "$nota" >> "$pasta/anotacoes.md"
echo ">>> anotado em $pasta/anotacoes.md"
