# -*- coding: utf-8 -*-
"""
grain_core.py - Lõi phân tích cỡ hạt trung bình theo ASTM E112-25 / E1382-97(2023).

Phương pháp đoạn chắn (intercept), đếm giao điểm P:
  * Abrams   (E112 §14.3): 3 vòng tròn đồng tâm, chu vi 3:2:1 (250 / 166,7 / 83,3 trên 500 mm).
  * Heyn     (E112 §13)  : lưới đường thẳng ngang + dọc.
  * Chord hỗn hợp (Fig. 5): 3 vòng tròn Abrams + 2 đường chéo 150 đơn vị
                           (tuỳ chọn thêm 2 đường 100 đơn vị như Fig. 5 đầy đủ).
Quy tắc đếm (E112 §13.3, E1382 §12.4.5):
  cắt qua biên = 1 ; tiếp tuyến = 1 ; điểm ba hạt (triple point) = 1,5 ;
  đầu mút đoạn thẳng chạm biên = 0,5.
ℓ = L / P ;  G = −6,643856·log10(ℓ[mm]) − 3,288   (E112 Bảng 6)
Thống kê nhiều trường (E112 §15): trung bình các ℓ từng trường, s, 95 %CI = t·s/√n, %RA.

Hằng số trọng số, hình học Fig. 5 và cách thống kê theo trường kế thừa từ GSCM Rev.0.1 RC7.
"""
import math
import os

import cv2
import numpy as np
from skimage.morphology import skeletonize

STEP = 0.5                      # px giữa 2 điểm lấy mẫu trên chord
GAP = 2                         # số mẫu (= 1 px) khe hở được lấp giữa 2 cụm biên
JUNCTION_R = 5                  # kích thước nhân giãn điểm nối (px) để nhận điểm ba hạt
WEIGHTS = {"cross": 1.0, "triple": 1.5, "endpoint": 0.5}
KIND_VI = {"cross": "cắt biên (1)", "triple": "điểm ba hạt (1,5)", "endpoint": "đầu mút (0,5)"}
METHODS = ("abrams", "heyn", "mixed")
METHOD_VI = {"abrams": "Abrams (3 vòng tròn)", "heyn": "Heyn (đường thẳng)",
             "mixed": "Chord hỗn hợp (3 vòng tròn + 2 đường chéo)"}
METHOD_SHORT = {"abrams": "Abrams", "heyn": "Heyn", "mixed": "Chord HH"}

# t(0,975; n−1) — E112 Bảng 7 cho n = 5..20; mở rộng cho n = 2..4 và > 20
_T975 = {2: 12.706, 3: 4.303, 4: 3.182, 5: 2.776, 6: 2.571, 7: 2.447, 8: 2.365, 9: 2.306,
         10: 2.262, 11: 2.228, 12: 2.201, 13: 2.179, 14: 2.160, 15: 2.145, 16: 2.131,
         17: 2.120, 18: 2.110, 19: 2.101, 20: 2.093, 21: 2.086, 22: 2.080, 23: 2.074,
         24: 2.069, 25: 2.064, 26: 2.060, 27: 2.056, 28: 2.052, 29: 2.048, 30: 2.045, 31: 2.042}


def t975(n):
    return _T975.get(n, 1.96 if n > 31 else float("nan"))


# --------------------------------------------------------------------------- #
#  I/O
# --------------------------------------------------------------------------- #
def imread(path):
    data = np.fromfile(path, dtype=np.uint8)
    return cv2.imdecode(data, cv2.IMREAD_COLOR) if data.size else None


def imwrite(path, img, quality=92):
    ext = os.path.splitext(path)[1] or ".jpg"
    ok, buf = cv2.imencode(ext, img, [cv2.IMWRITE_JPEG_QUALITY, quality])
    if ok:
        buf.tofile(path)
    return ok


def pack(mask):
    return (mask.shape, np.packbits(mask.astype(bool), axis=None))


def unpack(p):
    shape, bits = p
    return np.unpackbits(bits, count=shape[0] * shape[1]).reshape(shape).astype(bool)


# --------------------------------------------------------------------------- #
#  Xử lý ảnh nhẹ (hiển thị và/hoặc trước khi tách biên)
# --------------------------------------------------------------------------- #
def enhance(img, brightness=0, contrast=1.0, gamma=1.0, clahe=False):
    """brightness: −100..100 ; contrast: 0,3..3 ; gamma: 0,3..3 ; clahe: cân bằng sáng cục bộ."""
    if brightness == 0 and contrast == 1.0 and gamma == 1.0 and not clahe:
        return img
    out = img
    if clahe:
        lab = cv2.cvtColor(out, cv2.COLOR_BGR2LAB)
        lab[:, :, 0] = cv2.createCLAHE(clipLimit=2.0, tileGridSize=(8, 8)).apply(lab[:, :, 0])
        out = cv2.cvtColor(lab, cv2.COLOR_LAB2BGR)
    lut = np.arange(256, dtype=np.float32)
    lut = (lut - 128.0) * contrast + 128.0 + brightness * 1.28
    lut = np.clip(lut, 0, 255) / 255.0
    if gamma != 1.0:
        lut = np.power(lut, 1.0 / gamma)
    lut = np.clip(lut * 255.0 + 0.5, 0, 255).astype(np.uint8)
    return cv2.LUT(out, lut)


def auto_enhance(img, fov=None):
    """Tự chọn độ sáng / tương phản / gamma / CLAHE để biên hạt rõ nhất.
    Kéo dải sáng p1–p99 về khoảng 20–235, đưa trung vị về giữa thang xám, bật CLAHE khi
    ảnh tối góc hoặc độ tương phản cục bộ thấp."""
    L = cv2.cvtColor(img, cv2.COLOR_BGR2LAB)[:, :, 0]
    H, W = L.shape
    if fov:
        yy, xx = np.ogrid[:H, :W]
        m = (xx - fov[0]) ** 2 + (yy - fov[1]) ** 2 <= (0.9 * fov[2]) ** 2
    else:
        m = np.ones((H, W), bool)
    v = L[m].astype(np.float32)
    p1, p99, med = np.percentile(v, 1), np.percentile(v, 99), np.median(v)
    c = float(np.clip(215.0 / max(p99 - p1, 1.0), 0.5, 3.0))
    center = (p1 + p99) / 2.0
    b = float(np.clip(-(center - 128.0) * c / 1.28, -100, 100))
    m2 = float(np.clip(((med - 128.0) * c + 128.0 + b * 1.28) / 255.0, 0.05, 0.95))
    g = float(np.clip(math.log(m2) / math.log(0.5), 0.6, 1.8))
    bg = cv2.GaussianBlur(L.astype(np.float32), (0, 0), max(H, W) / 15.0)
    vign = float(np.std(bg[m]) / max(float(np.mean(bg[m])), 1.0))
    local = cv2.Laplacian(L, cv2.CV_32F)
    clahe = vign > 0.08 or float(np.std(local[m])) < 6.0
    return dict(brightness=int(round(b)), contrast=round(c, 2), gamma=round(g, 2), clahe=bool(clahe))


def dark_mask(data):
    """Vùng làm tối khi hiển thị = ngoài vùng đo, trừ vùng thanh scale (để còn đọc được thước)."""
    dm = ~unpack(data["valid"])
    ex = data.get("excl")
    if ex:
        x0, y0, x1, y1 = [int(v) for v in ex]
        dm[y0:y1, x0:x1] = False
    return dm


def enhance_desc(e):
    if not e or (e.get("brightness", 0) == 0 and e.get("contrast", 1) == 1 and e.get("gamma", 1) == 1
                 and not e.get("clahe")):
        return "không"
    parts = []
    if e.get("brightness", 0):
        parts.append(f"độ sáng {e['brightness']:+.0f}")
    if e.get("contrast", 1) != 1:
        parts.append(f"tương phản ×{e['contrast']:.2f}")
    if e.get("gamma", 1) != 1:
        parts.append(f"gamma {e['gamma']:.2f}")
    if e.get("clahe"):
        parts.append("CLAHE")
    return ", ".join(parts)


# --------------------------------------------------------------------------- #
#  Trường nhìn tròn, thanh scale đỏ
# --------------------------------------------------------------------------- #
def detect_fov(img):
    g = img.max(axis=2)
    m = (g > 45).astype(np.uint8)
    m = cv2.morphologyEx(m, cv2.MORPH_OPEN, np.ones((9, 9), np.uint8))
    cnts, _ = cv2.findContours(m, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_SIMPLE)
    if not cnts:
        return None
    c = max(cnts, key=cv2.contourArea)
    (cx, cy), r = cv2.minEnclosingCircle(c)
    circ = cv2.contourArea(c) / (math.pi * r * r + 1e-9)
    if circ > 0.85 and m.mean() < 0.85 and r > 100:
        return float(cx), float(cy), float(r)
    return None


def find_red_bar(img):
    h, w = img.shape[:2]
    y0 = int(h * 0.55)
    hsv = cv2.cvtColor(img[y0:], cv2.COLOR_BGR2HSV)
    m = cv2.inRange(hsv, (0, 130, 140), (10, 255, 255)) | cv2.inRange(hsv, (165, 130, 140), (180, 255, 255))
    m = cv2.morphologyEx(m, cv2.MORPH_CLOSE, cv2.getStructuringElement(cv2.MORPH_RECT, (5, 1)))
    n, lab, stats, _ = cv2.connectedComponentsWithStats(m, connectivity=8)
    best = None
    for i in range(1, n):
        x, y, cw, ch, _a = stats[i]
        if cw >= 3 * ch and cw > max(30, 0.03 * w):
            if best is None or cw > best[0]:
                comp = (lab[y:y + ch, x:x + cw] == i)
                row_len = int(comp.sum(axis=1).max())
                best = (cw, row_len, (int(x), int(y + y0), int(cw), int(ch)))
    if best is None:
        return None
    cw, row_len, box = best
    return float(row_len if row_len >= 0.8 * cw else cw), box


def exclusion_box(bar_box, W, H):
    bx, by, bw, bh = bar_box
    px = int(0.25 * bw + 10)
    return (max(0, bx - px), max(0, int(by - 0.08 * H)),
            min(W, bx + bw + px), min(H, int(by + bh + 0.04 * H)))


# --------------------------------------------------------------------------- #
#  Tách biên hạt
# --------------------------------------------------------------------------- #
def extract_boundaries(crop_bgr, valid, sensitivity=85.0):
    h, w = crop_bgr.shape[:2]
    L = cv2.cvtColor(crop_bgr, cv2.COLOR_BGR2LAB)[:, :, 0].astype(np.float32)
    L[~valid] = float(L[valid].mean()) if valid.any() else 128.0
    bg = cv2.GaussianBlur(L, (0, 0), sigmaX=max(h, w) / 12.0)
    Lf = np.clip(L / np.maximum(bg, 1.0) * 128.0, 0, 255).astype(np.uint8)
    Lf = cv2.createCLAHE(clipLimit=2.0, tileGridSize=(8, 8)).apply(Lf)
    Lf = cv2.GaussianBlur(Lf, (0, 0), 1.0)
    k = int(np.clip(round(min(h, w) / 110.0), 5, 13))
    k += (k + 1) % 2
    bh = cv2.morphologyEx(Lf, cv2.MORPH_BLACKHAT, cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (k, k)))
    vals = bh[valid] if valid.any() else bh.ravel()
    thr = max(float(np.percentile(vals, sensitivity)), 4.0)
    bw = ((bh > thr) & valid).astype(np.uint8)
    bw = cv2.morphologyEx(bw, cv2.MORPH_CLOSE, np.ones((3, 3), np.uint8))
    n, lab, stats, _ = cv2.connectedComponentsWithStats(bw, connectivity=8)
    keep = np.zeros(n, bool)
    keep[1:] = stats[1:, cv2.CC_STAT_AREA] >= int(max(20, 2.5 * k))
    return skeletonize(keep[lab]) & valid


def sharp_region_mask(skeleton, valid):
    h, w = skeleton.shape
    dens = cv2.GaussianBlur(skeleton.astype(np.float32), (0, 0), sigmaX=max(h, w) / 25.0)
    ref = float(np.percentile(dens[valid], 75)) if valid.any() else 0.0
    if ref <= 0:
        return valid
    good = (dens > 0.30 * ref).astype(np.uint8)
    kk = max(3, int(max(h, w) / 60) | 1)
    el = cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (kk, kk))
    good = cv2.morphologyEx(good, cv2.MORPH_OPEN, el)
    good = cv2.morphologyEx(good, cv2.MORPH_CLOSE, el)
    return valid & (good > 0)


def junctions(skeleton):
    """Điểm nối ≥ 3 nhánh trên skeleton (crossing number ≥ 3) -> ứng viên điểm ba hạt."""
    s = np.pad(skeleton.astype(np.uint8), 1)
    nb = [s[:-2, 1:-1], s[:-2, 2:], s[1:-1, 2:], s[2:, 2:], s[2:, 1:-1], s[2:, :-2], s[1:-1, :-2], s[:-2, :-2]]
    trans = np.zeros(skeleton.shape, np.uint8)
    for i in range(8):
        trans += ((nb[i] == 0) & (nb[(i + 1) % 8] == 1)).astype(np.uint8)
    return skeleton & (trans >= 3)


# --------------------------------------------------------------------------- #
#  Lưới thử (test pattern)
# --------------------------------------------------------------------------- #
def field_geometry(W, H, fov):
    """Tâm và bán kính vùng đặt lưới."""
    if fov:
        cx, cy, R = fov
        return dict(cx=cx, cy=cy, r=0.92 * R, round=True, W=W, H=H)
    return dict(cx=(W - 1) / 2.0, cy=(H - 1) / 2.0, r=min(W, H) / 2.0, round=False, W=W, H=H)


def build_pattern(method, geo, n_lines=5, mixed_hv=False):
    cx, cy, r = geo["cx"], geo["cy"], geo["r"]
    W, H = geo["W"], geo["H"]
    chords = []
    if method == "abrams":
        rmax = 0.95 * r
        for i, f in enumerate((1.0, 2 / 3, 1 / 3)):
            chords.append(dict(id=f"C{i + 1}", kind="circle", cx=cx, cy=cy, r=rmax * f))
    elif method == "heyn":
        fr = [0.08 + 0.84 * (i + 0.5) / n_lines for i in range(n_lines)]
        for i, f in enumerate(fr):
            y = f * (H - 1)
            chords.append(dict(id=f"H{i + 1}", kind="line", a=(0.0, y), b=(W - 1.0, y)))
        for i, f in enumerate(fr):
            x = f * (W - 1)
            chords.append(dict(id=f"V{i + 1}", kind="line", a=(x, 0.0), b=(x, H - 1.0)))
    elif method == "mixed":
        # Fig. 5 (đơn vị: 100 = cạnh khung 100 mm). Vòng tròn chu vi 250/166,7/83,3,
        # đường chéo dài 150, đường ngang/dọc 100 (nằm ngoài các vòng tròn).
        reach = 78.1 if mixed_hv else 75.0          # khoảng cách xa nhất từ tâm (đơn vị)
        if geo["round"]:
            unit = 0.97 * r / reach
        else:
            unit = 0.95 * min(W, H) / 2.0 / (60.0 if mixed_hv else 53.1)
            unit = min(unit, 0.97 * math.hypot(W, H) / 2.0 / reach)
        R1 = 250.0 / (2 * math.pi)
        for i, f in enumerate((1.0, 2 / 3, 1 / 3)):
            chords.append(dict(id=f"C{i + 1}", kind="circle", cx=cx, cy=cy, r=R1 * f * unit))
        d = 150.0 / (2 * math.sqrt(2))
        segs = [((-d, -d), (d, d)), ((-d, d), (d, -d))]
        if mixed_hv:
            segs += [((-50, 60), (50, 60)), ((-60, -50), (-60, 50))]
        for i, (p, q) in enumerate(segs):
            chords.append(dict(id=f"D{i + 1}" if i < 2 else f"L{i - 1}", kind="line",
                               a=(cx + p[0] * unit, cy + p[1] * unit), b=(cx + q[0] * unit, cy + q[1] * unit)))
    else:
        raise ValueError("Phương pháp không hợp lệ")
    return chords


def chord_samples(ch):
    if ch["kind"] == "circle":
        n = max(int(2 * math.pi * ch["r"] / STEP), 16)
        th = np.linspace(0, 2 * math.pi, n, endpoint=False)
        return ch["cx"] + ch["r"] * np.cos(th), ch["cy"] + ch["r"] * np.sin(th), True
    (x1, y1), (x2, y2) = ch["a"], ch["b"]
    n = max(int(math.hypot(x2 - x1, y2 - y1) / STEP) + 1, 2)
    return np.linspace(x1, x2, n), np.linspace(y1, y2, n), False


def project(ch, x, y):
    """Chiếu điểm lên chord. Trả về (px, py, khoảng cách)."""
    if ch["kind"] == "circle":
        t = math.atan2(y - ch["cy"], x - ch["cx"])
        px, py = ch["cx"] + ch["r"] * math.cos(t), ch["cy"] + ch["r"] * math.sin(t)
    else:
        (x1, y1), (x2, y2) = ch["a"], ch["b"]
        vx, vy = x2 - x1, y2 - y1
        u = max(0.0, min(1.0, ((x - x1) * vx + (y - y1) * vy) / (vx * vx + vy * vy)))
        px, py = x1 + u * vx, y1 + u * vy
    return px, py, math.hypot(x - px, y - py)


def detect_on_chord(ch, bmask, valid, jmask):
    """Đếm tự động trên 1 chord. Trả về (L_px, [events])."""
    h, w = bmask.shape
    xs, ys, closed = chord_samples(ch)
    xi, yi = np.rint(xs).astype(int), np.rint(ys).astype(int)
    inside = (xi >= 0) & (xi < w) & (yi >= 0) & (yi < h)
    xc, yc = np.clip(xi, 0, w - 1), np.clip(yi, 0, h - 1)
    ok = inside & valid[yc, xc]
    b = ok & bmask[yc, xc]
    jj = ok & jmask[yc, xc]
    n = len(b)
    # lấp khe hở ngắn (≤ GAP mẫu) giữa 2 cụm: tránh đếm 2 lần 1 biên khi chord cắt chéo/lướt biên
    if GAP > 0 and b.any():
        bi = b.astype(np.uint8)
        k = np.ones(2 * GAP + 1, np.uint8)
        if closed:
            ext = np.r_[bi[-GAP:], bi, bi[:GAP]]
            dil = np.convolve(ext, k, "same")[GAP:-GAP] > 0
            ero = np.convolve(np.r_[dil[-GAP:], dil, dil[:GAP]].astype(np.uint8), k, "same")[GAP:-GAP] == len(k)
        else:
            dil = np.convolve(bi, k, "same") > 0
            ero = np.convolve(dil.astype(np.uint8), k, "same") == len(k)
            ero[:GAP] = dil[:GAP] & b[:GAP] | ero[:GAP]
            ero[-GAP:] = dil[-GAP:] & b[-GAP:] | ero[-GAP:]
        b = (b | ero) & ok
    L = float(ok.sum()) * STEP
    if not b.any():
        return L, []
    events = []
    if closed and b.all():
        return L, []
    # duyệt các cụm liên tiếp (có xử lý vòng kín)
    start = 0
    if closed:
        nz = np.flatnonzero(~b)
        start = int(nz[0])                     # bắt đầu từ 1 điểm không phải biên
    order = np.r_[start:n, 0:start] if closed else np.arange(n)
    bo, oko, jo = b[order], ok[order], jj[order]
    i = 0
    while i < n:
        if not bo[i]:
            i += 1
            continue
        j = i
        while j + 1 < n and bo[j + 1]:
            j += 1
        # cụm chạm đầu/cuối đoạn hợp lệ -> đầu mút (chỉ áp dụng cho đường thẳng / cung bị cắt)
        prv_ok = oko[i - 1] if i > 0 else (oko[-1] if closed else False)
        nxt_ok = oko[j + 1] if j + 1 < n else (oko[0] if closed else False)
        touch_start, touch_end = not prv_ok, not nxt_ok
        mid = order[(i + j) // 2]
        if touch_start or touch_end:
            kind = "endpoint"
        elif jo[i:j + 1].any():
            kind = "triple"
        else:
            kind = "cross"
        events.append(dict(chord=ch["id"], x=float(xs[mid]), y=float(ys[mid]), kind=kind, src="auto",
                           removed=False))
        i = j + 1
    return L, events


# --------------------------------------------------------------------------- #
#  Phân tích 1 ảnh (1 trường)
# --------------------------------------------------------------------------- #
def analyze(img, um_per_px, fov=None, bar_box=None, methods=METHODS, sensitivity=85.0, focus=True,
            n_lines=5, mixed_hv=False, enhance_params=None):
    H, W = img.shape[:2]
    src = enhance(img, **enhance_params) if enhance_params else img
    geo = field_geometry(W, H, fov)
    if fov:
        cx, cy, R = fov
        r_use = geo["r"]
        x0, y0 = max(0, int(cx - r_use)), max(0, int(cy - r_use))
        x1, y1 = min(W, int(cx + r_use) + 1), min(H, int(cy + r_use) + 1)
        yy, xx = np.mgrid[y0:y1, x0:x1]
        vcrop = (xx - cx) ** 2 + (yy - cy) ** 2 <= r_use ** 2
    else:
        x0 = y0 = 0
        x1, y1 = W, H
        vcrop = np.ones((H, W), bool)
    if bar_box:
        ex0, ey0, ex1, ey1 = exclusion_box(bar_box, W, H)
        vcrop[max(0, ey0 - y0):max(0, ey1 - y0), max(0, ex0 - x0):max(0, ex1 - x0)] = False

    sk = extract_boundaries(src[y0:y1, x0:x1], vcrop, sensitivity)
    excluded = 0.0
    if focus:
        v2 = sharp_region_mask(sk, vcrop)
        excluded = 1.0 - v2.sum() / max(vcrop.sum(), 1)
        vcrop = v2
        sk = sk & vcrop
    skeleton = np.zeros((H, W), bool)
    valid = np.zeros((H, W), bool)
    skeleton[y0:y1, x0:x1] = sk
    valid[y0:y1, x0:x1] = vcrop
    bmask = cv2.dilate(skeleton.astype(np.uint8), np.ones((3, 3), np.uint8)) > 0
    jmask = cv2.dilate(junctions(skeleton).astype(np.uint8),
                       cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (JUNCTION_R, JUNCTION_R))) > 0

    out = {}
    for m in methods:
        chords = build_pattern(m, geo, n_lines, mixed_hv)
        Ls, evs = {}, []
        for ch in chords:
            L, e = detect_on_chord(ch, bmask, valid, jmask)
            Ls[ch["id"]] = L
            evs += e
        out[m] = dict(chords=chords, L_px=Ls, events=evs, edited=False)
    excl = exclusion_box(bar_box, W, H) if bar_box else None
    return dict(methods=out, skeleton=pack(skeleton), valid=pack(valid), excluded=excluded, excl=excl,
                size=(W, H), params=dict(sensitivity=sensitivity, focus=focus, n_lines=n_lines,
                                         mixed_hv=mixed_hv, enhance=enhance_params or {}))


# --------------------------------------------------------------------------- #
#  Tính kết quả 1 trường (dùng lại sau mỗi lần sửa thủ công)
# --------------------------------------------------------------------------- #
def g_from_l_um(l_um):
    """G = −6,643856·log10(ℓ[mm]) − 3,288  (E112 Bảng 6)"""
    return -6.643856 * math.log10(l_um / 1000.0) - 3.288


def round_g(g):
    v = g * 10
    return (math.floor(v + 0.5) if v >= 0 else -math.floor(-v + 0.5)) / 10


def summarize(md, um_per_px):
    act = [e for e in md["events"] if not e.get("removed")]
    P = sum(WEIGHTS[e["kind"]] for e in act)
    L_px = sum(md["L_px"].values())
    L_um = L_px * um_per_px
    res = dict(P=P, L_um=L_um, L_mm=L_um / 1000.0, l_um=None, G=None,
               n_cross=sum(e["kind"] == "cross" for e in act),
               n_triple=sum(e["kind"] == "triple" for e in act),
               n_end=sum(e["kind"] == "endpoint" for e in act),
               n_manual=sum(e["src"] == "manual" for e in act),
               n_removed=sum(1 for e in md["events"] if e.get("removed")),
               n_chords=len(md["chords"]))
    if P > 0 and L_um > 0:
        res["l_um"] = L_um / P
        res["G"] = g_from_l_um(res["l_um"])
    # theo hướng (Heyn) để đánh giá dị hướng
    for pref, key in (("H", "l_H"), ("V", "l_V")):
        ids = [c["id"] for c in md["chords"] if c["id"].startswith(pref)]
        if ids:
            p = sum(WEIGHTS[e["kind"]] for e in act if e["chord"] in ids)
            l = sum(md["L_px"][i] for i in ids) * um_per_px
            if p > 0:
                res[key] = l / p
    flags = []
    if P < 50:
        flags.append("P<50")
    if any(c["kind"] == "circle" for c in md["chords"]) and P > 150:
        flags.append("P>150")
    res["flags"] = flags
    return res


def add_manual(md, x, y, kind, valid=None, tol=12.0):
    """Thêm giao điểm thủ công: hít vào chord gần nhất. Trả về (event, thông báo lỗi)."""
    best = None
    for ch in md["chords"]:
        if kind == "endpoint" and ch["kind"] != "line":
            continue
        if kind == "endpoint":
            cand = min((ch["a"], ch["b"]), key=lambda p: math.hypot(p[0] - x, p[1] - y))
            px, py, d = cand[0], cand[1], math.hypot(cand[0] - x, cand[1] - y)
        else:
            px, py, d = project(ch, x, y)
        if best is None or d < best[3]:
            best = (ch, px, py, d)
    if best is None or best[3] > tol:
        return None, "Điểm click cách xa mọi đường chord."
    ch, px, py, _ = best
    if valid is not None:
        h, w = valid.shape
        xi, yi = int(round(px)), int(round(py))
        if not (0 <= xi < w and 0 <= yi < h and valid[yi, xi]):
            return None, "Điểm nằm ngoài vùng đo (vùng xám tối)."
    for e in md["events"]:
        if not e.get("removed") and e["chord"] == ch["id"] and math.hypot(e["x"] - px, e["y"] - py) < 1.5:
            return None, "Đã có giao điểm tại vị trí này."
    ev = dict(chord=ch["id"], x=float(px), y=float(py), kind=kind, src="manual", removed=False)
    md["events"].append(ev)
    md["edited"] = True
    return ev, None


def remove_near(md, x, y, tol=10.0):
    """Bỏ giao điểm gần nhất (điểm tự động -> đánh dấu đã loại; điểm thủ công -> xoá)."""
    best, bd = None, tol
    for e in md["events"]:
        if e.get("removed"):
            continue
        d = math.hypot(e["x"] - x, e["y"] - y)
        if d <= bd:
            best, bd = e, d
    if best is None:
        return None
    if best["src"] == "manual":
        md["events"].remove(best)
    else:
        best["removed"] = True
    md["edited"] = True
    return best


def restore_near(md, x, y, tol=10.0):
    best, bd = None, tol
    for e in md["events"]:
        if e.get("removed") and math.hypot(e["x"] - x, e["y"] - y) <= bd:
            best, bd = e, math.hypot(e["x"] - x, e["y"] - y)
    if best:
        best["removed"] = False
        md["edited"] = True
    return best


# --------------------------------------------------------------------------- #
#  Thống kê nhiều trường (E112 §15)
# --------------------------------------------------------------------------- #
def field_stats(l_values_um, P_values=None, method=None):
    n = len(l_values_um)
    if n == 0:
        return None
    mean = sum(l_values_um) / n
    out = dict(n=n, l_mean=mean, G=g_from_l_um(mean), G_round=round_g(g_from_l_um(mean)), sd=None, t=None,
               ci=None, ra=None, G_lo=None, G_hi=None)
    if n >= 2:
        sd = math.sqrt(sum((v - mean) ** 2 for v in l_values_um) / (n - 1))
        t = t975(n)
        ci = t * sd / math.sqrt(n)
        out.update(sd=sd, t=t, ci=ci, ra=100.0 * ci / mean,
                   G_lo=g_from_l_um(mean + ci), G_hi=g_from_l_um(mean - ci) if mean - ci > 0 else None)
    # kiểm tra theo khuyến nghị E112 / E1382
    checks = []
    checks.append(("Số trường đo n ≥ 5 (E112 §13.2, §14.3.2)", n >= 5, f"n = {n}"))
    if out["ra"] is not None:
        checks.append(("Độ chính xác tương đối %RA ≤ 10 % (E112 §15.6)", out["ra"] <= 10.0, f"%RA = {out['ra']:.1f} %"))
    else:
        checks.append(("Độ chính xác tương đối %RA ≤ 10 % (E112 §15.6)", False, "cần ≥ 2 trường"))
    if P_values:
        tot = sum(P_values)
        checks.append(("Tổng số giao điểm ≥ 500 (E1382 §12.4.3; E112 §14.3.2.1: 400–500)", tot >= 500,
                       f"ΣP = {tot:g}"))
        if method in ("abrams", "mixed"):
            ok = all(40 <= p <= 150 for p in P_values)
            checks.append(("Mỗi trường 40–150 giao điểm (E112 §14.3.2.1; E1382 §12.4.2)", ok,
                           f"P: {min(P_values):g} – {max(P_values):g}"))
        else:
            ok = all(p >= 50 for p in P_values)
            checks.append(("Mỗi trường ≥ 50 giao điểm (E112 §13.1)", ok, f"P nhỏ nhất = {min(P_values):g}"))
    out["checks"] = checks
    return out


# --------------------------------------------------------------------------- #
#  Vẽ ảnh kết quả (dùng cho lưu ảnh và báo cáo PDF)
# --------------------------------------------------------------------------- #
def hex2bgr(h):
    h = h.lstrip("#")
    return (int(h[4:6], 16), int(h[2:4], 16), int(h[0:2], 16))


def render(base, data, method, colors, show_skeleton=True, show_pattern=True, banner=None):
    vis = base.copy()
    H, W = vis.shape[:2]
    if data is not None:
        dm = dark_mask(data)
        vis[dm] = (vis[dm] * 0.35).astype(np.uint8)
        if show_skeleton:
            sk = unpack(data["skeleton"])
            vis[sk] = hex2bgr(colors.get("boundary", "#00c800"))
        md = data["methods"].get(method)
        if md and show_pattern:
            th = max(1, int(round(min(H, W) / 500)))
            cc = hex2bgr(colors.get("chord", "#00ffff"))
            for ch in md["chords"]:
                if ch["kind"] == "circle":
                    cv2.circle(vis, (int(round(ch["cx"])), int(round(ch["cy"]))), int(round(ch["r"])), cc, th,
                               cv2.LINE_AA)
                else:
                    cv2.line(vis, tuple(int(round(v)) for v in ch["a"]), tuple(int(round(v)) for v in ch["b"]), cc,
                             th, cv2.LINE_AA)
            rad = max(3, int(round(min(H, W) / 200)))
            pc = hex2bgr(colors.get("point", "#ff0000"))
            mc = hex2bgr(colors.get("manual", "#ff00ff"))
            for e in md["events"]:
                if e.get("removed"):
                    continue
                c = mc if e["src"] == "manual" else pc
                p = (int(round(e["x"])), int(round(e["y"])))
                if e["kind"] == "triple":
                    cv2.circle(vis, p, rad + 2, c, max(1, th), cv2.LINE_AA)
                    cv2.circle(vis, p, max(1, rad - 1), c, -1, cv2.LINE_AA)
                elif e["kind"] == "endpoint":
                    cv2.circle(vis, p, rad + 1, c, max(1, th), cv2.LINE_AA)
                else:
                    cv2.circle(vis, p, rad, c, -1, cv2.LINE_AA)
    if banner:
        fs = float(np.clip(min(H, W) / 900.0, 0.5, 1.8))
        (tw, tht), _ = cv2.getTextSize(banner, cv2.FONT_HERSHEY_SIMPLEX, fs, 2)
        cv2.rectangle(vis, (0, 0), (tw + 20, tht + 18), (0, 0, 0), -1)
        cv2.putText(vis, banner, (10, tht + 6), cv2.FONT_HERSHEY_SIMPLEX, fs, (0, 255, 255), 2, cv2.LINE_AA)
    return vis
