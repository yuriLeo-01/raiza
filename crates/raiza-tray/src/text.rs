//! Textos do ícone de bandeja. Ficam todos aqui para a tradução (T26).

/// Título do ícone e do tooltip.
pub const TITLE: &str = "Raiza";

/// Item do menu que abre o app.
pub const OPEN_APP: &str = "Abrir o Raiza";

/// Item do menu que encerra o ícone de bandeja.
pub const QUIT: &str = "Sair";

/// Tooltip e menu quando nenhum aparelho informa bateria.
pub const NO_BATTERIES: &str = "Nenhum aparelho com bateria";

/// Tooltip e menu quando o daemon não responde.
pub const DAEMON_DOWN: &str = "O serviço do Raiza não está rodando";

/// Uma linha da lista de baterias: `Rapoo VT9: 80%` ou `Rapoo VT9: 80%, carregando`.
pub fn battery_line(name: &str, percent: u8, charging: bool) -> String {
    if charging {
        format!("{name}: {percent}%, carregando")
    } else {
        format!("{name}: {percent}%")
    }
}

/// Escapa o texto para o subconjunto de HTML aceito na descrição do tooltip SNI.
pub fn escape_markup(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// Rótulo seguro para um item de menu D-Bus: `_` marca a tecla de acesso, então vira `__`.
pub fn menu_label(text: &str) -> String {
    text.replace('_', "__")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linha_de_bateria() {
        assert_eq!(battery_line("Rapoo VT9", 80, false), "Rapoo VT9: 80%");
        assert_eq!(
            battery_line("S9 Ultra", 5, true),
            "S9 Ultra: 5%, carregando"
        );
    }

    #[test]
    fn escapa_markup_e_sublinhado() {
        assert_eq!(escape_markup("A & <B>"), "A &amp; &lt;B&gt;");
        assert_eq!(menu_label("mouse_x"), "mouse__x");
    }
}
