use crate::engine::{model::Model, state::State, values::Values};

pub struct View<'a> {
    pub m: &'a Model,
    pub s: &'a State,
    pub blocked: &'a [Option<String>],
    pub out: Option<&'a Values>,
    pub stale: bool,
}

pub enum Action {
    Set(String, f64),
    Comp(String, String),
    ClearComp(String),
    Reset(String),
    Toggle(usize, bool),
    AddLink(String, String),
    RemoveLink(usize),
    SetMany(Vec<(String, f64)>),
    Parts(Vec<(String, String)>),
}
