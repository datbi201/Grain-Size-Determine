//! Báo cáo PDF A4 (printpdf) – cùng bố cục với bản Python (grain_report.py):
//! tiêu đề, thông tin mẫu, phương pháp, tóm tắt, chi tiết từng phương pháp, đánh giá, chữ ký, ảnh minh hoạ.
use crate::model::{method_short, method_vi};
use gsd_core::*;
use printpdf::*;
use std::io::BufWriter;

pub struct Field {
    pub name: String,
    pub um_per_px: f64,
    pub calib: String,
    pub res: Summary,
    pub jpeg: Option<(Vec<u8>, usize, usize)>,
}

#[derive(Default, Clone)]
pub struct Info {
    pub report_no: String,
    pub date: String,
    pub client: String,
    pub sample: String,
    pub material: String,
    pub location: String,
    pub prep: String,
    pub equipment: String,
    pub magnification: String,
    pub operator: String,
    pub reviewer: String,
    pub approver: String,
    pub notes: String,
}

pub struct RParams {
    pub sensitivity: f64,
    pub focus: bool,
    pub n_lines: usize,
    pub mixed_hv: bool,
    pub enhance_display: String,
    pub enhance_detect: String,
}

const PW: f64 = 210.0;
const PH: f64 = 297.0;
const ML: f64 = 18.0;
const MR: f64 = 18.0;
const MT: f64 = 15.0;
const MB: f64 = 22.0;
const NAVY: (f64, f64, f64) = (0.12, 0.23, 0.36);
const LIGHT: (f64, f64, f64) = (0.91, 0.93, 0.96);
const GRID: (f64, f64, f64) = (0.60, 0.66, 0.72);

fn num(v: Option<f64>, d: usize) -> String {
    v.map(|x| format!("{x:.d$}").replace('.', ",")).unwrap_or_else(|| "–".into())
}

struct Fonts {
    reg: IndirectFontRef,
    bold: IndirectFontRef,
    ital: IndirectFontRef,
    m_reg: Vec<u8>,
    m_bold: Vec<u8>,
}

struct Doc {
    doc: PdfDocumentReference,
    pages: Vec<(PdfPageIndex, PdfLayerIndex)>,
    y: f64, // khoảng cách từ mép trên (mm)
    f: Fonts,
}

#[derive(Clone, Copy, PartialEq)]
enum Style {
    Reg,
    Bold,
    Ital,
}

impl Doc {
    fn layer(&self) -> PdfLayerReference {
        let (p, l) = *self.pages.last().unwrap();
        self.doc.get_page(p).get_layer(l)
    }
    fn new_page(&mut self) {
        let (p, l) = self.doc.add_page(Mm(PW as f32), Mm(PH as f32), "Layer 1");
        self.pages.push((p, l));
        self.y = MT;
    }
    fn ensure(&mut self, h: f64) {
        if self.y + h > PH - MB {
            self.new_page();
        }
    }
    fn width(&self, s: &str, size: f64, st: Style) -> f64 {
        let data = if st == Style::Bold { &self.f.m_bold } else { &self.f.m_reg };
        let face = ttf_parser::Face::parse(data, 0).unwrap();
        let upm = face.units_per_em() as f64;
        let w: f64 = s.chars().map(|c| face.glyph_index(c).and_then(|g| face.glyph_hor_advance(g)).unwrap_or(500) as f64).sum();
        w / upm * size * 0.352778
    }
    fn wrap(&self, s: &str, size: f64, st: Style, maxw: f64) -> Vec<String> {
        let mut lines = vec![];
        for para in s.split('\n') {
            let mut cur = String::new();
            for word in para.split(' ') {
                let cand = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
                if self.width(&cand, size, st) > maxw && !cur.is_empty() {
                    lines.push(cur);
                    cur = word.to_string();
                } else {
                    cur = cand;
                }
            }
            lines.push(cur);
        }
        lines
    }
    fn text(&self, s: &str, x: f64, y_top: f64, size: f64, st: Style, rgb: (f64, f64, f64)) {
        let l = self.layer();
        l.set_fill_color(Color::Rgb(printpdf::Rgb::new(rgb.0 as f32, rgb.1 as f32, rgb.2 as f32, None)));
        let font = match st {
            Style::Reg => &self.f.reg,
            Style::Bold => &self.f.bold,
            Style::Ital => &self.f.ital,
        };
        l.use_text(s, size as f32, Mm(x as f32), Mm((PH - y_top - size * 0.352778 * 0.8) as f32), font);
    }
    fn rect(&self, x: f64, y: f64, w: f64, h: f64, fill: Option<(f64, f64, f64)>, stroke: Option<((f64, f64, f64), f64)>) {
        let l = self.layer();
        let pts = vec![
            (Point::new(Mm(x as f32), Mm((PH - y) as f32)), false),
            (Point::new(Mm((x + w) as f32), Mm((PH - y) as f32)), false),
            (Point::new(Mm((x + w) as f32), Mm((PH - y - h) as f32)), false),
            (Point::new(Mm(x as f32), Mm((PH - y - h) as f32)), false),
        ];
        if let Some(c) = fill {
            l.set_fill_color(Color::Rgb(printpdf::Rgb::new(c.0 as f32, c.1 as f32, c.2 as f32, None)));
            l.add_polygon(Polygon { rings: vec![pts.clone()], mode: path::PaintMode::Fill, winding_order: path::WindingOrder::NonZero });
        }
        if let Some((c, th)) = stroke {
            l.set_outline_color(Color::Rgb(printpdf::Rgb::new(c.0 as f32, c.1 as f32, c.2 as f32, None)));
            l.set_outline_thickness(th as f32);
            l.add_line(Line { points: pts, is_closed: true });
        }
    }
    fn hline(&self, x1: f64, x2: f64, y: f64, c: (f64, f64, f64), th: f64) {
        let l = self.layer();
        l.set_outline_color(Color::Rgb(printpdf::Rgb::new(c.0 as f32, c.1 as f32, c.2 as f32, None)));
        l.set_outline_thickness(th as f32);
        l.add_line(Line { points: vec![(Point::new(Mm(x1 as f32), Mm((PH - y) as f32)), false),
                                       (Point::new(Mm(x2 as f32), Mm((PH - y) as f32)), false)], is_closed: false });
    }
    fn heading(&mut self, s: &str) {
        self.ensure(12.0);
        self.y += 3.0;
        self.text(s, ML, self.y, 11.0, Style::Bold, NAVY);
        self.y += 6.5;
    }
    fn para(&mut self, s: &str, size: f64, st: Style, rgb: (f64, f64, f64)) {
        let lh = size * 0.352778 * 1.35;
        for line in self.wrap(s, size, st, PW - ML - MR) {
            self.ensure(lh);
            self.text(&line, ML, self.y, size, st, rgb);
            self.y += lh;
        }
    }
    /// Bảng: widths (mm), dòng đầu là tiêu đề nếu header = true; bold_cols: cột in đậm/nền nhạt (khoá–giá trị).
    fn table(&mut self, widths: &[f64], rows: &[Vec<String>], header: bool, key_cols: &[usize], center_from: usize,
             colors: Option<&[Option<(f64, f64, f64)>]>) {
        let size = 8.0;
        let lh = size * 0.352778 * 1.3;
        for (ri, row) in rows.iter().enumerate() {
            let is_head = header && ri == 0;
            let st = |ci: usize| if is_head || key_cols.contains(&ci) { Style::Bold } else { Style::Reg };
            let wrapped: Vec<Vec<String>> = row.iter().enumerate().map(|(ci, c)| self.wrap(c, size, st(ci), widths[ci] - 2.4)).collect();
            let nl = wrapped.iter().map(|w| w.len()).max().unwrap_or(1);
            let h = nl as f64 * lh + 2.2;
            if self.y + h > PH - MB {
                self.new_page();
                if header && ri > 0 {
                    // lặp lại tiêu đề bảng
                    let hr = rows[0].clone();
                    self.table(widths, &[hr], true, key_cols, center_from, None);
                }
            }
            let mut x = ML;
            for (ci, w) in widths.iter().enumerate() {
                let fill = if is_head { Some(NAVY) } else if key_cols.contains(&ci) { Some(LIGHT) }
                           else if header && ri % 2 == 0 { Some(LIGHT) } else { None };
                self.rect(x, self.y, *w, h, fill, Some((GRID, 0.3)));
                let col = if is_head { (1.0, 1.0, 1.0) } else {
                    colors.and_then(|c| c.get(ri).cloned().flatten()).filter(|_| ci == row.len() - 1).unwrap_or((0.0, 0.0, 0.0))
                };
                for (li, line) in wrapped[ci].iter().enumerate() {
                    let tw = self.width(line, size, st(ci));
                    let tx = if ci >= center_from { x + (w - tw) / 2.0 } else { x + 1.2 };
                    self.text(line, tx, self.y + 1.1 + li as f64 * lh, size, st(ci), col);
                }
                x += w;
            }
            self.y += h;
        }
    }
    fn image(&mut self, jpeg: &[u8], pw: usize, ph: usize, maxw: f64, maxh: f64, x_center: f64) -> f64 {
        let s = (maxw / pw as f64).min(maxh / ph as f64);
        let (wmm, hmm) = (pw as f64 * s, ph as f64 * s);
        self.ensure(hmm + 8.0);
        let img = printpdf::Image::from(ImageXObject {
            width: Px(pw), height: Px(ph), color_space: ColorSpace::Rgb, bits_per_component: ColorBits::Bit8,
            interpolate: true, image_data: jpeg.to_vec(), image_filter: Some(ImageFilter::DCT), smask: None,
            clipping_bbox: None,
        });
        let dpi = 300.0;
        let native_w = pw as f64 / dpi * 25.4;
        let sc = wmm / native_w;
        img.add_to_layer(self.layer(), ImageTransform {
            translate_x: Some(Mm((x_center - wmm / 2.0) as f32)), translate_y: Some(Mm((PH - self.y - hmm) as f32)),
            scale_x: Some(sc as f32), scale_y: Some(sc as f32), dpi: Some(dpi as f32), ..Default::default() });
        self.y += hmm;
        hmm
    }
}

fn method_text(m: Method, n_lines: usize, hv: bool) -> String {
    match m {
        Method::Abrams => "Abrams – ba vòng tròn đồng tâm, chu vi tỉ lệ 3:2:1 (250 / 166,7 / 83,3 mm trên tổng 500 mm của Fig. 5, E112 §14.3), đặt tại tâm trường đo.".into(),
        Method::Heyn => format!("Heyn – lưới {n_lines} đường thẳng ngang và {n_lines} đường thẳng dọc phân bố đều trên trường đo (E112 §13); có thêm ℓ theo phương ngang/dọc để đánh giá dị hướng."),
        Method::Mixed => format!("Chord hỗn hợp – mẫu lưới Fig. 5 của E112: ba vòng tròn đồng tâm (chu vi 250 / 166,7 / 83,3 đơn vị) và hai đường chéo dài 150 đơn vị qua tâm{}; mọi đoạn chắn của một trường gộp chung ℓ = ΣL/ΣP.",
                                 if hv { ", cùng hai đường 100 đơn vị ngang/dọc" } else { "" }),
    }
}

fn load_fonts(doc: &PdfDocumentReference) -> Result<Fonts, String> {
    let fdir = std::path::PathBuf::from(std::env::var("WINDIR").unwrap_or("C:\\Windows".into())).join("Fonts");
    for (r, b, i) in [("arial.ttf", "arialbd.ttf", "ariali.ttf"), ("tahoma.ttf", "tahomabd.ttf", "tahoma.ttf"),
                      ("times.ttf", "timesbd.ttf", "timesi.ttf")] {
        let (Ok(dr), Ok(db), Ok(di)) = (std::fs::read(fdir.join(r)), std::fs::read(fdir.join(b)), std::fs::read(fdir.join(i))) else { continue };
        let reg = doc.add_external_font(dr.as_slice()).map_err(|e| e.to_string())?;
        let bold = doc.add_external_font(db.as_slice()).map_err(|e| e.to_string())?;
        let ital = doc.add_external_font(di.as_slice()).map_err(|e| e.to_string())?;
        return Ok(Fonts { reg, bold, ital, m_reg: dr, m_bold: db });
    }
    Err("Không tìm thấy font Arial/Tahoma/Times trong Windows\\Fonts".into())
}

pub fn build_pdf(path: &str, info: &Info, methods: &[Method], fields: &[(Method, Vec<Field>)], params: &RParams,
                 legend: &str, per_page: usize, app_title: &str) -> Result<(), String> {
    let (doc, p1, l1) = PdfDocument::new("Báo cáo đo cỡ hạt ASTM E112", Mm(PW as f32), Mm(PH as f32), "Layer 1");
    let f = load_fonts(&doc)?;
    let mut d = Doc { doc, pages: vec![(p1, l1)], y: MT, f };
    let w = PW - ML - MR;
    let fb = |m: Method| fields.iter().find(|x| x.0 == m).map(|x| &x.1).unwrap();
    let stats: Vec<(Method, Stats)> = methods.iter().map(|&m| {
        let fs = fb(m);
        (m, stats_with_checks(&fs.iter().map(|f| f.res.lbar_um.unwrap_or(0.0)).collect::<Vec<_>>(),
                              &fs.iter().map(|f| f.res.p).collect::<Vec<_>>(), m))
    }).collect();

    // ---- tiêu đề
    let top = d.y;
    d.rect(PW - MR - 52.0, top - 1.0, 52.0, 13.0, None, Some((NAVY, 0.6)));
    d.text(&format!("Số BC: {}", if info.report_no.is_empty() { "…" } else { &info.report_no }), PW - MR - 50.0, top + 1.0, 9.0, Style::Bold, (0.0, 0.0, 0.0));
    d.text(&format!("Ngày: {}", info.date), PW - MR - 50.0, top + 6.5, 9.0, Style::Bold, (0.0, 0.0, 0.0));
    let tw = w - 56.0;
    for line in d.wrap("BÁO CÁO KẾT QUẢ XÁC ĐỊNH CỠ HẠT TRUNG BÌNH", 14.5, Style::Bold, tw) {
        d.text(&line, ML, d.y, 14.5, Style::Bold, NAVY);
        d.y += 6.6;
    }
    for line in d.wrap("Phương pháp đoạn chắn (intercept) theo ASTM E112-25; phân tích ảnh bán tự động theo ASTM E1382-97 (2023)", 8.5, Style::Reg, tw) {
        d.text(&line, ML, d.y, 8.5, Style::Reg, (0.25, 0.31, 0.38));
        d.y += 4.0;
    }
    d.y = d.y.max(top + 14.0) + 1.5;
    d.hline(ML, PW - MR, d.y, NAVY, 1.0);
    d.y += 2.0;

    // ---- 1
    let nf: usize = { let mut v: Vec<&str> = fields.iter().flat_map(|x| x.1.iter().map(|f| f.name.as_str())).collect(); v.sort(); v.dedup(); v.len() };
    d.heading("1. Thông tin mẫu và điều kiện thử");
    let dash = |s: &str| if s.is_empty() { "–".to_string() } else { s.to_string() };
    d.table(&[33.0, w / 2.0 - 33.0, 33.0, w / 2.0 - 33.0], &[
        vec!["Khách hàng / Dự án".into(), dash(&info.client), "Mã mẫu".into(), dash(&info.sample)],
        vec!["Vật liệu / Mác".into(), dash(&info.material), "Vị trí, hướng mặt cắt".into(), dash(&info.location)],
        vec!["Chuẩn bị mẫu, tẩm thực".into(), dash(&info.prep), "Thiết bị".into(), dash(&info.equipment)],
        vec!["Độ phóng đại".into(), dash(&info.magnification), "Số trường đo".into(), nf.to_string()],
    ], false, &[0, 2], 99, None);

    // ---- 2
    d.heading("2. Phương pháp thử");
    let mut calibs: Vec<String> = fields.iter().flat_map(|x| x.1.iter().map(|f| f.calib.clone())).filter(|s| !s.is_empty()).collect();
    calibs.sort();
    calibs.dedup();
    let sc: Vec<f64> = fields.iter().flat_map(|x| x.1.iter().map(|f| f.um_per_px)).collect();
    let (smin, smax) = sc.iter().fold((f64::MAX, f64::MIN), |a, &v| (a.0.min(v), a.1.max(v)));
    d.table(&[42.0, w - 42.0], &[
        vec!["Tiêu chuẩn áp dụng".into(), "ASTM E112-25 (§13 Heyn, §14.3 Abrams, Fig. 5, §15 thống kê); ASTM E1382-97 (2023)".into()],
        vec!["Lưới thử (test pattern)".into(), methods.iter().map(|&m| method_text(m, params.n_lines, params.mixed_hv)).collect::<Vec<_>>().join("\n")],
        vec!["Quy tắc đếm giao điểm P".into(), "Cắt qua biên hạt = 1; tiếp tuyến = 1; điểm ba hạt = 1,5; đầu mút đoạn thẳng chạm biên = 0,5 (E112 §13.3; E1382 §12.4.5).".into()],
        vec!["Tính toán".into(), "ℓ = L/P cho từng trường; ℓ tb = trung bình các trường; G = −6,643856·log10(ℓ tb [mm]) − 3,288 (E112 Bảng 6); 95 %CI = t·s/√n; %RA = 95 %CI/ℓ tb·100 (E112 §15).".into()],
        vec!["Hiệu chuẩn thang đo".into(), format!("{}; {:.4} – {:.4} µm/px", if calibs.is_empty() { "theo thanh tỉ lệ trên ảnh".into() } else { calibs.join(", ") }, smin, smax)],
        vec!["Phát hiện biên hạt".into(), format!("Tự động (lọc black-hat trên kênh L*, ngưỡng phân vị {:.0}, làm mảnh 1 px){}; người đo rà soát, thêm/bớt giao điểm thủ công.",
                                                  params.sensitivity, if params.focus { "; loại vùng mờ/nhòe" } else { "" })],
        vec!["Xử lý ảnh (E1382 §15.4)".into(), format!("Hiển thị: {}. Ảnh dùng để tách biên: {}.", params.enhance_display, params.enhance_detect)],
    ], false, &[0], 99, None);

    // ---- 3
    d.heading("3. Tóm tắt kết quả");
    let mut rows = vec![vec!["Phương pháp".to_string(), "n".into(), "ΣP".into(), "ℓ tb (µm)".into(), "95 %CI (µm)".into(), "%RA".into(), "G".into(), "G làm tròn".into()]];
    for (m, s) in &stats {
        let tp: f64 = fb(*m).iter().map(|f| f.res.p).sum();
        rows.push(vec![method_vi(*m).into(), s.n.to_string(), num(Some(tp), 1), num(Some(s.l_mean), 2),
                       s.ci.map(|c| format!("± {}", num(Some(c), 2))).unwrap_or("–".into()),
                       s.ra.map(|r| format!("{} %", num(Some(r), 1))).unwrap_or("–".into()), num(Some(s.g), 2), num(Some(s.g_round), 1)]);
    }
    d.table(&[62.0, 10.0, 17.0, 18.0, 22.0, 15.0, 13.0, w - 157.0], &rows, true, &[], 1, None);
    let (m0, s0) = &stats[0];
    d.ensure(18.0);
    d.y += 3.0;
    d.rect(ML, d.y, w, 15.0, Some(LIGHT), Some((NAVY, 1.0)));
    let big = format!("CẤP CỠ HẠT TRUNG BÌNH ASTM  G = {}", num(Some(s0.g_round), 1));
    let bw = d.width(&big, 13.0, Style::Bold);
    d.text(&big, ML + (w - bw) / 2.0, d.y + 2.0, 13.0, Style::Bold, NAVY);
    let sub = format!("ℓ tb = {} µm{} · theo {}{}", num(Some(s0.l_mean), 2),
                      s0.ci.map(|c| format!(" ± {} µm (95 %CI), %RA = {} %", num(Some(c), 2), num(s0.ra, 1))).unwrap_or_default(),
                      method_vi(*m0), if stats.len() > 1 { " (phương pháp chính)" } else { "" });
    let sw = d.width(&sub, 8.0, Style::Reg);
    d.text(&sub, ML + (w - sw).max(0.0) / 2.0, d.y + 9.0, 8.0, Style::Reg, (0.0, 0.0, 0.0));
    d.y += 17.0;

    // ---- 4
    for (k, (m, s)) in stats.iter().enumerate() {
        d.heading(&format!("4.{}. Kết quả từng trường – {}", k + 1, method_vi(*m)));
        let mut rows = vec![vec!["TT".to_string(), "Trường / ảnh".into(), "µm/px".into(), "L (mm)".into(), "P".into(),
                                 "Cắt / 3 hạt / Đầu mút".into(), "Thêm / Bớt TC".into(), "ℓ (µm)".into(), "G".into()]];
        for (i, f) in fb(*m).iter().enumerate() {
            let r = &f.res;
            rows.push(vec![(i + 1).to_string(), f.name.clone(), num(Some(f.um_per_px), 4), num(Some(r.l_mm), 3),
                           num(Some(r.p), 1), format!("{} / {} / {}", r.n_cross, r.n_triple, r.n_end),
                           format!("{} / {}", r.n_manual, r.n_removed), num(r.lbar_um, 2), num(r.g, 2)]);
        }
        d.table(&[8.0, 46.0, 15.0, 15.0, 13.0, 28.0, 20.0, 15.0, w - 160.0], &rows, true, &[], 2, None);
        d.y += 2.0;
        d.table(&[42.0, w / 2.0 - 42.0, 42.0, w / 2.0 - 42.0], &[
            vec!["Đại lượng".into(), "Giá trị".into(), "Đại lượng".into(), "Giá trị".into()],
            vec!["Số trường n".into(), s.n.to_string(), "Hệ số t (95 %)".into(), num(s.t, 3)],
            vec!["ℓ tb (µm)".into(), num(Some(s.l_mean), 3), "Độ lệch chuẩn s (µm)".into(), num(s.sd, 3)],
            vec!["95 %CI (µm)".into(), s.ci.map(|c| format!("± {}", num(Some(c), 3))).unwrap_or("–".into()), "%RA".into(),
                 s.ra.map(|r| format!("{} %", num(Some(r), 1))).unwrap_or("–".into())],
            vec!["G (từ ℓ tb)".into(), num(Some(s.g), 2), "Khoảng G ứng với 95 %CI".into(),
                 match (s.g_lo, s.g_hi) { (Some(a), Some(b)) => format!("{} – {}", num(Some(a), 2), num(Some(b), 2)), _ => "–".into() }],
        ], true, &[], 99, None);
        d.y += 2.0;
        let mut rows = vec![vec!["Tiêu chí theo tiêu chuẩn".to_string(), "Giá trị".into(), "Đánh giá".into()]];
        let mut cols: Vec<Option<(f64, f64, f64)>> = vec![None];
        for (name, ok, val) in &s.checks {
            rows.push(vec![name.clone(), val.clone(), if *ok { "Đạt".into() } else { "Chưa đạt".into() }]);
            cols.push(Some(if *ok { (0.11, 0.50, 0.23) } else { (0.70, 0.15, 0.12) }));
        }
        d.table(&[w - 62.0, 36.0, 26.0], &rows, true, &[], 1, Some(&cols));
    }
    d.y += 2.0;
    d.para("P tính theo trọng số; “Thêm / Bớt TC” là số giao điểm người đo thêm vào / loại bỏ so với kết quả phát hiện tự động.",
           7.5, Style::Ital, (0.31, 0.35, 0.40));
    let note = format!("Ghi chú: {}Kết quả chỉ đại diện cho các trường ảnh đã đo. Biên hạt được phát hiện tự động và đã được người đo rà soát; độ chính xác phụ thuộc chất lượng tẩm thực và ảnh (E1382 §10). G làm tròn tới 0,1.",
                       if info.notes.trim().is_empty() { String::new() } else { format!("{}. ", info.notes.trim()) });
    d.para(&note, 7.5, Style::Ital, (0.31, 0.35, 0.40));
    // ---- chữ ký
    d.ensure(30.0);
    d.y += 5.0;
    for (i, (t, n)) in [("Người thực hiện", &info.operator), ("Người kiểm tra", &info.reviewer), ("Phê duyệt", &info.approver)].iter().enumerate() {
        let cx = ML + w / 6.0 + i as f64 * w / 3.0;
        let tw = d.width(t, 9.0, Style::Bold);
        d.text(t, cx - tw / 2.0, d.y, 9.0, Style::Bold, (0.0, 0.0, 0.0));
        let nw = d.width(n, 9.0, Style::Reg);
        d.text(n, cx - nw / 2.0, d.y + 20.0, 9.0, Style::Reg, (0.0, 0.0, 0.0));
    }
    d.y += 26.0;

    // ---- 5 ảnh
    let per = per_page.max(1);
    let two_col = per > 2;
    let maxw = if two_col { (w - 6.0) / 2.0 } else { w };
    let maxh = (PH - MT - MB - 30.0) / if per >= 2 { 2.0 } else { 1.0 } - 12.0;
    let mut first = true;
    for (k, &m) in methods.iter().enumerate() {
        let fs = fb(m);
        if fs.iter().all(|f| f.jpeg.is_none()) {
            continue;
        }
        if first {
            d.new_page();
            first = false;
        } else {
            d.y += 4.0;
            d.ensure(maxh + 25.0); // không để tiêu đề mục ảnh một mình cuối trang
        }
        d.heading(&format!("5.{}. Ảnh các trường đo – {}", k + 1, method_vi(m)));
        if k == 0 && !legend.is_empty() {
            d.para(legend, 7.5, Style::Reg, (0.0, 0.0, 0.0));
            d.y += 2.0;
        }
        let mut col = 0;
        let mut row_h: f64 = 0.0;
        let mut row_y = d.y;
        for (i, f) in fs.iter().enumerate() {
            let Some((jpg, iw, ih)) = &f.jpeg else { continue };
            let s = (maxw / *iw as f64).min(maxh / *ih as f64);
            let hmm = *ih as f64 * s;
            if !two_col || col == 0 {
                if d.y + hmm + 8.0 > PH - MB {
                    d.new_page();
                }
                row_y = d.y;
                row_h = 0.0;
            }
            d.y = row_y;
            let cx = if two_col { ML + maxw / 2.0 + col as f64 * (maxw + 6.0) } else { ML + w / 2.0 };
            let h = d.image(jpg, *iw, *ih, maxw, maxh, cx);
            let cap = format!("Trường {}: {} – P = {}, ℓ = {} µm, G = {}", i + 1, f.name, num(Some(f.res.p), 1), num(f.res.lbar_um, 2), num(f.res.g, 2));
            let mut lines = d.wrap(&cap, 7.5, Style::Ital, maxw);
            lines.truncate(2);
            for (li, l) in lines.iter().enumerate() {
                let tw = d.width(l, 7.5, Style::Ital);
                d.text(l, cx - tw / 2.0, row_y + h + 1.2 + li as f64 * 3.4, 7.5, Style::Ital, (0.0, 0.0, 0.0));
            }
            row_h = row_h.max(h + 9.0);
            if two_col && col == 0 {
                col = 1;
                d.y = row_y;
            } else {
                col = 0;
                d.y = row_y + row_h + 2.0;
            }
        }
        if col == 1 {
            d.y = row_y + row_h + 2.0;
        }
    }

    // ---- chân trang
    let total = d.pages.len();
    let footer = format!("{app_title} · ASTM E112-25 / E1382 · Mã mẫu: {} · Số BC: {}", dash(&info.sample), dash(&info.report_no));
    for (i, (p, l)) in d.pages.clone().iter().enumerate() {
        let layer = d.doc.get_page(*p).get_layer(*l);
        layer.set_outline_color(Color::Rgb(printpdf::Rgb::new(GRID.0 as f32, GRID.1 as f32, GRID.2 as f32, None)));
        layer.set_outline_thickness(0.5);
        layer.add_line(Line { points: vec![(Point::new(Mm(ML as f32), Mm(14.0)), false), (Point::new(Mm((PW - MR) as f32), Mm(14.0)), false)], is_closed: false });
        layer.set_fill_color(Color::Rgb(printpdf::Rgb::new(0.31, 0.35, 0.40, None)));
        layer.use_text(footer.clone(), 7.5, Mm(ML as f32), Mm(10.0), &d.f.reg);
        let pg = format!("Trang {}/{}", i + 1, total);
        let pw_ = d.width(&pg, 7.5, Style::Reg);
        layer.use_text(pg, 7.5, Mm((PW - MR - pw_) as f32), Mm(10.0), &d.f.reg);
    }
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    d.doc.save(&mut BufWriter::new(file)).map_err(|e| e.to_string())?;
    let _ = method_short;
    Ok(())
}
