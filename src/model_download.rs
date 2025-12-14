use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Context;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::blocking::Client;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelKind {
    HandposeEstimator,
    PalmDetector,
}

const HANDPOSE_ESTIMATOR_MODEL_FILENAME: &str = "handpose_estimation_mediapipe_2023feb.onnx";
const HANDPOSE_ESTIMATOR_MODEL_URL: &str = "https://raw.githubusercontent.com/214zzl995/gesture-universe/refs/heads/main/models/handpose_estimation_mediapipe_2023feb.onnx";
const PALM_DETECTOR_MODEL_FILENAME: &str = "palm_detection_mediapipe_2023feb.onnx";
const PALM_DETECTOR_MODEL_URL: &str = "https://raw.githubusercontent.com/214zzl995/gesture-universe/refs/heads/main/models/palm_detection_mediapipe_2023feb.onnx";

pub fn default_handpose_estimator_model_path() -> PathBuf {
    PathBuf::from("models").join(HANDPOSE_ESTIMATOR_MODEL_FILENAME)
}

pub fn default_palm_detector_model_path() -> PathBuf {
    PathBuf::from("models").join(PALM_DETECTOR_MODEL_FILENAME)
}

#[derive(Clone, Debug)]
pub enum ModelDownloadEvent {
    AlreadyPresent {
        model: ModelKind,
    },
    Started {
        model: ModelKind,
        total: Option<u64>,
    },
    Progress {
        model: ModelKind,
        downloaded: u64,
        total: Option<u64>,
    },
    Finished {
        model: ModelKind,
    },
}

pub fn ensure_handpose_estimator_model_ready<F>(
    model_path: &Path,
    mut on_event: F,
) -> anyhow::Result<()>
where
    F: FnMut(ModelDownloadEvent),
{
    if model_path.exists() {
        on_event(ModelDownloadEvent::AlreadyPresent {
            model: ModelKind::HandposeEstimator,
        });
        on_event(ModelDownloadEvent::Finished {
            model: ModelKind::HandposeEstimator,
        });
        return Ok(());
    }

    if let Some(parent) = model_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create model directory {}", parent.display()))?;
    }

    let mut progress: Option<ProgressBar> = None;
    download_to_path(
        ModelKind::HandposeEstimator,
        HANDPOSE_ESTIMATOR_MODEL_URL,
        model_path,
        &mut |event| {
            match &event {
                ModelDownloadEvent::Started { total, .. } => {
                    progress = Some(create_progress_bar(*total));
                }
                ModelDownloadEvent::Progress { downloaded, .. } => {
                    if let Some(pb) = progress.as_ref() {
                        pb.set_position(*downloaded);
                    }
                }
                ModelDownloadEvent::Finished { .. } => {
                    if let Some(pb) = progress.take() {
                        pb.finish_with_message("handpose model ready");
                    }
                }
                ModelDownloadEvent::AlreadyPresent { .. } => {}
            }
            on_event(event);
        },
    )
}

fn download_to_path<F>(
    model: ModelKind,
    url: &str,
    dest: &Path,
    on_event: &mut F,
) -> anyhow::Result<()>
where
    F: FnMut(ModelDownloadEvent),
{
    let model_label = match model {
        ModelKind::HandposeEstimator => "handpose estimator",
        ModelKind::PalmDetector => "palm detector",
    };
    log::info!(
        "downloading {model_label} model from {url} to {}",
        dest.display()
    );

    let client = Client::new();
    let mut response = client
        .get(url)
        .send()
        .context("failed to start model download")?
        .error_for_status()
        .context("model download returned error status")?;

    let total_size = response.content_length();
    on_event(ModelDownloadEvent::Started {
        model,
        total: total_size,
    });

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
        on_event(ModelDownloadEvent::Progress {
            model,
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

    on_event(ModelDownloadEvent::Finished { model });
    Ok(())
}

pub fn ensure_palm_detector_model_ready<F>(model_path: &Path, mut on_event: F) -> anyhow::Result<()>
where
    F: FnMut(ModelDownloadEvent),
{
    if model_path.exists() {
        on_event(ModelDownloadEvent::AlreadyPresent {
            model: ModelKind::PalmDetector,
        });
        on_event(ModelDownloadEvent::Finished {
            model: ModelKind::PalmDetector,
        });
        return Ok(());
    }
    if let Some(parent) = model_path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create palm detector model directory {}",
                parent.display()
            )
        })?;
    }

    let bundled = Path::new("palm_detection_mediapipe").join(PALM_DETECTOR_MODEL_FILENAME);
    if bundled.exists() {
        on_event(ModelDownloadEvent::Started {
            model: ModelKind::PalmDetector,
            total: None,
        });
        fs::copy(&bundled, model_path).with_context(|| {
            format!(
                "failed to copy bundled palm detector model from {} to {}",
                bundled.display(),
                model_path.display()
            )
        })?;
        on_event(ModelDownloadEvent::Finished {
            model: ModelKind::PalmDetector,
        });
        return Ok(());
    }

    log::info!(
        "bundled palm detector not found, downloading from {}",
        PALM_DETECTOR_MODEL_URL
    );
    download_to_path(
        ModelKind::PalmDetector,
        PALM_DETECTOR_MODEL_URL,
        model_path,
        &mut on_event,
    )
    .with_context(|| {
        format!(
            "failed to download palm detector model to {}",
            model_path.display()
        )
    })
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
            let style = ProgressStyle::with_template("{spinner:.green} downloading model").unwrap();
            pb.set_style(style);
            pb.enable_steady_tick(Duration::from_millis(100));
            pb
        }
    }
}
