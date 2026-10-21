use super::{
    library::Library,
    values::{Index, Values},
    var::VarDef,
};
use std::sync::Arc;

pub struct Model {
    pub defs: Vec<VarDef>,
    pub index: Arc<Index>,
    pub lib: Library,
}

impl Model {
    pub fn new(defs: Vec<VarDef>, lib: Library) -> Self {
        let values = Values::new(defs.iter().map(|d| d.id.clone()), vec![]);
        Self {
            index: values.index,
            defs,
            lib,
        }
    }

    pub fn def(&self, id: &str) -> &VarDef {
        &self.defs[self.index[id]]
    }

    pub fn defaults(&self) -> Values {
        Values {
            v: self.defs.iter().map(|d| d.default).collect(),
            index: self.index.clone(),
        }
    }

    pub fn hidden(&self, v: &Values, i: usize) -> bool {
        let d = &self.defs[i];
        d.hide.is_some_and(|(local, value)| {
            let on =
                d.id.split_once('.')
                    .and_then(|(p, _)| self.index.get(&format!("{p}.{local}")));
            on.is_some_and(|&k| (v.v[k] - value).abs() < 0.5)
        })
    }
}
