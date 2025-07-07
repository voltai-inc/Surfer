use crate::variable_ref::VariableRef;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

/// A reference to a field of a larger variable, such as a field in a struct. The fields
/// are the recursive path to the fields inside the (translated) root
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldRef<VarId, ScopeId> {
    pub root: VariableRef<VarId, ScopeId>,
    pub field: Vec<String>,
}

// Manual implementation because of https://github.com/rust-lang/rust/issues/26925
impl<VarId, ScopeId> Hash for FieldRef<VarId, ScopeId> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let FieldRef { root, field } = self;
        root.hash(state);
        field.hash(state);
    }
}
