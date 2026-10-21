#[derive(Clone, Default)]
pub struct Curve(pub Vec<(f64, f64)>);

impl Curve {
    pub fn parse(csv: &str) -> Self {
        Self(
            csv.lines()
                .filter_map(|l| {
                    let (x, y) = l.split_once(',')?;
                    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
                })
                .collect(),
        )
    }

    pub fn at(&self, x: f64) -> f64 {
        let p = &self.0;
        match p.partition_point(|&(px, _)| px < x) {
            0 => p[0].1,
            i if i == p.len() => p[i - 1].1,
            i => {
                let ((x0, y0), (x1, y1)) = (p[i - 1], p[i]);
                y0 + (y1 - y0) * (x - x0) / (x1 - x0)
            }
        }
    }

    pub fn integral(&self, lo: f64, hi: f64, n: usize) -> f64 {
        let dx = (hi - lo) / n as f64;
        (0..n)
            .map(|i| self.at(lo + (i as f64 + 0.5) * dx))
            .sum::<f64>()
            * dx
    }
}
