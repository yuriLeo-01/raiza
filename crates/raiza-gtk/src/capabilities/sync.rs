//! Estado de um controle entre o usuário e o daemon (sem GTK).
//!
//! Guarda dois valores: o **confirmado** (o que o daemon disse por último, ou a última
//! escrita aceita) e o **mostrado** (o que o widget exibe). Regras:
//!
//! - valor do daemon ([`ValueSync::on_daemon`]) muda o mostrado e **nunca** pede escrita;
//!   quando o widget repete esse valor no seu sinal, [`ValueSync::on_user`] vê que é igual
//!   ao mostrado e não escreve (sem laço de realimentação);
//! - enquanto há escrita pendente ou em andamento, ecos do daemon só atualizam o
//!   confirmado, para o controle não pular para trás no meio de um arraste;
//! - escrita recusada volta o mostrado ao confirmado.

/// Identifica uma escrita, para saber se o resultado é da mais recente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ticket(u64);

/// Veja o módulo.
#[derive(Debug)]
pub struct ValueSync<T> {
    confirmed: T,
    shown: T,
    /// O usuário mudou o valor e ele ainda não foi enviado (debounce).
    unsent: bool,
    in_flight: u32,
    latest: u64,
}

impl<T: Clone + PartialEq> ValueSync<T> {
    /// Começa com o valor anunciado pelo daemon.
    pub fn new(value: T) -> Self {
        ValueSync {
            confirmed: value.clone(),
            shown: value,
            unsent: false,
            in_flight: 0,
            latest: 0,
        }
    }

    /// O que o widget deve mostrar.
    #[cfg(test)]
    pub fn shown(&self) -> &T {
        &self.shown
    }

    fn busy(&self) -> bool {
        self.unsent || self.in_flight > 0
    }

    /// O daemon anunciou `value`. Devolve o valor a pôr no widget, se ele mudou.
    pub fn on_daemon(&mut self, value: T) -> Option<T> {
        self.confirmed = value;
        self.resync()
    }

    /// O usuário mudou o widget para `value`. Devolve `true` se é preciso escrever.
    pub fn on_user(&mut self, value: T) -> bool {
        if value == self.shown {
            return false;
        }
        self.shown = value;
        self.unsent = true;
        true
    }

    /// Uma escrita de `value` vai sair.
    pub fn begin_write(&mut self, value: &T) -> Ticket {
        self.latest += 1;
        self.in_flight += 1;
        if *value == self.shown {
            self.unsent = false;
        }
        Ticket(self.latest)
    }

    /// A escrita `ticket` de `value` terminou (`ok` = aceita). Devolve o valor a pôr
    /// no widget, se for preciso mudá-lo.
    pub fn finish(&mut self, ticket: Ticket, value: T, ok: bool) -> Option<T> {
        self.in_flight = self.in_flight.saturating_sub(1);
        let latest = ticket.0 == self.latest;
        if ok && latest {
            self.confirmed = value;
        }
        if !ok && latest && !self.unsent {
            // Recusada: o daemon continua com o valor confirmado.
            self.in_flight = 0;
        }
        self.resync()
    }

    /// Sem nada pendente, o mostrado volta a seguir o confirmado.
    fn resync(&mut self) -> Option<T> {
        if self.busy() || self.shown == self.confirmed {
            return None;
        }
        self.shown = self.confirmed.clone();
        Some(self.shown.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valor_do_daemon_nao_gera_escrita() {
        let mut s = ValueSync::new(50);
        // O daemon anuncia 70: o widget muda...
        assert_eq!(s.on_daemon(70), Some(70));
        // ...e o sinal do widget repete o valor: não é escrita.
        assert!(!s.on_user(70));
        // Eco repetido não mexe em nada.
        assert_eq!(s.on_daemon(70), None);
        assert!(!s.on_user(70));
        assert_eq!(*s.shown(), 70);
    }

    #[test]
    fn escrita_e_eco_sem_laco() {
        let mut s = ValueSync::new(false);
        assert!(s.on_user(true));
        let t = s.begin_write(&true);
        // O eco chega antes da resposta: o widget já mostra `true`.
        assert_eq!(s.on_daemon(true), None);
        assert_eq!(s.finish(t, true, true), None);
        assert!(!s.on_user(true));
    }

    #[test]
    fn recusa_volta_ao_confirmado() {
        let mut s = ValueSync::new(1600);
        assert!(s.on_user(1625));
        let t = s.begin_write(&1625);
        assert_eq!(s.finish(t, 1625, false), Some(1600));
        assert_eq!(*s.shown(), 1600);
        // O widget, ao voltar para 1600, dispara o sinal: não reescreve.
        assert!(!s.on_user(1600));
    }

    #[test]
    fn eco_antigo_nao_puxa_o_controle_no_meio_do_arraste() {
        let mut s = ValueSync::new(10);
        assert!(s.on_user(20));
        let t1 = s.begin_write(&20);
        assert!(s.on_user(30)); // ainda arrastando, guardado pelo debounce
        // Eco da primeira escrita: não mexe no widget.
        assert_eq!(s.on_daemon(20), None);
        assert_eq!(s.finish(t1, 20, true), None);
        assert_eq!(*s.shown(), 30);
        let t2 = s.begin_write(&30);
        assert_eq!(s.finish(t2, 30, true), None);
        assert_eq!(s.on_daemon(30), None);
        assert_eq!(*s.shown(), 30);
    }

    #[test]
    fn resultado_de_escrita_antiga_nao_reverte() {
        let mut s = ValueSync::new(10);
        assert!(s.on_user(20));
        let t1 = s.begin_write(&20);
        assert!(s.on_user(30));
        let t2 = s.begin_write(&30);
        // A primeira falha depois que a segunda saiu: não volta o controle.
        assert_eq!(s.finish(t1, 20, false), None);
        assert_eq!(s.finish(t2, 30, true), None);
        assert_eq!(*s.shown(), 30);
    }

    #[test]
    fn mudanca_externa_durante_escrita_aparece_no_fim() {
        let mut s = ValueSync::new(10);
        assert!(s.on_user(20));
        let t = s.begin_write(&20);
        // Outro cliente mudou para 40 enquanto a escrita estava no ar; depois a nossa
        // falhou: o widget mostra o que o daemon tem.
        assert_eq!(s.on_daemon(40), None);
        assert_eq!(s.finish(t, 20, false), Some(40));
    }
}
