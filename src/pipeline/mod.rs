pub mod camera;
pub mod compositor;
pub mod recognizer;
pub mod rgba_converter;
pub mod skeleton;

// Re-exports for convenience
pub use camera::{CameraDevice, CameraStream, available_cameras, start_camera_stream};
pub use compositor::{CompositedFrame, start_frame_compositor};
pub use recognizer::{RecognizerBackend, start_recognizer};
