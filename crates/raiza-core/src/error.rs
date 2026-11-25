//! Erros comuns a provedores, daemon e clientes.

use crate::{CapabilityKind, DeviceId};

/// Erro do Raiza.
///
/// As variantes carregam texto em vez de erros de outras bibliotecas, para que o erro
/// possa ser clonado e atravessar o D-Bus sem perder informação útil.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RaizaError {
    /// O dispositivo não existe (ou não está mais conectado).
    #[error("dispositivo não encontrado: {0}")]
    NotFound(DeviceId),
    /// O dispositivo não tem essa capacidade, ou o valor não combina com ela.
    #[error("capacidade não suportada: {0}")]
    Unsupported(CapabilityKind),
    /// A capacidade existe, mas é somente leitura (ex.: perfil ainda não verificado).
    #[error("capacidade somente leitura: {0}")]
    ReadOnly(CapabilityKind),
    /// O valor está fora dos limites aceitos pela capacidade.
    #[error("valor fora dos limites para {kind}: {detail}")]
    OutOfRange {
        /// Capacidade que recusou o valor.
        kind: CapabilityKind,
        /// Explicação legível do limite violado.
        detail: String,
    },
    /// Falha de entrada e saída ao falar com o dispositivo ou com um serviço do sistema.
    #[error("erro de E/S: {0}")]
    Io(String),
    /// Sem permissão para acessar o dispositivo (ex.: falta a regra udev).
    #[error("sem permissão: {0}")]
    Permission(String),
}

impl From<std::io::Error> for RaizaError {
    fn from(err: std::io::Error) -> Self {
        if err.kind() == std::io::ErrorKind::PermissionDenied {
            RaizaError::Permission(err.to_string())
        } else {
            RaizaError::Io(err.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_permission_vira_permission() {
        let err: RaizaError = std::io::Error::from(std::io::ErrorKind::PermissionDenied).into();
        assert!(matches!(err, RaizaError::Permission(_)));
        let err: RaizaError = std::io::Error::from(std::io::ErrorKind::BrokenPipe).into();
        assert!(matches!(err, RaizaError::Io(_)));
    }
}
