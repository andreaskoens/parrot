//! egui/eframe app shell wiring the Parrot rewrite's crates together.

fn main() -> anyhow::Result<()> {
    // Wide enough for all four panels at their default widths (ui-description §2) plus a
    // 640px text column.
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Parrot")
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png"))
                    .expect("assets/icon.png is a valid PNG"),
            )
            .with_inner_size([1400.0, 860.0])
            .with_min_inner_size([960.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Parrot",
        options,
        Box::new(|cc| {
            Ok(Box::new(parrot_app::ui::ParrotApp::with_context(
                &cc.egui_ctx,
            )))
        }),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {e}"))
}
