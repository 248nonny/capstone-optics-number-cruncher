use super::ctx::Ctx;

pub type Refs = &'static [(&'static str, &'static str)];

pub struct Rule {
    pub vars: Vec<String>,
    pub f: Box<dyn Fn(&Ctx) -> f64>,
    pub eq: &'static str,
    pub refs: Refs,
}

impl Rule {
    pub fn new(vars: &[&str], eq: &'static str, refs: Refs, f: impl Fn(&Ctx) -> f64 + 'static) -> Self {
        Self { vars: vars.iter().map(|s| s.to_string()).collect(), f: Box::new(f), eq, refs }
    }

    pub fn per(vars: &[&str], eq: &'static str, refs: Refs, f: impl Fn(&Ctx, &[String]) -> f64 + 'static) -> Self {
        let names: Vec<String> = vars.iter().map(|s| s.to_string()).collect();
        Self::new(vars, eq, refs, move |c| f(c, &names))
    }

    pub fn out(&self) -> &str {
        &self.vars[0]
    }
}
