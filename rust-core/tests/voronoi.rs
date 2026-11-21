//! Độ đúng bộ đếm trên ảnh Voronoi tổng hợp: so ℓ đo được với ℓ tính từ số lần đổi hạt thật dọc chord.
//! Chạy: cargo test --release
use gsd_core::imgproc::*;
use gsd_core::*;

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn voronoi(seed: u64, n: usize, w: usize, h: usize) -> (Rgb, Vec<(f64, f64)>) {
    let mut r = Lcg(seed);
    let seeds: Vec<(f64, f64)> = (0..n).map(|_| (r.next() * w as f64, r.next() * h as f64)).collect();
    let mut lab = vec![0usize; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut best = (f64::MAX, 0);
            for (i, s) in seeds.iter().enumerate() {
                let d = (x as f64 - s.0).powi(2) + (y as f64 - s.1).powi(2);
                if d < best.0 {
                    best = (d, i);
                }
            }
            lab[y * w + x] = best.1;
        }
    }
    let mut b = Img::<bool>::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let l = lab[y * w + x];
            b.d[y * w + x] = (y + 1 < h && lab[(y + 1) * w + x] != l) || (x + 1 < w && lab[y * w + x + 1] != l);
        }
    }
    // cv2.dilate 2x2 (neo ở (1,1)): điểm (x,y) sáng nếu (x..x-1, y..y-1) có biên
    let mut d = vec![0u8; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let mut on = false;
            for dy in 0..2 {
                for dx in 0..2 {
                    let (xx, yy) = (x as isize - dx, y as isize - dy);
                    if xx >= 0 && yy >= 0 && b.d[yy as usize * w + xx as usize] {
                        on = true;
                    }
                }
            }
            let v = if on { 60 } else { 200 };
            d[3 * (y * w + x)..3 * (y * w + x) + 3].copy_from_slice(&[v, v, v]);
        }
    }
    (Rgb { w, h, d }, seeds)
}

fn truth(ch: &Chord, seeds: &[(f64, f64)]) -> (usize, f64) {
    let (xs, ys, closed) = chord_samples(ch);
    let lab: Vec<usize> = xs.iter().zip(&ys).map(|(x, y)| {
        seeds.iter().enumerate().map(|(i, s)| ((x - s.0).powi(2) + (y - s.1).powi(2), i))
            .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap()).unwrap().1
    }).collect();
    let n = lab.len();
    let mut c = 0;
    for i in 0..n {
        if i == 0 && !closed {
            continue;
        }
        if lab[i] != lab[(i + n - 1) % n] {
            c += 1;
        }
    }
    (c, n as f64 * STEP)
}

#[test]
fn do_dung_tren_anh_voronoi() {
    let mut worst: f64 = 0.0;
    for (seed, n) in [(11u64, 400usize), (23, 150), (37, 800)] {
        let (img, seeds) = voronoi(seed, n, 1200, 900);
        let a = analyze(&img, None, None, &[Method::Abrams, Method::Heyn, Method::Mixed], &Params::default());
        for r in &a.methods {
            let (mut tp, mut tl) = (0usize, 0.0);
            for ch in &r.chords {
                let (p, l) = truth(ch, &seeds);
                tp += p;
                tl += l;
            }
            let s = summarize(r, 1.0);
            let err = 100.0 * (s.lbar_um.unwrap() / (tl / tp as f64) - 1.0);
            println!("seed {seed} n {n} {:7} P={:6.1} (truth {tp}) err {err:+.1} %", r.method.name(), s.p);
            worst = worst.max(err.abs());
        }
    }
    println!("sai số lớn nhất {worst:.1} %");
    assert!(worst <= 8.0, "sai số ℓ vượt 8 %: {worst:.1}");
}

#[test]
fn cong_thuc_va_thong_ke() {
    assert!((g_from_l_um(32.0) - 6.644).abs() < 0.001);
    let (_m, sd, ci, _ra, _g) = field_stats(&[10.1, 9.6, 10.8, 10.3, 9.9]);
    assert!((ci - 2.776 * sd / 5f64.sqrt()).abs() < 1e-12);
    assert_eq!(t975(5), 2.776);
}

#[test]
fn hinh_thai_nhanh_trung_ban_vet_can() {
    let mut r = Lcg(5);
    let (w, h) = (157, 93);
    let img = Img { w, h, d: (0..w * h).map(|_| (r.next() * 255.0) as u8).collect() };
    for se in [ellipse(5), ellipse(13), ellipse(25), rect(9, 9), rect(5, 1), rect(3, 3)] {
        assert_eq!(dilate(&img, &se).d, morph_ref(&img, &se, true).d);
        assert_eq!(erode(&img, &se).d, morph_ref(&img, &se, false).d);
    }
}
