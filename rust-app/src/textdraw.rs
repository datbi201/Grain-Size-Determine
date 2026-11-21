//! Vẽ chữ lên ảnh RGB (dòng thông số trên ảnh kết quả) bằng font Arial / Segoe UI của Windows.
use gsd_core::Rgb;
use std::sync::OnceLock;

static FONT: OnceLock<Option<fontdue::Font>> = OnceLock::new();

fn font() -> Option<&'static fontdue::Font> {
    FONT.get_or_init(|| {
        let dir = std::path::PathBuf::from(std::env::var("WINDIR").unwrap_or("C:\\Windows".into())).join("Fonts");
        for f in ["arialbd.ttf", "arial.ttf", "segoeuib.ttf", "segoeui.ttf", "tahoma.ttf"] {
            if let Ok(b) = std::fs::read(dir.join(f)) {
                if let Ok(font) = fontdue::Font::from_bytes(b, fontdue::FontSettings::default()) {
                    return Some(font);
                }
            }
        }
        None
    }).as_ref()
}

pub fn text_width(text: &str, px: f32) -> f32 {
    let Some(f) = font() else { return 0.0 };
    text.chars().map(|c| f.metrics(c, px).advance_width).sum()
}

/// Vẽ chữ, (x, y) là góc trên-trái dòng chữ.
pub fn draw_text(img: &mut Rgb, text: &str, x: f32, y: f32, px: f32, color: [u8; 3]) {
    let Some(f) = font() else { return };
    let ascent = f.horizontal_line_metrics(px).map(|m| m.ascent).unwrap_or(px * 0.8);
    let mut pen = x;
    for c in text.chars() {
        let (m, bmp) = f.rasterize(c, px);
        let gx = pen + m.xmin as f32;
        let gy = y + ascent - m.height as f32 - m.ymin as f32;
        for row in 0..m.height {
            for col in 0..m.width {
                let a = bmp[row * m.width + col] as f32 / 255.0;
                if a <= 0.0 {
                    continue;
                }
                let (xx, yy) = (gx as i64 + col as i64, gy as i64 + row as i64);
                if xx < 0 || yy < 0 || xx as usize >= img.w || yy as usize >= img.h {
                    continue;
                }
                let i = 3 * (yy as usize * img.w + xx as usize);
                for k in 0..3 {
                    img.d[i + k] = (img.d[i + k] as f32 * (1.0 - a) + color[k] as f32 * a).round() as u8;
                }
            }
        }
        pen += m.advance_width;
    }
}

/// Dải nền đen ở góc trên-trái với dòng thông số màu vàng (như bản Python).
pub fn banner(img: &mut Rgb, text: &str) {
    let px = (img.w.min(img.h) as f32 / 32.0).clamp(14.0, 64.0);
    let tw = text_width(text, px);
    let (bw, bh) = ((tw + px).min(img.w as f32) as usize, (px * 1.45) as usize);
    for y in 0..bh.min(img.h) {
        for x in 0..bw {
            let i = 3 * (y * img.w + x);
            img.d[i..i + 3].copy_from_slice(&[0, 0, 0]);
        }
    }
    draw_text(img, text, px * 0.5, px * 0.2, px, [255, 235, 59]);
}
