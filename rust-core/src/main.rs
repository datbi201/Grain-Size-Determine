//! gsd-core: chạy lõi Rust trên một hoặc nhiều ảnh, in kết quả JSON (dùng để đối chiếu với bản Python).
//!
//!   gsd_core.exe <ảnh>... [--um 0.5] [--sens 85] [--no-focus] [--no-fov] [--no-bar]
use gsd_core::*;
use serde_json::json;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut um = 0.5;
    let mut p = Params::default();
    let (mut use_fov, mut use_bar) = (true, true);
    let mut files = vec![];
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--um" => { um = args[i + 1].parse().unwrap(); i += 1; }
            "--sens" => { p.sensitivity = args[i + 1].parse().unwrap(); i += 1; }
            "--no-focus" => p.focus = false,
            "--no-fov" => use_fov = false,
            "--no-bar" => use_bar = false,
            "--hv" => p.mixed_hv = true,
            f => files.push(f.to_string()),
        }
        i += 1;
    }
    let mut out = vec![];
    for f in &files {
        let t0 = Instant::now();
        let img = match load(f) {
            Ok(v) => v,
            Err(e) => { out.push(json!({"image": f, "error": e})); continue; }
        };
        let t_load = t0.elapsed().as_secs_f64();
        let mut tt = Instant::now();
        tick("load", &mut tt);
        let fov = if use_fov { detect_fov(&img) } else { None };
        tick("fov", &mut tt);
        let bar = if use_bar { find_red_bar(&img) } else { None };
        tick("bar", &mut tt);
        let a = analyze(&img, fov, bar.map(|b| b.1), &[Method::Abrams, Method::Heyn, Method::Mixed], &p);
        let mut ms = serde_json::Map::new();
        for r in &a.methods {
            let s = summarize(r, um);
            ms.insert(r.method.name().into(), json!({"P": s.p, "L_um": s.l_um, "l_um": s.lbar_um, "G": s.g,
                "cross": s.n_cross, "triple": s.n_triple, "end": s.n_end}));
        }
        out.push(json!({"image": f, "w": img.w, "h": img.h,
            "fov": fov.map(|v| [v.0, v.1, v.2]), "bar": bar.map(|b| json!({"len": b.0, "box": b.1})),
            "excluded": a.excluded, "skeleton_px": a.skeleton.d.iter().filter(|&&v| v).count(),
            "valid_px": a.valid.d.iter().filter(|&&v| v).count(),
            "methods": ms, "t_load_s": t_load, "t_total_s": t0.elapsed().as_secs_f64()}));
    }
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
