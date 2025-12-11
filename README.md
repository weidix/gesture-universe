# Gesture Universe

Gesture Universe is a high-performance, real-time hand gesture recognition application built with Rust and [GPUI](https://github.com/zed-industries/zed). It uses MediaPipe ONNX models to detect hand landmarks and classify gestures directly from your webcam feed.

## Features

- **Real-time Hand Tracking**: Fast and accurate hand landmark detection using MediaPipe models.
- **Gesture Recognition**: Supports detection of various gestures:
  - ✌️ Victory (V)
  - 👌 OK
  - 👍 Thumbs Up
  - ☝️ Pointing
  - 🤟 I Love You
  - 🫶 Finger Heart
  - ✊ Fist
  - 👋 Open Hand
- **Modern UI**: Built with GPUI for a native, high-performance user interface on macOS.
- **Live Camera Feed**: Integrated camera support for real-time interaction.

## Getting Started

### Prerequisites

- **Rust**: Ensure you have the latest stable version of Rust installed.
- **Webcam**: A functional webcam is required for the main application.

### Installation

1.  Clone the repository:
    ```bash
    git clone https://github.com/214zzl995/gesture-universe.git
    cd gesture-universe
    ```

2.  Build the project:
    ```bash
    cargo build --release
    ```

### Running the Application

To start the main application with the UI:

```bash
cargo run --release
```

### Running Examples

You can also run standalone examples to test the recognition logic on static images:

```bash
# Run gesture recognition on a sample image
cargo run --example gesture_from_image
```

## Project Structure

- **`src/`**:
    - `main.rs`: Application entry point.
    - `ui/`: GPUI-based user interface components.
    - `camera.rs`: Camera capture and frame processing.
    - `recognizer.rs`: ONNX model inference engine.
    - `gesture.rs`: Gesture classification logic.
    - `types.rs`: Common data types and structures.
- **`examples/`**: Example scripts for testing and demonstration.
- **`handpose_estimation_mediapipe/`**: Contains the ONNX models used for inference.

## License

This project is open source.
