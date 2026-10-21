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
    pub design: bool,
    pub ideal: Option<f64>,
    pub hide: Option<(&'static str, f64)>,
    pub doc: &'static str,
}

impl VarDef {
    pub fn new(
        id: &str,
        label: &'static str,
        unit: &'static str,
        scale: f64,
        min: f64,
        max: f64,
        default: f64,
    ) -> Self {
        Self {
            id: id.into(),
            label,
            unit,
            scale,
            min,
            max,
            default,
            spacing: Spacing::Lin,
            choices: &[],
            output: false,
            design: false,
            ideal: None,
            hide: None,
            doc: "",
        }
    }

    pub fn design(mut self) -> Self {
        self.design = true;
        self
    }

    pub fn imperfect(mut self, ideal: f64) -> Self {
        self.ideal = Some(ideal);
        self.hide = Some(("on", 1.0));
        self
    }

    pub fn hidden_if_ideal(mut self) -> Self {
        self.hide = Some(("on", 1.0));
        self
    }

    pub fn when_on(mut self) -> Self {
        self.hide = Some(("on", 0.0));
        self
    }

    pub fn log(mut self) -> Self {
        self.spacing = Spacing::Log;
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

    pub fn prefix(&self) -> &str {
        self.id.split_once('.').map_or("", |(p, _)| p)
    }
}
