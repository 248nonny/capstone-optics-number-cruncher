use super::{
    formula::Formula,
    library::Library,
    values::{Index, Values},
    var::VarDef,
};
use std::sync::Arc;

pub struct Model {
    pub defs: Vec<VarDef>,
    pub index: Arc<Index>,
    pub formulas: Vec<Formula>,
    pub lib: Library,
    ideals: Vec<(usize, usize, f64)>,
}

impl Model {
    pub fn new(defs: Vec<VarDef>, formulas: Vec<Formula>, lib: Library) -> Self {
        let values = Values::new(defs.iter().map(|d| d.id.clone()), vec![]);
        let ideals = defs
            .iter()
            .enumerate()
            .filter_map(|(i, d)| {
                Some((
                    i,
                    *values
                        .index
                        .get(&format!("{}.on", d.id.split_once('.')?.0))?,
                    d.ideal?,
                ))
            })
            .collect();
        Self {
            index: values.index,
            defs,
            formulas,
            lib,
            ideals,
        }
    }

    pub fn def(&self, id: &str) -> &VarDef {
        &self.defs[self.index[id]]
    }

    pub fn effective(&self, v: &Values) -> Values {
        let mut out = v.clone();
        self.ideals
            .iter()
            .filter(|&&(_, on, _)| v.v[on] > 0.5)
            .for_each(|&(i, _, x)| out.v[i] = x);
        out
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

    pub fn defaults(&self) -> Values {
        Values {
            v: self.defs.iter().map(|d| d.default).collect(),
            index: self.index.clone(),
        }
    }
}
