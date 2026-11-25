#!/usr/bin/env bash
# Despacha uma ficha de tarefa para um subagente numa worktree nova do Orca.
#
# Uso:
#   scripts/despachar.sh TNN [claude|hydrogen] [esforço] [--dry-run]
#
#   agente  : padrão = "Agente sugerido" da ficha (claude ou hydrogen)
#   esforço : hydrogen -> low|medium|high|xhigh (padrão high, escolhe o modelo gemini-4-hydrogen-<esforço>)
#             claude   -> low|medium|high|xhigh|max (padrão: o do Orca)
set -euo pipefail

raiz=$(git -C "$(dirname "$0")/.." rev-parse --show-toplevel)
dry_run=false
args=()
for a in "$@"; do
  if [[ $a == --dry-run ]]; then dry_run=true; else args+=("$a"); fi
done
[[ ${#args[@]} -ge 1 ]] || { sed -n '2,10p' "$0"; exit 2; }

id=${args[0]^^}
agente=${args[1]:-}
esforco=${args[2]:-}

shopt -s nullglob
fichas=("$raiz"/docs/tarefas/"$id"-*.md)
[[ ${#fichas[@]} -eq 1 ]] || { echo "ficha $id não encontrada (ou duplicada) em docs/tarefas/" >&2; exit 1; }
ficha=${fichas[0]}
nome=$(basename "$ficha" .md); nome=${nome,,}

if [[ -z $agente ]]; then
  sugerido=$(grep -m1 -i 'Agente sugerido' "$ficha" | tr 'A-Z' 'a-z')
  case $sugerido in
    *humano*) echo "$id exige o mantenedor (capturas/hardware). Despache só se as capturas já estiverem na main; passe o agente explicitamente." >&2; exit 1 ;;
    *hydrogen*) agente=hydrogen ;;
    *) agente=claude ;;
  esac
fi

# Dependências: avisa se alguma ainda não está marcada como concluída no roadmap.
deps=$(grep -m1 'Depende de' "$ficha" | grep -o 'T[0-9][0-9]' || true)
for d in $deps; do
  if ! grep -E "^\| $d \|.*✅" "$raiz/docs/tarefas/README.md" >/dev/null; then
    echo "aviso: dependência $d de $id ainda não está ✅ em docs/tarefas/README.md" >&2
  fi
done

# Executável do Orca: dentro de um terminal do Orca usa o dele; fora, o shim (nunca o `orca` do GNOME).
if [[ -n ${ORCA_CLI_COMMAND:-} ]]; then orca=$ORCA_CLI_COMMAND
elif [[ -n ${ORCA_PANE_KEY:-}${ORCA_TERMINAL_HANDLE:-} ]] && command -v orca >/dev/null; then orca=orca
else orca="$HOME/.config/orca/linux-orca-cli-shim/orca"
fi

prompt="# $id
Você é um subagente do projeto Raiza, despachado pelo líder técnico para executar UMA tarefa.
Antes de tudo, leia AGENTS.md, docs/PILARES.md, docs/ARQUITETURA.md e o código de crates/raiza-core.
Siga estritamente o escopo de arquivos da ficha. Ao terminar, cumpra a 'Definição de pronto' do AGENTS.md.

$(cat "$ficha")"

cmd=("$orca" worktree create --repo "path:$raiz" --name "$nome" --no-parent --base-branch main)
case $agente in
  hydrogen) cmd+=(--agent antigravity --model "gemini-4-hydrogen-${esforco:-high}") ;;
  claude)   cmd+=(--agent claude); [[ -n $esforco ]] && cmd+=(--effort "$esforco") ;;
  *) echo "agente desconhecido: $agente (use claude ou hydrogen)" >&2; exit 2 ;;
esac
cmd+=(--comment "$id em andamento ($agente)" --prompt "$prompt" --json)

if $dry_run; then
  printf '%q ' "${cmd[@]:0:${#cmd[@]}-3}"; echo '--prompt <ficha> --json'
  echo "--- prompt ---"; echo "$prompt" | head -20; echo "..."
  exit 0
fi

echo "despachando $id para $agente..." >&2
"${cmd[@]}"
