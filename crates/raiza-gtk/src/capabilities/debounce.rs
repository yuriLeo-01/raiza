//! Limitador de escritas enquanto o usuário arrasta um controle (sem GTK).
//!
//! Regra: o primeiro valor de uma rajada sai na hora; dentro da janela seguinte, os
//! valores só ficam guardados, e quando a janela fecha sai **o último**. Assim há no
//! máximo uma escrita por janela e o valor final sempre chega ao daemon.
//!
//! O relógio é passado de fora (`now`), para testar sem esperar.

use std::time::{Duration, Instant};

/// Janela padrão entre duas escritas.
pub const WINDOW: Duration = Duration::from_millis(150);

/// O que fazer com um valor recebido por [`Debouncer::push`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Push<T> {
    /// Escrever este valor agora.
    Send(T),
    /// Guardado. Chame [`Debouncer::fire`] depois deste prazo.
    Schedule(Duration),
    /// Guardado; já há um prazo marcado.
    Wait,
}

/// Limitador de escritas. Veja o módulo.
#[derive(Debug)]
pub struct Debouncer<T> {
    window: Duration,
    last_sent: Option<Instant>,
    pending: Option<T>,
    armed: bool,
}

impl<T> Debouncer<T> {
    /// Limitador com a janela dada.
    pub fn new(window: Duration) -> Self {
        Debouncer {
            window,
            last_sent: None,
            pending: None,
            armed: false,
        }
    }

    /// Um valor novo do usuário.
    pub fn push(&mut self, value: T, now: Instant) -> Push<T> {
        let elapsed = self.last_sent.map(|t| now.saturating_duration_since(t));
        match elapsed {
            Some(elapsed) if elapsed < self.window => {
                self.pending = Some(value);
                if self.armed {
                    Push::Wait
                } else {
                    self.armed = true;
                    Push::Schedule(self.window - elapsed)
                }
            }
            _ if self.armed => {
                // O prazo marcado ainda não rodou (atraso do laço de eventos): ele envia.
                self.pending = Some(value);
                Push::Wait
            }
            _ => {
                self.last_sent = Some(now);
                self.pending = None;
                Push::Send(value)
            }
        }
    }

    /// O prazo marcado venceu: devolve o último valor guardado, se houver.
    pub fn fire(&mut self, now: Instant) -> Option<T> {
        self.armed = false;
        let value = self.pending.take()?;
        self.last_sent = Some(now);
        Some(value)
    }

    /// Se há um valor guardado esperando a janela fechar.
    #[cfg(test)]
    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Simula uma rajada: um valor a cada `every`, por `total`, com o prazo marcado
    /// rodando exatamente quando vence. Devolve `(instante, valor)` de cada envio.
    fn burst(every: Duration, total: Duration) -> (Vec<(Duration, u32)>, u32) {
        let start = Instant::now();
        let mut d = Debouncer::new(WINDOW);
        let mut sent = Vec::new();
        let mut deadline: Option<Duration> = None;
        let mut t = Duration::ZERO;
        let mut value = 0;
        while t <= total {
            if let Some(at) = deadline.filter(|at| *at <= t) {
                deadline = None;
                if let Some(v) = d.fire(start + at) {
                    sent.push((at, v));
                }
            }
            value += 1;
            match d.push(value, start + t) {
                Push::Send(v) => sent.push((t, v)),
                Push::Schedule(after) => deadline = Some(t + after),
                Push::Wait => {}
            }
            t += every;
        }
        // O usuário soltou: o prazo pendente roda.
        if let Some(at) = deadline
            && let Some(v) = d.fire(start + at)
        {
            sent.push((at, v));
        }
        assert!(!d.has_pending());
        (sent, value)
    }

    #[test]
    fn rajada_um_envio_por_janela_e_o_ultimo_sempre() {
        let (sent, last) = burst(Duration::from_millis(7), Duration::from_secs(2));
        // Primeiro valor sai na hora.
        assert_eq!(sent[0], (Duration::ZERO, 1));
        // Nunca dois envios dentro da mesma janela.
        for pair in sent.windows(2) {
            assert!(pair[1].0 - pair[0].0 >= WINDOW, "{pair:?}");
        }
        // Bem menos envios que valores, mas o último valor foi enviado.
        assert!(sent.len() <= 2000 / 150 + 2, "{}", sent.len());
        assert_eq!(sent.last().map(|s| s.1), Some(last));
        // Valores em ordem crescente: nada velho depois de novo.
        assert!(sent.windows(2).all(|p| p[0].1 < p[1].1));
    }

    #[test]
    fn valores_espacados_saem_todos_na_hora() {
        let (sent, last) = burst(Duration::from_millis(200), Duration::from_secs(1));
        assert_eq!(sent.len() as u32, last);
        assert!(
            sent.iter()
                .all(|(at, v)| *at == Duration::from_millis(200) * (v - 1))
        );
    }

    #[test]
    fn prazo_sem_valor_nao_envia() {
        let start = Instant::now();
        let mut d = Debouncer::new(WINDOW);
        assert_eq!(d.push(1, start), Push::Send(1));
        assert_eq!(
            d.push(2, start + Duration::from_millis(50)),
            Push::Schedule(Duration::from_millis(100))
        );
        assert_eq!(d.push(3, start + Duration::from_millis(60)), Push::Wait);
        assert_eq!(d.fire(start + WINDOW), Some(3));
        assert_eq!(d.fire(start + WINDOW * 2), None);
    }

    #[test]
    fn prazo_atrasado_ainda_envia_o_ultimo() {
        let start = Instant::now();
        let mut d = Debouncer::new(WINDOW);
        assert_eq!(d.push(1, start), Push::Send(1));
        assert!(matches!(
            d.push(2, start + Duration::from_millis(10)),
            Push::Schedule(_)
        ));
        // O laço de eventos atrasou: chega valor novo depois da janela, antes do prazo rodar.
        assert_eq!(d.push(3, start + Duration::from_millis(400)), Push::Wait);
        assert_eq!(d.fire(start + Duration::from_millis(401)), Some(3));
    }
}
