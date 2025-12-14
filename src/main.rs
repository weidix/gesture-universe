#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod gesture;
mod model_download;
mod pipeline;
mod types;
mod ui;

use anyhow::Result;
use crossbeam_channel::bounded;
use gpui::Application;
use gpui_component;
use pipeline::RecognizerBackend;

fn main() -> Result<()> {
    env_logger::init();

    let (camera_frame_tx, camera_frame_rx) = bounded(1);

    let recognizer_backend = RecognizerBackend::default();

    Application::new()
        .with_assets(gpui_component_assets::Assets)
        .run(move |app| {
            gpui_component::init(app);

            if let Err(err) = ui::launch_ui(
                app,
                camera_frame_rx,
                camera_frame_tx,
                recognizer_backend.clone(),
            ) {
                eprintln!("failed to launch ui: {err:?}");
            }
        });

    Ok(())
}
