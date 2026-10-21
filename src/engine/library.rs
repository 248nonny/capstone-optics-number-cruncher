use serde::Deserialize;
use std::collections::HashMap;

#[derive(Clone, Deserialize)]
pub struct Component {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub note: String,
    pub params: HashMap<String, f64>,
}

#[derive(Clone, Default)]
pub struct Library {
    pub comps: Vec<Component>,
    pub errors: Vec<String>,
}

impl Library {
    pub fn load(embedded: &[&str], dir: &str) -> Self {
        let from_dir = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "toml"))
            .filter_map(|e| Some((e.path().display().to_string(), std::fs::read_to_string(e.path()).ok()?)));
        let texts = embedded.iter().map(|t| ("embedded".to_string(), t.to_string())).chain(from_dir);
        let mut lib = Self::default();
        for (origin, text) in texts {
            match toml::from_str::<Component>(&text) {
                Ok(c) => match lib.comps.iter().position(|o| o.name == c.name) {
                    Some(i) => lib.comps[i] = c,
                    None => lib.comps.push(c),
                },
                Err(e) => lib.errors.push(format!("Component file {origin} ignored: {}", e.message())),
            }
        }
        lib
    }

    pub fn value(&self, name: &str, local: &str) -> Option<f64> {
        self.comps.iter().find(|c| c.name == name)?.params.get(local).copied()
    }

    pub fn of_kind<'a>(&'a self, kind: &'a str) -> impl Iterator<Item = &'a Component> {
        self.comps.iter().filter(move |c| c.kind == kind)
    }

    pub fn get(&self, name: &str) -> Option<&Component> {
        self.comps.iter().find(|c| c.name == name)
    }
}
