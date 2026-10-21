mod engine;
mod model;
mod optimize;
mod physics;
mod ui;

fn main() -> eframe::Result {
    let arg = |i: usize| std::env::args().nth(i).and_then(|a| a.parse().ok()).unwrap_or(0);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1500.0, 950.0]),
        ..Default::default()
    };
    eframe::run_native("Chromatic confocal number cruncher", options, Box::new(move |_| Ok(Box::new(ui::App::new(arg(1), arg(2), arg(3) > 0)))))
}
