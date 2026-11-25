# Orquestração: Claude líder + subagentes no Orca

## Papéis

| Papel | Quem | Faz |
|---|---|---|
| **Líder** | Claude, na worktree `main` | Mantém pilares, arquitetura e contratos (`raiza-core`, API D-Bus). Escreve as fichas de tarefa, despacha, revisa e faz merge. |
| **Subagente** | Claude ou Gemini 4 Hydrogen (via `agy`) | Executa **uma** ficha de tarefa numa worktree própria, dentro do escopo de arquivos da ficha. |
| **Humano** | Mantenedor | Aprova pilares e roadmap, decide prioridades, testa com hardware real e faz capturas USB. |

## Qual agente usar

| Situação | Agente |
|---|---|
| Contratos, API pública, decisões de arquitetura, código de risco (motor de perfis HID, escrita em dispositivo) | `claude` |
| Provedores bem delimitados, CLI, CI, empacotamento, telas da interface, testes | `hydrogen` (`gemini-4-hydrogen-high`) |
| Tarefa grande ou ambígua | o líder quebra em tarefas menores antes de despachar |
| Revisão cruzada | o agente que **não** escreveu o código |

## Ciclo de uma tarefa

1. **Planejar**: o líder escreve `docs/tarefas/TNN-nome.md` a partir do `_MODELO.md`.
2. **Despachar**:
   ```sh
   scripts/despachar.sh T08            # usa o agente sugerido na ficha
   scripts/despachar.sh T08 claude     # força Claude
   scripts/despachar.sh T08 hydrogen xhigh
   ```
   O script cria uma worktree `tNN-nome` no Orca, a partir da `main`, e inicia o agente
   com a ficha como prompt.
3. **Executar**: o subagente segue o `AGENTS.md`. Trabalha só nos arquivos do escopo, faz
   commits na própria branch e, ao final, deixa `cargo fmt`, `cargo clippy` e `cargo test` passando.
4. **Revisar**: o líder lê o diff (`git diff main...<branch>`), roda as verificações e
   devolve pedidos de ajuste pela mensageria do Orca (`orca orchestration send`).
5. **Integrar**: o líder faz merge na `main`, marca a tarefa como concluída em
   `docs/tarefas/README.md` e despacha as tarefas que dependiam dela.

## Paralelismo

- Só despache tarefas cujas dependências já estejam na `main`.
- Duas tarefas em paralelo nunca podem ter escopos de arquivos sobrepostos.
- Mudança em `raiza-core` ou na API D-Bus é sempre do líder. Um subagente que precisar
  de uma mudança dessas para e pede (veja `AGENTS.md`).

## Comandos úteis do Orca

```sh
ORCA=~/.config/orca/linux-orca-cli-shim/orca   # fora de um terminal do Orca, não use `orca` puro
$ORCA worktree ps --json                         # worktrees e agentes ativos
$ORCA agent list --json                          # estado de cada agente
$ORCA agent read <orca_session_id> --json        # o que o agente disse
$ORCA orchestration send --to <handle> --type dispatch --subject "T08: ajustes" --body-file /tmp/ajustes.md --json
```
