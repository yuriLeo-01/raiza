# T12: Provedor OpenRGB

- **Fase:** F2
- **Agente sugerido:** hydrogen
- **Depende de:** T01
- **Escopo:** `crates/raiza-provider-openrgb/**`

## Objetivo
Controlar RGB de tudo que o OpenRGB já suporta, conectando ao servidor SDK dele (TCP em localhost:6742), se estiver rodando.

## Contexto
Crate `openrgb` (ou implementar o protocolo SDK, que é simples e documentado). Se o OpenRGB não estiver rodando, o provedor fica inativo e sem erro barulhento (pilar 5: aproveitar o que existe, nunca exigir). Dispositivo do mantenedor que pode ser suportado: controlador JIGUANG ARGB 1a86:e30b.

## O que fazer
1. Conectar, listar controladores, zonas e modos e mapear para `Capability::Rgb`.
2. Definir uma cor por zona e trocar de modo.
3. Reconectar com backoff se o OpenRGB reiniciar.

## Critérios de aceite
- [ ] Testes com um servidor SDK falso (TCP local)
- [ ] OpenRGB ausente: o provedor só loga em nível info e devolve lista vazia
- [ ] `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings` e `cargo test` passam
