use super::values::Values;
use std::sync::Arc;

pub type Refs = &'static [(&'static str, &'static str)];

#[derive(Clone)]
pub struct Formula {
    pub id: String,
    pub title: String,
    pub vars: Vec<String>,
    pub free: usize,
    pub always: bool,
    pub default_on: bool,
    pub eq: String,
    pub desc: String,
    pub refs: Refs,
    pub f: Arc<dyn Fn(&Values) -> f64 + Send + Sync>,
}

impl Formula {
    pub fn new(
        id: &str,
        title: &str,
        vars: &[&str],
        free: usize,
        eq: &str,
        f: impl Fn(&Values, &[String]) -> f64 + Send + Sync + 'static,
    ) -> Self {
        let names: Vec<String> = vars.iter().map(|s| s.to_string()).collect();
        let own = names.clone();
        Self {
            id: id.into(),
            title: title.into(),
            vars: names,
            free,
            always: false,
            default_on: true,
            eq: eq.into(),
            desc: String::new(),
            refs: &[],
            f: Arc::new(move |v| f(v, &own)),
        }
    }

    pub fn always(mut self) -> Self {
        self.always = true;
        self
    }

    pub fn desc(mut self, d: &str) -> Self {
        self.desc = d.into();
        self
    }

    pub fn refs(mut self, r: Refs) -> Self {
        self.refs = r;
        self
    }
}
