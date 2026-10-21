use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Source {
    Manual(f64),
    Link(String),
    Rule(usize),
    Component(String),
}
