//! Grain Size Determine – bản giao diện Rust (egui). Rev 0.1.1_rust.
#![windows_subsystem = "windows"]
mod app;
mod model;
mod report;

fn icon() -> Option<egui::IconData> {
    let img = image::load_from_memory(include_bytes!("../../assets/app.png")).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    Some(egui::IconData { rgba: img.into_raw(), width: w, height: h })
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(|s| s.as_str()) == Some("--readproj") {
        let mut n = 0;
        match model::read_project(&args[1], &mut n) {
            Ok(p) => {
                let mut s = String::new();
                for it in &p.items {
                    let g: Vec<String> = model::METHODS.iter().map(|&m| it.res(m).and_then(|r| r.g).map(|g| format!("{}={g:.3}", m.name())).unwrap_or_default()).collect();
                    s += &format!("{} include={} edited={:?} {}
", it.name, it.include, it.edited(), g.join(" "));
                }
                let _ = std::fs::write(&args[2], s);
            }
            Err(e) => { let _ = std::fs::write(&args[2], format!("LỖI {e}")); }
        }
        return Ok(());
    }
    if args.first().map(|s| s.as_str()) == Some("--selftest") {
        let code = selftest::run(&args[1..]);
        std::process::exit(code);
    }
    let mut vp = egui::ViewportBuilder::default().with_inner_size([1440.0, 900.0]).with_min_inner_size([1100.0, 700.0])
        .with_drag_and_drop(true).with_title(format!("{} Rev {}", app::APP_NAME, app::APP_VER));
    if let Some(i) = icon() {
        vp = vp.with_icon(std::sync::Arc::new(i));
    }
    let opts = eframe::NativeOptions { viewport: vp, ..Default::default() };
    eframe::run_native(app::APP_NAME, opts, Box::new(move |cc| Ok(Box::new(app::App::new(cc, args)))))
}

/// Tự kiểm tra không cần giao diện: đo ảnh, sửa thủ công, lưu/mở project, xuất PDF.
mod selftest {
    use crate::model::*;
    use crate::report;
    use gsd_core::*;

    pub fn run(args: &[String]) -> i32 {
        let out = args.first().cloned().unwrap_or_else(|| std::env::temp_dir().join("gsd_selftest").to_string_lossy().to_string());
        let _ = std::fs::create_dir_all(&out);
        let mut log = vec![];
        let mut ok = true;
        let imgs: Vec<String> = args.iter().skip(1).cloned().collect();
        let mut items = vec![];
        let mut next = 0u64;
        for p in &imgs {
            next += 1;
            let mut it = Item::new(p, next);
            let Ok(img) = load(p) else { log.push(format!("LỖI đọc {p}")); ok = false; continue };
            it.fov = detect_fov(&img);
            let bar = find_red_bar(&img);
            it.bar_box = bar.map(|b| b.1);
            it.bar_px = bar.map(|b| b.0);
            it.size = Some((img.w, img.h));
            it.probed = true;
            it.um_per_px = Some(bar.map(|b| 100.0 / b.0).unwrap_or(0.5));
            it.calib = "selftest".into();
            it.data = Some(analyze(&img, it.fov, it.bar_box, &METHODS, &Params::default()));
            it.state = "Đã đo".into();
            items.push(it);
        }
        // sửa thủ công trên ảnh đầu
        if let Some(it) = items.first_mut() {
            let a = it.data.as_mut().unwrap();
            let valid = a.valid.clone();
            let r = a.method_mut(Method::Mixed).unwrap();
            let p0 = summarize(r, 1.0).p;
            if let Chord::Circle { cx, cy, r: rr, .. } = r.chords[0].clone() {
                let res = add_manual(r, cx + rr * 0.6f64.cos(), cy + rr * 0.6f64.sin(), Kind::Triple, &valid, 20.0);
                log.push(format!("thêm điểm ba hạt: {res:?}; P {p0} -> {}", summarize(r, 1.0).p));
            }
            let e = r.events.iter().find(|e| !e.manual).cloned().unwrap();
            remove_near(r, e.x, e.y, 0.5);
            log.push(format!("loại 1 điểm tự động: P = {}", summarize(r, 1.0).p));
        }
        let proj = format!("{out}/selftest.gsd");
        match write_project(&proj, &items, serde_json::json!({}), serde_json::json!({}), "selftest") {
            Ok(()) => log.push(format!("lưu project OK ({} byte)", std::fs::metadata(&proj).map(|m| m.len()).unwrap_or(0))),
            Err(e) => { log.push(format!("LỖI lưu project: {e}")); ok = false; }
        }
        let mut nid = 100;
        match read_project(&proj, &mut nid) {
            Ok(pj) => {
                let same = pj.items.iter().zip(&items).all(|(a, b)| METHODS.iter().all(|&m| a.res(m).map(|r| r.p) == b.res(m).map(|r| r.p)));
                let edited = pj.items.first().map(|i| i.edited()).unwrap_or_default();
                log.push(format!("mở lại project: {} ảnh, số liệu trùng = {same}, sửa TC giữ = {:?}", pj.items.len(), edited));
                ok &= same && !pj.items.is_empty();
            }
            Err(e) => { log.push(format!("LỖI mở project: {e}")); ok = false; }
        }
        let fields: Vec<(Method, Vec<report::Field>)> = METHODS.iter().map(|&m| (m, items.iter().filter_map(|it| {
            let r = it.res(m)?;
            r.lbar_um?;
            let img = load(&it.path).ok()?;
            let ov = gsd_core::draw::render(&img, it.data.as_ref(), m, &Default::default(), true, true);
            let rgb = image::RgbImage::from_raw(ov.w as u32, ov.h as u32, ov.d)?;
            let mut buf = vec![];
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 85).encode_image(&rgb).ok()?;
            Some(report::Field { name: it.name.clone(), um_per_px: it.um_per_px?, calib: it.calib.clone(), res: r, jpeg: Some((buf, ov.w, ov.h)) })
        }).collect())).collect();
        let pdf = format!("{out}/selftest.pdf");
        let info = report::Info { report_no: "SELFTEST".into(), date: "21/11/2026".into(), sample: "Mẫu thử".into(), ..Default::default() };
        let params = report::RParams { sensitivity: 85.0, focus: true, n_lines: 5, mixed_hv: false, enhance_display: "không".into(), enhance_detect: "ảnh gốc".into() };
        match report::build_pdf(&pdf, &info, &METHODS, &fields, &params, "Chú giải thử", 2, "Grain Size Determine") {
            Ok(()) => log.push(format!("PDF OK ({} byte)", std::fs::metadata(&pdf).map(|m| m.len()).unwrap_or(0))),
            Err(e) => { log.push(format!("LỖI PDF: {e}")); ok = false; }
        }
        for it in &items {
            let g: Vec<String> = METHODS.iter().map(|&m| it.res(m).and_then(|r| r.g).map(|g| format!("{}={g:.3}", m.name())).unwrap_or_default()).collect();
            log.push(format!("{}: {}", it.name, g.join(" ")));
        }
        log.push(if ok { "KẾT QUẢ: ĐẠT".into() } else { "KẾT QUẢ: LỖI".into() });
        let _ = std::fs::write(format!("{out}/selftest.txt"), log.join("\n"));
        if ok { 0 } else { 1 }
    }
}
