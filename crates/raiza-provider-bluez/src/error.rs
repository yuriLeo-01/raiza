//! Erros da camada D-Bus.

use raiza_core::RaizaError;

/// Erro de um backend ([`crate::BluezBackend`] ou [`crate::UpowerBackend`]).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BackendError {
    /// O serviço do sistema (BlueZ, UPower) ou o próprio barramento não está disponível.
    #[error("serviço indisponível: {0}")]
    Unavailable(String),
    /// O objeto D-Bus não existe mais (ex.: dispositivo removido).
    #[error("objeto não existe mais: {0}")]
    Gone(String),
    /// Qualquer outra falha.
    #[error("falha no D-Bus: {0}")]
    Other(String),
}

impl From<BackendError> for RaizaError {
    fn from(err: BackendError) -> Self {
        RaizaError::Io(err.to_string())
    }
}
