use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::Result;
use crossbeam_channel::Sender;
use nokhwa::{
    Camera,
    pixel_format::RgbFormat,
    query,
    utils::{ApiBackend, CameraIndex, CameraInfo, RequestedFormat, RequestedFormatType},
};

use crate::types::Frame;

// Limit the number of frames we hand over to the recognizer to reduce load.
const RECOGNIZER_TARGET_FPS: u64 = 10;
const RECOGNIZER_FRAME_INTERVAL: Duration = Duration::from_millis(1_000 / RECOGNIZER_TARGET_FPS);

#[derive(Clone, Debug)]
pub struct CameraDevice {
    pub index: CameraIndex,
    pub label: String,
}

#[derive(Debug)]
pub struct CameraStream {
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl CameraStream {
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for CameraStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

pub fn available_cameras() -> Result<Vec<CameraDevice>> {
    let cameras = query(ApiBackend::Auto)?;
    Ok(cameras
        .into_iter()
        .map(|info| CameraDevice {
            index: info.index().clone(),
            label: format_camera_label(&info),
        })
        .collect())
}

fn format_camera_label(info: &CameraInfo) -> String {
    info.human_name()
}

fn build_camera(index: CameraIndex) -> Result<Camera> {
    let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::None);
    let mut camera = Camera::new(index, requested)?;
    camera.open_stream()?;
    Ok(camera)
}

pub fn start_camera_stream(
    index: CameraIndex,
    ui_tx: Sender<Frame>,
    recog_tx: Sender<Frame>,
) -> Result<CameraStream> {
    // Fail fast before spawning the capture thread.
    build_camera(index.clone())?;

    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = stop.clone();

    let handle = thread::spawn(move || {
        let mut last_recog_frame = Instant::now() - RECOGNIZER_FRAME_INTERVAL;

        let mut camera = match build_camera(index) {
            Ok(cam) => cam,
            Err(err) => {
                log::error!("failed to open camera: {err:?}");
                return;
            }
        };

        while !stop_flag.load(Ordering::Relaxed) {
            let frame = match camera.frame() {
                Ok(frame) => frame,
                Err(err) => {
                    log::warn!("camera frame read failed: {err:?}");
                    continue;
                }
            };

            let decoded = match frame.decode_image::<RgbFormat>() {
                Ok(img) => img,
                Err(err) => {
                    log::warn!("failed to decode camera frame: {err:?}");
                    continue;
                }
            };

            let (width, height) = decoded.dimensions();
            let rgb = decoded.into_raw();
            if rgb.is_empty() {
                continue;
            }

            // Expand RGB to RGBA for the UI pipeline.
            let mut rgba = Vec::with_capacity(rgb.len() / 3 * 4);
            for chunk in rgb.chunks_exact(3) {
                rgba.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
            }

            let frame_timestamp = Instant::now();
            let frame = Frame {
                rgba,
                width,
                height,
                timestamp: frame_timestamp,
            };

            let should_queue_recog = last_recog_frame.elapsed() >= RECOGNIZER_FRAME_INTERVAL;
            let recog_frame = if should_queue_recog {
                Some(frame.clone())
            } else {
                None
            };

            // Send the raw frame to the UI; if the UI queue is full we drop it.
            let _ = ui_tx.try_send(frame);

            // Throttle recognizer input to ~10fps and drop if the worker is busy.
            if let Some(frame) = recog_frame {
                last_recog_frame = frame_timestamp;
                let _ = recog_tx.try_send(frame);
            }
        }
    });

    Ok(CameraStream {
        stop,
        handle: Some(handle),
    })
}
