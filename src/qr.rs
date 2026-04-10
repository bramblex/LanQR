use egui::ColorImage;
use image::Luma;
use qrcode::QrCode;

use crate::errors::{LanQrError, Result};

pub fn build_qr_texture_input(url: &str, size: u32) -> Result<ColorImage> {
    let code = QrCode::new(url.as_bytes())
        .map_err(|error| LanQrError::Message(format!("生成二维码失败：{error}")))?;

    let image = code
        .render::<Luma<u8>>()
        .min_dimensions(size, size)
        .quiet_zone(true)
        .build();

    let width = image.width() as usize;
    let height = image.height() as usize;
    let mut rgba = Vec::with_capacity(width * height * 4);

    for pixel in image.into_raw() {
        rgba.extend_from_slice(&[pixel, pixel, pixel, 255]);
    }

    Ok(ColorImage::from_rgba_unmultiplied([width, height], &rgba))
}
