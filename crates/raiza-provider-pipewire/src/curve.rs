//! Curva cúbica de volume, a mesma do `wpctl` e do GNOME.
//!
//! O PipeWire guarda o volume dos canais em escala linear (`channelVolumes`). A interface
//! mostra a raiz cúbica disso, que acompanha melhor a percepção do ouvido. Exemplo real: o
//! fifine com `channelVolumes = 0.1905` aparece como `0.58` no `wpctl` e 58 % no GNOME.

/// Volume linear do PipeWire → porcentagem na curva cúbica (arredondada).
pub fn linear_to_percent(linear: f64) -> u32 {
    if !linear.is_finite() || linear <= 0.0 {
        return 0;
    }
    // Teto alto só para não estourar a conversão; o provedor limita a 100.
    (linear.cbrt() * 100.0).round().min(10_000.0) as u32
}

/// Porcentagem na curva cúbica → volume linear do PipeWire.
pub fn percent_to_linear(percent: u32) -> f64 {
    (f64::from(percent) / 100.0).powi(3)
}

/// Porcentagem → valor que o `wpctl set-volume` espera (já na curva cúbica, `1.0` = 100 %).
pub fn percent_to_wpctl(percent: u32) -> f64 {
    f64::from(percent) / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valores_reais() {
        assert_eq!(linear_to_percent(0.1905), 58);
        assert_eq!(linear_to_percent(0.830590), 94);
        assert_eq!(linear_to_percent(0.063997), 40);
        assert_eq!(linear_to_percent(1.0), 100);
        assert_eq!(linear_to_percent(0.0), 0);
        assert_eq!(linear_to_percent(-1.0), 0);
        assert_eq!(linear_to_percent(f64::NAN), 0);
    }

    #[test]
    fn ida_e_volta() {
        for p in 0..=100 {
            assert_eq!(linear_to_percent(percent_to_linear(p)), p, "{p} %");
        }
    }
}
