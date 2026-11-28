//! Cola entre um widget e o daemon: escritas fora da thread da interface, debounce e
//! a regra contra laço de realimentação ([`ValueSync`]).

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Instant;

use raiza_core::dbus::RaizaClient;
use raiza_core::{CapabilityKind, CapabilityValue, DeviceId};
use relm4::gtk::glib;
use tracing::{debug, warn};

use super::debounce::{self, Debouncer, Push};
use super::sync::ValueSync;
use crate::text;

/// Como uma linha avisa a janela de um erro (vira `AdwToast`). Recebe texto puro.
pub type Reporter = Rc<dyn Fn(&str)>;

/// Manda escritas de uma capacidade ao daemon.
#[derive(Clone)]
pub struct Writer {
    device: DeviceId,
    client: Option<RaizaClient>,
    report: Reporter,
    kind: CapabilityKind,
}

impl Writer {
    /// Escritor da capacidade `kind` de `device`.
    pub fn new(
        device: DeviceId,
        client: Option<RaizaClient>,
        report: Reporter,
        kind: CapabilityKind,
    ) -> Self {
        Writer {
            device,
            client,
            report,
            kind,
        }
    }

    /// Envia `value`. A chamada D-Bus roda no tokio do relm4; `done(aceita)` roda de volta
    /// na thread do GTK. Em caso de erro, a janela recebe o aviso antes de `done`.
    pub fn send(&self, value: CapabilityValue, done: impl FnOnce(bool) + 'static) {
        let Some(client) = self.client.clone() else {
            (self.report)(&text::write_without_connection(self.kind));
            done(false);
            return;
        };
        let id = self.device.clone();
        let report = self.report.clone();
        let kind = self.kind;
        debug!(%id, ?value, "escrita");
        relm4::spawn_local(async move {
            let call = relm4::spawn(async move { client.set_capability(&id, &value).await });
            let ok = match call.await {
                Ok(Ok(())) => true,
                Ok(Err(err)) => {
                    warn!(%err, %kind, "escrita recusada");
                    report(&text::write_error(kind, &err));
                    false
                }
                Err(err) => {
                    warn!(%err, %kind, "a tarefa da escrita falhou");
                    report(&text::write_without_connection(kind));
                    false
                }
            };
            done(ok);
        });
    }
}

/// Liga um valor do tipo `T` a um widget.
///
/// O widget chama [`Link::user_changed`] no seu sinal; a página chama
/// [`Link::daemon_changed`] pelo `update()`. Pôr um valor no widget acontece sempre sob
/// uma trava, e o [`ValueSync`] compara com o valor mostrado: as duas coisas impedem que
/// um valor vindo do daemon vire escrita.
pub struct Link<T> {
    sync: RefCell<ValueSync<T>>,
    debounce: Option<RefCell<Debouncer<T>>>,
    writer: Writer,
    encode: Box<dyn Fn(&T) -> CapabilityValue>,
    show: Box<dyn Fn(&T)>,
    applying: Cell<bool>,
}

impl<T: Clone + PartialEq + 'static> Link<T> {
    /// Cria a ligação. `encode` faz o valor de escrita; `show` põe um valor no widget.
    pub fn new(
        initial: T,
        writer: Writer,
        debounced: bool,
        encode: impl Fn(&T) -> CapabilityValue + 'static,
        show: impl Fn(&T) + 'static,
    ) -> Rc<Self> {
        let link = Rc::new(Link {
            sync: RefCell::new(ValueSync::new(initial.clone())),
            debounce: debounced.then(|| RefCell::new(Debouncer::new(debounce::WINDOW))),
            writer,
            encode: Box::new(encode),
            show: Box::new(show),
            applying: Cell::new(false),
        });
        link.apply(&initial);
        link
    }

    /// Põe `value` no widget sem gerar escrita.
    fn apply(&self, value: &T) {
        self.quietly(|| (self.show)(value));
    }

    /// Mexe no widget (limites, lista de opções...) sem que os sinais dele virem escrita.
    pub fn quietly(&self, change: impl FnOnce()) {
        let before = self.applying.replace(true);
        change();
        self.applying.set(before);
    }

    /// O daemon anunciou `value`.
    pub fn daemon_changed(&self, value: T) {
        let shown = self.sync.borrow_mut().on_daemon(value);
        if let Some(value) = shown {
            self.apply(&value);
        }
    }

    /// O widget mudou para `value` (sinal do GTK).
    pub fn user_changed(self: &Rc<Self>, value: T) {
        if self.applying.get() || !self.sync.borrow_mut().on_user(value.clone()) {
            return;
        }
        let Some(debounce) = &self.debounce else {
            self.write(value);
            return;
        };
        let push = debounce.borrow_mut().push(value, Instant::now());
        match push {
            Push::Send(value) => self.write(value),
            Push::Schedule(after) => {
                let weak = Rc::downgrade(self);
                glib::timeout_add_local_once(after, move || {
                    if let Some(this) = weak.upgrade() {
                        this.fire();
                    }
                });
            }
            Push::Wait => {}
        }
    }

    fn fire(self: &Rc<Self>) {
        let value = self
            .debounce
            .as_ref()
            .and_then(|d| d.borrow_mut().fire(Instant::now()));
        if let Some(value) = value {
            self.write(value);
        }
    }

    fn write(self: &Rc<Self>, value: T) {
        let ticket = self.sync.borrow_mut().begin_write(&value);
        let weak: Weak<Self> = Rc::downgrade(self);
        self.writer.send((self.encode)(&value), move |ok| {
            let Some(this) = weak.upgrade() else { return };
            let shown = this.sync.borrow_mut().finish(ticket, value, ok);
            if let Some(value) = shown {
                this.apply(&value);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    type Log<T> = Rc<RefCell<Vec<T>>>;

    /// Sem cliente, toda tentativa de escrita vira um aviso: dá para contar escritas
    /// sem daemon e sem GTK.
    fn fixture() -> (Rc<Link<u32>>, Log<String>, Log<u32>) {
        let reports = Rc::new(RefCell::new(Vec::new()));
        let report: Reporter = Rc::new({
            let reports = Rc::clone(&reports);
            move |msg: &str| reports.borrow_mut().push(msg.to_owned())
        });
        let shown = Rc::new(RefCell::new(Vec::new()));
        // O "widget" falso repete no seu sinal todo valor que recebe, como o GTK faz.
        let me: Rc<RefCell<Weak<Link<u32>>>> = Rc::new(RefCell::new(Weak::new()));
        let writer = Writer::new(
            DeviceId::Other {
                provider: "mock".into(),
                key: "headset".into(),
            },
            None,
            report,
            CapabilityKind::Volume,
        );
        let link = Link::new(50, writer, false, |v| CapabilityValue::Volume(*v), {
            let shown = Rc::clone(&shown);
            let me = Rc::clone(&me);
            move |v| {
                shown.borrow_mut().push(*v);
                if let Some(link) = me.borrow().upgrade() {
                    link.user_changed(*v);
                }
            }
        });
        *me.borrow_mut() = Rc::downgrade(&link);
        (link, reports, shown)
    }

    #[test]
    fn valor_do_daemon_nao_gera_escrita() {
        let (link, reports, shown) = fixture();
        link.daemon_changed(70);
        link.daemon_changed(70);
        link.daemon_changed(20);
        assert_eq!(*shown.borrow(), vec![50, 70, 20]);
        // O widget, já em 20, repete o valor por conta própria: nada sai.
        link.user_changed(20);
        assert!(reports.borrow().is_empty(), "{:?}", reports.borrow());
    }

    #[test]
    fn mudanca_do_usuario_escreve_e_volta_se_falhar() {
        let (link, reports, shown) = fixture();
        link.user_changed(80);
        // Uma tentativa (sem conexão: aviso) e o widget volta ao confirmado sem reescrever.
        assert_eq!(reports.borrow().len(), 1);
        assert!(reports.borrow()[0].contains("no connection"));
        assert_eq!(*shown.borrow(), vec![50, 50]);
    }
}
