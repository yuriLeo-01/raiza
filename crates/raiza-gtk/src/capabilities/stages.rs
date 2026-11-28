//! Tabela de estágios de DPI: um grupo à parte com uma `AdwSpinRow` por estágio.
//!
//! Cada estágio tem o seu [`Link`], com debounce, e escreve
//! [`CapabilityValue::DpiStage`]. O estágio em uso leva a marca "Em uso". Mudar o estágio
//! em uso também muda o DPI atual no daemon, e o `CapabilityChanged` que volta atualiza a
//! linha de DPI.
//!
//! A montagem (o que cada linha mostra) fica em [`stage_views`], sem GTK.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use raiza_core::{CapabilityValue, Level};
use relm4::adw;
use relm4::adw::prelude::*;

use super::level::{set_limits, snap, spin_adjustment};
use super::link::{Link, Writer};
use super::set_writable;
use crate::text;

/// O que a linha de um estágio mostra.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageView {
    /// Título da linha (`Estágio 1`...).
    pub title: String,
    /// DPI do estágio.
    pub value: u32,
    /// Se é o estágio em uso.
    pub active: bool,
}

/// As linhas da tabela, na ordem do aparelho. Um `active_stage` fora da tabela não marca
/// nenhuma linha.
pub fn stage_views(stages: &[u32], active_stage: Option<u32>) -> Vec<StageView> {
    stages
        .iter()
        .enumerate()
        .map(|(i, &value)| StageView {
            title: text::dpi_stage_title(i),
            value,
            active: active_stage.is_some_and(|a| a as usize == i),
        })
        .collect()
}

/// A linha de um estágio.
struct StageRow {
    row: adw::SpinRow,
    link: Rc<Link<u32>>,
}

impl StageRow {
    fn new(index: u32, view: &StageView, level: &Rc<Cell<Level>>, writer: Writer) -> Self {
        let row = adw::SpinRow::builder()
            .title(&view.title)
            .adjustment(&spin_adjustment(view.value, &level.get()))
            .snap_to_ticks(true)
            .numeric(true)
            .build();
        let link = Link::new(
            view.value,
            writer,
            true,
            move |value| CapabilityValue::DpiStage {
                stage: index,
                value: *value,
            },
            {
                let row = row.clone();
                move |value| row.set_value(f64::from(*value))
            },
        );
        row.connect_value_notify({
            let link = Rc::downgrade(&link);
            let level = Rc::clone(level);
            move |row| {
                if let Some(link) = link.upgrade() {
                    link.user_changed(snap(row.value(), &level.get()));
                }
            }
        });
        StageRow { row, link }
    }
}

/// O grupo "Estágios de DPI". Fica escondido enquanto a tabela é desconhecida.
pub struct DpiStages {
    group: adw::PreferencesGroup,
    writer: Writer,
    level: Rc<Cell<Level>>,
    rows: RefCell<Vec<StageRow>>,
}

impl DpiStages {
    /// Grupo vazio; o conteúdo vem de [`DpiStages::update`].
    pub fn new(level: Level, writer: Writer) -> Self {
        let group = adw::PreferencesGroup::builder()
            .title(text::DPI_STAGES_TITLE)
            .description(text::DPI_STAGES_DESCRIPTION)
            .visible(false)
            .build();
        DpiStages {
            group,
            writer,
            level: Rc::new(Cell::new(level)),
            rows: RefCell::new(Vec::new()),
        }
    }

    /// O grupo, para a página.
    pub fn group(&self) -> &adw::PreferencesGroup {
        &self.group
    }

    /// Mostra a tabela anunciada pelo daemon. Só recria as linhas quando o número de
    /// estágios muda; nos outros casos, atualiza no lugar (sem perder o foco).
    pub fn update(&self, level: &Level, stages: &[u32], active_stage: Option<u32>, writable: bool) {
        let views = stage_views(stages, active_stage);
        let level_changed = self.level.get() != *level;
        self.level.set(*level);
        let mut rows = self.rows.borrow_mut();
        if rows.len() != views.len() {
            for old in rows.drain(..) {
                self.group.remove(&old.row);
            }
            for (i, view) in views.iter().enumerate() {
                let index = u32::try_from(i).unwrap_or(u32::MAX);
                let row = StageRow::new(index, view, &self.level, self.writer.clone());
                self.group.add(&row.row);
                rows.push(row);
            }
        } else if level_changed {
            for row in rows.iter() {
                row.link
                    .quietly(|| set_limits(&row.row.adjustment(), level));
            }
        }
        for (row, view) in rows.iter().zip(&views) {
            row.row.set_subtitle(if view.active {
                text::DPI_STAGE_IN_USE
            } else {
                ""
            });
            set_writable(&row.row, writable);
            row.link.daemon_changed(view.value);
        }
        self.group.set_visible(!views.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monta_uma_linha_por_estagio_e_marca_o_em_uso() {
        let views = stage_views(&[800, 1600, 3200], Some(1));
        assert_eq!(
            views,
            vec![
                StageView {
                    title: "Estágio 1".into(),
                    value: 800,
                    active: false
                },
                StageView {
                    title: "Estágio 2".into(),
                    value: 1600,
                    active: true
                },
                StageView {
                    title: "Estágio 3".into(),
                    value: 3200,
                    active: false
                },
            ]
        );
    }

    #[test]
    fn sem_estagio_em_uso_ou_fora_da_tabela_nada_e_marcado() {
        for active in [None, Some(3), Some(u32::MAX)] {
            let views = stage_views(&[800, 1600, 3200], active);
            assert_eq!(views.len(), 3);
            assert!(views.iter().all(|v| !v.active), "{active:?}");
        }
    }

    #[test]
    fn tabela_vazia_nao_monta_nada() {
        assert!(stage_views(&[], Some(0)).is_empty());
    }

    #[test]
    fn valores_repetidos_continuam_separados() {
        let views = stage_views(&[1600, 1600], Some(0));
        assert_eq!(views.len(), 2);
        assert!(views[0].active && !views[1].active);
        assert_eq!(views[1].title, "Estágio 2");
    }
}
