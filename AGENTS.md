# Raiza: instruções para subagentes

Você foi despachado pelo líder técnico do projeto para executar **uma** tarefa.
Raiza é um hub único para usuários de Linux gerenciarem periféricos de qualquer marca
(Rust: daemon `raizad` + API D-Bus + interfaces nativas).

## Antes de começar

1. Leia a sua ficha de tarefa (o prompt que você recebeu, também em `docs/tarefas/`).
2. Leia `docs/PILARES.md` e `docs/ARQUITETURA.md`.
3. Leia o código de `crates/raiza-core`, que tem os contratos que você deve usar.

## Regras da tarefa

- Trabalhe **só** nos arquivos e diretórios listados em "Escopo" na ficha, mais o seu arquivo de notas `docs/tarefas/notas/TNN.md`. Criar arquivos
  novos dentro do escopo é permitido.
- **Não altere** `crates/raiza-core`, `docs/DBUS.md`, `docs/PILARES.md`, `docs/ARQUITETURA.md`
  ou o `Cargo.toml` da raiz. Crates em `crates/*` já entram no workspace sozinhos. Declare
  dependências externas no `Cargo.toml` do seu próprio crate, para não gerar conflito com outras tarefas.
- Se a tarefa exigir mudar um contrato ou algo fora do escopo, **pare**. Descreva o que
  precisa e por quê em `docs/tarefas/notas/TNN.md` (com o número da sua tarefa), faça commit e encerre. O líder decide.
- Não despache outros agentes.
- Não faça push nem merge. Faça commits na branch da sua worktree. O líder integra.
- Não envie nada para fora da máquina (sem `git push`, sem publicar pacotes).

## Regras de código

- Rust edição 2024. Sem `unsafe`, exceto se a ficha permitir, e sempre com comentário `// SAFETY:`.
- Erros com `thiserror` nos crates de biblioteca e `anyhow` só em binários. Nada de `unwrap()`/`expect()`
  em caminho que dependa de hardware ou entrada externa.
- Logs com `tracing`. Nunca `println!` em código de biblioteca.
- I/O de hardware fica atrás de uma trait ou camada injetável, para testar sem o dispositivo.
- Pilar 1: nunca envie a um dispositivo um comando que não esteja descrito e validado. Na dúvida, somente leitura.
- Comentários e documentação em português. Nomes de código em inglês.

## Definição de pronto

Tudo isto passa na raiz do workspace:

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Além disso:
- todos os critérios de aceite da ficha estão atendidos;
- `docs/tarefas/notas/TNN.md` resume o que foi feito, o que ficou de fora e como testar com hardware real;
- o último commit tem uma mensagem `TNN: <resumo>`.
