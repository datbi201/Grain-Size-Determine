//! Các phép xử lý ảnh cơ bản, bám sát hành vi OpenCV / NumPy / scikit-image mà bản Python dùng.
use rayon::prelude::*;

/// Ảnh một kênh kích thước w×h, lưu theo hàng.
#[derive(Clone)]
pub struct Img<T> {
    pub w: usize,
    pub h: usize,
    pub d: Vec<T>,
}

impl<T: Copy + Default + Send + Sync> Img<T> {
    pub fn new(w: usize, h: usize) -> Self {
        Img { w, h, d: vec![T::default(); w * h] }
    }
    pub fn filled(w: usize, h: usize, v: T) -> Self {
        Img { w, h, d: vec![v; w * h] }
    }
    #[inline]
    pub fn at(&self, x: usize, y: usize) -> T {
        self.d[y * self.w + x]
    }
    pub fn crop(&self, x0: usize, y0: usize, x1: usize, y1: usize) -> Img<T> {
        let (w, h) = (x1 - x0, y1 - y0);
        let mut d = Vec::with_capacity(w * h);
        for y in y0..y1 {
            d.extend_from_slice(&self.d[y * self.w + x0..y * self.w + x1]);
        }
        Img { w, h, d }
    }
}

/// Làm tròn kiểu "banker" như np.rint / round() của Python.
#[inline]
pub fn rint(v: f64) -> f64 {
    let r = v.round();
    if (v - v.trunc()).abs() == 0.5 {
        2.0 * (v / 2.0).round()
    } else {
        r
    }
}

#[inline]
fn sat_u8(v: f32) -> u8 {
    // saturate_cast<uchar> của OpenCV: làm tròn gần nhất (cvRound = nửa lên chẵn) rồi kẹp
    let r = rint(v as f64);
    r.clamp(0.0, 255.0) as u8
}

/// Chỉ số phản xạ BORDER_REFLECT_101 (…3 2 1 | 0 1 2 … n-1 | n-2 …).
#[inline]
pub fn reflect101(mut i: isize, n: isize) -> usize {
    if n == 1 {
        return 0;
    }
    loop {
        if i < 0 {
            i = -i;
        } else if i >= n {
            i = 2 * n - 2 - i;
        } else {
            return i as usize;
        }
    }
}

// --------------------------------------------------------------------------- //
//  Chuyển màu
// --------------------------------------------------------------------------- //
/// Kênh L* 8-bit, khớp từng điểm với cv2.cvtColor(BGR2LAB)[:,:,0] (thuật toán số nguyên RGB2Lab_b của OpenCV:
/// bảng gamma sRGB 255·2³, bảng căn bậc ba 2¹⁵, hệ số Y × 2¹², L = (296·fY − 1336934 + 2¹⁴) >> 15).
pub fn lab_l(rgb: &[u8], w: usize, h: usize) -> Img<u8> {
    const GS: u32 = 3;
    const LS: u32 = 12;
    const LS2: u32 = 15;
    let gam: Vec<i64> = (0..256).map(|i| {
        let x = i as f64 / 255.0;
        let g = if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) };
        rint(255.0 * (1u32 << GS) as f64 * g).min(65535.0) as i64
    }).collect();
    let n = 256 * 3 / 2 * (1usize << GS);
    let cbrt: Vec<i64> = (0..n).map(|i| {
        let x = i as f64 / (255.0 * (1u32 << GS) as f64);
        let f = if x < 0.008856 { x * 7.787 + 16.0 / 116.0 } else { x.cbrt() };
        rint((1u32 << LS2) as f64 * f) as i64
    }).collect();
    let c = |v: f64| rint(v * (1u32 << LS) as f64) as i64;
    let (c3, c4, c5) = (c(0.212671), c(0.715160), c(0.072169));
    let lscale: i64 = (116 * 255 + 50) / 100;
    let lshift: i64 = -((16 * 255 * (1i64 << LS2) + 50) / 100);
    let desc = |x: i64, s: u32| (x + (1i64 << (s - 1))) >> s;
    let mut out = Img::<u8>::new(w, h);
    out.d.par_iter_mut().enumerate().for_each(|(i, o)| {
        let (r, g, b) = (gam[rgb[3 * i] as usize], gam[rgb[3 * i + 1] as usize], gam[rgb[3 * i + 2] as usize]);
        let fy = cbrt[desc(r * c3 + g * c4 + b * c5, LS) as usize];
        *o = desc(lscale * fy + lshift, LS2).clamp(0, 255) as u8;
    });
    out
}

/// H (0..180), S, V 8-bit như cv2 BGR2HSV.
pub fn hsv(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let (rf, gf, bf) = (r as f32, g as f32, b as f32);
    let mx = rf.max(gf).max(bf);
    let mn = rf.min(gf).min(bf);
    let v = mx;
    let s = if mx > 0.0 { 255.0 * (mx - mn) / mx } else { 0.0 };
    let mut hdeg = if mx == mn {
        0.0
    } else if mx == rf {
        60.0 * (gf - bf) / (mx - mn)
    } else if mx == gf {
        120.0 + 60.0 * (bf - rf) / (mx - mn)
    } else {
        240.0 + 60.0 * (rf - gf) / (mx - mn)
    };
    if hdeg < 0.0 {
        hdeg += 360.0;
    }
    (sat_u8(hdeg / 2.0).min(179), sat_u8(s), sat_u8(v))
}

// --------------------------------------------------------------------------- //
//  Lọc Gauss tách được
// --------------------------------------------------------------------------- //
fn gauss_kernel(sigma: f64, radius: usize) -> Vec<f32> {
    let mut k: Vec<f64> = (0..=2 * radius).map(|i| {
        let x = i as f64 - radius as f64;
        (-x * x / (2.0 * sigma * sigma)).exp()
    }).collect();
    let s: f64 = k.iter().sum();
    k.iter_mut().for_each(|v| *v /= s);
    k.into_iter().map(|v| v as f32).collect()
}

/// Lọc Gauss trên ảnh số thực, viền BORDER_REFLECT_101.
/// `trunc` = số sigma (OpenCV: 4 cho ảnh số thực, 3 cho 8-bit).
pub fn gauss_f32(src: &Img<f32>, sigma: f64, trunc: f64) -> Img<f32> {
    let radius = ((sigma * trunc * 2.0 + 1.0).round() as usize | 1) / 2;
    let k = gauss_kernel(sigma, radius);
    let (w, h) = (src.w, src.h);
    let r = radius as isize;
    // ngang
    let mut tmp = Img::<f32>::new(w, h);
    tmp.d.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        let s = &src.d[y * w..(y + 1) * w];
        let idx: Vec<usize> = (-(r)..(w as isize + r)).map(|i| reflect101(i, w as isize)).collect();
        for x in 0..w {
            let mut acc = 0.0f32;
            for (j, kv) in k.iter().enumerate() {
                acc += kv * s[idx[x + j]];
            }
            row[x] = acc;
        }
    });
    // dọc
    let mut out = Img::<f32>::new(w, h);
    let idy: Vec<usize> = (-(r)..(h as isize + r)).map(|i| reflect101(i, h as isize)).collect();
    out.d.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for (j, kv) in k.iter().enumerate() {
            let sr = &tmp.d[idy[y + j] * w..idy[y + j] * w + w];
            for x in 0..w {
                row[x] += kv * sr[x];
            }
        }
    });
    out
}

/// Lọc Gauss ảnh 8-bit, khớp từng điểm với cv2.GaussianBlur(CV_8U): nhân 8 bit phần lẻ (phân bổ sai số),
/// lọc ngang chính xác (8 bit lẻ), lọc dọc 16 bit lẻ, làm tròn (v + 2¹⁵) >> 16.
pub fn gauss_u8(src: &Img<u8>, sigma: f64) -> Img<u8> {
    let radius = ((sigma * 3.0 * 2.0 + 1.0).round() as usize | 1) / 2;
    let kd: Vec<f64> = {
        let v: Vec<f64> = (0..=2 * radius).map(|i| {
            let x = i as f64 - radius as f64;
            (-x * x / (2.0 * sigma * sigma)).exp()
        }).collect();
        let s: f64 = v.iter().sum();
        v.into_iter().map(|x| x / s).collect()
    };
    let n = kd.len();
    let m = 256i64;
    let mut k = vec![0i64; n];
    let (mut err, mut sum) = (0.0f64, 0i64);
    for i in 0..n / 2 {
        let adj = kd[i] * m as f64 + err;
        let v0 = rint(adj) as i64;
        err = adj - v0 as f64;
        k[i] = v0;
        k[n - 1 - i] = v0;
        sum += v0;
    }
    k[n / 2] = m - 2 * sum;
    let (w, h) = (src.w, src.h);
    let r = radius as isize;
    let mut tmp = vec![0i64; w * h];
    tmp.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        let s = &src.d[y * w..(y + 1) * w];
        for x in 0..w {
            let mut acc = 0i64;
            for (j, kv) in k.iter().enumerate() {
                acc += kv * s[reflect101(x as isize + j as isize - r, w as isize)] as i64;
            }
            row[x] = acc;
        }
    });
    let mut out = Img::<u8>::new(w, h);
    out.d.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for x in 0..w {
            let mut acc = 0i64;
            for (j, kv) in k.iter().enumerate() {
                acc += kv * tmp[reflect101(y as isize + j as isize - r, h as isize) * w + x];
            }
            row[x] = ((acc + (1 << 15)) >> 16).clamp(0, 255) as u8;
        }
    });
    out
}

// --------------------------------------------------------------------------- //
//  CLAHE (cv2.createCLAHE(clipLimit, tileGridSize=(8,8)))
// --------------------------------------------------------------------------- //
pub fn clahe(src: &Img<u8>, clip: f64, tiles: usize) -> Img<u8> {
    let (w, h) = (src.w, src.h);
    // OpenCV: nếu một trong hai chiều không chia hết số ô thì đệm (REFLECT_101) CẢ HAI chiều thêm
    // tiles − (n mod tiles) — chiều đã chia hết vẫn bị đệm thêm đúng `tiles` hàng/cột.
    let (pw, ph) = if w % tiles == 0 && h % tiles == 0 { (w, h) } else { (w + tiles - w % tiles, h + tiles - h % tiles) };
    let (tw, th) = (pw / tiles, ph / tiles);
    let area = tw * th;
    let clip_lim = ((clip * area as f64 / 256.0) as i64).max(1);
    let mut luts = vec![[0u8; 256]; tiles * tiles];
    luts.par_iter_mut().enumerate().for_each(|(t, lut)| {
        let (tx, ty) = (t % tiles, t / tiles);
        let mut hist = [0i64; 256];
        for y in ty * th..(ty + 1) * th {
            let yy = reflect101(y as isize, h as isize);
            for x in tx * tw..(tx + 1) * tw {
                let xx = reflect101(x as isize, w as isize);
                hist[src.d[yy * w + xx] as usize] += 1;
            }
        }
        let mut excess = 0i64;
        for v in hist.iter_mut() {
            if *v > clip_lim {
                excess += *v - clip_lim;
                *v = clip_lim;
            }
        }
        let batch = excess / 256;
        let mut residual = excess - batch * 256;
        for v in hist.iter_mut() {
            *v += batch;
        }
        if residual > 0 {
            let step = (256 / residual).max(1) as usize;
            let mut i = 0;
            while i < 256 && residual > 0 {
                hist[i] += 1;
                residual -= 1;
                i += step;
            }
        }
        let scale = 255.0f32 / area as f32;
        let mut sum = 0i64;
        for i in 0..256 {
            sum += hist[i];
            lut[i] = sat_u8(sum as f32 * scale);
        }
    });
    let (inv_tw, inv_th) = (1.0f32 / tw as f32, 1.0f32 / th as f32);
    let mut out = Img::<u8>::new(w, h);
    out.d.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        let tyf = y as f32 * inv_th - 0.5;
        let ty1f = tyf.floor();
        let ya = tyf - ty1f;
        let ty1 = (ty1f as isize).max(0) as usize;
        let ty2 = ((ty1f as isize) + 1).min(tiles as isize - 1) as usize;
        for x in 0..w {
            let txf = x as f32 * inv_tw - 0.5;
            let tx1f = txf.floor();
            let xa = txf - tx1f;
            let tx1 = (tx1f as isize).max(0) as usize;
            let tx2 = ((tx1f as isize) + 1).min(tiles as isize - 1) as usize;
            let v = src.d[y * w + x] as usize;
            let l = |ty: usize, tx: usize| luts[ty * tiles + tx][v] as f32;
            let res = (l(ty1, tx1) * (1.0 - xa) + l(ty1, tx2) * xa) * (1.0 - ya)
                + (l(ty2, tx1) * (1.0 - xa) + l(ty2, tx2) * xa) * ya;
            row[x] = sat_u8(res);
        }
    });
    out
}

// --------------------------------------------------------------------------- //
//  Hình thái học (bỏ qua điểm ngoài ảnh, như giá trị viền mặc định của OpenCV)
// --------------------------------------------------------------------------- //
/// Phần tử cấu trúc elip của cv2.getStructuringElement(MORPH_ELLIPSE, (k,k)) -> danh sách (dx, dy).
pub fn ellipse(k: usize) -> Vec<(isize, isize)> {
    let r = (k / 2) as isize;
    let c = r;
    let inv_r2 = if r > 0 { 1.0 / (r * r) as f64 } else { 0.0 };
    let mut pts = vec![];
    for i in 0..k as isize {
        let dy = i - r;
        if dy.abs() <= r {
            let dx = rint(c as f64 * (((r * r - dy * dy) as f64) * inv_r2).sqrt()) as isize;
            let j1 = (c - dx).max(0);
            let j2 = (c + dx + 1).min(k as isize);
            for j in j1..j2 {
                pts.push((j - c, dy));
            }
        }
    }
    pts
}

pub fn rect(kw: usize, kh: usize) -> Vec<(isize, isize)> {
    let (cx, cy) = ((kw / 2) as isize, (kh / 2) as isize);
    let mut v = vec![];
    for y in 0..kh as isize {
        for x in 0..kw as isize {
            v.push((x - cx, y - cy));
        }
    }
    v
}

/// Cực trị trượt 1 chiều cửa sổ [x+a, x+b] (bỏ qua phần ngoài ảnh) – thuật toán van Herk / Gil–Werman, O(1)/điểm.
fn run_extreme(row: &[u8], a: isize, b: isize, dilate: bool, out: &mut [u8]) {
    let w = row.len();
    let l = (b - a + 1) as usize;
    let neutral: u8 = if dilate { 0 } else { 255 };
    let ext = |p: u8, q: u8| if dilate { p.max(q) } else { p.min(q) };
    let n = w + l - 1;
    let pad: Vec<u8> = (0..n).map(|i| {
        let x = i as isize + a;
        if x >= 0 && (x as usize) < w { row[x as usize] } else { neutral }
    }).collect();
    let mut g = vec![neutral; n];
    let mut hh = vec![neutral; n];
    for i in 0..n {
        g[i] = if i % l == 0 { pad[i] } else { ext(g[i - 1], pad[i]) };
    }
    for i in (0..n).rev() {
        hh[i] = if (i + 1) % l == 0 || i == n - 1 { pad[i] } else { ext(hh[i + 1], pad[i]) };
    }
    for x in 0..w {
        out[x] = ext(hh[x], g[x + l - 1]);
    }
}

/// Giãn/co với phần tử cấu trúc có mỗi hàng là một đoạn liền (hình chữ nhật, elip): tách theo hàng.
fn morph(src: &Img<u8>, se: &[(isize, isize)], dilate: bool) -> Img<u8> {
    use std::collections::BTreeMap;
    let mut rows: BTreeMap<isize, (isize, isize)> = BTreeMap::new();
    for &(dx, dy) in se {
        let e = rows.entry(dy).or_insert((dx, dx));
        e.0 = e.0.min(dx);
        e.1 = e.1.max(dx);
    }
    // kiểm tra mỗi hàng liền mạch; nếu không thì dùng cách vét cạn
    if rows.iter().any(|(dy, (a, b))| (b - a + 1) as usize != se.iter().filter(|p| p.1 == *dy).count()) {
        return morph_ref(src, se, dilate);
    }
    let (w, h) = (src.w, src.h);
    let mut runs: Vec<(isize, isize)> = rows.values().cloned().collect();
    runs.sort();
    runs.dedup();
    let hor: Vec<Vec<u8>> = runs.iter().map(|&(a, b)| {
        let mut buf = vec![0u8; w * h];
        buf.par_chunks_mut(w).enumerate().for_each(|(y, out)| run_extreme(&src.d[y * w..(y + 1) * w], a, b, dilate, out));
        buf
    }).collect();
    let plan: Vec<(isize, usize)> = rows.iter().map(|(dy, r)| (*dy, runs.iter().position(|q| q == r).unwrap())).collect();
    let mut out = Img::<u8>::new(w, h);
    out.d.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        let neutral: u8 = if dilate { 0 } else { 255 };
        row.iter_mut().for_each(|v| *v = neutral);
        for &(dy, ri) in &plan {
            let yy = y as isize + dy;
            if yy < 0 || yy >= h as isize {
                continue;
            }
            let src_row = &hor[ri][yy as usize * w..yy as usize * w + w];
            for x in 0..w {
                row[x] = if dilate { row[x].max(src_row[x]) } else { row[x].min(src_row[x]) };
            }
        }
    });
    out
}

/// Bản vét cạn (đối chiếu trong kiểm thử).
pub fn morph_ref(src: &Img<u8>, se: &[(isize, isize)], dilate: bool) -> Img<u8> {
    let (w, h) = (src.w as isize, src.h as isize);
    let mut out = Img::<u8>::new(src.w, src.h);
    out.d.par_chunks_mut(src.w).enumerate().for_each(|(y, row)| {
        let y = y as isize;
        for x in 0..w {
            let mut v: u8 = if dilate { 0 } else { 255 };
            for &(dx, dy) in se {
                let (xx, yy) = (x + dx, y + dy);
                if xx < 0 || yy < 0 || xx >= w || yy >= h {
                    continue;
                }
                let s = src.d[(yy * w + xx) as usize];
                if dilate {
                    if s > v {
                        v = s;
                    }
                } else if s < v {
                    v = s;
                }
            }
            row[x as usize] = v;
        }
    });
    out
}

pub fn dilate(src: &Img<u8>, se: &[(isize, isize)]) -> Img<u8> {
    morph(src, se, true)
}
pub fn erode(src: &Img<u8>, se: &[(isize, isize)]) -> Img<u8> {
    morph(src, se, false)
}
pub fn close(src: &Img<u8>, se: &[(isize, isize)]) -> Img<u8> {
    erode(&dilate(src, se), se)
}
pub fn open(src: &Img<u8>, se: &[(isize, isize)]) -> Img<u8> {
    dilate(&erode(src, se), se)
}

pub fn dilate_bool(src: &Img<bool>, se: &[(isize, isize)]) -> Img<bool> {
    let u = Img { w: src.w, h: src.h, d: src.d.iter().map(|&b| b as u8).collect() };
    let d = dilate(&u, se);
    Img { w: src.w, h: src.h, d: d.d.iter().map(|&v| v > 0).collect() }
}

// --------------------------------------------------------------------------- //
//  Thành phần liên thông 8 hướng
// --------------------------------------------------------------------------- //
/// Trả về (nhãn mỗi điểm, diện tích mỗi nhãn, hộp bao [x0,y0,x1,y1] mỗi nhãn). Nhãn 0 = nền.
pub fn components(mask: &Img<bool>) -> (Vec<u32>, Vec<usize>, Vec<[usize; 4]>) {
    let (w, h) = (mask.w, mask.h);
    let mut lab = vec![0u32; w * h];
    let mut areas = vec![0usize];
    let mut boxes = vec![[0usize; 4]];
    let mut stack = Vec::new();
    let mut next = 1u32;
    for start in 0..w * h {
        if !mask.d[start] || lab[start] != 0 {
            continue;
        }
        lab[start] = next;
        stack.push(start);
        let (mut a, mut bx) = (0usize, [usize::MAX, usize::MAX, 0, 0]);
        while let Some(p) = stack.pop() {
            a += 1;
            let (x, y) = (p % w, p / w);
            bx[0] = bx[0].min(x);
            bx[1] = bx[1].min(y);
            bx[2] = bx[2].max(x);
            bx[3] = bx[3].max(y);
            for dy in -1isize..=1 {
                for dx in -1isize..=1 {
                    let (xx, yy) = (x as isize + dx, y as isize + dy);
                    if xx < 0 || yy < 0 || xx >= w as isize || yy >= h as isize {
                        continue;
                    }
                    let q = yy as usize * w + xx as usize;
                    if mask.d[q] && lab[q] == 0 {
                        lab[q] = next;
                        stack.push(q);
                    }
                }
            }
        }
        areas.push(a);
        boxes.push(bx);
        next += 1;
    }
    (lab, areas, boxes)
}

// --------------------------------------------------------------------------- //
//  Làm mảnh Zhang–Suen theo đúng cài đặt skimage.morphology.skeletonize (_fast_skeletonize):
//  bảng tra 256 mục, bit NW=1 N=2 NE=4 E=8 SE=16 S=32 SW=64 W=128; lượt 1 xoá nhãn 1 và 3, lượt 2 xoá 2 và 3.
// --------------------------------------------------------------------------- //
const SKEL_LUT: [u8; 256] = [
    0, 0, 0, 1, 0, 0, 1, 3, 0, 0, 3, 1, 1, 0, 1, 3, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 2, 0, 3, 0, 3, 3,
    0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 3, 0, 2, 2,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    2, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 3, 0, 2, 0,
    0, 0, 3, 1, 0, 0, 1, 3, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
    3, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    2, 3, 1, 3, 0, 0, 1, 3, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    2, 3, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 3, 3, 0, 1, 0, 0, 0, 0, 2, 2, 0, 0, 2, 0, 0, 0];

pub fn skeletonize(mask: &Img<bool>) -> Img<bool> {
    let (w, h) = (mask.w, mask.h);
    let pw = w + 2;
    let mut img: Vec<u8> = vec![0; pw * (h + 2)];
    for y in 0..h {
        for x in 0..w {
            img[(y + 1) * pw + x + 1] = mask.d[y * w + x] as u8;
        }
    }
    let p = pw as isize;
    let offs: [isize; 8] = [-p - 1, -p, -p + 1, 1, p + 1, p, p - 1, -1];
    loop {
        let mut changed = false;
        for pass in 0..2 {
            let first = pass == 0;
            let del: Vec<usize> = (1..=h).into_par_iter().flat_map_iter(|y| {
                let img = &img;
                (1..=w).filter_map(move |x| {
                    let q = y * pw + x;
                    if img[q] == 0 {
                        return None;
                    }
                    let mut code = 0usize;
                    for (i, o) in offs.iter().enumerate() {
                        code |= (img[(q as isize + o) as usize] as usize) << i;
                    }
                    let n = SKEL_LUT[code];
                    if n == 3 || (n == 1 && first) || (n == 2 && !first) { Some(q) } else { None }
                })
            }).collect();
            if !del.is_empty() {
                changed = true;
                for q in del {
                    img[q] = 0;
                }
            }
        }
        if !changed {
            break;
        }
    }
    let mut out = Img::<bool>::new(w, h);
    for y in 0..h {
        for x in 0..w {
            out.d[y * w + x] = img[(y + 1) * pw + x + 1] != 0;
        }
    }
    out
}

/// np.percentile (nội suy tuyến tính).
pub fn percentile(vals: &mut Vec<f32>, q: f64) -> f64 {
    if vals.is_empty() {
        return 0.0;
    }
    vals.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
    let pos = q / 100.0 * (vals.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = (lo + 1).min(vals.len() - 1);
    let f = pos - lo as f64;
    vals[lo] as f64 * (1.0 - f) + vals[hi] as f64 * f
}
