use super::{RenderImage, CONNECTIONS, SKELETON_LINE_THICKNESS};
use super::{Arc, ImageBuffer, ImageFrame, Rgba};
use crate::types::Frame;

pub(super) fn frame_to_image(frame: &Frame, overlay: Option<&[(f32, f32)]>) -> Option<Arc<RenderImage>> {
    let mut rgba = frame.rgba.clone();
    if let Some(points) = overlay {
        draw_skeleton(&mut rgba, frame.width, frame.height, points);
    }

    // GPUI expects BGRA; convert in place to avoid the async asset pipeline and flicker.
    for px in rgba.chunks_exact_mut(4) {
        px.swap(0, 2);
    }

    let buffer = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(frame.width, frame.height, rgba)?;
    let frame = ImageFrame::new(buffer);

    Some(Arc::new(RenderImage::new(vec![frame])))
}

fn draw_skeleton(buffer: &mut [u8], width: u32, height: u32, points: &[(f32, f32)]) {
    if points.len() < 2 {
        return;
    }

    let line_color = [96u8, 165u8, 250u8, 0u8];
    for &(a, b) in CONNECTIONS {
        if let (Some(pa), Some(pb)) = (points.get(a), points.get(b)) {
            draw_line(
                buffer,
                width,
                height,
                pa,
                pb,
                line_color,
                SKELETON_LINE_THICKNESS,
            );
        }
    }

    let point_color = [59u8, 130u8, 246u8, 0u8];
    for &(x, y) in points {
        draw_circle(buffer, width, height, (x as i32, y as i32), 3, point_color);
    }
}

fn draw_line(
    buffer: &mut [u8],
    width: u32,
    height: u32,
    p0: &(f32, f32),
    p1: &(f32, f32),
    color: [u8; 4],
    thickness: i32,
) {
    let (mut x0, mut y0) = (p0.0 as i32, p0.1 as i32);
    let (x1, y1) = (p1.0 as i32, p1.1 as i32);
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let radius = (thickness.max(1) - 1) / 2;

    loop {
        put_pixel_safe(buffer, width, height, x0, y0, color);
        if radius > 0 {
            for ox in -radius..=radius {
                for oy in -radius..=radius {
                    if ox == 0 && oy == 0 {
                        continue;
                    }
                    if ox.abs() + oy.abs() <= radius {
                        put_pixel_safe(buffer, width, height, x0 + ox, y0 + oy, color);
                    }
                }
            }
        }
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

fn draw_circle(
    buffer: &mut [u8],
    width: u32,
    height: u32,
    center: (i32, i32),
    radius: i32,
    color: [u8; 4],
) {
    let (cx, cy) = center;
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx * dx + dy * dy <= radius * radius {
                put_pixel_safe(buffer, width, height, cx + dx, cy + dy, color);
            }
        }
    }
}

fn put_pixel_safe(buffer: &mut [u8], width: u32, height: u32, x: i32, y: i32, color: [u8; 4]) {
    if x < 0 || y < 0 {
        return;
    }
    let (ux, uy) = (x as u32, y as u32);
    if ux >= width || uy >= height {
        return;
    }
    let idx = ((uy * width + ux) as usize) * 4;
    if idx + 3 < buffer.len() {
        buffer[idx..idx + 4].copy_from_slice(&color);
    }
}
