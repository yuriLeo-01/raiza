//! Leitura mínima do descritor de relatório HID: as coleções de nível mais alto.
//!
//! Serve para comparar com `usage_page`/`usage` do `match` de um perfil. Só entende o
//! necessário: Usage Page, Usage, Collection, End Collection, Push e Pop.

/// `(usage page, usage)` de cada coleção de nível mais alto, na ordem do descritor.
/// Descritor truncado: devolve o que foi lido até ali.
pub fn top_level_collections(desc: &[u8]) -> Vec<(u16, u16)> {
    let mut out = Vec::new();
    let mut usage_page: u16 = 0;
    let mut page_stack: Vec<u16> = Vec::new();
    // Primeiro Usage local desde o último item principal: (página estendida, usage).
    let mut usage: Option<(Option<u16>, u16)> = None;
    let mut depth = 0usize;
    let mut i = 0;
    while i < desc.len() {
        let prefix = desc[i];
        if prefix == 0xFE {
            // Item longo: 0xFE, tamanho, tag, dados.
            let size = desc.get(i + 1).copied().map_or(0, usize::from);
            i += 3 + size;
            continue;
        }
        let size = match prefix & 0x03 {
            0 => 0,
            1 => 1,
            2 => 2,
            _ => 4,
        };
        let Some(data) = desc.get(i + 1..i + 1 + size) else {
            break;
        };
        i += 1 + size;
        let value = data
            .iter()
            .rev()
            .fold(0u32, |acc, b| (acc << 8) | u32::from(*b));
        let low = (value & 0xFFFF) as u16;
        match prefix & 0xFC {
            // Usage Page (global).
            0x04 => usage_page = low,
            // Push / Pop (global).
            0xA4 => page_stack.push(usage_page),
            0xB4 => usage_page = page_stack.pop().unwrap_or(usage_page),
            // Usage (local). Com 4 bytes, a página vem nos 16 bits altos.
            0x08 => {
                if usage.is_none() {
                    let page = (size == 4).then_some((value >> 16) as u16);
                    usage = Some((page, low));
                }
            }
            // Collection (principal).
            0xA0 => {
                if depth == 0 {
                    let (page, id) = usage.unwrap_or((None, 0));
                    out.push((page.unwrap_or(usage_page), id));
                }
                depth += 1;
                usage = None;
            }
            // End Collection (principal).
            0xC0 => {
                depth = depth.saturating_sub(1);
                usage = None;
            }
            // Input, Output, Feature (principais) zeram os locais.
            0x80 | 0x90 | 0xB0 => usage = None,
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_e_vendor() {
        let desc = [
            0x05, 0x01, // Usage Page (Generic Desktop)
            0x09, 0x02, // Usage (Mouse)
            0xA1, 0x01, // Collection (Application)
            0x09, 0x01, //   Usage (Pointer)
            0xA1, 0x00, //   Collection (Physical)
            0x81, 0x02, //     Input
            0xC0, //       End Collection
            0xC0, //     End Collection
            0x06, 0x00, 0xFF, // Usage Page (Vendor 0xFF00)
            0x09, 0x0E, // Usage (0x0E)
            0xA1, 0x01, // Collection (Application)
            0x85, 0x06, //   Report ID 6
            0x91, 0x02, //   Output
            0xC0, //     End Collection
            0x0B, 0x01, 0x00, 0x0C, 0x00, // Usage (Consumer: 0x0001), estendido
            0xA1, 0x01, // Collection
            0xC0, // End Collection
        ];
        assert_eq!(
            top_level_collections(&desc),
            [(0x0001, 0x0002), (0xFF00, 0x000E), (0x000C, 0x0001)]
        );
    }

    #[test]
    fn truncado_e_vazio() {
        assert!(top_level_collections(&[]).is_empty());
        assert_eq!(
            top_level_collections(&[0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, 0x06, 0x00]),
            [(0x0001, 0x0006)]
        );
    }
}
