use super::{get::Get, library::Library, rule::Rule, solve::bisect, source::Source, var::{Spacing, VarDef}};
use std::{any::Any, cell::RefCell, collections::HashMap, rc::Rc};

pub struct Model {
    pub defs: Vec<VarDef>,
    pub index: HashMap<String, usize>,
    pub sources: HashMap<String, Source>,
    pub defaults: HashMap<String, Source>,
    pub rules: Vec<Rule>,
    pub lib: Library,
}

impl Model {
    pub fn new(defs: Vec<VarDef>, rules: Vec<Rule>, lib: Library, defaults: Vec<(String, Source)>) -> Self {
        let index = defs.iter().enumerate().map(|(i, d)| (d.id.clone(), i)).collect();
        let mut sources: HashMap<String, Source> = defs.iter().map(|d| (d.id.clone(), Source::Manual(d.default))).collect();
        sources.extend(defaults);
        Self { defs, index, defaults: sources.clone(), sources, rules, lib }
    }

    pub fn def(&self, id: &str) -> &VarDef {
        &self.defs[self.index[id]]
    }

    pub fn rules_of<'a>(&'a self, id: &'a str) -> impl Iterator<Item = usize> + 'a {
        (0..self.rules.len()).filter(move |&r| self.rules[r].vars.iter().any(|v| v == id))
    }

    pub fn set_source(&mut self, id: &str, src: Source) {
        if let Source::Rule(r) = src {
            let frozen: Vec<(String, f64)> = {
                let ctx = Ctx::new(self);
                self.rules[r]
                    .vars
                    .iter()
                    .filter(|v| v.as_str() != id && self.sources.get(*v) == Some(&Source::Rule(r)))
                    .map(|v| (v.clone(), ctx.g(v)))
                    .collect()
            };
            self.sources.extend(frozen.into_iter().map(|(v, x)| (v, Source::Manual(x))));
        }
        self.sources.insert(id.into(), src);
    }

    pub fn reset(&mut self, prefix: &str) {
        let keep = format!("{prefix}.");
        for (k, v) in self.defaults.iter().filter(|(k, _)| k.starts_with(&keep)) {
            self.sources.insert(k.clone(), v.clone());
        }
    }

    pub fn set_value(&mut self, id: &str, x: f64) {
        self.sources.insert(id.into(), Source::Manual(x));
    }
}

pub struct Ctx<'a> {
    pub m: &'a Model,
    over: HashMap<String, f64>,
    cache: RefCell<HashMap<String, f64>>,
    stack: RefCell<Vec<String>>,
    memo: RefCell<HashMap<&'static str, Rc<dyn Any>>>,
    pub errors: RefCell<Vec<String>>,
}

impl<'a> Ctx<'a> {
    pub fn new(m: &'a Model) -> Self {
        Self::with(m, HashMap::new())
    }

    pub fn with(m: &'a Model, over: HashMap<String, f64>) -> Self {
        Self { m, over, cache: Default::default(), stack: Default::default(), memo: Default::default(), errors: Default::default() }
    }

    fn fail(&self, msg: String) -> f64 {
        let mut e = self.errors.borrow_mut();
        if !e.contains(&msg) {
            e.push(msg);
        }
        f64::NAN
    }

    fn resolve(&self, id: &str) -> f64 {
        if let Some(&v) = self.over.get(id) {
            return v;
        }
        if let Some(&v) = self.cache.borrow().get(id) {
            return v;
        }
        if self.stack.borrow().iter().any(|s| s == id) {
            return self.fail(format!("cycle through {id}"));
        }
        let Some(src) = self.m.sources.get(id) else {
            return self.fail(format!("unknown variable {id}"));
        };
        self.stack.borrow_mut().push(id.into());
        let v = match src {
            Source::Manual(v) => *v,
            Source::Link(o) => self.g(o),
            Source::Component(c) => {
                let d = self.m.def(id);
                self.m.lib.value(c, id).or_else(|| self.m.lib.value(c, d.local())).unwrap_or(d.default)
            }
            Source::Rule(r) => self.solve(*r, id),
        };
        self.stack.borrow_mut().pop();
        self.cache.borrow_mut().insert(id.into(), v);
        v
    }

    fn solve(&self, r: usize, id: &str) -> f64 {
        let rule = &self.m.rules[r];
        if rule.out() == id {
            return (rule.f)(self);
        }
        let d = self.m.def(id);
        let target = self.g(rule.out());
        let residual = |x: f64| {
            let mut over = self.over.clone();
            over.insert(id.into(), x);
            (rule.f)(&Ctx::with(self.m, over)) - target
        };
        bisect(d.min, d.max, d.spacing == Spacing::Log, residual)
            .unwrap_or_else(|| self.fail(format!("no solution for {id} in range")))
    }

    pub fn memo<T: 'static>(&self, key: &'static str, f: impl FnOnce() -> T) -> Rc<T> {
        if let Some(v) = self.memo.borrow().get(key) {
            return v.clone().downcast().unwrap();
        }
        let v = Rc::new(f());
        self.memo.borrow_mut().insert(key, v.clone());
        v
    }
}

impl Get for Ctx<'_> {
    fn g(&self, k: &str) -> f64 {
        self.resolve(k)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> Model {
        let defs = vec![
            VarDef::new("a.f", "f", "", 1.0, 1.0, 100.0, 10.0),
            VarDef::new("a.d", "d", "", 1.0, 1.0, 100.0, 4.0),
            VarDef::new("a.na", "na", "", 1.0, 0.01, 1.0, 0.2).out(),
            VarDef::new("b.x", "x", "", 1.0, 0.0, 10.0, 1.0),
        ];
        let rule = Rule::new(&["a.na", "a.d", "a.f"], "NA = D/2f", &[], |c| c.g("a.d") / (2.0 * c.g("a.f")));
        Model::new(defs, vec![rule], Library::default(), vec![("a.na".into(), Source::Rule(0))])
    }

    #[test]
    fn forward_rule() {
        assert!((Ctx::new(&model()).g("a.na") - 0.2).abs() < 1e-12);
    }

    #[test]
    fn inverse_round_trip() {
        let mut m = model();
        m.set_value("a.na", 0.1);
        m.set_source("a.f", Source::Rule(0));
        assert!((Ctx::new(&m).g("a.f") - 20.0).abs() < 1e-6);
    }

    #[test]
    fn dependent_swap_freezes_previous() {
        let mut m = model();
        m.set_source("a.d", Source::Rule(0));
        assert_eq!(m.sources["a.na"], Source::Manual(0.2));
        assert!((Ctx::new(&m).g("a.d") - 4.0).abs() < 1e-6);
    }

    #[test]
    fn link_cycle_reports_error() {
        let mut m = model();
        m.sources.insert("a.f".into(), Source::Link("b.x".into()));
        m.sources.insert("b.x".into(), Source::Link("a.f".into()));
        let c = Ctx::new(&m);
        assert!(c.g("a.f").is_nan());
        assert!(!c.errors.borrow().is_empty());
    }
}
