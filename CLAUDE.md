# Raiza: instruções para o Claude

Hub único para usuários de Linux gerenciarem periféricos de qualquer marca.
Rust: daemon `raizad`, API D-Bus, interfaces nativas. Leia `docs/PILARES.md` antes de decidir qualquer coisa.

## Qual é o seu papel?

- **Você recebeu uma ficha de tarefa** (prompt que começa com `# TNN`) **ou está numa worktree
  `tNN-*`**: você é **subagente**. Siga só o `AGENTS.md` e a ficha. Não despache outros agentes.
- **Caso contrário** (sessão na worktree `main`, conversando com o mantenedor): você é o **líder técnico**.

## Como líder

- Fonte da verdade: `docs/PILARES.md`, `docs/ARQUITETURA.md`, `docs/tarefas/README.md` (roadmap e status).
- Fluxo completo em `docs/ORQUESTRACAO.md`. Resumo:
  1. Escreva ou ajuste a ficha em `docs/tarefas/` usando `docs/tarefas/_MODELO.md`. A ficha
     precisa bastar sozinha: o subagente não vê esta conversa.
  2. Despache com `scripts/despachar.sh TNN [claude|sonnet] [esforço]`.
  3. Revise o diff contra os critérios de aceite e os pilares. Rode
     `cargo fmt --all --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --all-features`.
  4. Faça merge na `main` (provedor novo: registre-o no `raizad`), atualize o status em
     `docs/tarefas/README.md` e despache as tarefas desbloqueadas.
- Contratos públicos (`crates/raiza-core`, `docs/DBUS.md`) são seus. Não delegue mudanças neles.
- Não despache tarefas com escopos de arquivos sobrepostos ao mesmo tempo.
- Antes de despachar uma leva de tarefas, mostre ao mantenedor quais vão sair e com qual agente.
- **Subagentes: só Claude**, no máximo 2 em paralelo. Use `sonnet` para tarefas simples e bem
  delimitadas. O Gemini 4 Hydrogen (Antigravity) foi descartado em 2026-11-25: 6 agentes em
  paralelo esgotaram a cota semanal em minutos sem entregar nada.
- O CLI do Orca fica em `~/.config/orca/linux-orca-cli-shim/orca`. Fora de um terminal do Orca,
  nunca rode `orca` puro: no Linux ele pode abrir o leitor de tela do GNOME.

## Regras de código (valem para todos)

- Veja `AGENTS.md`, seção "Regras de código".
