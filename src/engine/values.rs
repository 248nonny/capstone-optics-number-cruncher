use super::get::Get;
use std::{
    collections::HashMap,
    hash::{BuildHasherDefault, Hasher},
    sync::Arc,
};

pub struct Fnv(u64);

impl Default for Fnv {
    fn default() -> Self {
        Fnv(0xcbf29ce484222325)
    }
}

impl Hasher for Fnv {
    fn write(&mut self, bytes: &[u8]) {
        self.0 = bytes
            .iter()
            .fold(self.0, |h, &b| (h ^ b as u64).wrapping_mul(0x100000001b3));
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

pub type Index = HashMap<String, usize, BuildHasherDefault<Fnv>>;

#[derive(Clone)]
pub struct Values {
    pub v: Vec<f64>,
    pub index: Arc<Index>,
}

impl Values {
    pub fn new(ids: impl Iterator<Item = String>, v: Vec<f64>) -> Self {
        Self {
            v,
            index: Arc::new(ids.enumerate().map(|(i, id)| (id, i)).collect()),
        }
    }

    pub fn set(&mut self, id: &str, x: f64) {
        self.v[self.index[id]] = x;
    }
}

impl Get for Values {
    fn g(&self, k: &str) -> f64 {
        match self.index.get(k) {
            Some(&i) => self.v[i],
            None => {
                debug_assert!(false, "unknown variable {k}");
                f64::NAN
            }
        }
    }
}
