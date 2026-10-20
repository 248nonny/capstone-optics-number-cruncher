use eframe::egui;

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions::default();

    eframe::run_native(
        "Hello egui",
        options,
        Box::new(|_cc| Ok(Box::new(MyApp {}))),
    )
}

struct MyApp {}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("Hello World!");

            if ui.button("Hello, world!").clicked() {
                println!("Hello, world!");
            }
        });
    }
}
