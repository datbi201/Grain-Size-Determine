//! Dữ liệu ảnh / project và đọc-ghi tệp project `.gsd` (tương thích bản Python: zip gồm project.json + masks.npz).
use gsd_core::enhance::Enhance;
use gsd_core::imgproc::Img;
use gsd_core::*;
use serde_json::{json, Map, Value};
use std::io::{Read, Write};

pub const METHODS: [Method; 3] = [Method::Abrams, Method::Heyn, Method::Mixed];

pub fn method_from(s: &str) -> Option<Method> {
    match s {
        "abrams" => Some(Method::Abrams),
        "heyn" => Some(Method::Heyn),
        "mixed" => Some(Method::Mixed),
        _ => None,
    }
}

pub fn method_vi(m: Method) -> &'static str {
    match m {
        Method::Abrams => "Abrams (3 vòng tròn)",
        Method::Heyn => "Heyn (đường thẳng)",
        Method::Mixed => "Chord hỗn hợp (3 vòng tròn + 2 đường chéo)",
    }
}

pub fn method_short(m: Method) -> &'static str {
    match m {
        Method::Abrams => "Abrams",
        Method::Heyn => "Heyn",
        Method::Mixed => "Chord HH",
    }
}

pub fn kind_str(k: Kind) -> &'static str {
    match k {
        Kind::Cross => "cross",
        Kind::Triple => "triple",
        Kind::Endpoint => "endpoint",
    }
}

pub struct Item {
    pub path: String,
    pub name: String,
    pub size: Option<(usize, usize)>,
    pub fov: Option<(f64, f64, f64)>,
    pub probed: bool,
    pub bar_box: Option<[usize; 4]>,
    pub bar_px: Option<f64>,
    pub um_per_px: Option<f64>,
    pub calib: String,
    pub data: Option<Analysis>,
    pub undo: Vec<(Method, Vec<Event>, bool)>,
    pub include: bool,
    pub state: String,
    pub note: String,
    pub id: u64,
}

impl Item {
    pub fn new(path: &str, id: u64) -> Item {
        let name = std::path::Path::new(path).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        Item { path: path.to_string(), name, size: None, fov: None, probed: false, bar_box: None, bar_px: None,
               um_per_px: None, calib: String::new(), data: None, undo: vec![], include: true,
               state: "Cần scale".into(), note: String::new(), id }
    }
    pub fn res(&self, m: Method) -> Option<Summary> {
        let a = self.data.as_ref()?;
        Some(summarize(a.method(m)?, self.um_per_px?))
    }
    pub fn edited(&self) -> Vec<Method> {
        self.data.as_ref().map(|a| a.methods.iter().filter(|r| r.edited).map(|r| r.method).collect()).unwrap_or_default()
    }
}

// --------------------------------------------------------------------------- //
//  npy / packbits (để tương thích masks.npz của NumPy)
// --------------------------------------------------------------------------- //
fn packbits(m: &Img<bool>) -> Vec<u8> {
    let n = m.d.len();
    let mut out = vec![0u8; (n + 7) / 8];
    for (i, &b) in m.d.iter().enumerate() {
        if b {
            out[i / 8] |= 0x80 >> (i % 8);
        }
    }
    out
}

fn unpackbits(bits: &[u8], w: usize, h: usize) -> Img<bool> {
    Img { w, h, d: (0..w * h).map(|i| bits.get(i / 8).map_or(false, |b| b & (0x80 >> (i % 8)) != 0)).collect() }
}

fn npy_u8(data: &[u8]) -> Vec<u8> {
    let mut header = format!("{{'descr': '|u1', 'fortran_order': False, 'shape': ({},), }}", data.len());
    let total = 10 + header.len() + 1;
    let pad = (64 - total % 64) % 64;
    header.push_str(&" ".repeat(pad));
    header.push('\n');
    let mut v = b"\x93NUMPY\x01\x00".to_vec();
    v.extend_from_slice(&(header.len() as u16).to_le_bytes());
    v.extend_from_slice(header.as_bytes());
    v.extend_from_slice(data);
    v
}

fn npy_read(buf: &[u8]) -> Result<Vec<u8>, String> {
    if buf.len() < 10 || &buf[..6] != b"\x93NUMPY" {
        return Err("npy không hợp lệ".into());
    }
    let (hl, off) = if buf[6] == 1 {
        (u16::from_le_bytes([buf[8], buf[9]]) as usize, 10)
    } else {
        (u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]) as usize, 12)
    };
    Ok(buf[off + hl..].to_vec())
}

fn zip_bytes(entries: &[(String, Vec<u8>)]) -> Result<Vec<u8>, String> {
    let mut cur = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut cur);
        let opt = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, data) in entries {
            z.start_file(name.as_str(), opt).map_err(|e| e.to_string())?;
            z.write_all(data).map_err(|e| e.to_string())?;
        }
        z.finish().map_err(|e| e.to_string())?;
    }
    Ok(cur.into_inner())
}

fn unzip_all(data: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(data)).map_err(|e| e.to_string())?;
    let mut out = vec![];
    for i in 0..z.len() {
        let mut f = z.by_index(i).map_err(|e| e.to_string())?;
        let mut v = vec![];
        f.read_to_end(&mut v).map_err(|e| e.to_string())?;
        out.push((f.name().to_string(), v));
    }
    Ok(out)
}

// --------------------------------------------------------------------------- //
//  Chuyển đổi JSON
// --------------------------------------------------------------------------- //
fn chord_json(c: &Chord) -> Value {
    match c {
        Chord::Circle { id, cx, cy, r } => json!({"id": id, "kind": "circle", "cx": cx, "cy": cy, "r": r}),
        Chord::Line { id, a, b } => json!({"id": id, "kind": "line", "a": [a.0, a.1], "b": [b.0, b.1]}),
    }
}

fn chord_from(v: &Value) -> Option<Chord> {
    let id = v["id"].as_str()?.to_string();
    if v["kind"] == "circle" {
        Some(Chord::Circle { id, cx: v["cx"].as_f64()?, cy: v["cy"].as_f64()?, r: v["r"].as_f64()? })
    } else {
        Some(Chord::Line { id, a: (v["a"][0].as_f64()?, v["a"][1].as_f64()?), b: (v["b"][0].as_f64()?, v["b"][1].as_f64()?) })
    }
}

pub fn enh_json(e: &Enhance) -> Value {
    json!({"brightness": e.brightness, "contrast": e.contrast, "gamma": e.gamma, "clahe": e.clahe})
}

pub fn enh_from(v: &Value) -> Enhance {
    Enhance { brightness: v["brightness"].as_f64().unwrap_or(0.0), contrast: v["contrast"].as_f64().unwrap_or(1.0),
              gamma: v["gamma"].as_f64().unwrap_or(1.0), clahe: v["clahe"].as_bool().unwrap_or(false) }
}

fn analysis_json(a: &Analysis) -> Value {
    let mut ms = Map::new();
    for r in &a.methods {
        let mut lp = Map::new();
        for (c, l) in r.chords.iter().zip(&r.l_px) {
            lp.insert(c.id().into(), json!(l));
        }
        let ev: Vec<Value> = r.events.iter().map(|e| json!({"chord": e.chord, "x": e.x, "y": e.y, "kind": kind_str(e.kind),
            "src": if e.manual {"manual"} else {"auto"}, "removed": e.removed})).collect();
        ms.insert(r.method.name().into(), json!({"chords": r.chords.iter().map(chord_json).collect::<Vec<_>>(),
            "L_px": lp, "events": ev, "edited": r.edited}));
    }
    json!({"methods": ms, "excluded": a.excluded, "excl": a.excl, "size": [a.valid.w, a.valid.h],
           "params": {"sensitivity": a.params.sensitivity, "focus": a.params.focus, "n_lines": a.params.n_lines,
                      "mixed_hv": a.params.mixed_hv, "enhance": {}, "enhance_desc": a.enhance_desc},
           "skeleton_shape": [a.skeleton.h, a.skeleton.w], "valid_shape": [a.valid.h, a.valid.w]})
}

fn analysis_from(v: &Value, skel: &[u8], valid: &[u8]) -> Option<Analysis> {
    let sh = &v["skeleton_shape"];
    let (h, w) = (sh[0].as_u64()? as usize, sh[1].as_u64()? as usize);
    let mut methods = vec![];
    for m in METHODS {
        let Some(mv) = v["methods"].get(m.name()) else { continue };
        let chords: Vec<Chord> = mv["chords"].as_array()?.iter().filter_map(chord_from).collect();
        let l_px = chords.iter().map(|c| mv["L_px"][c.id()].as_f64().unwrap_or(0.0)).collect();
        let events = mv["events"].as_array()?.iter().filter_map(|e| Some(Event {
            chord: e["chord"].as_str()?.to_string(), x: e["x"].as_f64()?, y: e["y"].as_f64()?,
            kind: match e["kind"].as_str()? { "triple" => Kind::Triple, "endpoint" => Kind::Endpoint, _ => Kind::Cross },
            manual: e["src"] == "manual", removed: e["removed"].as_bool().unwrap_or(false) })).collect();
        methods.push(MethodResult { method: m, chords, l_px, events, edited: mv["edited"].as_bool().unwrap_or(false) });
    }
    let p = &v["params"];
    let excl = v["excl"].as_array().map(|a| {
        let g = |i: usize| a.get(i).and_then(|x| x.as_f64()).unwrap_or(0.0) as usize;
        [g(0), g(1), g(2), g(3)]
    });
    let enhance_desc = p["enhance_desc"].as_str().map(String::from).unwrap_or_else(|| {
        let e = enh_from(&p["enhance"]);
        if e.is_identity() { String::new() } else { e.describe() }
    });
    Some(Analysis { methods, skeleton: unpackbits(skel, w, h), valid: unpackbits(valid, w, h),
                    excluded: v["excluded"].as_f64().unwrap_or(0.0), excl,
                    params: Params { sensitivity: p["sensitivity"].as_f64().unwrap_or(85.0),
                                     focus: p["focus"].as_bool().unwrap_or(true),
                                     n_lines: p["n_lines"].as_u64().unwrap_or(5) as usize,
                                     mixed_hv: p["mixed_hv"].as_bool().unwrap_or(false) },
                    enhance_desc })
}

fn rel_path(target: &str, base_dir: &std::path::Path) -> Option<String> {
    let t = std::path::Path::new(target);
    let (tc, bc): (Vec<_>, Vec<_>) = (t.components().collect(), base_dir.components().collect());
    if tc.first() != bc.first() {
        return None;
    }
    let common = tc.iter().zip(&bc).take_while(|(a, b)| a == b).count();
    let mut p = std::path::PathBuf::new();
    for _ in common..bc.len() {
        p.push("..");
    }
    for c in &tc[common..] {
        p.push(c);
    }
    Some(p.to_string_lossy().to_string())
}

pub fn write_project(path: &str, items: &[Item], settings: Value, report: Value, app_ver: &str) -> Result<(), String> {
    let pdir = std::path::Path::new(path).parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let mut arrays = vec![];
    let mut its = vec![];
    for (k, it) in items.iter().enumerate() {
        let abs = std::fs::canonicalize(&it.path).map(|p| p.to_string_lossy().trim_start_matches(r"\\?\").to_string())
            .unwrap_or(it.path.clone());
        let mut d = json!({"path": abs, "name": it.name, "um_per_px": it.um_per_px, "calib": it.calib,
            "bar_box": it.bar_box, "bar_px": it.bar_px, "fov": it.fov.map(|f| [f.0, f.1, f.2]),
            "size": it.size.map(|s| [s.0, s.1]), "include": it.include, "state": it.state, "note": it.note,
            "rel": rel_path(&abs, &pdir), "data": Value::Null});
        if let Some(a) = &it.data {
            arrays.push((format!("{k}_skeleton.npy"), npy_u8(&packbits(&a.skeleton))));
            arrays.push((format!("{k}_valid.npy"), npy_u8(&packbits(&a.valid))));
            d["data"] = analysis_json(a);
        }
        its.push(d);
    }
    if arrays.is_empty() {
        arrays.push(("empty.npy".into(), npy_u8(&[0])));
    }
    let proj = json!({"format": "GrainSizeDetermine", "version": 1, "app": app_ver,
        "saved": chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
        "settings": settings, "report": report, "items": its});
    let npz = zip_bytes(&arrays)?;
    let all = zip_bytes(&[("project.json".into(), serde_json::to_string_pretty(&proj).unwrap().into_bytes()),
                          ("masks.npz".into(), npz)])?;
    let tmp = format!("{path}.tmp");
    std::fs::write(&tmp, all).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

pub struct Project {
    pub items: Vec<Item>,
    pub settings: Value,
    pub report: Value,
}

pub fn read_project(path: &str, next_id: &mut u64) -> Result<Project, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let files = unzip_all(&data)?;
    let get = |n: &str| files.iter().find(|f| f.0 == n).map(|f| f.1.clone());
    let pj: Value = serde_json::from_slice(&get("project.json").ok_or("thiếu project.json")?).map_err(|e| e.to_string())?;
    if pj["format"] != "GrainSizeDetermine" {
        return Err("không phải tệp project của Grain Size Determine".into());
    }
    let npz = unzip_all(&get("masks.npz").ok_or("thiếu masks.npz")?)?;
    let arr = |n: String| -> Option<Vec<u8>> { npz.iter().find(|f| f.0 == n).and_then(|f| npy_read(&f.1).ok()) };
    let pdir = std::path::Path::new(path).parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let mut items = vec![];
    for (k, d) in pj["items"].as_array().ok_or("thiếu danh sách ảnh")?.iter().enumerate() {
        let mut p = d["path"].as_str().unwrap_or("").to_string();
        if !std::path::Path::new(&p).exists() {
            if let Some(r) = d["rel"].as_str() {
                let alt = pdir.join(r);
                if alt.exists() {
                    p = alt.to_string_lossy().to_string();
                }
            }
        }
        *next_id += 1;
        let mut it = Item::new(&p, *next_id);
        if let Some(n) = d["name"].as_str() {
            it.name = n.into();
        }
        it.um_per_px = d["um_per_px"].as_f64();
        it.calib = d["calib"].as_str().unwrap_or("").into();
        it.bar_box = d["bar_box"].as_array().map(|a| {
            let g = |i: usize| a.get(i).and_then(|x| x.as_f64()).unwrap_or(0.0) as usize;
            [g(0), g(1), g(2), g(3)]
        });
        it.bar_px = d["bar_px"].as_f64();
        it.fov = d["fov"].as_array().and_then(|a| Some((a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?)));
        it.size = d["size"].as_array().and_then(|a| Some((a.first()?.as_u64()? as usize, a.get(1)?.as_u64()? as usize)));
        it.probed = it.size.is_some();
        it.include = d["include"].as_bool().unwrap_or(true);
        it.state = d["state"].as_str().unwrap_or("").into();
        it.note = d["note"].as_str().unwrap_or("").into();
        if d["data"].is_object() {
            if let (Some(s), Some(v)) = (arr(format!("{k}_skeleton.npy")), arr(format!("{k}_valid.npy"))) {
                it.data = analysis_from(&d["data"], &s, &v);
            }
        }
        if !std::path::Path::new(&it.path).exists() {
            it.state = "Không tìm thấy ảnh".into();
        }
        items.push(it);
    }
    Ok(Project { items, settings: pj["settings"].clone(), report: pj["report"].clone() })
}
