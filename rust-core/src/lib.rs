//! gsd_core – bản thử Rust của lõi tính cỡ hạt `src/grain_core.py` (ASTM E112-25 / E1382).
//!
//! Port 1:1: trường nhìn tròn, thanh scale đỏ, tách biên (L*, san phẳng nền, CLAHE, black-hat,
//! ngưỡng phân vị, làm mảnh), loại vùng mờ, lưới Abrams / Heyn / Chord hỗn hợp, đếm giao điểm
//! (1 / 1,5 / 0,5), ℓ = L/P, G = −6,643856·log10(ℓ[mm]) − 3,288, thống kê E112 §15.
pub mod draw;
pub mod enhance;
pub mod imgproc;

/// In thời gian từng bước khi đặt biến môi trường GSD_DEBUG.
pub fn tick(label: &str, t: &mut std::time::Instant) {
    if std::env::var("GSD_DEBUG").is_ok() {
        eprintln!("TIME {:<18} {:7.3} s", label, t.elapsed().as_secs_f64());
    }
    *t = std::time::Instant::now();
}

use imgproc::*;
use std::f64::consts::PI;

pub const STEP: f64 = 0.5;
pub const GAP: usize = 2;
pub const JUNCTION_R: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Method {
    Abrams,
    Heyn,
    Mixed,
}

impl Method {
    pub fn name(&self) -> &'static str {
        match self {
            Method::Abrams => "abrams",
            Method::Heyn => "heyn",
            Method::Mixed => "mixed",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Cross,
    Triple,
    Endpoint,
}

impl Kind {
    pub fn weight(&self) -> f64 {
        match self {
            Kind::Cross => 1.0,
            Kind::Triple => 1.5,
            Kind::Endpoint => 0.5,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Chord {
    Circle { id: String, cx: f64, cy: f64, r: f64 },
    Line { id: String, a: (f64, f64), b: (f64, f64) },
}

impl Chord {
    pub fn id(&self) -> &str {
        match self {
            Chord::Circle { id, .. } | Chord::Line { id, .. } => id,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Event {
    pub chord: String,
    pub x: f64,
    pub y: f64,
    pub kind: Kind,
    /// true = người đo thêm thủ công; false = phát hiện tự động
    pub manual: bool,
    /// điểm tự động đã bị người đo loại (giữ lại để khôi phục / truy vết)
    pub removed: bool,
}

#[derive(Clone, Debug)]
pub struct MethodResult {
    pub method: Method,
    pub chords: Vec<Chord>,
    pub l_px: Vec<f64>,
    pub events: Vec<Event>,
    pub edited: bool,
}

#[derive(Clone)]
pub struct Analysis {
    pub methods: Vec<MethodResult>,
    pub skeleton: Img<bool>,
    pub valid: Img<bool>,
    pub excluded: f64,
    /// hộp loại trừ thanh scale (x0, y0, x1, y1) – không làm tối khi hiển thị
    pub excl: Option<[usize; 4]>,
    pub params: Params,
    /// mô tả xử lý ảnh dùng khi tách biên ("" = ảnh gốc)
    pub enhance_desc: String,
}

impl Analysis {
    pub fn method(&self, m: Method) -> Option<&MethodResult> {
        self.methods.iter().find(|r| r.method == m)
    }
    pub fn method_mut(&mut self, m: Method) -> Option<&mut MethodResult> {
        self.methods.iter_mut().find(|r| r.method == m)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Params {
    pub sensitivity: f64,
    pub focus: bool,
    pub n_lines: usize,
    pub mixed_hv: bool,
}

impl Default for Params {
    fn default() -> Self {
        Params { sensitivity: 85.0, focus: true, n_lines: 5, mixed_hv: false }
    }
}

/// Ảnh màu RGB 8-bit.
#[derive(Clone)]
pub struct Rgb {
    pub w: usize,
    pub h: usize,
    pub d: Vec<u8>,
}

/// Đọc ảnh. JPEG giải mã bằng mozjpeg (họ libjpeg-turbo, IDCT islow, nội suy "fancy") để điểm ảnh
/// trùng với cv2.imread; định dạng khác dùng crate `image`.
pub fn load(path: &str) -> Result<Rgb, String> {
    let lower = path.to_lowercase();
    if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        let data = std::fs::read(path).map_err(|e| e.to_string())?;
        let r = std::panic::catch_unwind(|| -> Result<Rgb, String> {
            let d = mozjpeg::Decompress::new_mem(&data).map_err(|e| e.to_string())?;
            let mut s = d.rgb().map_err(|e| e.to_string())?;
            let (w, h) = (s.width(), s.height());
            let px: Vec<[u8; 3]> = s.read_scanlines().map_err(|e| e.to_string())?;
            s.finish().map_err(|e| e.to_string())?;
            Ok(Rgb { w, h, d: px.into_iter().flatten().collect() })
        });
        if let Ok(Ok(img)) = r {
            return Ok(img);
        }
    }
    let im = image::open(path).map_err(|e| e.to_string())?.to_rgb8();
    Ok(Rgb { w: im.width() as usize, h: im.height() as usize, d: im.into_raw() })
}

// --------------------------------------------------------------------------- //
//  Trường nhìn tròn
// --------------------------------------------------------------------------- //
fn min_enclosing_circle(pts: &[(f64, f64)]) -> (f64, f64, f64) {
    // Welzl lặp (đã trộn thứ tự bằng bước nhảy nguyên tố để tránh trường hợp xấu)
    let n = pts.len();
    let order: Vec<usize> = (0..n).map(|i| (i * 7919) % n).collect();
    let p: Vec<(f64, f64)> = order.iter().map(|&i| pts[i]).collect();
    let inside = |c: (f64, f64, f64), q: (f64, f64)| ((q.0 - c.0).powi(2) + (q.1 - c.1).powi(2)).sqrt() <= c.2 + 1e-7;
    let circ2 = |a: (f64, f64), b: (f64, f64)| (((a.0 + b.0) / 2.0), ((a.1 + b.1) / 2.0),
                                                 ((a.0 - b.0).hypot(a.1 - b.1)) / 2.0);
    let circ3 = |a: (f64, f64), b: (f64, f64), c: (f64, f64)| {
        let d = 2.0 * (a.0 * (b.1 - c.1) + b.0 * (c.1 - a.1) + c.0 * (a.1 - b.1));
        if d.abs() < 1e-12 {
            return circ2(a, b);
        }
        let ux = ((a.0 * a.0 + a.1 * a.1) * (b.1 - c.1) + (b.0 * b.0 + b.1 * b.1) * (c.1 - a.1)
            + (c.0 * c.0 + c.1 * c.1) * (a.1 - b.1)) / d;
        let uy = ((a.0 * a.0 + a.1 * a.1) * (c.0 - b.0) + (b.0 * b.0 + b.1 * b.1) * (a.0 - c.0)
            + (c.0 * c.0 + c.1 * c.1) * (b.0 - a.0)) / d;
        (ux, uy, (a.0 - ux).hypot(a.1 - uy))
    };
    let mut c = (p[0].0, p[0].1, 0.0);
    for i in 1..n {
        if inside(c, p[i]) {
            continue;
        }
        c = (p[i].0, p[i].1, 0.0);
        for j in 0..i {
            if inside(c, p[j]) {
                continue;
            }
            c = circ2(p[i], p[j]);
            for k in 0..j {
                if !inside(c, p[k]) {
                    c = circ3(p[i], p[j], p[k]);
                }
            }
        }
    }
    c
}

/// (cx, cy, r) nếu ảnh có trường nhìn tròn trên nền đen.
pub fn detect_fov(img: &Rgb) -> Option<(f64, f64, f64)> {
    let (w, h) = (img.w, img.h);
    let m = Img { w, h, d: (0..w * h).map(|i| (img.d[3 * i].max(img.d[3 * i + 1]).max(img.d[3 * i + 2]) > 45) as u8).collect() };
    let m = open(&m, &rect(9, 9));
    let mb = Img { w, h, d: m.d.iter().map(|&v| v > 0).collect() };
    let (lab, areas, _) = components(&mb);
    if areas.len() < 2 {
        return None;
    }
    let best = (1..areas.len()).max_by_key(|&i| areas[i])?;
    // điểm biên của thành phần lớn nhất
    let mut pts = vec![];
    for y in 0..h {
        for x in 0..w {
            if lab[y * w + x] as usize != best {
                continue;
            }
            let edge = x == 0 || y == 0 || x == w - 1 || y == h - 1
                || lab[y * w + x - 1] as usize != best || lab[y * w + x + 1] as usize != best
                || lab[(y - 1) * w + x] as usize != best || lab[(y + 1) * w + x] as usize != best;
            if edge {
                pts.push((x as f64, y as f64));
            }
        }
    }
    let (cx, cy, r) = min_enclosing_circle(&pts);
    let circ = areas[best] as f64 / (PI * r * r + 1e-9);
    let cover = m.d.iter().filter(|&&v| v > 0).count() as f64 / (w * h) as f64;
    if circ > 0.85 && cover < 0.85 && r > 100.0 {
        Some((cx, cy, r))
    } else {
        None
    }
}

/// Thanh scale đỏ ở 45 % dưới ảnh -> (độ dài px, hộp x, y, w, h).
pub fn find_red_bar(img: &Rgb) -> Option<(f64, [usize; 4])> {
    let (w, h) = (img.w, img.h);
    let y0 = (h as f64 * 0.55) as usize;
    let hh = h - y0;
    let mut m = Img::<u8>::new(w, hh);
    use rayon::prelude::*;
    m.d.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for x in 0..w {
            let i = 3 * ((y + y0) * w + x);
            let (r, g, b) = (img.d[i], img.d[i + 1], img.d[i + 2]);
            if r.max(g).max(b) < 140 {
                continue; // V < 140 -> không phải đỏ, bỏ qua sớm
            }
            let (hu, s, v) = hsv(r, g, b);
            let red = (hu <= 10 && s >= 130 && v >= 140) || (hu >= 165 && s >= 130 && v >= 140);
            row[x] = red as u8 * 255;
        }
    });
    let m = close(&m, &rect(5, 1));
    let mb = Img { w, h: hh, d: m.d.iter().map(|&v| v > 0).collect() };
    let (lab, _areas, boxes) = components(&mb);
    let mut best: Option<(usize, usize, [usize; 4])> = None;
    for i in 1..boxes.len() {
        let b = boxes[i];
        let (cw, ch) = (b[2] - b[0] + 1, b[3] - b[1] + 1);
        if cw >= 3 * ch && cw as f64 > (30.0f64).max(0.03 * w as f64) {
            if best.map_or(true, |bb| cw > bb.0) {
                let mut row_len = 0;
                for y in b[1]..=b[3] {
                    let c = (b[0]..=b[2]).filter(|&x| lab[y * w + x] as usize == i).count();
                    row_len = row_len.max(c);
                }
                best = Some((cw, row_len, [b[0], b[1] + y0, cw, ch]));
            }
        }
    }
    let (cw, row_len, bx) = best?;
    Some((if row_len as f64 >= 0.8 * cw as f64 { row_len } else { cw } as f64, bx))
}

pub fn exclusion_box(bar: [usize; 4], w: usize, h: usize) -> [usize; 4] {
    let (bx, by, bw, bh) = (bar[0] as f64, bar[1] as f64, bar[2] as f64, bar[3] as f64);
    let px = (0.25 * bw + 10.0) as i64 as f64;
    [(bx - px).max(0.0) as usize, (by - 0.08 * h as f64).max(0.0) as i64 as usize,
     ((bx + bw + px) as usize).min(w), ((by + bh + 0.04 * h as f64) as i64 as usize).min(h)]
}

// --------------------------------------------------------------------------- //
//  Tách biên hạt
// --------------------------------------------------------------------------- //
pub fn extract_boundaries(l8: &Img<u8>, valid: &Img<bool>, sensitivity: f64) -> Img<bool> {
    let (w, h) = (l8.w, l8.h);
    let nvalid = valid.d.iter().filter(|&&v| v).count();
    let mean_in = if nvalid > 0 {
        l8.d.iter().zip(&valid.d).filter(|(_, &v)| v).map(|(&l, _)| l as f64).sum::<f64>() / nvalid as f64
    } else {
        128.0
    };
    let lf = Img { w, h, d: l8.d.iter().zip(&valid.d).map(|(&l, &v)| if v { l as f32 } else { mean_in as f32 }).collect() };
    let mut tt = std::time::Instant::now();
    let bg = gauss_f32(&lf, w.max(h) as f64 / 12.0, 4.0);
    tick("bg gauss", &mut tt);
    // np.clip(L/max(bg,1)*128, 0, 255).astype(uint8) -> cắt phần thập phân
    let flat = Img { w, h, d: lf.d.iter().zip(&bg.d).map(|(&l, &b)| (l / b.max(1.0) * 128.0).clamp(0.0, 255.0) as u8).collect() };
    let eq = clahe(&flat, 2.0, 8);
    tick("clahe", &mut tt);
    let sm = gauss_u8(&eq, 1.0);
    tick("gauss u8", &mut tt);
    let mut k = (rint(w.min(h) as f64 / 110.0) as i64).clamp(5, 13) as usize;
    k += (k + 1) % 2;
    let se = ellipse(k);
    let cl = close(&sm, &se);
    tick("blackhat", &mut tt);
    let bh = Img { w, h, d: cl.d.iter().zip(&sm.d).map(|(&c, &s)| c.saturating_sub(s)).collect() };
    let mut vals: Vec<f32> = bh.d.iter().zip(&valid.d).filter(|(_, &v)| v).map(|(&b, _)| b as f32).collect();
    if vals.is_empty() {
        vals = bh.d.iter().map(|&b| b as f32).collect();
    }
    let thr = percentile(&mut vals, sensitivity).max(4.0);
    if std::env::var("GSD_DEBUG").is_ok() {
        let mean_u8 = |v: &Img<u8>| v.d.iter().map(|&x| x as f64).sum::<f64>() / v.d.len() as f64;
        let mean_f = |v: &Img<f32>| v.d.iter().map(|&x| x as f64).sum::<f64>() / v.d.len() as f64;
        eprintln!("DBG size {}x{} mean_in {:.4} L {:.4} bg {:.4} flat {:.4} clahe {:.4} smooth {:.4} close {:.4} bh {:.4} k {} thr {}",
            w, h, mean_in, mean_u8(l8), mean_f(&bg), mean_u8(&flat), mean_u8(&eq), mean_u8(&sm), mean_u8(&cl), mean_u8(&bh), k, thr);
        if let Ok(dir) = std::env::var("GSD_DUMP") {
            for (name, im) in [("flat", &flat), ("clahe", &eq), ("smooth", &sm), ("bh", &bh)] {
                std::fs::write(format!("{dir}/{name}.raw"), &im.d).ok();
            }
        }
    }
    let bw = Img { w, h, d: bh.d.iter().zip(&valid.d).map(|(&b, &v)| (v && b as f64 > thr) as u8).collect() };
    let bw = close(&bw, &rect(3, 3));
    let bwb = Img { w, h, d: bw.d.iter().map(|&v| v > 0).collect() };
    let (lab, areas, _) = components(&bwb);
    let min_area = (20.0f64).max(2.5 * k as f64) as usize;
    tick("thr+close+cc", &mut tt);
    let keep = Img { w, h, d: lab.iter().map(|&l| l > 0 && areas[l as usize] >= min_area).collect() };
    let sk = skeletonize(&keep);
    tick("skeletonize", &mut tt);
    if std::env::var("GSD_DEBUG").is_ok() {
        eprintln!("DBG bw {} keep {} skel {}", bwb.d.iter().filter(|&&v| v).count(), keep.d.iter().filter(|&&v| v).count(),
            sk.d.iter().filter(|&&v| v).count());
    }
    Img { w, h, d: sk.d.iter().zip(&valid.d).map(|(&s, &v)| s && v).collect() }
}

pub fn sharp_region_mask(skel: &Img<bool>, valid: &Img<bool>) -> Img<bool> {
    let (w, h) = (skel.w, skel.h);
    let f = Img { w, h, d: skel.d.iter().map(|&s| s as u8 as f32).collect() };
    let dens = gauss_f32(&f, w.max(h) as f64 / 25.0, 4.0);
    let mut vals: Vec<f32> = dens.d.iter().zip(&valid.d).filter(|(_, &v)| v).map(|(&d, _)| d).collect();
    let r = percentile(&mut vals, 75.0);
    if r <= 0.0 {
        return valid.clone();
    }
    let good = Img { w, h, d: dens.d.iter().map(|&d| (d as f64 > 0.30 * r) as u8).collect() };
    let kk = 3usize.max((w.max(h) / 60) | 1);
    let se = ellipse(kk);
    let good = close(&open(&good, &se), &se);
    Img { w, h, d: good.d.iter().zip(&valid.d).map(|(&g, &v)| v && g > 0).collect() }
}

pub fn junctions(sk: &Img<bool>) -> Img<bool> {
    let (w, h) = (sk.w as isize, sk.h as isize);
    let get = |x: isize, y: isize| x >= 0 && y >= 0 && x < w && y < h && sk.d[(y * w + x) as usize];
    let mut out = Img::<bool>::new(sk.w, sk.h);
    for y in 0..h {
        for x in 0..w {
            if !sk.d[(y * w + x) as usize] {
                continue;
            }
            let nb = [get(x, y - 1), get(x + 1, y - 1), get(x + 1, y), get(x + 1, y + 1), get(x, y + 1), get(x - 1, y + 1),
                get(x - 1, y), get(x - 1, y - 1)];
            let t = (0..8).filter(|&i| !nb[i] && nb[(i + 1) % 8]).count();
            out.d[(y * w + x) as usize] = t >= 3;
        }
    }
    out
}

// --------------------------------------------------------------------------- //
//  Lưới thử
// --------------------------------------------------------------------------- //
pub struct Geo {
    pub cx: f64,
    pub cy: f64,
    pub r: f64,
    pub round: bool,
    pub w: f64,
    pub h: f64,
}

pub fn field_geometry(w: usize, h: usize, fov: Option<(f64, f64, f64)>) -> Geo {
    match fov {
        Some((cx, cy, r)) => Geo { cx, cy, r: 0.92 * r, round: true, w: w as f64, h: h as f64 },
        None => Geo { cx: (w as f64 - 1.0) / 2.0, cy: (h as f64 - 1.0) / 2.0, r: w.min(h) as f64 / 2.0, round: false,
                      w: w as f64, h: h as f64 },
    }
}

pub fn build_pattern(m: Method, g: &Geo, n_lines: usize, mixed_hv: bool) -> Vec<Chord> {
    let mut v = vec![];
    match m {
        Method::Abrams => {
            let rmax = 0.95 * g.r;
            for (i, f) in [1.0, 2.0 / 3.0, 1.0 / 3.0].iter().enumerate() {
                v.push(Chord::Circle { id: format!("C{}", i + 1), cx: g.cx, cy: g.cy, r: rmax * f });
            }
        }
        Method::Heyn => {
            let fr: Vec<f64> = (0..n_lines).map(|i| 0.08 + 0.84 * (i as f64 + 0.5) / n_lines as f64).collect();
            for (i, f) in fr.iter().enumerate() {
                let y = f * (g.h - 1.0);
                v.push(Chord::Line { id: format!("H{}", i + 1), a: (0.0, y), b: (g.w - 1.0, y) });
            }
            for (i, f) in fr.iter().enumerate() {
                let x = f * (g.w - 1.0);
                v.push(Chord::Line { id: format!("V{}", i + 1), a: (x, 0.0), b: (x, g.h - 1.0) });
            }
        }
        Method::Mixed => {
            let reach = if mixed_hv { 78.1 } else { 75.0 };
            let unit = if g.round {
                0.97 * g.r / reach
            } else {
                let u = 0.95 * g.w.min(g.h) / 2.0 / (if mixed_hv { 60.0 } else { 53.1 });
                u.min(0.97 * g.w.hypot(g.h) / 2.0 / reach)
            };
            let r1 = 250.0 / (2.0 * PI);
            for (i, f) in [1.0, 2.0 / 3.0, 1.0 / 3.0].iter().enumerate() {
                v.push(Chord::Circle { id: format!("C{}", i + 1), cx: g.cx, cy: g.cy, r: r1 * f * unit });
            }
            let d = 150.0 / (2.0 * 2f64.sqrt());
            let mut segs = vec![((-d, -d), (d, d)), ((-d, d), (d, -d))];
            if mixed_hv {
                segs.push(((-50.0, 60.0), (50.0, 60.0)));
                segs.push(((-60.0, -50.0), (-60.0, 50.0)));
            }
            for (i, (p, q)) in segs.iter().enumerate() {
                let id = if i < 2 { format!("D{}", i + 1) } else { format!("L{}", i - 1) };
                v.push(Chord::Line { id, a: (g.cx + p.0 * unit, g.cy + p.1 * unit), b: (g.cx + q.0 * unit, g.cy + q.1 * unit) });
            }
        }
    }
    v
}

fn linspace(a: f64, b: f64, n: usize, endpoint: bool) -> Vec<f64> {
    let div = if endpoint { (n - 1).max(1) as f64 } else { n as f64 };
    (0..n).map(|i| a + (b - a) * i as f64 / div).collect()
}

pub fn chord_samples(ch: &Chord) -> (Vec<f64>, Vec<f64>, bool) {
    match ch {
        Chord::Circle { cx, cy, r, .. } => {
            let n = ((2.0 * PI * r / STEP) as usize).max(16);
            let th = linspace(0.0, 2.0 * PI, n, false);
            (th.iter().map(|t| cx + r * t.cos()).collect(), th.iter().map(|t| cy + r * t.sin()).collect(), true)
        }
        Chord::Line { a, b, .. } => {
            let n = (((b.0 - a.0).hypot(b.1 - a.1) / STEP) as usize + 1).max(2);
            (linspace(a.0, b.0, n, true), linspace(a.1, b.1, n, true), false)
        }
    }
}

/// Tổng cửa sổ ±g (np.convolve(x, ones(2g+1), "same")).
fn window_sum(x: &[u8], g: usize) -> Vec<usize> {
    let n = x.len();
    let mut pre = vec![0usize; n + 1];
    for i in 0..n {
        pre[i + 1] = pre[i] + x[i] as usize;
    }
    (0..n).map(|i| pre[(i + g + 1).min(n)] - pre[i.saturating_sub(g)]).collect()
}

pub fn detect_on_chord(ch: &Chord, bmask: &Img<bool>, valid: &Img<bool>, jmask: &Img<bool>) -> (f64, Vec<Event>) {
    let (w, h) = (bmask.w as i64, bmask.h as i64);
    let (xs, ys, closed) = chord_samples(ch);
    let n = xs.len();
    let mut ok = vec![false; n];
    let mut b = vec![false; n];
    let mut jj = vec![false; n];
    for i in 0..n {
        let (xi, yi) = (rint(xs[i]) as i64, rint(ys[i]) as i64);
        let inside = xi >= 0 && xi < w && yi >= 0 && yi < h;
        let (xc, yc) = (xi.clamp(0, w - 1) as usize, yi.clamp(0, h - 1) as usize);
        let p = yc * bmask.w + xc;
        ok[i] = inside && valid.d[p];
        b[i] = ok[i] && bmask.d[p];
        jj[i] = ok[i] && jmask.d[p];
    }
    let l = ok.iter().filter(|&&v| v).count() as f64 * STEP;
    if !b.iter().any(|&v| v) {
        return (l, vec![]);
    }
    if GAP > 0 {
        let g = GAP;
        let k = 2 * g + 1;
        let bi: Vec<u8> = b.iter().map(|&v| v as u8).collect();
        let ero: Vec<bool> = if closed {
            let mut ext = bi[n - g..].to_vec();
            ext.extend_from_slice(&bi);
            ext.extend_from_slice(&bi[..g]);
            let dil: Vec<u8> = window_sum(&ext, g)[g..g + n].iter().map(|&s| (s > 0) as u8).collect();
            let mut ext2 = dil[n - g..].to_vec();
            ext2.extend_from_slice(&dil);
            ext2.extend_from_slice(&dil[..g]);
            window_sum(&ext2, g)[g..g + n].iter().map(|&s| s == k).collect()
        } else {
            let dil: Vec<u8> = window_sum(&bi, g).iter().map(|&s| (s > 0) as u8).collect();
            let mut e: Vec<bool> = window_sum(&dil, g).iter().map(|&s| s == k).collect();
            for i in (0..g.min(n)).chain(n.saturating_sub(g)..n) {
                e[i] = (dil[i] > 0 && b[i]) || e[i];
            }
            e
        };
        for i in 0..n {
            b[i] = (b[i] || ero[i]) && ok[i];
        }
    }
    if closed && b.iter().all(|&v| v) {
        return (l, vec![]);
    }
    let start = if closed { b.iter().position(|&v| !v).unwrap() } else { 0 };
    let order: Vec<usize> = (start..n).chain(0..start).collect();
    let bo: Vec<bool> = order.iter().map(|&i| b[i]).collect();
    let oko: Vec<bool> = order.iter().map(|&i| ok[i]).collect();
    let jo: Vec<bool> = order.iter().map(|&i| jj[i]).collect();
    let mut ev = vec![];
    let mut i = 0;
    while i < n {
        if !bo[i] {
            i += 1;
            continue;
        }
        let mut j = i;
        while j + 1 < n && bo[j + 1] {
            j += 1;
        }
        let prv_ok = if i > 0 { oko[i - 1] } else if closed { oko[n - 1] } else { false };
        let nxt_ok = if j + 1 < n { oko[j + 1] } else if closed { oko[0] } else { false };
        let mid = order[(i + j) / 2];
        let kind = if !prv_ok || !nxt_ok {
            Kind::Endpoint
        } else if jo[i..=j].iter().any(|&v| v) {
            Kind::Triple
        } else {
            Kind::Cross
        };
        ev.push(Event { chord: ch.id().to_string(), x: xs[mid], y: ys[mid], kind, manual: false, removed: false });
        i = j + 1;
    }
    (l, ev)
}

// --------------------------------------------------------------------------- //
//  Phân tích một ảnh
// --------------------------------------------------------------------------- //
/// Phân tích với ảnh đã xử lý (nếu có) dùng để tách biên.
pub fn analyze_enh(img: &Rgb, fov: Option<(f64, f64, f64)>, bar: Option<[usize; 4]>, methods: &[Method], p: &Params,
                   enh: Option<&enhance::Enhance>) -> Analysis {
    match enh {
        Some(e) if !e.is_identity() => {
            let im2 = enhance::apply(img, e);
            let mut a = analyze(&im2, fov, bar, methods, p);
            a.enhance_desc = e.describe();
            a
        }
        _ => analyze(img, fov, bar, methods, p),
    }
}

pub fn analyze(img: &Rgb, fov: Option<(f64, f64, f64)>, bar: Option<[usize; 4]>, methods: &[Method], p: &Params) -> Analysis {
    let (w, h) = (img.w, img.h);
    let g = field_geometry(w, h, fov);
    let (x0, y0, x1, y1, vcrop) = match fov {
        Some((cx, cy, _)) => {
            let r = g.r;
            let x0 = ((cx - r) as i64).max(0) as usize;
            let y0 = ((cy - r) as i64).max(0) as usize;
            let x1 = (((cx + r) as i64 + 1) as usize).min(w);
            let y1 = (((cy + r) as i64 + 1) as usize).min(h);
            let mut v = Img::<bool>::new(x1 - x0, y1 - y0);
            for y in y0..y1 {
                for x in x0..x1 {
                    v.d[(y - y0) * (x1 - x0) + (x - x0)] = (x as f64 - cx).powi(2) + (y as f64 - cy).powi(2) <= r * r;
                }
            }
            (x0, y0, x1, y1, v)
        }
        None => (0, 0, w, h, Img::filled(w, h, true)),
    };
    let mut vcrop = vcrop;
    if let Some(bb) = bar {
        let e = exclusion_box(bb, w, h);
        let (cw, ch) = (vcrop.w, vcrop.h);
        let ys = e[1].saturating_sub(y0).min(ch);
        let ye = e[3].saturating_sub(y0).min(ch);
        let xs = e[0].saturating_sub(x0).min(cw);
        let xe = e[2].saturating_sub(x0).min(cw);
        for y in ys..ye {
            for x in xs..xe {
                vcrop.d[y * cw + x] = false;
            }
        }
    }
    let mut tt = std::time::Instant::now();
    let l_full = lab_l(&img.d, w, h);
    tick("lab", &mut tt);
    let l8 = l_full.crop(x0, y0, x1, y1);
    let mut sk = extract_boundaries(&l8, &vcrop, p.sensitivity);
    tick("extract total", &mut tt);
    let mut excluded = 0.0;
    if p.focus {
        let v2 = sharp_region_mask(&sk, &vcrop);
        let n1 = vcrop.d.iter().filter(|&&v| v).count().max(1);
        let n2 = v2.d.iter().filter(|&&v| v).count();
        excluded = 1.0 - n2 as f64 / n1 as f64;
        vcrop = v2;
        for i in 0..sk.d.len() {
            sk.d[i] = sk.d[i] && vcrop.d[i];
        }
    }
    let mut skeleton = Img::<bool>::new(w, h);
    let mut valid = Img::<bool>::new(w, h);
    for y in y0..y1 {
        for x in x0..x1 {
            let i = (y - y0) * (x1 - x0) + (x - x0);
            skeleton.d[y * w + x] = sk.d[i];
            valid.d[y * w + x] = vcrop.d[i];
        }
    }
    tick("sharp mask", &mut tt);
    let bmask = dilate_bool(&skeleton, &rect(3, 3));
    let jmask = dilate_bool(&junctions(&skeleton), &ellipse(JUNCTION_R));
    let mut out = vec![];
    for &m in methods {
        let chords = build_pattern(m, &g, p.n_lines, p.mixed_hv);
        let mut lpx = vec![];
        let mut evs = vec![];
        for ch in &chords {
            let (l, e) = detect_on_chord(ch, &bmask, &valid, &jmask);
            lpx.push(l);
            evs.extend(e);
        }
        out.push(MethodResult { method: m, chords, l_px: lpx, events: evs, edited: false });
    }
    tick("masks+count", &mut tt);
    Analysis { methods: out, skeleton, valid, excluded, excl: bar.map(|b| exclusion_box(b, w, h)), params: p.clone(),
               enhance_desc: String::new() }
}

// --------------------------------------------------------------------------- //
//  Kết quả
// --------------------------------------------------------------------------- //
pub fn g_from_l_um(l_um: f64) -> f64 {
    -6.643856 * (l_um / 1000.0).log10() - 3.288
}

#[derive(Clone, Debug)]
pub struct Summary {
    pub n_manual: usize,
    pub n_removed: usize,
    pub l_mm: f64,
    pub l_h: Option<f64>,
    pub l_v: Option<f64>,
    pub p: f64,
    pub l_um: f64,
    pub lbar_um: Option<f64>,
    pub g: Option<f64>,
    pub n_cross: usize,
    pub n_triple: usize,
    pub n_end: usize,
}

pub fn summarize(r: &MethodResult, um_per_px: f64) -> Summary {
    let act: Vec<&Event> = r.events.iter().filter(|e| !e.removed).collect();
    let p: f64 = act.iter().map(|e| e.kind.weight()).sum();
    let l_um = r.l_px.iter().sum::<f64>() * um_per_px;
    let lbar = if p > 0.0 && l_um > 0.0 { Some(l_um / p) } else { None };
    let dir = |pref: char| -> Option<f64> {
        let ids: Vec<usize> = r.chords.iter().enumerate().filter(|(_, c)| c.id().starts_with(pref)).map(|(i, _)| i).collect();
        if ids.is_empty() {
            return None;
        }
        let pp: f64 = act.iter().filter(|e| ids.iter().any(|&i| r.chords[i].id() == e.chord)).map(|e| e.kind.weight()).sum();
        let l: f64 = ids.iter().map(|&i| r.l_px[i]).sum::<f64>() * um_per_px;
        if pp > 0.0 { Some(l / pp) } else { None }
    };
    Summary {
        p, l_um, l_mm: l_um / 1000.0, lbar_um: lbar, g: lbar.map(g_from_l_um),
        n_cross: act.iter().filter(|e| e.kind == Kind::Cross).count(),
        n_triple: act.iter().filter(|e| e.kind == Kind::Triple).count(),
        n_end: act.iter().filter(|e| e.kind == Kind::Endpoint).count(),
        n_manual: act.iter().filter(|e| e.manual).count(),
        n_removed: r.events.iter().filter(|e| e.removed).count(),
        l_h: if r.method == Method::Heyn { dir('H') } else { None },
        l_v: if r.method == Method::Heyn { dir('V') } else { None },
    }
}

pub fn round_g(g: f64) -> f64 {
    let v = g * 10.0;
    (if v >= 0.0 { (v + 0.5).floor() } else { -(-v + 0.5).floor() }) / 10.0
}

// --------------------------------------------------------------------------- //
//  Sửa giao điểm thủ công
// --------------------------------------------------------------------------- //
pub fn project_point(ch: &Chord, x: f64, y: f64) -> (f64, f64, f64) {
    match ch {
        Chord::Circle { cx, cy, r, .. } => {
            let t = (y - cy).atan2(x - cx);
            let (px, py) = (cx + r * t.cos(), cy + r * t.sin());
            (px, py, (x - px).hypot(y - py))
        }
        Chord::Line { a, b, .. } => {
            let (vx, vy) = (b.0 - a.0, b.1 - a.1);
            let u = (((x - a.0) * vx + (y - a.1) * vy) / (vx * vx + vy * vy)).clamp(0.0, 1.0);
            let (px, py) = (a.0 + u * vx, a.1 + u * vy);
            (px, py, (x - px).hypot(y - py))
        }
    }
}

/// Thêm giao điểm thủ công (hít vào chord gần nhất). Err = thông báo cho người dùng.
pub fn add_manual(r: &mut MethodResult, x: f64, y: f64, kind: Kind, valid: &Img<bool>, tol: f64) -> Result<String, String> {
    let mut best: Option<(usize, f64, f64, f64)> = None;
    for (i, ch) in r.chords.iter().enumerate() {
        let (px, py, d) = match (kind, ch) {
            (Kind::Endpoint, Chord::Line { a, b, .. }) => {
                let da = (a.0 - x).hypot(a.1 - y);
                let db = (b.0 - x).hypot(b.1 - y);
                if da <= db { (a.0, a.1, da) } else { (b.0, b.1, db) }
            }
            (Kind::Endpoint, _) => continue,
            _ => project_point(ch, x, y),
        };
        if best.map_or(true, |b| d < b.3) {
            best = Some((i, px, py, d));
        }
    }
    let (i, px, py, d) = best.ok_or("Không có đường chord phù hợp.")?;
    if d > tol {
        return Err("Điểm click cách xa mọi đường chord.".into());
    }
    let (xi, yi) = (px.round() as i64, py.round() as i64);
    if xi < 0 || yi < 0 || xi >= valid.w as i64 || yi >= valid.h as i64 || !valid.d[yi as usize * valid.w + xi as usize] {
        return Err("Điểm nằm ngoài vùng đo (vùng tối).".into());
    }
    let id = r.chords[i].id().to_string();
    if r.events.iter().any(|e| !e.removed && e.chord == id && (e.x - px).hypot(e.y - py) < 1.5) {
        return Err("Đã có giao điểm tại vị trí này.".into());
    }
    r.events.push(Event { chord: id.clone(), x: px, y: py, kind, manual: true, removed: false });
    r.edited = true;
    Ok(id)
}

/// Loại giao điểm gần nhất: điểm thủ công bị xoá, điểm tự động được đánh dấu đã loại.
pub fn remove_near(r: &mut MethodResult, x: f64, y: f64, tol: f64) -> Option<(String, bool)> {
    let mut best: Option<(usize, f64)> = None;
    for (i, e) in r.events.iter().enumerate() {
        if e.removed {
            continue;
        }
        let d = (e.x - x).hypot(e.y - y);
        if d <= tol && best.map_or(true, |b| d < b.1) {
            best = Some((i, d));
        }
    }
    let (i, _) = best?;
    let info = (r.events[i].chord.clone(), r.events[i].manual);
    if r.events[i].manual {
        r.events.remove(i);
    } else {
        r.events[i].removed = true;
    }
    r.edited = true;
    Some(info)
}

pub fn restore_near(r: &mut MethodResult, x: f64, y: f64, tol: f64) -> bool {
    let mut best: Option<(usize, f64)> = None;
    for (i, e) in r.events.iter().enumerate() {
        let d = (e.x - x).hypot(e.y - y);
        if e.removed && d <= tol && best.map_or(true, |b| d < b.1) {
            best = Some((i, d));
        }
    }
    if let Some((i, _)) = best {
        r.events[i].removed = false;
        r.edited = true;
        true
    } else {
        false
    }
}

/// Vùng làm tối khi hiển thị = ngoài vùng đo, trừ hộp thanh scale.
pub fn dark_mask(a: &Analysis) -> Img<bool> {
    let mut m = Img { w: a.valid.w, h: a.valid.h, d: a.valid.d.iter().map(|&v| !v).collect() };
    if let Some(e) = a.excl {
        for y in e[1]..e[3].min(m.h) {
            for x in e[0]..e[2].min(m.w) {
                m.d[y * m.w + x] = false;
            }
        }
    }
    m
}

/// t(0,975; n−1) theo E112 Bảng 7.
pub fn t975(n: usize) -> f64 {
    const T: [f64; 30] = [12.706, 4.303, 3.182, 2.776, 2.571, 2.447, 2.365, 2.306, 2.262, 2.228, 2.201, 2.179, 2.160,
        2.145, 2.131, 2.120, 2.110, 2.101, 2.093, 2.086, 2.080, 2.074, 2.069, 2.064, 2.060, 2.056, 2.052, 2.048,
        2.045, 2.042];
    if n >= 2 && n <= 31 { T[n - 2] } else if n > 31 { 1.96 } else { f64::NAN }
}

/// Thống kê nhiều trường kèm đánh giá theo khuyến nghị E112 / E1382.
#[derive(Clone, Debug)]
pub struct Stats {
    pub n: usize,
    pub l_mean: f64,
    pub sd: Option<f64>,
    pub t: Option<f64>,
    pub ci: Option<f64>,
    pub ra: Option<f64>,
    pub g: f64,
    pub g_round: f64,
    pub g_lo: Option<f64>,
    pub g_hi: Option<f64>,
    pub checks: Vec<(String, bool, String)>,
}

pub fn stats_with_checks(l: &[f64], p: &[f64], m: Method) -> Stats {
    let n = l.len();
    let mean = l.iter().sum::<f64>() / n as f64;
    let g = g_from_l_um(mean);
    let mut s = Stats { n, l_mean: mean, sd: None, t: None, ci: None, ra: None, g, g_round: round_g(g), g_lo: None,
                        g_hi: None, checks: vec![] };
    if n >= 2 {
        let sd = (l.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1) as f64).sqrt();
        let t = t975(n);
        let ci = t * sd / (n as f64).sqrt();
        s.sd = Some(sd);
        s.t = Some(t);
        s.ci = Some(ci);
        s.ra = Some(100.0 * ci / mean);
        s.g_lo = Some(g_from_l_um(mean + ci));
        s.g_hi = if mean - ci > 0.0 { Some(g_from_l_um(mean - ci)) } else { None };
    }
    s.checks.push(("Số trường đo n ≥ 5 (E112 §13.2, §14.3.2)".into(), n >= 5, format!("n = {n}")));
    match s.ra {
        Some(ra) => s.checks.push(("Độ chính xác tương đối %RA ≤ 10 % (E112 §15.6)".into(), ra <= 10.0, format!("%RA = {ra:.1} %"))),
        None => s.checks.push(("Độ chính xác tương đối %RA ≤ 10 % (E112 §15.6)".into(), false, "cần ≥ 2 trường".into())),
    }
    if !p.is_empty() {
        let tot: f64 = p.iter().sum();
        s.checks.push(("Tổng số giao điểm ≥ 500 (E1382 §12.4.3; E112 §14.3.2.1: 400–500)".into(), tot >= 500.0,
                       format!("ΣP = {tot}")));
        let (mn, mx) = p.iter().fold((f64::MAX, f64::MIN), |a, &v| (a.0.min(v), a.1.max(v)));
        if m == Method::Heyn {
            s.checks.push(("Mỗi trường ≥ 50 giao điểm (E112 §13.1)".into(), mn >= 50.0, format!("P nhỏ nhất = {mn}")));
        } else {
            s.checks.push(("Mỗi trường 40–150 giao điểm (E112 §14.3.2.1; E1382 §12.4.2)".into(), mn >= 40.0 && mx <= 150.0,
                           format!("P: {mn} – {mx}")));
        }
    }
    s
}

/// (ℓ tb, s, 95 %CI, %RA, G) cho n trường (E112 §15).
pub fn field_stats(l: &[f64]) -> (f64, f64, f64, f64, f64) {
    let n = l.len();
    let mean = l.iter().sum::<f64>() / n as f64;
    if n < 2 {
        return (mean, f64::NAN, f64::NAN, f64::NAN, g_from_l_um(mean));
    }
    let sd = (l.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1) as f64).sqrt();
    let ci = t975(n) * sd / (n as f64).sqrt();
    (mean, sd, ci, 100.0 * ci / mean, g_from_l_um(mean))
}
