//! Mensagens de erro para o usuário, traduzidas pelo `raiza-i18n` (msgids em inglês).

use raiza_core::{RaizaError, dbus::ClientError, dbus::DbusError};
use raiza_i18n::{fill, gettext};

/// Mensagem mostrada quando o daemon não está no barramento.
pub fn daemon_down() -> &'static str {
    gettext("the daemon is not running; start it with: systemctl --user start raizad")
}

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
            daemon_down().to_owned()
        }
        zbus::Error::InputOutput(_) | zbus::Error::Address(_) => fill(
            gettext(
                "could not talk to the D-Bus session bus (is there an active user session?): {detail}",
            ),
            &[("detail", err)],
        ),
        other => fill(gettext("D-Bus error: {detail}"), &[("detail", other)]),
    }
}

fn describe_raiza(err: &RaizaError) -> String {
    match err {
        RaizaError::NotFound(id) => fill(
            gettext("device not found: {id} (list the ids with: raiza-ctl list)"),
            &[("id", id)],
        ),
        RaizaError::Unsupported(kind) => fill(
            gettext("the device does not have the {capability} capability"),
            &[("capability", kind)],
        ),
        RaizaError::ReadOnly(kind) => fill(
            gettext(
                "the {capability} capability is read-only on this device (the hardware does not accept writes or the profile has not been verified yet)",
            ),
            &[("capability", kind)],
        ),
        RaizaError::OutOfRange { kind, detail } => fill(
            gettext("value out of range for {capability}: {detail}"),
            &[("capability", kind), ("detail", detail)],
        ),
        RaizaError::Io(msg) => fill(
            gettext("failed to talk to the device: {detail}"),
            &[("detail", msg)],
        ),
        RaizaError::Permission(msg) => fill(gettext("no permission: {detail}"), &[("detail", msg)]),
    }
}

/// Explica um erro do cliente D-Bus ao usuário.
pub fn describe(err: &ClientError) -> String {
    match err {
        ClientError::Raiza(e) => describe_raiza(e),
        ClientError::Bus(DbusError::ZBus(e)) => describe_zbus(e),
        ClientError::Bus(DbusError::InvalidArgument(msg)) => fill(
            gettext("the daemon refused the request: {detail}"),
            &[("detail", msg)],
        ),
        ClientError::Bus(e) => fill(gettext("daemon error: {detail}"), &[("detail", e)]),
        ClientError::Decode(msg) => fill(
            gettext("could not understand the daemon reply (different versions?): {detail}"),
            &[("detail", msg)],
        ),
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
        assert!(msg.contains("not found") && msg.contains("usb:3142:a010:0"));

        let msg = describe(&ClientError::Raiza(RaizaError::ReadOnly(
            CapabilityKind::Dpi,
        )));
        assert!(msg.contains("read-only") && msg.contains("dpi"));

        let msg = describe(&ClientError::Raiza(RaizaError::OutOfRange {
            kind: CapabilityKind::Volume,
            detail: "151 fora do intervalo 0..=150".into(),
        }));
        assert!(msg.contains("out of range") && msg.contains("volume"));
        assert!(msg.contains("151 fora do intervalo"));
    }

    #[test]
    fn outros_erros() {
        let msg = describe(&ClientError::Bus(DbusError::InvalidArgument("x".into())));
        assert!(msg.contains("refused"));
        let msg = describe(&ClientError::Decode("json".into()));
        assert!(msg.contains("could not understand"));
    }
}
