//! Cửa sổ chính Grain Size Determine (egui) – cùng chức năng với bản Python.
use crate::model::*;
use crate::report;
use eframe::egui;
use egui::{Color32, Pos2, Rect, Stroke, Vec2};
use gsd_core::draw::Colors;
use gsd_core::enhance::{self, Enhance};
use gsd_core::*;
use serde_json::{json, Value};
use std::sync::mpsc;

pub const APP_NAME: &str = "Grain Size Determine";
pub const APP_VER: &str = "0.1.1_rust";
const IMG_EXT: [&str; 7] = ["jpg", "jpeg", "png", "bmp", "tif", "tiff", "webp"];

fn data_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var("APPDATA").unwrap_or(".".into())).join("GrainSizeDetermine")
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn from_hex(s: &str) -> Option<[u8; 3]> {
    let s = s.trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    Some([u8::from_str_radix(&s[0..2], 16).ok()?, u8::from_str_radix(&s[2..4], 16).ok()?, u8::from_str_radix(&s[4..6], 16).ok()?])
}

fn c32(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

fn parse_f(s: &str) -> Option<f64> {
    s.trim().replace(',', ".").parse::<f64>().ok().filter(|v| v.is_finite() && *v > 0.0)
}

fn open_with_system(path: &str) {
    let _ = std::process::Command::new("cmd").args(["/C", "start", "", path]).spawn();
}

fn ask_yes_no_cancel(title: &str, msg: &str) -> Option<bool> {
    match rfd::MessageDialog::new().set_title(title).set_description(msg)
        .set_buttons(rfd::MessageButtons::YesNoCancel).set_level(rfd::MessageLevel::Warning).show() {
        rfd::MessageDialogResult::Yes => Some(true),
        rfd::MessageDialogResult::No => Some(false),
        _ => None,
    }
}

fn ask_yes_no(title: &str, msg: &str) -> bool {
    matches!(rfd::MessageDialog::new().set_title(title).set_description(msg).set_buttons(rfd::MessageButtons::YesNo).show(),
             rfd::MessageDialogResult::Yes)
}

fn info(title: &str, msg: &str) {
    rfd::MessageDialog::new().set_title(title).set_description(msg).set_buttons(rfd::MessageButtons::Ok).show();
}

struct Job {
    id: u64,
    path: String,
    fov: Option<(f64, f64, f64)>,
    bar: Option<[usize; 4]>,
    probed: bool,
    params: Params,
    methods: Vec<Method>,
    enh: Option<Enhance>,
}

struct JobResult {
    id: u64,
    res: Result<(Analysis, (usize, usize), Option<(f64, f64, f64)>, Option<(f64, [usize; 4])>), String>,
}

#[derive(Default)]
struct ReportForm {
    open: bool,
    info: report::Info,
    methods: [bool; 3],
    images: bool,
    per_page: usize,
}

pub struct App {
    items: Vec<Item>,
    next_id: u64,
    cur: Option<u64>,
    cur_img: Option<(u64, gsd_core::Rgb)>,
    tex: Option<egui::TextureHandle>,
    disp_key: String,
    data_ver: u64,
    img_size: (usize, usize),
    zoom: f32,
    off: Vec2,
    fit_pending: bool,
    // thang đo
    px_s: String,
    um_s: String,
    direct_s: String,
    fov_s: String,
    pick: Option<Vec<(f64, f64)>>,
    // tham số
    m_on: [bool; 3],
    mixed_hv: bool,
    n_lines: usize,
    sens: f64,
    focus: bool,
    enh: Enhance,
    enh_detect: bool,
    colors: Colors,
    view: Method,
    show_bound: bool,
    show_pat: bool,
    show_removed: bool,
    edit: bool,
    kind: Kind,
    // trạng thái
    status: String,
    project_path: Option<String>,
    dirty: bool,
    title_shown: String,
    allow_close: bool,
    settings: Value,
    report: ReportForm,
    help_open: bool,
    about_open: bool,
    // worker
    job_tx: mpsc::Sender<Job>,
    res_rx: mpsc::Receiver<JobResult>,
    pending: usize,
    done: usize,
    frame: u64,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, args: Vec<String>) -> Self {
        setup_fonts(&cc.egui_ctx);
        let (job_tx, job_rx) = mpsc::channel::<Job>();
        let (res_tx, res_rx) = mpsc::channel::<JobResult>();
        let ctx = cc.egui_ctx.clone();
        std::thread::spawn(move || {
            while let Ok(j) = job_rx.recv() {
                let res = (|| {
                    let img = load(&j.path)?;
                    let size = (img.w, img.h);
                    let (fov, bar) = if j.probed { (j.fov, None) } else { (detect_fov(&img), find_red_bar(&img)) };
                    let bar_box = j.bar.or(bar.map(|b| b.1));
                    let a = analyze_enh(&img, fov, bar_box, &j.methods, &j.params, j.enh.as_ref());
                    Ok((a, size, fov, bar))
                })();
                let _ = res_tx.send(JobResult { id: j.id, res });
                ctx.request_repaint();
            }
        });
        let settings: Value = std::fs::read(data_dir().join("settings_rust.json")).ok()
            .and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(json!({}));
        let s = &settings;
        let mut colors = Colors::default();
        if let Some(c) = s["colors"].as_object() {
            for (k, v) in c {
                if let Some(rgb) = v.as_str().and_then(from_hex) {
                    match k.as_str() {
                        "chord" => colors.chord = rgb,
                        "point" => colors.point = rgb,
                        "manual" => colors.manual = rgb,
                        "boundary" => colors.boundary = rgb,
                        _ => {}
                    }
                }
            }
        }
        let methods_on = s["methods"].as_array().map(|a| {
            let names: Vec<&str> = a.iter().filter_map(|v| v.as_str()).collect();
            [names.contains(&"abrams"), names.contains(&"heyn"), names.contains(&"mixed")]
        }).unwrap_or([true, true, true]);
        let mut app = App {
            items: vec![], next_id: 0, cur: None, cur_img: None, tex: None, disp_key: String::new(), data_ver: 0,
            img_size: (1, 1), zoom: 1.0, off: Vec2::ZERO, fit_pending: false,
            px_s: String::new(), um_s: format!("{}", s["last_um"].as_f64().unwrap_or(100.0)), direct_s: String::new(),
            fov_s: String::new(), pick: None,
            m_on: methods_on, mixed_hv: s["mixed_hv"].as_bool().unwrap_or(false),
            n_lines: s["n_lines"].as_u64().unwrap_or(5) as usize, sens: s["sens"].as_f64().unwrap_or(85.0),
            focus: s["focus"].as_bool().unwrap_or(true), enh: enh_from(&s["enhance"]),
            enh_detect: s["enh_detect"].as_bool().unwrap_or(false), colors,
            view: s["view_method"].as_str().and_then(method_from).unwrap_or(Method::Abrams),
            show_bound: true, show_pat: true, show_removed: false, edit: false, kind: Kind::Cross,
            status: "Sẵn sàng. Kéo ảnh vào cửa sổ hoặc bấm «Thêm ảnh».".into(), project_path: None, dirty: false,
            title_shown: String::new(), allow_close: false, settings,
            report: ReportForm { per_page: 2, images: true, methods: [true, false, false], ..Default::default() },
            help_open: false, about_open: false, job_tx, res_rx, pending: 0, done: 0, frame: 0,
        };
        if let Some(a) = args.first() {
            if a.to_lowercase().ends_with(".gsd") {
                app.load_project(a.clone());
            } else {
                app.add_paths(args.iter().map(std::path::PathBuf::from).collect());
            }
        }
        app
    }

    // ============================================================ tiện ích ===
    fn idx(&self, id: u64) -> Option<usize> {
        self.items.iter().position(|i| i.id == id)
    }
    fn cur_item(&self) -> Option<&Item> {
        self.cur.and_then(|id| self.items.iter().find(|i| i.id == id))
    }
    fn cur_mut(&mut self) -> Option<&mut Item> {
        let id = self.cur?;
        self.items.iter_mut().find(|i| i.id == id)
    }
    fn set_dirty(&mut self) {
        self.dirty = true;
    }
    fn save_settings(&mut self) {
        let methods: Vec<&str> = METHODS.iter().zip(self.m_on).filter(|(_, on)| *on).map(|(m, _)| m.name()).collect();
        let s = &mut self.settings;
        s["last_um"] = json!(parse_f(&self.um_s).unwrap_or(100.0));
        s["colors"] = json!({"chord": hex(self.colors.chord), "point": hex(self.colors.point),
                             "manual": hex(self.colors.manual), "boundary": hex(self.colors.boundary)});
        s["methods"] = json!(methods);
        s["mixed_hv"] = json!(self.mixed_hv);
        s["n_lines"] = json!(self.n_lines);
        s["sens"] = json!(self.sens);
        s["focus"] = json!(self.focus);
        s["enhance"] = enh_json(&self.enh);
        s["enh_detect"] = json!(self.enh_detect);
        s["view_method"] = json!(self.view.name());
        let _ = std::fs::create_dir_all(data_dir());
        let _ = std::fs::write(data_dir().join("settings_rust.json"), serde_json::to_vec_pretty(&self.settings).unwrap());
    }
    fn remember_scale(&mut self, id: u64) {
        let Some(i) = self.idx(id) else { return };
        let it = &self.items[i];
        let key = std::fs::canonicalize(&it.path).map(|p| p.to_string_lossy().to_string()).unwrap_or(it.path.clone());
        if !self.settings["scales"].is_object() {
            self.settings["scales"] = json!({});
        }
        self.settings["scales"][key] = json!({"um_per_px": it.um_per_px, "calib": it.calib, "bar_box": it.bar_box});
        self.save_settings();
    }

    // ============================================================== ảnh ===
    fn add_paths(&mut self, paths: Vec<std::path::PathBuf>) {
        let mut files = vec![];
        for p in paths {
            if p.is_dir() {
                if let Ok(rd) = std::fs::read_dir(&p) {
                    let mut v: Vec<_> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
                    v.sort();
                    files.extend(v.into_iter().filter(|f| f.is_file()));
                }
            } else if p.is_file() {
                files.push(p);
            }
        }
        let have: Vec<String> = self.items.iter().map(|i| i.path.to_lowercase()).collect();
        let mut added = 0;
        let mut first = None;
        for f in files {
            let ext = f.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            if !IMG_EXT.contains(&ext.as_str()) {
                continue;
            }
            let ps = f.to_string_lossy().to_string();
            if have.contains(&ps.to_lowercase()) {
                continue;
            }
            self.next_id += 1;
            let mut it = Item::new(&ps, self.next_id);
            let key = std::fs::canonicalize(&f).map(|p| p.to_string_lossy().to_string()).unwrap_or(ps.clone());
            let sc = &self.settings["scales"][key.as_str()];
            if let Some(u) = sc["um_per_px"].as_f64() {
                it.um_per_px = Some(u);
                it.calib = sc["calib"].as_str().unwrap_or("").into();
                it.bar_box = sc["bar_box"].as_array().map(|a| {
                    let g = |i: usize| a.get(i).and_then(|x| x.as_f64()).unwrap_or(0.0) as usize;
                    [g(0), g(1), g(2), g(3)]
                });
                it.state = "Đã có scale (chưa đo)".into();
            }
            first.get_or_insert(it.id);
            self.items.push(it);
            added += 1;
        }
        if added > 0 {
            self.set_dirty();
            if let Some(id) = first {
                self.select(id);
            }
            self.status = format!("Đã thêm {added} ảnh.");
        }
    }

    fn select(&mut self, id: u64) {
        let Some(i) = self.idx(id) else { return };
        let path = self.items[i].path.clone();
        match load(&path) {
            Ok(img) => {
                let it = &mut self.items[i];
                if !it.probed {
                    it.size = Some((img.w, img.h));
                    it.fov = detect_fov(&img);
                    if let Some((len, bx)) = find_red_bar(&img) {
                        it.bar_px = Some(len);
                        if it.bar_box.is_none() {
                            it.bar_box = Some(bx);
                        }
                    }
                    it.probed = true;
                }
                self.px_s = it.bar_px.map(|v| format!("{v:.0}")).unwrap_or_default();
                self.direct_s = it.um_per_px.map(|v| format!("{v:.5}")).unwrap_or_default();
                self.fov_s.clear();
                self.status = format!("{}: {}×{} px{}{}", it.name, img.w, img.h,
                                      it.fov.map(|f| format!(", trường nhìn tròn R = {:.0} px", f.2)).unwrap_or(", toàn khung".into()),
                                      it.bar_px.map(|b| format!(", thanh đỏ {b:.0} px")).unwrap_or_default());
                self.img_size = (img.w, img.h);
                self.cur_img = Some((id, img));
                self.cur = Some(id);
                self.pick = None;
                self.fit_pending = true;
                self.disp_key.clear();
            }
            Err(e) => self.status = format!("Không mở được ảnh {}: {e}", self.items[i].name),
        }
    }

    fn remove_current(&mut self) {
        if let Some(id) = self.cur {
            self.items.retain(|i| i.id != id);
            self.cur = None;
            self.cur_img = None;
            self.tex = None;
            self.set_dirty();
        }
    }

    // ========================================================== thang đo ===
    fn auto_bar(&mut self) {
        let Some((_, img)) = &self.cur_img else { return };
        match find_red_bar(img) {
            Some((len, bx)) => {
                if let Some(it) = self.cur_mut() {
                    it.bar_px = Some(len);
                    it.bar_box = Some(bx);
                }
                self.px_s = format!("{len:.0}");
                self.status = format!("Thanh đỏ dài {len:.0} px. Nhập độ dài thực (µm) ghi trên nhãn rồi Áp dụng.");
            }
            None => info(APP_NAME, "Không tìm thấy thanh scale màu đỏ ở nửa dưới ảnh.\nHãy dùng «Chọn 2 điểm»."),
        }
    }

    fn confirm_overwrite(&self, ids: &[u64]) -> bool {
        let n = self.items.iter().filter(|i| ids.contains(&i.id) && !i.edited().is_empty()).count();
        n == 0 || ask_yes_no(APP_NAME, &format!("{n} ảnh có chỉnh sửa giao điểm thủ công. Đo lại sẽ xóa các chỉnh sửa này. Tiếp tục?"))
    }

    fn set_scale(&mut self, id: u64, upp: f64, calib: String) {
        if let Some(i) = self.idx(id) {
            let it = &mut self.items[i];
            it.um_per_px = Some(upp);
            it.calib = calib;
            it.state = "Đã có scale (chưa đo)".into();
            if Some(id) == self.cur {
                self.direct_s = format!("{upp:.5}");
            }
            self.remember_scale(id);
            self.set_dirty();
        }
    }

    fn apply_scale(&mut self, to_selected_all: bool) {
        let Some(id) = self.cur else { return };
        let (Some(px), Some(um)) = (parse_f(&self.px_s), parse_f(&self.um_s)) else {
            info(APP_NAME, "Cần độ dài thanh (px) và độ dài thực (µm) hợp lệ.");
            return;
        };
        let upp = um / px;
        let ids: Vec<u64> = if to_selected_all { self.items.iter().filter(|i| i.include).map(|i| i.id).collect() } else { vec![id] };
        if !self.confirm_overwrite(&ids) {
            return;
        }
        let name = self.cur_item().map(|i| i.name.clone()).unwrap_or_default();
        for &i in &ids {
            let calib = if i == id { format!("thanh tỉ lệ {um} µm = {px:.1} px") } else { format!("dùng chung scale của {name}") };
            self.set_scale(i, upp, calib);
        }
        self.save_settings();
        self.queue(ids, false);
    }

    fn apply_direct(&mut self) {
        let Some(id) = self.cur else { return };
        let Some(v) = parse_f(&self.direct_s) else { info(APP_NAME, "Giá trị µm/px không hợp lệ."); return };
        if self.confirm_overwrite(&[id]) {
            self.set_scale(id, v, "nhập trực tiếp µm/px".into());
            self.queue(vec![id], false);
        }
    }

    fn apply_fov(&mut self) {
        let Some(id) = self.cur else { return };
        let Some(fov) = self.cur_item().and_then(|i| i.fov) else { info(APP_NAME, "Ảnh này không có trường nhìn tròn nhận diện được."); return };
        let Some(d) = parse_f(&self.fov_s) else { info(APP_NAME, "Đường kính trường nhìn không hợp lệ."); return };
        if self.confirm_overwrite(&[id]) {
            self.set_scale(id, d / (2.0 * fov.2), format!("đường kính trường nhìn {d} µm"));
            self.queue(vec![id], false);
        }
    }

    // ============================================================= đo ===
    fn params(&self) -> Params {
        Params { sensitivity: self.sens, focus: self.focus, n_lines: self.n_lines, mixed_hv: self.mixed_hv }
    }
    fn methods(&self) -> Vec<Method> {
        let v: Vec<Method> = METHODS.iter().zip(self.m_on).filter(|(_, on)| *on).map(|(m, _)| *m).collect();
        if v.is_empty() { vec![Method::Abrams] } else { v }
    }

    fn queue(&mut self, ids: Vec<u64>, confirm: bool) {
        if confirm && !self.confirm_overwrite(&ids) {
            return;
        }
        let (params, methods) = (self.params(), self.methods());
        let enh = if self.enh_detect { Some(self.enh.clone()) } else { None };
        let mut n = 0;
        let mut miss = 0;
        for id in ids {
            let Some(i) = self.idx(id) else { continue };
            let it = &mut self.items[i];
            if it.um_per_px.is_none() {
                it.state = "Thiếu scale".into();
                miss += 1;
                continue;
            }
            it.state = "Đang đo…".into();
            let _ = self.job_tx.send(Job { id, path: it.path.clone(), fov: it.fov, bar: it.bar_box, probed: it.probed,
                                           params: params.clone(), methods: methods.clone(), enh: enh.clone() });
            self.pending += 1;
            n += 1;
        }
        if n == 0 && miss > 0 {
            info(APP_NAME, "Ảnh chưa có scale. Hãy thiết lập thang đo (mục 1) trước.");
        }
        self.status = format!("Đang đo {n} ảnh…{}", if miss > 0 { format!(" (bỏ qua {miss} ảnh thiếu scale)") } else { String::new() });
    }

    fn poll(&mut self) {
        while let Ok(r) = self.res_rx.try_recv() {
            self.pending = self.pending.saturating_sub(1);
            self.done += 1;
            let Some(i) = self.idx(r.id) else { continue };
            match r.res {
                Ok((a, size, fov, bar)) => {
                    let it = &mut self.items[i];
                    if !it.probed {
                        it.size = Some(size);
                        it.fov = fov;
                        if let Some((len, bx)) = bar {
                            it.bar_px = Some(len);
                            it.bar_box.get_or_insert(bx);
                        }
                        it.probed = true;
                    }
                    it.note = if a.excluded > 0.02 { format!("loại {:.0}% vùng mờ", a.excluded * 100.0) } else { String::new() };
                    it.data = Some(a);
                    it.undo.clear();
                    it.state = "Đã đo".into();
                    self.data_ver += 1;
                    self.dirty = true;
                    if !self.items[i].data.as_ref().unwrap().methods.iter().any(|m| m.method == self.view) {
                        self.view = self.items[i].data.as_ref().unwrap().methods[0].method;
                    }
                }
                Err(e) => {
                    self.items[i].state = "Lỗi".into();
                    self.items[i].note = e;
                }
            }
            if self.pending == 0 {
                self.done = 0;
                self.status = "Đã đo xong. Rà soát giao điểm ở chế độ sửa (phím E) nếu cần.".into();
            }
        }
    }

    // ===================================================== sửa thủ công ===
    fn edit_at(&mut self, action: u8, x: f64, y: f64) {
        let view = self.view;
        let zoom = self.zoom as f64;
        let kind = self.kind;
        let Some(it) = self.cur_mut() else { return };
        let Some(a) = it.data.as_mut() else { return };
        let valid = a.valid.clone();
        let Some(r) = a.method_mut(view) else { return };
        let snap = (view, r.events.clone(), r.edited);
        let msg = match action {
            0 => match add_manual(r, x, y, kind, &valid, (12.0 / zoom).max(3.0)) {
                Ok(id) => format!("Đã thêm giao điểm trên {id}."),
                Err(e) => { self.status = e; return; }
            },
            1 => match remove_near(r, x, y, (10.0 / zoom).max(3.0)) {
                Some((id, manual)) => format!("Đã loại 1 giao điểm ({}) trên {id}.", if manual { "thủ công" } else { "tự động" }),
                None => { self.status = "Không có giao điểm nào gần vị trí click.".into(); return; }
            },
            _ => if restore_near(r, x, y, (10.0 / zoom).max(3.0)) { "Đã khôi phục 1 giao điểm tự động.".into() }
                 else { self.status = "Không có điểm đã loại nào gần vị trí click.".into(); return; },
        };
        it.undo.push(snap);
        if it.undo.len() > 50 {
            it.undo.remove(0);
        }
        self.status = msg;
        self.set_dirty();
    }

    fn undo(&mut self) {
        let Some(it) = self.cur_mut() else { return };
        let Some((m, ev, ed)) = it.undo.pop() else { return };
        if let Some(r) = it.data.as_mut().and_then(|a| a.method_mut(m)) {
            r.events = ev;
            r.edited = ed;
        }
        self.view = m;
        self.status = "Đã hoàn tác.".into();
        self.set_dirty();
    }

    fn reset_edits(&mut self) {
        let view = self.view;
        let Some(it) = self.cur_mut() else { return };
        let Some(r) = it.data.as_mut().and_then(|a| a.method_mut(view)) else { return };
        if !r.edited {
            return;
        }
        if !ask_yes_no(APP_NAME, &format!("Bỏ mọi chỉnh sửa thủ công của {} trên ảnh này?", method_short(view))) {
            return;
        }
        let snap = (view, r.events.clone(), true);
        r.events.retain(|e| !e.manual);
        r.events.iter_mut().for_each(|e| e.removed = false);
        r.edited = false;
        it.undo.push(snap);
        self.set_dirty();
    }

    // ============================================================ project ===
    fn settings_json(&self) -> Value {
        let methods: Vec<&str> = METHODS.iter().zip(self.m_on).filter(|(_, on)| *on).map(|(m, _)| m.name()).collect();
        json!({"methods": methods, "mixed_hv": self.mixed_hv, "n_lines": self.n_lines, "sens": self.sens, "focus": self.focus,
               "enhance": enh_json(&self.enh), "enh_detect": self.enh_detect,
               "colors": {"chord": hex(self.colors.chord), "point": hex(self.colors.point), "manual": hex(self.colors.manual),
                          "boundary": hex(self.colors.boundary)}, "view_method": self.view.name()})
    }

    fn report_json(&self) -> Value {
        let i = &self.report.info;
        json!({"client": i.client, "material": i.material, "prep": i.prep, "equipment": i.equipment,
               "magnification": i.magnification, "operator": i.operator, "reviewer": i.reviewer, "approver": i.approver,
               "location": i.location, "sample": i.sample, "per_page": self.report.per_page})
    }

    fn save_project(&mut self, as_new: bool) -> bool {
        let path = match (&self.project_path, as_new) {
            (Some(p), false) => p.clone(),
            _ => match rfd::FileDialog::new().set_title("Lưu project").add_filter("Project Grain Size Determine", &["gsd"])
                .set_file_name("Project_CoHat.gsd").save_file() {
                Some(p) => p.to_string_lossy().to_string(),
                None => return false,
            },
        };
        match write_project(&path, &self.items, self.settings_json(), self.report_json(), APP_VER) {
            Ok(()) => {
                self.project_path = Some(path.clone());
                self.dirty = false;
                self.status = format!("Đã lưu project: {path}");
                true
            }
            Err(e) => {
                info(APP_NAME, &format!("Không lưu được project: {e}"));
                false
            }
        }
    }

    fn ask_save(&mut self) -> bool {
        if !self.dirty || self.items.is_empty() {
            return true;
        }
        match ask_yes_no_cancel(APP_NAME, "Project có thay đổi chưa lưu. Lưu trước khi tiếp tục?") {
            None => false,
            Some(true) => self.save_project(false),
            Some(false) => true,
        }
    }

    fn new_project(&mut self) {
        if !self.ask_save() {
            return;
        }
        self.items.clear();
        self.cur = None;
        self.cur_img = None;
        self.tex = None;
        self.project_path = None;
        self.dirty = false;
        self.status = "Project mới.".into();
    }

    fn open_project(&mut self) {
        if !self.ask_save() {
            return;
        }
        if let Some(p) = rfd::FileDialog::new().set_title("Mở project").add_filter("Project Grain Size Determine", &["gsd"]).pick_file() {
            self.load_project(p.to_string_lossy().to_string());
        }
    }

    fn load_project(&mut self, path: String) {
        match read_project(&path, &mut self.next_id) {
            Ok(pj) => {
                let s = &pj.settings;
                if let Some(a) = s["methods"].as_array() {
                    let n: Vec<&str> = a.iter().filter_map(|v| v.as_str()).collect();
                    self.m_on = [n.contains(&"abrams"), n.contains(&"heyn"), n.contains(&"mixed")];
                }
                self.mixed_hv = s["mixed_hv"].as_bool().unwrap_or(self.mixed_hv);
                self.n_lines = s["n_lines"].as_u64().map(|v| v as usize).unwrap_or(self.n_lines);
                self.sens = s["sens"].as_f64().unwrap_or(self.sens);
                self.focus = s["focus"].as_bool().unwrap_or(self.focus);
                if s["enhance"].is_object() {
                    self.enh = enh_from(&s["enhance"]);
                }
                self.enh_detect = s["enh_detect"].as_bool().unwrap_or(false);
                if let Some(c) = s["colors"].as_object() {
                    for (k, v) in c {
                        if let Some(rgb) = v.as_str().and_then(from_hex) {
                            match k.as_str() {
                                "chord" => self.colors.chord = rgb,
                                "point" => self.colors.point = rgb,
                                "manual" => self.colors.manual = rgb,
                                "boundary" => self.colors.boundary = rgb,
                                _ => {}
                            }
                        }
                    }
                }
                if let Some(m) = s["view_method"].as_str().and_then(method_from) {
                    self.view = m;
                }
                let r = &pj.report;
                let g = |k: &str| r[k].as_str().unwrap_or("").to_string();
                let inf = &mut self.report.info;
                inf.client = g("client");
                inf.material = g("material");
                inf.prep = g("prep");
                inf.equipment = g("equipment");
                inf.magnification = g("magnification");
                inf.operator = g("operator");
                inf.reviewer = g("reviewer");
                inf.approver = g("approver");
                inf.location = g("location");
                inf.sample = g("sample");
                let missing: Vec<String> = pj.items.iter().filter(|i| !std::path::Path::new(&i.path).exists()).map(|i| i.name.clone()).collect();
                self.items = pj.items;
                self.project_path = Some(path.clone());
                self.cur = None;
                self.cur_img = None;
                self.tex = None;
                self.data_ver += 1;
                if let Some(id) = self.items.iter().find(|i| std::path::Path::new(&i.path).exists()).map(|i| i.id) {
                    self.select(id);
                }
                self.dirty = false;
                self.status = format!("Đã mở project: {path} ({} ảnh)", self.items.len());
                if !missing.is_empty() {
                    info(APP_NAME, &format!("Không tìm thấy các ảnh sau (đã di chuyển hoặc đổi tên?):\n{}", missing.join("\n")));
                }
            }
            Err(e) => info(APP_NAME, &format!("Không mở được project: {e}")),
        }
    }

    // ============================================================ xuất ===
    fn included(&self) -> Vec<&Item> {
        self.items.iter().filter(|i| i.include && i.data.is_some()).collect()
    }

    fn render_item(&self, it: &Item, m: Method) -> Option<gsd_core::Rgb> {
        let img = load(&it.path).ok()?;
        let base = enhance::apply(&img, &self.enh);
        Some(gsd_core::draw::render(&base, it.data.as_ref(), m, &self.colors, self.show_bound, true))
    }

    fn export_csv(&mut self) {
        let its = self.included();
        if its.is_empty() {
            info(APP_NAME, "Chưa có kết quả nào để xuất.");
            return;
        }
        let Some(p) = rfd::FileDialog::new().set_title("Lưu kết quả").add_filter("CSV (mở bằng Excel)", &["csv"])
            .set_file_name("ket_qua_co_hat.csv").save_file() else { return };
        let mut s = String::from("\u{feff}Ảnh,µm/px,Hiệu chuẩn,Phương pháp,L (mm),P,Cắt,Ba hạt,Đầu mút,Thêm TC,Loại TC,ℓ (µm),ASTM G,ℓ ngang,ℓ dọc\r\n");
        let q = |t: &str| format!("\"{}\"", t.replace('"', "\"\""));
        for it in &its {
            for m in METHODS {
                let Some(r) = it.res(m) else { continue };
                let Some(g) = r.g else { continue };
                s += &format!("{},{:.5},{},{},{:.4},{},{},{},{},{},{},{:.3},{:.3},{},{}\r\n", q(&it.name), it.um_per_px.unwrap(),
                              q(&it.calib), method_short(m), r.l_mm, r.p, r.n_cross, r.n_triple, r.n_end, r.n_manual, r.n_removed,
                              r.lbar_um.unwrap(), g, r.l_h.map(|v| format!("{v:.3}")).unwrap_or_default(),
                              r.l_v.map(|v| format!("{v:.3}")).unwrap_or_default());
            }
        }
        s += "\r\nThống kê,n,ℓ tb (µm),s,t,95%CI,%RA,G,G làm tròn\r\n";
        for m in METHODS {
            let l: Vec<f64> = its.iter().filter_map(|i| i.res(m).and_then(|r| r.lbar_um)).collect();
            if l.is_empty() {
                continue;
            }
            let st = stats_with_checks(&l, &[], m);
            let o = |v: Option<f64>, d: usize| v.map(|x| format!("{x:.d$}")).unwrap_or_default();
            s += &format!("{},{},{:.3},{},{},{},{},{:.3},{:.1}\r\n", method_short(m), st.n, st.l_mean, o(st.sd, 3), o(st.t, 3),
                          o(st.ci, 3), o(st.ra, 2), st.g, st.g_round);
        }
        match std::fs::write(&p, s) {
            Ok(()) => self.status = format!("Đã lưu {}", p.display()),
            Err(e) => info(APP_NAME, &format!("Không ghi được file (đang mở trong Excel?): {e}")),
        }
    }

    fn save_overlays(&mut self) {
        let ids: Vec<u64> = self.included().iter().map(|i| i.id).collect();
        if ids.is_empty() {
            info(APP_NAME, "Chưa có kết quả nào.");
            return;
        }
        let Some(dir) = rfd::FileDialog::new().set_title("Chọn thư mục lưu ảnh kết quả").pick_folder() else { return };
        let mut n = 0;
        for id in ids {
            let it = &self.items[self.idx(id).unwrap()];
            for r in &it.data.as_ref().unwrap().methods {
                if let Some(img) = self.render_item(it, r.method) {
                    let stem = std::path::Path::new(&it.name).file_stem().unwrap().to_string_lossy().to_string();
                    let out = dir.join(format!("{stem}_{}.jpg", r.method.name()));
                    if let Some(buf) = image::RgbImage::from_raw(img.w as u32, img.h as u32, img.d) {
                        if buf.save(&out).is_ok() {
                            n += 1;
                        }
                    }
                }
            }
        }
        self.status = format!("Đã lưu {n} ảnh vào {}", dir.display());
    }

    fn make_report(&mut self, path: &str) -> Result<(), String> {
        let its: Vec<u64> = self.included().iter().map(|i| i.id).collect();
        let mut methods: Vec<Method> = METHODS.iter().zip(self.report.methods).filter(|(_, on)| *on).map(|(m, _)| *m).collect();
        if let Some(p) = methods.iter().position(|m| *m == self.view) {
            let v = methods.remove(p);
            methods.insert(0, v);
        }
        let mut fields = vec![];
        for &m in &methods {
            let mut fs = vec![];
            for &id in &its {
                let it = &self.items[self.idx(id).unwrap()];
                let Some(r) = it.res(m) else { continue };
                if r.lbar_um.is_none() {
                    continue;
                }
                let jpeg = if self.report.images {
                    self.render_item(it, m).and_then(|img| {
                        let s = (1600.0 / img.w.max(img.h) as f64).min(1.0);
                        let dimg = image::RgbImage::from_raw(img.w as u32, img.h as u32, img.d)?;
                        let dimg = if s < 1.0 {
                            image::imageops::resize(&dimg, (img.w as f64 * s) as u32, (img.h as f64 * s) as u32, image::imageops::FilterType::Triangle)
                        } else { dimg };
                        let mut buf = vec![];
                        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 88).encode_image(&dimg).ok()?;
                        Some((buf, dimg.width() as usize, dimg.height() as usize))
                    })
                } else { None };
                fs.push(report::Field { name: it.name.clone(), um_per_px: it.um_per_px.unwrap(), calib: it.calib.clone(), res: r, jpeg });
            }
            if !fs.is_empty() {
                fields.push((m, fs));
            }
        }
        let methods: Vec<Method> = methods.into_iter().filter(|m| fields.iter().any(|f| f.0 == *m)).collect();
        if methods.is_empty() {
            return Err("Không có ảnh nào có kết quả cho phương pháp đã chọn.".into());
        }
        let first = its.iter().find_map(|id| self.items[self.idx(*id).unwrap()].data.as_ref()).unwrap();
        let params = report::RParams { sensitivity: first.params.sensitivity, focus: first.params.focus, n_lines: first.params.n_lines,
                                       mixed_hv: first.params.mixed_hv, enhance_display: self.enh.describe(),
                                       enhance_detect: if first.enhance_desc.is_empty() { "ảnh gốc".into() } else { format!("ảnh đã xử lý ({})", first.enhance_desc) } };
        let legend = format!("Chú giải: đường chord – màu {}; giao điểm tự động – chấm tròn màu {}; giao điểm thủ công – màu {}; điểm ba hạt – chấm có vòng bao; đầu mút – vòng tròn rỗng; vùng tối – ngoài vùng đo.",
                             hex(self.colors.chord), hex(self.colors.point), hex(self.colors.manual));
        report::build_pdf(path, &self.report.info, &methods, &fields, &params, &legend, self.report.per_page, APP_NAME)
    }
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let fdir = std::path::PathBuf::from(std::env::var("WINDIR").unwrap_or("C:\\Windows".into())).join("Fonts");
    if let Ok(b) = std::fs::read(fdir.join("segoeui.ttf")) {
        fonts.font_data.insert("segoe".into(), egui::FontData::from_owned(b));
        fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap().insert(0, "segoe".into());
    }
    if let Ok(b) = std::fs::read(fdir.join("seguisym.ttf")) {
        fonts.font_data.insert("sym".into(), egui::FontData::from_owned(b));
        fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap().push("sym".into());
    }
    if let Ok(b) = std::fs::read(fdir.join("consola.ttf")) {
        fonts.font_data.insert("cons".into(), egui::FontData::from_owned(b));
        fonts.families.get_mut(&egui::FontFamily::Monospace).unwrap().insert(0, "cons".into());
        if fonts.font_data.contains_key("segoe") {
            fonts.families.get_mut(&egui::FontFamily::Monospace).unwrap().insert(1, "segoe".into());
        }
    }
    ctx.set_fonts(fonts);
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(6.0, 5.0);
    style.visuals = egui::Visuals::light();
    ctx.set_style(style);
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        // chế độ tự chụp (kiểm thử): GSD_SHOT=<tệp png>, GSD_SHOT_EDIT=1 bật chế độ sửa, GSD_SHOT_REPORT=1 mở hộp báo cáo
        self.frame += 1;
        if let Ok(shot) = std::env::var("GSD_SHOT") {
            if self.frame == 5 {
                if std::env::var("GSD_SHOT_EDIT").is_ok() { self.edit = true; }
                if let Ok(v) = std::env::var("GSD_SHOT_VIEW") { if let Some(m) = method_from(&v) { self.view = m; } }
                if std::env::var("GSD_SHOT_REPORT").is_ok() { self.open_report(); }
            }
            if self.frame == 40 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
            }
            let shots: Vec<std::sync::Arc<egui::ColorImage>> = ctx.input(|i| i.raw.events.iter().filter_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()), _ => None }).collect());
            if let Some(img) = shots.first() {
                let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| [c.r(), c.g(), c.b(), c.a()]).collect();
                if let Some(b) = image::RgbaImage::from_raw(img.size[0] as u32, img.size[1] as u32, rgba) {
                    let _ = b.save(&shot);
                }
                self.dirty = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ctx.request_repaint();
        }
        // ---- tiêu đề cửa sổ
        let title = format!("{}{} – {APP_NAME} Rev {APP_VER} (ASTM E112 / E1382)", if self.dirty { "● " } else { "" },
                            self.project_path.as_ref().and_then(|p| std::path::Path::new(p).file_name()).map(|s| s.to_string_lossy().to_string())
                                .unwrap_or("Project chưa lưu".into()));
        if title != self.title_shown {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title_shown = title;
        }
        // ---- đóng cửa sổ
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            if self.ask_save() {
                self.save_settings();
                self.allow_close = true;
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }
        // ---- kéo thả
        let dropped: Vec<std::path::PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if !dropped.is_empty() {
            if dropped.len() == 1 && dropped[0].extension().map_or(false, |e| e.eq_ignore_ascii_case("gsd")) {
                if self.ask_save() {
                    self.load_project(dropped[0].to_string_lossy().to_string());
                }
            } else {
                self.add_paths(dropped);
            }
        }
        // ---- phím tắt
        let typing = ctx.wants_keyboard_input();
        let (f5, ctrl_s, ctrl_o, ctrl_n, ctrl_z, key_e, esc) = ctx.input(|i| (
            i.key_pressed(egui::Key::F5), i.modifiers.command && i.key_pressed(egui::Key::S),
            i.modifiers.command && i.key_pressed(egui::Key::O), i.modifiers.command && i.key_pressed(egui::Key::N),
            i.modifiers.command && i.key_pressed(egui::Key::Z), !i.modifiers.command && i.key_pressed(egui::Key::E),
            i.key_pressed(egui::Key::Escape)));
        if f5 {
            let ids: Vec<u64> = self.cur.into_iter().collect();
            self.queue(ids, true);
        }
        if ctrl_s {
            self.save_project(false);
        }
        if ctrl_o {
            self.open_project();
        }
        if ctrl_n {
            self.new_project();
        }
        if ctrl_z && !typing {
            self.undo();
        }
        if key_e && !typing {
            self.edit = !self.edit;
            self.pick = None;
        }
        if esc {
            self.pick = None;
        }

        self.menu(ctx);
        self.right_panel(ctx);
        self.bottom_panel(ctx);
        self.viewer(ctx);
        self.windows(ctx);
        if self.pending > 0 {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_settings();
    }
}

// ============================================================== giao diện ===
impl App {
    fn menu(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("Tệp", |ui| {
                    if ui.button("Project mới   (Ctrl+N)").clicked() { ui.close_menu(); self.new_project(); }
                    if ui.button("Mở project…   (Ctrl+O)").clicked() { ui.close_menu(); self.open_project(); }
                    if ui.button("Lưu project   (Ctrl+S)").clicked() { ui.close_menu(); self.save_project(false); }
                    if ui.button("Lưu project thành…").clicked() { ui.close_menu(); self.save_project(true); }
                    ui.separator();
                    if ui.button("Thêm ảnh…").clicked() { ui.close_menu(); self.pick_files(); }
                    if ui.button("Thêm thư mục ảnh…").clicked() { ui.close_menu(); self.pick_folder(); }
                    ui.separator();
                    if ui.button("Lập báo cáo PDF…").clicked() { ui.close_menu(); self.open_report(); }
                    if ui.button("Xuất CSV…").clicked() { ui.close_menu(); self.export_csv(); }
                    if ui.button("Lưu ảnh kết quả…").clicked() { ui.close_menu(); self.save_overlays(); }
                    ui.separator();
                    if ui.button("Thoát").clicked() { ui.close_menu(); ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
                });
                ui.menu_button("Trợ giúp", |ui| {
                    if ui.button("Hướng dẫn").clicked() { ui.close_menu(); self.help_open = true; }
                    if ui.button("Giới thiệu").clicked() { ui.close_menu(); self.about_open = true; }
                });
            });
            ui.horizontal_wrapped(|ui| {
                if ui.button("📂 Mở project").clicked() { self.open_project(); }
                if ui.button("💾 Lưu project").clicked() { self.save_project(false); }
                ui.separator();
                if ui.button("➕ Thêm ảnh").clicked() { self.pick_files(); }
                if ui.button("📁 Thư mục").clicked() { self.pick_folder(); }
                if ui.button("✖ Xóa ảnh đang xem").clicked() { self.remove_current(); }
                ui.separator();
                if ui.button(egui::RichText::new("▶ Đo ảnh đang xem (F5)").strong()).clicked() {
                    let ids: Vec<u64> = self.cur.into_iter().collect();
                    self.queue(ids, true);
                }
                if ui.button("⏩ Đo tất cả").clicked() {
                    let ids = self.items.iter().map(|i| i.id).collect();
                    self.queue(ids, true);
                }
                ui.separator();
                if ui.button(egui::RichText::new("📄 Báo cáo PDF…").strong()).clicked() { self.open_report(); }
                if ui.button("💾 CSV…").clicked() { self.export_csv(); }
                if ui.button("🖼 Lưu ảnh KQ…").clicked() { self.save_overlays(); }
            });
        });
    }

    fn pick_files(&mut self) {
        if let Some(f) = rfd::FileDialog::new().set_title("Chọn ảnh kim tương").add_filter("Ảnh", &IMG_EXT).pick_files() {
            self.add_paths(f);
        }
    }
    fn pick_folder(&mut self) {
        if let Some(d) = rfd::FileDialog::new().set_title("Chọn thư mục ảnh").pick_folder() {
            self.add_paths(vec![d]);
        }
    }
    fn open_report(&mut self) {
        if self.included().is_empty() {
            info(APP_NAME, "Chưa có ảnh nào đã đo có đánh dấu «Báo cáo». Hãy đo trước khi lập báo cáo.");
            return;
        }
        let r = &mut self.report;
        if r.info.date.is_empty() {
            r.info.date = chrono::Local::now().format("%d/%m/%Y").to_string();
        }
        r.methods = [self.view == Method::Abrams, self.view == Method::Heyn, self.view == Method::Mixed];
        r.open = true;
    }

    fn right_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("right").resizable(true).default_width(370.0).min_width(330.0).show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::CollapsingHeader::new(egui::RichText::new("1. Thang đo (scale)").strong()).default_open(true).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("🔍 Tự tìm thanh đỏ").clicked() { self.auto_bar(); }
                        if ui.button("📏 Chọn 2 điểm").clicked() && self.cur.is_some() {
                            self.pick = Some(vec![]);
                            self.edit = false;
                            self.status = "Click vào 2 đầu của thanh scale (Esc để hủy).".into();
                        }
                    });
                    egui::Grid::new("scale").num_columns(2).show(ui, |ui| {
                        ui.label("Độ dài thanh (px):");
                        ui.add(egui::TextEdit::singleline(&mut self.px_s).desired_width(90.0));
                        ui.end_row();
                        ui.label("Độ dài thực (µm):");
                        let r = ui.add(egui::TextEdit::singleline(&mut self.um_s).desired_width(90.0));
                        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) { self.apply_scale(false); }
                        ui.end_row();
                    });
                    if let (Some(px), Some(um)) = (parse_f(&self.px_s), parse_f(&self.um_s)) {
                        ui.label(egui::RichText::new(format!("= {:.5} µm/px", um / px)).strong());
                    }
                    if ui.add_sized([ui.available_width(), 26.0], egui::Button::new(egui::RichText::new("✔ Áp dụng cho ảnh này").strong())).clicked() {
                        self.apply_scale(false);
                    }
                    if ui.add_sized([ui.available_width(), 24.0], egui::Button::new("✔✔ Áp dụng cho các ảnh có đánh dấu «Báo cáo»")).clicked() {
                        self.apply_scale(true);
                    }
                    ui.separator();
                    egui::Grid::new("scale2").num_columns(3).show(ui, |ui| {
                        ui.label("Hoặc nhập µm/px:");
                        ui.add(egui::TextEdit::singleline(&mut self.direct_s).desired_width(80.0));
                        if ui.button("Đặt").clicked() { self.apply_direct(); }
                        ui.end_row();
                        ui.label("Hoặc Ø trường nhìn (µm):");
                        ui.add(egui::TextEdit::singleline(&mut self.fov_s).desired_width(80.0));
                        if ui.button("Đặt").clicked() { self.apply_fov(); }
                        ui.end_row();
                    });
                    let st = match self.cur_item() {
                        None => "Chưa chọn ảnh.".to_string(),
                        Some(it) => match it.um_per_px {
                            Some(u) => format!("Scale: {u:.5} µm/px · {}", if it.calib.is_empty() { "—" } else { &it.calib }),
                            None => "Ảnh chưa có scale: «Tự tìm thanh đỏ» hoặc «Chọn 2 điểm», nhập µm rồi Áp dụng.".into(),
                        },
                    };
                    ui.label(egui::RichText::new(st).color(Color32::from_rgb(0, 140, 70)));
                });
                egui::CollapsingHeader::new(egui::RichText::new("2. Phương pháp và tham số đo").strong()).default_open(true).show(ui, |ui| {
                    for (k, m) in METHODS.iter().enumerate() {
                        if ui.checkbox(&mut self.m_on[k], method_vi(*m)).changed() { self.set_dirty(); }
                    }
                    ui.checkbox(&mut self.mixed_hv, "   Chord HH: thêm 2 đường 100 mm (Fig. 5 đầy đủ)");
                    ui.horizontal(|ui| {
                        ui.label("Số đường Heyn mỗi hướng:");
                        ui.add(egui::DragValue::new(&mut self.n_lines).range(3..=12));
                    });
                    ui.add(egui::Slider::new(&mut self.sens, 70.0..=95.0).step_by(1.0).text("Độ nhạy tách biên"));
                    ui.label(egui::RichText::new("↑ tăng nếu đếm dư nhiễu · ↓ giảm nếu sót biên nhạt").weak());
                    ui.checkbox(&mut self.focus, "Loại vùng mờ / nhòe khỏi phép đo");
                });
                egui::CollapsingHeader::new(egui::RichText::new("3. Xử lý ảnh (sáng / tương phản)").strong()).default_open(true).show(ui, |ui| {
                    let before = self.enh.clone();
                    ui.add(egui::Slider::new(&mut self.enh.brightness, -100.0..=100.0).step_by(1.0).text("Độ sáng"));
                    ui.add(egui::Slider::new(&mut self.enh.contrast, 0.3..=3.0).step_by(0.01).text("Tương phản"));
                    ui.add(egui::Slider::new(&mut self.enh.gamma, 0.3..=3.0).step_by(0.01).text("Gamma"));
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.enh.clahe, "CLAHE (cân bằng sáng cục bộ)");
                        if ui.button("Đặt lại").clicked() { self.enh = Enhance::default(); }
                    });
                    if ui.add_sized([ui.available_width(), 24.0], egui::Button::new("✨ Tự động chỉnh cho rõ biên hạt")).clicked() {
                        if let (Some((_, img)), Some(it)) = (&self.cur_img, self.cur_item()) {
                            self.enh = enhance::auto(img, it.fov);
                            self.status = format!("Đã tự chỉnh: {}. Bật «Dùng ảnh đã xử lý để tách biên» rồi đo lại nếu muốn áp dụng cho phép đo.", self.enh.describe());
                        }
                    }
                    ui.checkbox(&mut self.enh_detect, "Dùng ảnh đã xử lý để tách biên tự động");
                    if self.enh != before { self.set_dirty(); }
                });
                egui::CollapsingHeader::new(egui::RichText::new("4. Màu hiển thị").strong()).default_open(true).show(ui, |ui| {
                    egui::Grid::new("colors").num_columns(4).show(ui, |ui| {
                        let mut ch = false;
                        ch |= egui::color_picker::color_edit_button_srgb(ui, &mut self.colors.chord).changed();
                        ui.label("Đường chord");
                        ch |= egui::color_picker::color_edit_button_srgb(ui, &mut self.colors.point).changed();
                        ui.label("Giao điểm tự động");
                        ui.end_row();
                        ch |= egui::color_picker::color_edit_button_srgb(ui, &mut self.colors.manual).changed();
                        ui.label("Giao điểm thủ công");
                        ch |= egui::color_picker::color_edit_button_srgb(ui, &mut self.colors.boundary).changed();
                        ui.label("Biên hạt");
                        ui.end_row();
                        if ch { self.set_dirty(); }
                    });
                });
                egui::CollapsingHeader::new(egui::RichText::new("5. Sửa giao điểm thủ công").strong()).default_open(true).show(ui, |ui| {
                    if ui.checkbox(&mut self.edit, "✎ Bật chế độ sửa (phím E)").changed() { self.pick = None; }
                    ui.radio_value(&mut self.kind, Kind::Cross, "Cắt biên (1)");
                    ui.radio_value(&mut self.kind, Kind::Triple, "Điểm ba hạt (1,5)");
                    ui.radio_value(&mut self.kind, Kind::Endpoint, "Đầu mút (0,5)");
                    ui.label(egui::RichText::new("Click trái gần đường chord: thêm điểm (tự hít vào chord).\nClick phải lên điểm: loại điểm.\nShift + click trái: khôi phục điểm tự động đã loại.\nCtrl+Z: hoàn tác.").weak());
                    ui.horizontal(|ui| {
                        if ui.button("↶ Hoàn tác").clicked() { self.undo(); }
                        if ui.button("Về kết quả tự động").clicked() { self.reset_edits(); }
                    });
                });
            });
        });
    }

    fn bottom_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
                if self.pending > 0 {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add(egui::ProgressBar::new(self.done as f32 / (self.done + self.pending) as f32).desired_width(220.0)
                            .text(format!("{}/{}", self.done, self.done + self.pending)));
                    });
                }
            });
        });
        egui::TopBottomPanel::bottom("table").resizable(true).default_height(285.0).min_height(140.0).show(ctx, |ui| {
            // thống kê gộp
            let its = self.included();
            let mut lines = vec![];
            for m in METHODS {
                let l: Vec<f64> = its.iter().filter_map(|i| i.res(m).and_then(|r| r.lbar_um)).collect();
                if l.is_empty() {
                    continue;
                }
                let s = stats_with_checks(&l, &[], m);
                let mut t = format!("{:<9} n={}: ℓ tb = {:.2} µm", method_short(m), s.n, s.l_mean);
                if let (Some(ci), Some(ra)) = (s.ci, s.ra) {
                    t += &format!(" ± {ci:.2} (95%CI, %RA {ra:.1}%)");
                }
                t += &format!(" → G = {:.2}", s.g);
                lines.push(t);
            }
            ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 185.0), egui::Layout::left_to_right(egui::Align::Min), |ui| {
                ui.vertical(|ui| {
                    ui.set_width((ui.available_width() - 390.0).max(400.0));
                    ui.set_min_height(180.0);
                    self.table(ui);
                });
                ui.separator();
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("Kết quả ảnh đang xem").strong());
                    self.result_text(ui);
                });
            });
            if !lines.is_empty() {
                ui.separator();
                for l in lines {
                    ui.label(egui::RichText::new(l).strong().color(Color32::from_rgb(31, 59, 91)));
                }
                ui.label(egui::RichText::new("(gộp các ảnh có đánh dấu «Báo cáo»; chỉ có ý nghĩa với cùng mẫu, cùng độ phóng đại)").weak());
            }
        });
    }

    fn table(&mut self, ui: &mut egui::Ui) {
        let mut select = None;
        let mut toggled = false;
        egui::ScrollArea::both().max_height(175.0).auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("items").striped(true).min_col_width(40.0).show(ui, |ui| {
                let all = !self.items.is_empty() && self.items.iter().all(|i| i.include);
                let mut all2 = all;
                if ui.checkbox(&mut all2, "BC").on_hover_text("Đưa vào báo cáo / thống kê (bấm để chọn hoặc bỏ tất cả)").changed() {
                    self.items.iter_mut().for_each(|i| i.include = all2);
                    toggled = true;
                }
                for h in ["Ảnh", "µm/px", "Abrams P", "Abrams G", "Heyn P", "Heyn G", "Chord HH P", "Chord HH G", "Trạng thái / ghi chú"] {
                    ui.label(egui::RichText::new(h).strong());
                }
                ui.end_row();
                for it in self.items.iter_mut() {
                    if ui.checkbox(&mut it.include, "").changed() {
                        toggled = true;
                    }
                    if ui.selectable_label(Some(it.id) == self.cur, &it.name).clicked() {
                        select = Some(it.id);
                    }
                    ui.label(it.um_per_px.map(|u| format!("{u:.4}")).unwrap_or("–".into()));
                    for m in METHODS {
                        let r = it.data.as_ref().and_then(|a| a.method(m)).zip(it.um_per_px).map(|(r, u)| summarize(r, u));
                        ui.label(r.as_ref().map(|r| format!("{}", r.p)).unwrap_or("–".into()));
                        ui.label(r.and_then(|r| r.g).map(|g| format!("{g:.2}")).unwrap_or("–".into()));
                    }
                    let ed: Vec<&str> = it.edited().into_iter().map(method_short).collect();
                    ui.label(format!("{}{}{}", it.state, if ed.is_empty() { String::new() } else { format!(" · sửa TC: {}", ed.join(", ")) },
                                     if it.note.is_empty() { String::new() } else { format!(" · {}", it.note) }));
                    ui.end_row();
                }
            });
        });
        if toggled {
            self.set_dirty();
        }
        if let Some(id) = select {
            if Some(id) != self.cur {
                self.select(id);
            }
        }
    }

    fn result_text(&self, ui: &mut egui::Ui) {
        let Some(it) = self.cur_item() else { return };
        ui.label(egui::RichText::new(&it.name).monospace().strong());
        match it.res(self.view) {
            None => {
                ui.label(if it.um_per_px.is_some() { "Chưa đo. Bấm «Đo ảnh đang xem» (F5)." } else { "Chưa có scale." });
            }
            Some(r) => {
                ui.label(egui::RichText::new(format!("{} · {:.4} µm/px", method_vi(self.view), it.um_per_px.unwrap())).monospace());
                if let (Some(g), Some(l)) = (r.g, r.lbar_um) {
                    ui.label(egui::RichText::new(format!("G = {g:.2}  (≈ {:.1})", round_g(g))).size(18.0).strong().color(Color32::from_rgb(0, 150, 80)));
                    ui.label(egui::RichText::new(format!("ℓ = {l:.2} µm   L = {:.3} mm   P = {}", r.l_mm, r.p)).monospace());
                } else {
                    ui.label(egui::RichText::new("Không có giao điểm.").color(Color32::from_rgb(200, 100, 0)));
                }
                ui.label(egui::RichText::new(format!("cắt {} · 3 hạt {} · đầu mút {} · thêm TC {} · loại {}", r.n_cross, r.n_triple,
                                                     r.n_end, r.n_manual, r.n_removed)).monospace());
                if let (Some(h), Some(v)) = (r.l_h, r.l_v) {
                    ui.label(egui::RichText::new(format!("ℓ ngang/dọc = {h:.1}/{v:.1} µm (tỉ lệ {:.2})", h / v)).monospace());
                }
                if r.p < 50.0 {
                    ui.label(egui::RichText::new("⚠ P < 50: ít giao điểm (E112 §13.1)").color(Color32::from_rgb(200, 100, 0)));
                }
                if self.view != Method::Heyn && r.p > 150.0 {
                    ui.label(egui::RichText::new("⚠ P > 150: nên giảm độ phóng đại (E1382 §12.4.2)").color(Color32::from_rgb(200, 100, 0)));
                }
            }
        }
    }

    fn update_texture(&mut self, ctx: &egui::Context) {
        let Some(it) = self.cur_item() else { return };
        let key = format!("{}|{:?}|{}|{:?}|{}", it.id, self.enh, self.show_bound, self.colors.boundary, self.data_ver);
        if key == self.disp_key && self.tex.is_some() {
            return;
        }
        let Some((cid, img)) = &self.cur_img else { return };
        if *cid != it.id {
            return;
        }
        let mut base = enhance::apply(img, &self.enh);
        if let Some(a) = &it.data {
            let dm = dark_mask(a);
            for (i, &d) in dm.d.iter().enumerate() {
                if d {
                    for c in 0..3 {
                        base.d[3 * i + c] = (base.d[3 * i + c] as f32 * 0.4) as u8;
                    }
                }
            }
            if self.show_bound {
                for (i, &s) in a.skeleton.d.iter().enumerate() {
                    if s {
                        base.d[3 * i..3 * i + 3].copy_from_slice(&self.colors.boundary);
                    }
                }
            }
        }
        let ci = egui::ColorImage::from_rgb([base.w, base.h], &base.d);
        match &mut self.tex {
            Some(t) => t.set(ci, egui::TextureOptions::LINEAR),
            None => self.tex = Some(ctx.load_texture("image", ci, egui::TextureOptions::LINEAR)),
        }
        self.disp_key = key;
    }

    fn viewer(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Phương pháp:");
                for m in METHODS {
                    ui.radio_value(&mut self.view, m, method_short(m));
                }
                ui.separator();
                ui.checkbox(&mut self.show_bound, "Biên hạt");
                ui.checkbox(&mut self.show_pat, "Lưới & giao điểm");
                ui.checkbox(&mut self.show_removed, "Điểm đã loại");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Vừa khung").clicked() { self.fit_pending = true; }
                    if ui.button("1:1").clicked() { self.zoom = 1.0; }
                });
            });
            let hint = if self.edit {
                "CHẾ ĐỘ SỬA: click trái gần chord = thêm điểm · click phải lên điểm = loại · Shift+click = khôi phục · Ctrl+Z = hoàn tác · E = thoát."
            } else if self.pick.is_some() {
                "Click vào 2 đầu của thanh scale (Esc để hủy)."
            } else {
                "Kéo ảnh vào cửa sổ. Lăn chuột: phóng to · kéo: di chuyển · nhấp đúp: vừa khung."
            };
            self.update_texture(ctx);
            let avail = ui.available_size() - Vec2::new(0.0, 20.0);
            let (resp, painter) = ui.allocate_painter(avail, egui::Sense::click_and_drag());
            let rect = resp.rect;
            painter.rect_filled(rect, 0.0, Color32::from_gray(32));
            ui.label(egui::RichText::new(hint).weak());
            let Some(tex) = self.tex.clone() else { return };
            if self.cur.is_none() {
                return;
            }
            let (iw, ih) = (self.img_size.0 as f32, self.img_size.1 as f32);
            if self.fit_pending || resp.double_clicked() {
                self.zoom = (rect.width() / iw).min(rect.height() / ih);
                self.off = Vec2::new((iw - rect.width() / self.zoom) / 2.0, (ih - rect.height() / self.zoom) / 2.0);
                self.fit_pending = false;
            }
            // phóng to quanh con trỏ
            if resp.hovered() {
                let scroll = ui.input(|i| i.raw_scroll_delta.y);
                if scroll != 0.0 {
                    if let Some(p) = resp.hover_pos() {
                        let f = if scroll > 0.0 { 1.18 } else { 1.0 / 1.18 };
                        let nz = (self.zoom * f).clamp(0.02, 24.0);
                        let ip = self.off + (p - rect.min) / self.zoom;
                        self.zoom = nz;
                        self.off = ip - (p - rect.min) / nz;
                    }
                }
            }
            if resp.dragged_by(egui::PointerButton::Primary) || resp.dragged_by(egui::PointerButton::Middle) {
                self.off -= resp.drag_delta() / self.zoom;
            }
            let (off0, z0) = (self.off, self.zoom);
            let to_scr = move |x: f64, y: f64| -> Pos2 { rect.min + (Vec2::new(x as f32, y as f32) - off0) * z0 };
            let to_img = move |p: Pos2| -> (f64, f64) { let v = off0 + (p - rect.min) / z0; (v.x as f64, v.y as f64) };
            // click
            if resp.clicked() {
                if let Some(p) = resp.interact_pointer_pos() {
                    let (x, y) = to_img(p);
                    if let Some(pts) = self.pick.as_mut() {
                        pts.push((x, y));
                        if pts.len() == 2 {
                            let (a, b) = (pts[0], pts[1]);
                            self.px_s = format!("{:.1}", (b.0 - a.0).hypot(b.1 - a.1));
                            if let Some(it) = self.cur_mut() {
                                it.bar_box = Some([a.0.min(b.0) as usize, a.1.min(b.1) as usize, (a.0 - b.0).abs() as usize + 1, (a.1 - b.1).abs() as usize + 1]);
                            }
                            self.status = "Đã đo thanh scale. Nhập độ dài thực (µm) rồi bấm Áp dụng.".into();
                        }
                    } else if self.edit {
                        let shift = ui.input(|i| i.modifiers.shift);
                        self.edit_at(if shift { 2 } else { 0 }, x, y);
                    }
                }
            }
            if resp.secondary_clicked() && self.edit {
                if let Some(p) = resp.interact_pointer_pos() {
                    let (x, y) = to_img(p);
                    self.edit_at(1, x, y);
                }
            }
            if self.pick.as_ref().map_or(false, |p| p.len() >= 2) && resp.clicked() {
                // giữ 2 điểm để vẽ, thoát chế độ chọn ở khung hình sau
            }
            // vẽ ảnh
            let img_rect = Rect::from_min_max(to_scr(0.0, 0.0), to_scr(iw as f64, ih as f64));
            let painter = painter.with_clip_rect(rect);
            painter.image(tex.id(), img_rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            let Some(it) = self.cur_item() else { return };
            if it.data.is_none() {
                if let Some((cx, cy, r)) = it.fov {
                    painter.circle_stroke(to_scr(cx, cy), r as f32 * self.zoom, Stroke::new(1.0_f32, Color32::from_rgb(0, 188, 212)));
                }
            }
            if let Some(b) = it.bar_box {
                if it.data.is_none() || self.pick.is_some() {
                    let r = Rect::from_min_max(to_scr(b[0] as f64, b[1] as f64), to_scr((b[0] + b[2]) as f64, (b[1] + b[3]) as f64)).expand(2.0);
                    painter.rect_stroke(r, 0.0, Stroke::new(2.0_f32, Color32::from_rgb(0, 230, 118)));
                    painter.text(r.left_top(), egui::Align2::LEFT_BOTTOM, "thanh scale", egui::FontId::proportional(12.0), Color32::from_rgb(0, 230, 118));
                }
            }
            if let (Some(a), true) = (it.data.as_ref(), self.show_pat) {
                if let Some(r) = a.method(self.view) {
                    let cs = Stroke::new(if self.zoom >= 0.5 { 2.0_f32 } else { 1.2 }, c32(self.colors.chord));
                    for ch in &r.chords {
                        match ch {
                            Chord::Circle { cx, cy, r, .. } => { painter.circle_stroke(to_scr(*cx, *cy), *r as f32 * self.zoom, cs); }
                            Chord::Line { a, b, .. } => { painter.line_segment([to_scr(a.0, a.1), to_scr(b.0, b.1)], cs); }
                        }
                    }
                    let rad = if self.zoom < 1.5 { 4.0 } else { 5.0 };
                    for e in &r.events {
                        let p = to_scr(e.x, e.y);
                        if e.removed {
                            if self.show_removed || self.edit {
                                let g = Stroke::new(2.0_f32, Color32::GRAY);
                                painter.line_segment([p + Vec2::new(-rad, -rad), p + Vec2::new(rad, rad)], g);
                                painter.line_segment([p + Vec2::new(-rad, rad), p + Vec2::new(rad, -rad)], g);
                            }
                            continue;
                        }
                        let c = c32(if e.manual { self.colors.manual } else { self.colors.point });
                        match e.kind {
                            Kind::Triple => {
                                painter.circle_stroke(p, rad + 3.0, Stroke::new(2.0_f32, c));
                                painter.circle_filled(p, rad - 1.0, c);
                            }
                            Kind::Endpoint => { painter.circle_stroke(p, rad + 1.0, Stroke::new(2.0_f32, c)); }
                            Kind::Cross if e.manual => {
                                let rr = Rect::from_center_size(p, Vec2::splat(2.0 * rad));
                                painter.rect_filled(rr, 0.0, c);
                                painter.rect_stroke(rr, 0.0, Stroke::new(1.0_f32, Color32::WHITE));
                            }
                            Kind::Cross => { painter.circle_filled(p, rad, c); }
                        }
                    }
                }
            }
            if let Some(pts) = &self.pick {
                for &(x, y) in pts {
                    painter.circle_filled(to_scr(x, y), 4.0, Color32::from_rgb(255, 23, 68));
                }
                if pts.len() == 2 {
                    painter.line_segment([to_scr(pts[0].0, pts[0].1), to_scr(pts[1].0, pts[1].1)], Stroke::new(2.0_f32, Color32::YELLOW));
                }
            }
            if self.pick.as_ref().map_or(false, |p| p.len() >= 2) && !resp.clicked() {
                self.pick = None;
            }
            if self.edit {
                ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
            }
        });
    }

    fn windows(&mut self, ctx: &egui::Context) {
        if self.report.open {
            let mut open = true;
            let mut action = 0;
            egui::Window::new("Lập báo cáo PDF (A4)").collapsible(false).resizable(false).open(&mut open)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
                let inf = &mut self.report.info;
                egui::Grid::new("rep").num_columns(2).show(ui, |ui| {
                    for (lab, v) in [("Số báo cáo", &mut inf.report_no), ("Ngày đo", &mut inf.date), ("Khách hàng / Dự án", &mut inf.client),
                                     ("Mã mẫu", &mut inf.sample), ("Vật liệu / Mác", &mut inf.material), ("Vị trí, hướng mặt cắt", &mut inf.location),
                                     ("Chuẩn bị mẫu, tẩm thực", &mut inf.prep), ("Thiết bị (kính HV, camera)", &mut inf.equipment),
                                     ("Độ phóng đại", &mut inf.magnification), ("Người thực hiện", &mut inf.operator),
                                     ("Người kiểm tra", &mut inf.reviewer), ("Phê duyệt", &mut inf.approver), ("Ghi chú", &mut inf.notes)] {
                        ui.label(format!("{lab}:"));
                        ui.add(egui::TextEdit::singleline(v).desired_width(380.0));
                        ui.end_row();
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Phương pháp báo cáo:");
                    for (k, m) in METHODS.iter().enumerate() {
                        ui.checkbox(&mut self.report.methods[k], method_short(*m));
                    }
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.report.images, "Kèm ảnh minh hoạ");
                    ui.label("   Số ảnh / trang:");
                    for n in [1usize, 2, 4] {
                        ui.radio_value(&mut self.report.per_page, n, n.to_string());
                    }
                });
                ui.label(egui::RichText::new(format!("Gồm {} ảnh đã đo có đánh dấu «Báo cáo». Kết quả chính theo phương pháp đang xem ({}) nếu được chọn.",
                                                     self.items.iter().filter(|i| i.include && i.data.is_some()).count(), method_short(self.view))).weak());
                ui.horizontal(|ui| {
                    if ui.button("👁 Xem trước").clicked() { action = 1; }
                    if ui.button(egui::RichText::new("📄 Xuất PDF…").strong()).clicked() { action = 2; }
                    if ui.button("Hủy").clicked() { action = 3; }
                });
            });
            if !open || action == 3 {
                self.report.open = false;
            }
            if action == 1 || action == 2 {
                if !self.report.methods.iter().any(|v| *v) {
                    info(APP_NAME, "Chọn ít nhất một phương pháp.");
                } else {
                    let path = if action == 1 {
                        Some(std::env::temp_dir().join(format!("GSD_preview_{}.pdf", std::process::id())).to_string_lossy().to_string())
                    } else {
                        let name = format!("BaoCao_CoHat_{}.pdf", if self.report.info.sample.is_empty() { "mau".into() } else { self.report.info.sample.replace(' ', "_") });
                        rfd::FileDialog::new().set_title("Lưu báo cáo PDF").add_filter("PDF", &["pdf"]).set_file_name(&name).save_file()
                            .map(|p| p.to_string_lossy().to_string())
                    };
                    if let Some(p) = path {
                        match self.make_report(&p) {
                            Ok(()) => {
                                open_with_system(&p);
                                if action == 2 {
                                    self.status = format!("Đã lưu báo cáo: {p}");
                                    self.report.open = false;
                                }
                                self.settings["report"] = self.report_json();
                                self.save_settings();
                            }
                            Err(e) => info(APP_NAME, &format!("Lỗi khi lập báo cáo: {e}")),
                        }
                    }
                }
            }
        }
        if self.help_open {
            egui::Window::new("Hướng dẫn").open(&mut self.help_open).default_width(560.0).show(ctx, |ui| {
                ui.label("1. Kéo ảnh (hoặc thư mục, hoặc tệp .gsd) vào cửa sổ, hoặc bấm «Thêm ảnh».");
                ui.label("2. Chọn ảnh trong bảng, thiết lập thang đo: «Tự tìm thanh đỏ» hoặc «Chọn 2 điểm» trên thanh scale, nhập µm, bấm Áp dụng (Enter) – ảnh được đo ngay.");
                ui.label("3. Chọn phương pháp xem (Abrams / Heyn / Chord HH). Chỉnh sáng, tương phản, hoặc «Tự động» để nhìn rõ biên.");
                ui.label("4. Rà soát: phím E bật chế độ sửa. Click trái gần chord để thêm giao điểm (cắt biên 1, điểm ba hạt 1,5, đầu mút 0,5); click phải để loại; Shift+click để khôi phục; Ctrl+Z hoàn tác.");
                ui.label("5. Cột «BC»: ảnh được đưa vào thống kê, báo cáo, CSV. Nên đo ≥ 5 trường, mỗi trường ≥ 50 giao điểm (E112).");
                ui.label("6. «Báo cáo PDF» để lập báo cáo A4 (một hoặc nhiều phương pháp); «Lưu project» để làm tiếp sau (tương thích bản Python).");
                ui.label("G = −6,643856·log10(ℓ[mm]) − 3,288 (ASTM E112-25 Bảng 6).");
            });
        }
        if self.about_open {
            egui::Window::new("Giới thiệu").open(&mut self.about_open).show(ctx, |ui| {
                ui.label(egui::RichText::new(format!("{APP_NAME} Rev {APP_VER}")).strong());
                ui.label("Đo cấp cỡ hạt trung bình theo ASTM E112-25 / E1382-97(2023).");
                ui.label("Phương pháp: Abrams, Heyn, Chord hỗn hợp (Fig. 5). Giao diện Rust (egui), lõi tính gsd_core.");
            });
        }
    }
}
