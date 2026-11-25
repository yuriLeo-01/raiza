//! Núcleo do Raiza: tipos e contratos compartilhados por daemon, provedores e interfaces.
//!
//! Os contratos (dispositivo, capacidades, provedores, eventos) são definidos na tarefa T01.
//! Veja `docs/ARQUITETURA.md` e `docs/tarefas/T01-contratos-core.md`.

/// Nome do serviço D-Bus do daemon (sessão do usuário).
pub const DBUS_SERVICE: &str = "io.github.yurileo01.Raiza";
