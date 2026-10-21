#[derive(Clone, Copy, PartialEq)]
pub enum Spacing {
    Lin,
    Log,
}

#[derive(Clone)]
pub struct VarDef {
    pub id: String,
    pub label: &'static str,
    pub unit: &'static str,
    pub scale: f64,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    pub spacing: Spacing,
    pub choices: &'static [&'static str],
    pub output: bool,
    pub doc: &'static str,
}

impl VarDef {
    pub fn new(id: &str, label: &'static str, unit: &'static str, scale: f64, min: f64, max: f64, default: f64) -> Self {
        Self { id: id.into(), label, unit, scale, min, max, default, spacing: Spacing::Lin, choices: &[], output: false, doc: "" }
    }

    pub fn log(mut self) -> Self {
        self.spacing = Spacing::Log;
        self
    }

    pub fn out(mut self) -> Self {
        self.output = true;
        self
    }

    pub fn doc(mut self, doc: &'static str) -> Self {
        self.doc = doc;
        self
    }

    pub fn pick(mut self, choices: &'static [&'static str], default: usize) -> Self {
        self.choices = choices;
        self.min = 0.0;
        self.max = (choices.len() - 1) as f64;
        self.default = default as f64;
        self
    }

    pub fn discrete(&self) -> bool {
        !self.choices.is_empty()
    }

    pub fn local(&self) -> &str {
        self.id.split_once('.').map_or(&self.id, |(_, l)| l)
    }

    pub fn prefix(&self) -> &str {
        self.id.split_once('.').map_or("", |(p, _)| p)
    }
}
