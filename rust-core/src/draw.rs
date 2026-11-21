//! Vẽ ảnh kết quả (lưới chord, giao điểm, biên hạt, vùng ngoài vùng đo) lên ảnh RGB – dùng cho lưu ảnh và báo cáo.
use crate::{dark_mask, Analysis, Chord, Kind, Method, Rgb};

#[derive(Clone, Copy, Debug)]
pub struct Colors {
    pub chord: [u8; 3],
    pub point: [u8; 3],
    pub manual: [u8; 3],
    pub boundary: [u8; 3],
}

impl Default for Colors {
    fn default() -> Self {
        Colors { chord: [0, 229, 255], point: [255, 23, 68], manual: [255, 0, 255], boundary: [0, 200, 83] }
    }
}

fn put(img: &mut Rgb, x: i64, y: i64, c: [u8; 3]) {
    if x >= 0 && y >= 0 && (x as usize) < img.w && (y as usize) < img.h {
        let i = 3 * (y as usize * img.w + x as usize);
        img.d[i..i + 3].copy_from_slice(&c);
    }
}

pub fn disc(img: &mut Rgb, cx: f64, cy: f64, r: f64, c: [u8; 3]) {
    let ri = r.ceil() as i64;
    for dy in -ri..=ri {
        for dx in -ri..=ri {
            if (dx * dx + dy * dy) as f64 <= r * r {
                put(img, cx.round() as i64 + dx, cy.round() as i64 + dy, c);
            }
        }
    }
}

pub fn ring(img: &mut Rgb, cx: f64, cy: f64, r: f64, th: f64, c: [u8; 3]) {
    let ro = (r + th / 2.0).ceil() as i64;
    for dy in -ro..=ro {
        for dx in -ro..=ro {
            let d = ((dx * dx + dy * dy) as f64).sqrt();
            if (d - r).abs() <= th / 2.0 {
                put(img, cx.round() as i64 + dx, cy.round() as i64 + dy, c);
            }
        }
    }
}

pub fn line(img: &mut Rgb, a: (f64, f64), b: (f64, f64), th: f64, c: [u8; 3]) {
    let len = (b.0 - a.0).hypot(b.1 - a.1).max(1.0);
    let n = (len * 2.0) as usize + 1;
    for i in 0..=n {
        let t = i as f64 / n as f64;
        disc(img, a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1), th / 2.0, c);
    }
}

pub fn render(base: &Rgb, a: Option<&Analysis>, m: Method, col: &Colors, show_skel: bool, show_pattern: bool) -> Rgb {
    let mut out = Rgb { w: base.w, h: base.h, d: base.d.clone() };
    let Some(a) = a else { return out };
    let dm = dark_mask(a);
    for (i, &dk) in dm.d.iter().enumerate() {
        if dk {
            for c in 0..3 {
                out.d[3 * i + c] = (out.d[3 * i + c] as f32 * 0.35) as u8;
            }
        }
    }
    if show_skel {
        for (i, &s) in a.skeleton.d.iter().enumerate() {
            if s {
                out.d[3 * i..3 * i + 3].copy_from_slice(&col.boundary);
            }
        }
    }
    let Some(r) = a.method(m) else { return out };
    if !show_pattern {
        return out;
    }
    let th = ((base.w.min(base.h) as f64) / 500.0).round().max(1.0);
    for ch in &r.chords {
        match ch {
            Chord::Circle { cx, cy, r: rr, .. } => ring(&mut out, *cx, *cy, *rr, th, col.chord),
            Chord::Line { a: p, b: q, .. } => line(&mut out, *p, *q, th, col.chord),
        }
    }
    let rad = ((base.w.min(base.h) as f64) / 200.0).round().max(3.0);
    for e in r.events.iter().filter(|e| !e.removed) {
        let c = if e.manual { col.manual } else { col.point };
        match e.kind {
            Kind::Triple => {
                ring(&mut out, e.x, e.y, rad + 2.0, th, c);
                disc(&mut out, e.x, e.y, (rad - 1.0).max(1.0), c);
            }
            Kind::Endpoint => ring(&mut out, e.x, e.y, rad + 1.0, th, c),
            Kind::Cross => disc(&mut out, e.x, e.y, rad, c),
        }
    }
    out
}
