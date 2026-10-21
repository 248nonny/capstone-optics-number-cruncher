use std::sync::{Arc, Mutex};

type Job<T> = Box<dyn FnOnce() -> T + Send>;

struct Inner<T> {
    busy: bool,
    pending: Option<Job<T>>,
    result: Option<T>,
}

pub struct Latest<T> {
    inner: Arc<Mutex<Inner<T>>>,
}

impl<T: Send + 'static> Latest<T> {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                busy: false,
                pending: None,
                result: None,
            })),
        }
    }

    pub fn submit(
        &self,
        job: impl FnOnce() -> T + Send + 'static,
        done: impl Fn() + Send + 'static,
    ) {
        let mut g = self.inner.lock().unwrap();
        g.pending = Some(Box::new(job));
        if g.busy {
            return;
        }
        g.busy = true;
        let inner = self.inner.clone();
        std::thread::spawn(move || loop {
            let job = {
                let mut g = inner.lock().unwrap();
                match g.pending.take() {
                    Some(j) => j,
                    None => {
                        g.busy = false;
                        return;
                    }
                }
            };
            let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job));
            if let Ok(out) = out {
                inner.lock().unwrap().result = Some(out);
            }
            done();
        });
    }

    pub fn take(&self) -> Option<T> {
        self.inner.lock().unwrap().result.take()
    }

    pub fn busy(&self) -> bool {
        let g = self.inner.lock().unwrap();
        g.busy || g.pending.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panicking_job_does_not_freeze_the_worker() {
        let w: Latest<u32> = Latest::new();
        w.submit(|| panic!("boom"), || {});
        let t = std::time::Instant::now();
        while w.busy() && t.elapsed().as_secs() < 5 {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        w.submit(|| 7, || {});
        while w.busy() && t.elapsed().as_secs() < 10 {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(w.take(), Some(7));
    }
}
