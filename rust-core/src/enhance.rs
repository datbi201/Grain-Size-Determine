//! Xử lý ảnh nhẹ (độ sáng, tương phản, gamma, CLAHE) và tự chọn thông số – tương ứng enhance/auto_enhance
//! của grain_core.py. CLAHE áp lên độ sáng L* rồi nhân tỉ lệ vào 3 kênh màu (xấp xỉ đổi Lab ↔ RGB).
use crate::imgproc::*;
use crate::Rgb;
use rayon::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub struct Enhance {
    pub brightness: f64, // −100..100
    pub contrast: f64,   // 0,3..3
    pub gamma: f64,      // 0,3..3
    pub clahe: bool,
}

impl Default for Enhance {
    fn default() -> Self {
        Enhance { brightness: 0.0, contrast: 1.0, gamma: 1.0, clahe: false }
    }
}

impl Enhance {
    pub fn is_identity(&self) -> bool {
        self.brightness == 0.0 && self.contrast == 1.0 && self.gamma == 1.0 && !self.clahe
    }
    pub fn describe(&self) -> String {
        if self.is_identity() {
            return "không".into();
        }
        let mut p = vec![];
        if self.brightness != 0.0 {
            p.push(format!("độ sáng {:+.0}", self.brightness));
        }
        if self.contrast != 1.0 {
            p.push(format!("tương phản ×{:.2}", self.contrast));
        }
        if self.gamma != 1.0 {
            p.push(format!("gamma {:.2}", self.gamma));
        }
        if self.clahe {
            p.push("CLAHE".into());
        }
        p.join(", ")
    }
}

pub fn apply(img: &Rgb, e: &Enhance) -> Rgb {
    if e.is_identity() {
        return Rgb { w: img.w, h: img.h, d: img.d.clone() };
    }
    let mut d = img.d.clone();
    if e.clahe {
        let l = lab_l(&img.d, img.w, img.h);
        let l2 = clahe(&l, 2.0, 8);
        d.par_chunks_mut(3).enumerate().for_each(|(i, px)| {
            let f = (l2.d[i] as f32 + 1.0) / (l.d[i] as f32 + 1.0);
            for c in px.iter_mut() {
                *c = (*c as f32 * f).round().clamp(0.0, 255.0) as u8;
            }
        });
    }
    let mut lut = [0u8; 256];
    for (i, v) in lut.iter_mut().enumerate() {
        let mut x = (i as f64 - 128.0) * e.contrast + 128.0 + e.brightness * 1.28;
        x = x.clamp(0.0, 255.0) / 255.0;
        if e.gamma != 1.0 {
            x = x.powf(1.0 / e.gamma);
        }
        *v = (x * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
    }
    d.par_iter_mut().for_each(|c| *c = lut[*c as usize]);
    Rgb { w: img.w, h: img.h, d }
}

/// Tự chọn thông số: kéo p1–p99 về khoảng 20–235, trung vị về giữa thang xám,
/// bật CLAHE khi ảnh tối góc hoặc tương phản cục bộ thấp.
pub fn auto(img: &Rgb, fov: Option<(f64, f64, f64)>) -> Enhance {
    let (w, h) = (img.w, img.h);
    let l = lab_l(&img.d, w, h);
    let inside = |x: usize, y: usize| match fov {
        Some((cx, cy, r)) => (x as f64 - cx).powi(2) + (y as f64 - cy).powi(2) <= (0.9 * r).powi(2),
        None => true,
    };
    let mut v: Vec<f32> = vec![];
    for y in 0..h {
        for x in 0..w {
            if inside(x, y) {
                v.push(l.d[y * w + x] as f32);
            }
        }
    }
    let mut v2 = v.clone();
    let p1 = percentile(&mut v2, 1.0);
    let p99 = percentile(&mut v2, 99.0);
    let med = percentile(&mut v2, 50.0);
    let c = (215.0 / (p99 - p1).max(1.0)).clamp(0.5, 3.0);
    let center = (p1 + p99) / 2.0;
    let b = (-(center - 128.0) * c / 1.28).clamp(-100.0, 100.0);
    let m2 = (((med - 128.0) * c + 128.0 + b * 1.28) / 255.0).clamp(0.05, 0.95);
    let g = (m2.ln() / 0.5f64.ln()).clamp(0.6, 1.8);
    let lf = Img { w, h, d: l.d.iter().map(|&x| x as f32).collect() };
    let bg = gauss_f32(&lf, w.max(h) as f64 / 15.0, 4.0);
    let (mut s, mut s2, mut n) = (0.0f64, 0.0f64, 0.0f64);
    let (mut ls, mut ls2) = (0.0f64, 0.0f64);
    for y in 0..h {
        for x in 0..w {
            if !inside(x, y) {
                continue;
            }
            let bv = bg.d[y * w + x] as f64;
            s += bv;
            s2 += bv * bv;
            n += 1.0;
            let at = |xx: isize, yy: isize| l.d[reflect101(yy, h as isize) * w + reflect101(xx, w as isize)] as f64;
            let (xi, yi) = (x as isize, y as isize);
            let lap = at(xi - 1, yi) + at(xi + 1, yi) + at(xi, yi - 1) + at(xi, yi + 1) - 4.0 * at(xi, yi);
            ls += lap;
            ls2 += lap * lap;
        }
    }
    let mean = s / n;
    let vign = ((s2 / n - mean * mean).max(0.0)).sqrt() / mean.max(1.0);
    let lap_sd = ((ls2 / n - (ls / n).powi(2)).max(0.0)).sqrt();
    Enhance { brightness: b.round(), contrast: (c * 100.0).round() / 100.0, gamma: (g * 100.0).round() / 100.0,
              clahe: vign > 0.08 || lap_sd < 6.0 }
}
