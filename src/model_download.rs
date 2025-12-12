use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Context;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::blocking::Client;

const MODEL_FILENAME: &str = "handpose_estimation_mediapipe_2023feb.onnx";
const MODEL_URL: &str = "https://raw.githubusercontent.com/214zzl995/gesture-universe/refs/heads/main/models/handpose_estimation_mediapipe_2023feb.onnx";

pub fn default_model_path() -> PathBuf {
    PathBuf::from("models").join(MODEL_FILENAME)
}

#[derive(Clone, Debug)]
pub enum DownloadEvent {
    AlreadyPresent,
    Started {
        total: Option<u64>,
    },
    Progress {
        downloaded: u64,
        #[allow(dead_code)]
        total: Option<u64>,
    },
    Finished,
}

pub fn ensure_model_available(model_path: &Path) -> anyhow::Result<()> {
    let mut progress: Option<ProgressBar> = None;
    ensure_model_available_with_callback(model_path, |event| match event {
        DownloadEvent::AlreadyPresent => {}
        DownloadEvent::Started { total } => {
            progress = Some(create_progress_bar(total));
        }
        DownloadEvent::Progress { downloaded, .. } => {
            if let Some(pb) = progress.as_ref() {
                pb.set_position(downloaded);
            }
        }
        DownloadEvent::Finished => {
            if let Some(pb) = progress.take() {
                pb.finish_with_message("handpose model ready");
            }
        }
    })
}

pub fn ensure_model_available_with_callback<F>(
    model_path: &Path,
    mut on_event: F,
) -> anyhow::Result<()>
where
    F: FnMut(DownloadEvent),
{
    if model_path.exists() {
        on_event(DownloadEvent::AlreadyPresent);
        on_event(DownloadEvent::Finished);
        return Ok(());
    }

    if let Some(parent) = model_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create model directory {}", parent.display()))?;
    }

    download_to_path(MODEL_URL, model_path, &mut on_event)
}

fn download_to_path<F>(url: &str, dest: &Path, on_event: &mut F) -> anyhow::Result<()>
where
    F: FnMut(DownloadEvent),
{
    log::info!(
        "downloading handpose model from {url} to {}",
        dest.display()
    );

    let client = Client::new();
    let mut response = client
        .get(url)
        .send()
        .context("failed to start handpose model download")?
        .error_for_status()
        .context("handpose model download returned error status")?;

    let total_size = response.content_length();
    on_event(DownloadEvent::Started { total: total_size });

    let tmp_path = dest.with_extension("download");
    let mut file = fs::File::create(&tmp_path)
        .with_context(|| format!("failed to create {}", tmp_path.display()))?;

    let mut downloaded: u64 = 0;
    let mut buffer = [0u8; 16 * 1024];
    loop {
        let bytes_read = response
            .read(&mut buffer)
            .context("failed while reading model bytes")?;
        if bytes_read == 0 {
            break;
        }

        file.write_all(&buffer[..bytes_read])
            .context("failed while writing model to disk")?;
        downloaded += bytes_read as u64;
        on_event(DownloadEvent::Progress {
            downloaded,
            total: total_size,
        });
    }

    file.sync_all()
        .context("failed to flush downloaded model to disk")?;
    fs::rename(&tmp_path, dest).with_context(|| {
        format!(
            "failed to move temp model {} into place at {}",
            tmp_path.display(),
            dest.display()
        )
    })?;

    on_event(DownloadEvent::Finished);
    Ok(())
}

fn create_progress_bar(total_size: Option<u64>) -> ProgressBar {
    match total_size {
        Some(total) if total > 0 => {
            let pb = ProgressBar::new(total);
            let style = ProgressStyle::with_template(
                "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta})",
            )
            .unwrap()
            .progress_chars("=>-");
            pb.set_style(style);
            pb
        }
        _ => {
            let pb = ProgressBar::new_spinner();
            let style = ProgressStyle::with_template("{spinner:.green} downloading handpose model")
                .unwrap();
            pb.set_style(style);
            pb.enable_steady_tick(Duration::from_millis(100));
            pb
        }
    }
}
