pub const CONNECTIONS: &[(usize, usize)] = &[
    (0, 1),
    (1, 2),
    (2, 3),
    (3, 4),
    (0, 5),
    (5, 6),
    (6, 7),
    (7, 8),
    (0, 9),
    (9, 10),
    (10, 11),
    (11, 12),
    (0, 13),
    (13, 14),
    (14, 15),
    (15, 16),
    (0, 17),
    (17, 18),
    (18, 19),
    (19, 20),
    (5, 9),
    (9, 13),
    (13, 17),
];

pub const SKELETON_LINE_THICKNESS: i32 = 12;
const PALM_BOX_THICKNESS: i32 = 6;
const PALM_SCORE_THRESHOLD: f32 = 0.25;

pub const DRAW_PALM_BBOX: bool = true;
pub const DRAW_ENLARGED_BOX: bool = true;
pub const DRAW_ROTATED_BOX: bool = true;

pub fn draw_skeleton(buffer: &mut [u8], width: u32, height: u32, points: &[(f32, f32)]) {
    if points.len() < 2 {
        return;
    }

    let line_color = [56u8, 189u8, 248u8, 255u8];
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

    let point_color = [248u8, 113u8, 113u8, 255u8];
    let point_radius = (SKELETON_LINE_THICKNESS / 2).max(4) + 2;
    for &(x, y) in points {
        draw_circle(
            buffer,
            width,
            height,
            (x as i32, y as i32),
            point_radius,
            point_color,
        );
    }
}

pub fn draw_palm_regions(
    buffer: &mut [u8],
    width: u32,
    height: u32,
    regions: &[crate::types::PalmRegion],
) {
    for region in regions {
        if region.score < PALM_SCORE_THRESHOLD {
            continue;
        }

        if DRAW_PALM_BBOX {
            let [x1, y1, x2, y2] = region.bbox;
            let rect_color = [16u8, 185u8, 129u8, 200u8];
            draw_rect(
                buffer,
                width,
                height,
                x1,
                y1,
                x2,
                y2,
                rect_color,
                PALM_BOX_THICKNESS,
            );

            let point_color = [244u8, 114u8, 182u8, 230u8];
            for &(lx, ly) in &region.landmarks {
                draw_circle(
                    buffer,
                    width,
                    height,
                    (lx as i32, ly as i32),
                    (PALM_BOX_THICKNESS / 2).max(3),
                    point_color,
                );
            }
        }

        let bbox_center = (
            (region.bbox[0] + region.bbox[2]) * 0.5,
            (region.bbox[1] + region.bbox[3]) * 0.5,
        );

        const SHIFT_Y: f32 = -0.4;
        const ENLARGE_FACTOR: f32 = 3.0;

        let base_w = (region.bbox[2] - region.bbox[0]).abs();
        let base_h = (region.bbox[3] - region.bbox[1]).abs();
        let (center_x, center_y) = (bbox_center.0, bbox_center.1 + SHIFT_Y * base_h);

        let landmark_span = if region.landmarks.is_empty() {
            0.0
        } else {
            let (min_x, max_x, min_y, max_y) = region
                .landmarks
                .iter()
                .fold((f32::MAX, f32::MIN, f32::MAX, f32::MIN), |acc, (x, y)| {
                    (acc.0.min(*x), acc.1.max(*x), acc.2.min(*y), acc.3.max(*y))
                });
            (max_x - min_x).max(max_y - min_y)
        };

        let side = base_w.max(base_h).max(landmark_span).max(80.0) * ENLARGE_FACTOR;

        let angle = if region.landmarks.len() < 3 {
            0.0
        } else {
            use std::f32::consts::PI;
            let p1 = region.landmarks[0];
            let p2 = region.landmarks[2];
            let radians = PI / 2.0 - (-(p2.1 - p1.1)).atan2(p2.0 - p1.0);
            let two_pi = 2.0 * PI;
            radians - two_pi * ((radians + PI) / two_pi).floor()
        };

        if DRAW_ENLARGED_BOX {
            let half_side = side / 2.0;
            let x1 = center_x - half_side;
            let y1 = center_y - half_side;
            let x2 = center_x + half_side;
            let y2 = center_y + half_side;
            let enlarged_color = [255u8, 165u8, 0u8, 200u8];
            draw_rect(
                buffer,
                width,
                height,
                x1,
                y1,
                x2,
                y2,
                enlarged_color,
                PALM_BOX_THICKNESS,
            );
        }

        if DRAW_ROTATED_BOX {
            let half_side = side / 2.0;
            let cos_a = angle.cos();
            let sin_a = angle.sin();

            let corners = [
                (-half_side, -half_side),
                (half_side, -half_side),
                (half_side, half_side),
                (-half_side, half_side),
            ];

            let rotated_corners: Vec<(f32, f32)> = corners
                .iter()
                .map(|(dx, dy)| {
                    let rx = dx * cos_a - dy * sin_a + center_x;
                    let ry = dx * sin_a + dy * cos_a + center_y;
                    (rx, ry)
                })
                .collect();

            let rotated_color = [255u8, 0u8, 255u8, 200u8];
            for i in 0..4 {
                let next = (i + 1) % 4;
                draw_line(
                    buffer,
                    width,
                    height,
                    &rotated_corners[i],
                    &rotated_corners[next],
                    rotated_color,
                    PALM_BOX_THICKNESS,
                );
            }
        }
    }
}

fn draw_rect(
    buffer: &mut [u8],
    width: u32,
    height: u32,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    color: [u8; 4],
    thickness: i32,
) {
    draw_line(
        buffer,
        width,
        height,
        &(x1, y1),
        &(x2, y1),
        color,
        thickness,
    );
    draw_line(
        buffer,
        width,
        height,
        &(x2, y1),
        &(x2, y2),
        color,
        thickness,
    );
    draw_line(
        buffer,
        width,
        height,
        &(x2, y2),
        &(x1, y2),
        color,
        thickness,
    );
    draw_line(
        buffer,
        width,
        height,
        &(x1, y2),
        &(x1, y1),
        color,
        thickness,
    );
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
