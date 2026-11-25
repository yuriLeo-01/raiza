//! Mensagens de erro em português para o usuário.

use raiza_core::{RaizaError, dbus::ClientError, dbus::DbusError};

/// Mensagem mostrada quando o daemon não está no barramento.
pub const DAEMON_DOWN: &str =
    "o daemon não está rodando; inicie com: systemctl --user start raizad";

/// Nomes de erro D-Bus que significam "ninguém atende o serviço `raizad`".
fn is_daemon_down_name(name: &str) -> bool {
    matches!(
        name,
        "org.freedesktop.DBus.Error.ServiceUnknown" | "org.freedesktop.DBus.Error.NameHasNoOwner"
    ) || name.starts_with("org.freedesktop.DBus.Error.Spawn.")
}

fn describe_zbus(err: &zbus::Error) -> String {
    match err {
        zbus::Error::MethodError(name, _, _) if is_daemon_down_name(name.as_str()) => {
            DAEMON_DOWN.to_owned()
        }
        zbus::Error::InputOutput(_) | zbus::Error::Address(_) => format!(
            "não foi possível falar com o barramento de sessão do D-Bus (há uma sessão de usuário ativa?): {err}"
        ),
        other => format!("erro de D-Bus: {other}"),
    }
}

fn describe_raiza(err: &RaizaError) -> String {
    match err {
        RaizaError::NotFound(id) => {
            format!("dispositivo não encontrado: {id} (veja os ids com: raiza-ctl list)")
        }
        RaizaError::Unsupported(kind) => {
            format!("o dispositivo não tem a capacidade {kind}")
        }
        RaizaError::ReadOnly(kind) => format!(
            "a capacidade {kind} é somente leitura neste dispositivo (o hardware não aceita escrita ou o perfil ainda não foi verificado)"
        ),
        RaizaError::OutOfRange { kind, detail } => {
            format!("valor fora dos limites para {kind}: {detail}")
        }
        RaizaError::Io(msg) => format!("falha ao falar com o dispositivo: {msg}"),
        RaizaError::Permission(msg) => format!("sem permissão: {msg}"),
    }
}

/// Explica um erro do cliente D-Bus ao usuário.
pub fn describe(err: &ClientError) -> String {
    match err {
        ClientError::Raiza(e) => describe_raiza(e),
        ClientError::Bus(DbusError::ZBus(e)) => describe_zbus(e),
        ClientError::Bus(DbusError::InvalidArgument(msg)) => {
            format!("o daemon recusou o pedido: {msg}")
        }
        ClientError::Bus(e) => format!("erro do daemon: {e}"),
        ClientError::Decode(msg) => {
            format!("não entendi a resposta do daemon (versões diferentes?): {msg}")
        }
    }
}

#[cfg(test)]
mod tests {
    use raiza_core::{CapabilityKind, DeviceId};

    use super::*;

    #[test]
    fn nomes_de_daemon_fora_do_ar() {
        assert!(is_daemon_down_name(
            "org.freedesktop.DBus.Error.ServiceUnknown"
        ));
        assert!(is_daemon_down_name(
            "org.freedesktop.DBus.Error.NameHasNoOwner"
        ));
        assert!(is_daemon_down_name(
            "org.freedesktop.DBus.Error.Spawn.ChildExited"
        ));
        assert!(!is_daemon_down_name(
            "org.freedesktop.DBus.Error.UnknownMethod"
        ));
    }

    #[test]
    fn mensagens_do_raiza() {
        let id = DeviceId::usb(0x3142, 0xa010, "0");
        let msg = describe(&ClientError::Raiza(RaizaError::NotFound(id)));
        assert!(msg.contains("não encontrado") && msg.contains("usb:3142:a010:0"));

        let msg = describe(&ClientError::Raiza(RaizaError::ReadOnly(
            CapabilityKind::Dpi,
        )));
        assert!(msg.contains("somente leitura") && msg.contains("dpi"));

        let msg = describe(&ClientError::Raiza(RaizaError::OutOfRange {
            kind: CapabilityKind::Volume,
            detail: "151 fora do intervalo 0..=150".into(),
        }));
        assert!(msg.contains("fora dos limites") && msg.contains("volume"));
        assert!(msg.contains("151 fora do intervalo"));
    }

    #[test]
    fn outros_erros() {
        let msg = describe(&ClientError::Bus(DbusError::InvalidArgument("x".into())));
        assert!(msg.contains("recusou"));
        let msg = describe(&ClientError::Decode("json".into()));
        assert!(msg.contains("não entendi"));
    }
}
