use std::convert::TryFrom;

use anyhow::{Result, anyhow};
use nokhwa::{Buffer, utils::FrameFormat};
use rayon::prelude::*;
use yuv::{
    YuvBiPlanarImage, YuvConversionMode, YuvPackedImage, YuvRange, YuvStandardMatrix,
    yuv_nv12_to_rgba, yuyv422_to_rgba,
};
use zune_jpeg::{
    JpegDecoder,
    zune_core::{bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions},
};

#[derive(Debug)]
pub struct RgbaFrame {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub fn convert_camera_frame(frame: &Buffer) -> Result<RgbaFrame> {
    let resolution = frame.resolution();
    let width = resolution.width_x;
    let height = resolution.height_y;
    let data = frame.buffer();

    let rgba = match frame.source_frame_format() {
        FrameFormat::NV12 => nv12_to_rgba(data, width, height)?,
        FrameFormat::YUYV => yuyv_to_rgba(data, width, height)?,
        FrameFormat::MJPEG => mjpeg_to_rgba(data)?,
        FrameFormat::RAWRGB => raw_rgb_to_rgba(data, width, height)?,
        FrameFormat::RAWBGR => raw_bgr_to_rgba(data, width, height)?,
        FrameFormat::GRAY => gray_to_rgba(data, width, height)?,
    };

    Ok(RgbaFrame {
        rgba,
        width,
        height,
    })
}

fn nv12_to_rgba(data: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
    let y_plane_len = width as usize * height as usize;
    let uv_plane_len = y_plane_len / 2;

    if data.len() < y_plane_len + uv_plane_len {
        return Err(anyhow!(
            "NV12 buffer too small: got {}, expected {}",
            data.len(),
            y_plane_len + uv_plane_len
        ));
    }

    let y_plane = &data[..y_plane_len];
    let uv_plane = &data[y_plane_len..y_plane_len + uv_plane_len];
    let mut rgba = vec![0u8; y_plane_len * 4];

    let image = YuvBiPlanarImage {
        y_plane,
        y_stride: width,
        uv_plane,
        uv_stride: width,
        width,
        height,
    };

    yuv_nv12_to_rgba(
        &image,
        &mut rgba,
        width * 4,
        YuvRange::Full,
        YuvStandardMatrix::Bt709,
        YuvConversionMode::Balanced,
    )
    .map_err(|err| anyhow!("NV12→RGBA failed: {err:?}"))?;

    Ok(rgba)
}

fn yuyv_to_rgba(data: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
    let expected_len = width as usize * height as usize * 2;
    if data.len() < expected_len {
        return Err(anyhow!(
            "YUYV buffer too small: got {}, expected {}",
            data.len(),
            expected_len
        ));
    }

    let mut rgba = vec![0u8; (width as usize * height as usize) * 4];
    let packed = YuvPackedImage {
        yuy: data,
        yuy_stride: width * 2,
        width,
        height,
    };

    yuyv422_to_rgba(
        &packed,
        &mut rgba,
        width * 4,
        YuvRange::Full,
        YuvStandardMatrix::Bt709,
    )
    .map_err(|err| anyhow!("YUYV422→RGBA failed: {err:?}"))?;

    Ok(rgba)
}

fn mjpeg_to_rgba(data: &[u8]) -> Result<Vec<u8>> {
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(data), options);
    let rgba = decoder
        .decode()
        .map_err(|err| anyhow!("MJPEG decode failed: {err:?}"))?;

    if let Some(info) = decoder.info() {
        let expected_len = usize::try_from(info.width)
            .and_then(|w| usize::try_from(info.height).map(|h| w * h * 4))
            .map_err(|_| anyhow!("MJPEG dimensions do not fit usize"))?;
        if rgba.len() < expected_len {
            return Err(anyhow!(
                "MJPEG decode produced too few bytes: got {}, expected {}",
                rgba.len(),
                expected_len
            ));
        }
    }

    Ok(rgba)
}

fn raw_rgb_to_rgba(data: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
    rgb_like_to_rgba(data, width, height, false)
}

fn raw_bgr_to_rgba(data: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
    rgb_like_to_rgba(data, width, height, true)
}

fn rgb_like_to_rgba(data: &[u8], width: u32, height: u32, swap_rb: bool) -> Result<Vec<u8>> {
    let expected_len = width as usize * height as usize * 3;
    if data.len() < expected_len {
        return Err(anyhow!(
            "RGB buffer too small: got {}, expected {}",
            data.len(),
            expected_len
        ));
    }

    let mut rgba = vec![0u8; (width as usize * height as usize) * 4];
    rgba.par_chunks_mut(4)
        .zip(data.par_chunks_exact(3))
        .for_each(|(dst, src)| {
            if swap_rb {
                dst[0] = src[2];
                dst[1] = src[1];
                dst[2] = src[0];
            } else {
                dst[0] = src[0];
                dst[1] = src[1];
                dst[2] = src[2];
            }
            dst[3] = 255;
        });

    Ok(rgba)
}

fn gray_to_rgba(data: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
    let expected_len = width as usize * height as usize;
    if data.len() < expected_len {
        return Err(anyhow!(
            "GRAY buffer too small: got {}, expected {}",
            data.len(),
            expected_len
        ));
    }

    let mut rgba = vec![0u8; expected_len * 4];
    rgba.par_chunks_mut(4)
        .zip(data.par_iter().copied())
        .for_each(|(dst, value)| {
            dst[0] = value;
            dst[1] = value;
            dst[2] = value;
            dst[3] = 255;
        });

    Ok(rgba)
}
