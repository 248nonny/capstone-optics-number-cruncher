use super::{formula::Formula, library::Library, values::{Index, Values}, var::VarDef};
use std::sync::Arc;

pub struct Model {
    pub defs: Vec<VarDef>,
    pub index: Arc<Index>,
    pub formulas: Vec<Formula>,
    pub lib: Library,
}

impl Model {
    pub fn new(defs: Vec<VarDef>, formulas: Vec<Formula>, lib: Library) -> Self {
        let values = Values::new(defs.iter().map(|d| d.id.clone()), vec![]);
        Self { index: values.index, defs, formulas, lib }
    }

    pub fn def(&self, id: &str) -> &VarDef {
        &self.defs[self.index[id]]
    }

    pub fn defaults(&self) -> Values {
        Values { v: self.defs.iter().map(|d| d.default).collect(), index: self.index.clone() }
    }
}
