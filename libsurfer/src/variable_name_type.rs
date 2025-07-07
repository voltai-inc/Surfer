use derive_more::{Display, FromStr};
use enum_iterator::Sequence;
use itertools::Itertools;

use serde::{Deserialize, Serialize};

use crate::displayed_item_tree::Node;
use crate::wave_container::{ScopeRefExt, VariableRefExt};
use crate::{displayed_item::DisplayedItem, wave_container::VariableRef, wave_data::WaveData};

#[derive(PartialEq, Copy, Clone, Debug, Deserialize, Display, FromStr, Serialize, Sequence)]
pub enum VariableNameType {
    /// Local variable name only (i.e. for tb.dut.clk => clk)
    Local,

    /// Add unique prefix, prefix + local
    Unique,

    /// Full variable name (i.e. tb.dut.clk => tb.dut.clk)
    Global,
}

impl WaveData {
    pub fn compute_variable_display_names(&mut self) {
        let full_names = self
            .items_tree
            .iter()
            .map(|node| self.displayed_items.get(&node.item_ref))
            .filter_map(|item| match item {
                Some(DisplayedItem::Variable(variable)) => Some(variable.variable_ref.clone()),
                _ => None,
            })
            .unique()
            .collect_vec();

        for Node { item_ref, .. } in self.items_tree.iter() {
            self.displayed_items
                .entry(*item_ref)
                .and_modify(|item| match item {
                    DisplayedItem::Variable(variable) => {
                        let local_name = variable.variable_ref.name.clone();
                        variable.display_name = match variable.display_name_type {
                            VariableNameType::Local => local_name,
                            VariableNameType::Global => variable.variable_ref.full_path_string(),
                            VariableNameType::Unique => {
                                /// This function takes a full variable name and a list of other
                                /// full variable names and returns a minimal unique variable name.
                                /// It takes scopes from the back of the variable until the name is unique.
                                fn unique(
                                    variable: &VariableRef,
                                    variables: &[VariableRef],
                                ) -> String {
                                    let other_variables = variables
                                        .iter()
                                        .filter(|&s| {
                                            *s.full_path_string() != variable.full_path_string()
                                        })
                                        .collect_vec();

                                    fn take_front(v: &VariableRef, l: usize) -> String {
                                        if l == 0 {
                                            v.name.clone()
                                        } else {
                                            format!(
                                                "{}{}.{}",
                                                if l < v.path.strs().len() { "…" } else { "" },
                                                v.path.strs().iter().rev().take(l).rev().join("."),
                                                v.name
                                            )
                                        }
                                    }

                                    let mut l = 0;
                                    while other_variables
                                        .iter()
                                        .map(|v| take_front(v, l))
                                        .contains(&take_front(variable, l))
                                    {
                                        l += 1;
                                    }
                                    take_front(variable, l)
                                }

                                unique(&variable.variable_ref, &full_names)
                            }
                        };
                        if self.display_variable_indices {
                            let index = self
                                .inner
                                .as_waves()
                                .unwrap()
                                .variable_meta(&variable.variable_ref)
                                .ok()
                                .as_ref()
                                .and_then(|meta| meta.index)
                                .map(|index| format!(" {index}"))
                                .unwrap_or_default();
                            variable.display_name = format!("{}{}", variable.display_name, index);
                        }
                    }
                    DisplayedItem::Divider(_) => {}
                    DisplayedItem::Marker(_) => {}
                    DisplayedItem::TimeLine(_) => {}
                    DisplayedItem::Placeholder(_) => {}
                    DisplayedItem::Stream(_) => {}
                    DisplayedItem::Group(_) => {}
                });
        }
    }

    pub fn force_variable_name_type(&mut self, name_type: VariableNameType) {
        for Node { item_ref, .. } in self.items_tree.iter() {
            self.displayed_items.entry(*item_ref).and_modify(|item| {
                if let DisplayedItem::Variable(variable) = item {
                    variable.display_name_type = name_type;
                }
            });
        }
        self.default_variable_name_type = name_type;
        self.compute_variable_display_names();
    }
}
