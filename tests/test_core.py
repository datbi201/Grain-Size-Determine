# -*- coding: utf-8 -*-
"""Kiểm thử lõi tính toán (không cần pytest):  py -3.12 tests/test_core.py

1. Độ đúng của bộ đếm tự động trên ảnh tổng hợp Voronoi, so với số giao điểm hình học thật.
2. Sửa giao điểm thủ công: thêm / loại / khôi phục, trọng số 1 / 1,5 / 0,5.
3. Thống kê E112 §15: t theo Bảng 7, 95 %CI, %RA, G.
4. Xuất báo cáo PDF nhiều phương pháp.
"""
import math
import os
import sys

import cv2
import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(os.path.dirname(HERE), "src"))
import grain_core as core  # noqa: E402

OUT = os.path.join(HERE, "_out")
os.makedirs(OUT, exist_ok=True)
fails = []


def check(name, cond, info=""):
    print(("ĐẠT   " if cond else "LỖI   ") + name + (f"  ({info})" if info else ""))
    if not cond:
        fails.append(name)


def voronoi(seed, n, W=1200, H=900):
    rng = np.random.default_rng(seed)
    seeds = rng.uniform([0, 0], [W, H], (n, 2))
    yy, xx = np.mgrid[0:H, 0:W]
    d = np.full((H, W), np.inf)
    lab = np.zeros((H, W), int)
    for i, (sx, sy) in enumerate(seeds):
        di = (xx - sx) ** 2 + (yy - sy) ** 2
        m = di < d
        d[m] = di[m]
        lab[m] = i
    b = np.zeros((H, W), bool)
    b[:-1] |= lab[:-1] != lab[1:]
    b[:, :-1] |= lab[:, :-1] != lab[:, 1:]
    g = np.full((H, W), 200, np.uint8)
    g[cv2.dilate(b.astype(np.uint8), np.ones((2, 2), np.uint8)) > 0] = 60
    return cv2.cvtColor(g, cv2.COLOR_GRAY2BGR), seeds


def truth(ch, seeds):
    xs, ys, closed = core.chord_samples(ch)
    pts = np.stack([xs, ys], 1)
    L = ((pts[:, None, :] - seeds[None, :, :]) ** 2).sum(2).argmin(1)
    c = L != np.roll(L, 1)
    if not closed:
        c[0] = False
    return int(c.sum()), len(xs) * core.STEP


# 1. độ đúng
for seed, n in ((1, 400), (3, 150), (2, 800)):
    img, seeds = voronoi(seed, n)
    out = core.analyze(img, 1.0)
    for m, md in out["methods"].items():
        tP = tL = 0
        for c in md["chords"]:
            p, l = truth(c, seeds)
            tP += p
            tL += l
        r = core.summarize(md, 1.0)
        err = 100 * (r["l_um"] / (tL / tP) - 1)
        check(f"Độ đúng ℓ {m} (seed {seed}, {n} hạt)", abs(err) <= 8.0, f"sai số {err:+.1f} %")

# 2. sửa thủ công
img, _ = voronoi(5, 300)
out = core.analyze(img, 0.5, methods=("mixed",))
md = out["methods"]["mixed"]
valid = core.unpack(out["valid"])
P0 = core.summarize(md, 0.5)["P"]
c = md["chords"][0]
ev, err = core.add_manual(md, c["cx"] + c["r"] + 2, c["cy"] + 1, "triple", valid)
check("Thêm điểm ba hạt hít vào chord", ev is not None and ev["chord"] == "C1", err or "")
check("P tăng 1,5 sau khi thêm điểm ba hạt", abs(core.summarize(md, 0.5)["P"] - P0 - 1.5) < 1e-9)
core.remove_near(md, ev["x"], ev["y"])
check("Xoá điểm thủ công trả P về cũ", abs(core.summarize(md, 0.5)["P"] - P0) < 1e-9)
a = next(e for e in md["events"] if e["src"] == "auto" and e["kind"] == "cross")
core.remove_near(md, a["x"], a["y"], tol=0.5)
check("Loại điểm tự động giảm P đi 1", abs(core.summarize(md, 0.5)["P"] - (P0 - 1)) < 1e-9)
core.restore_near(md, a["x"], a["y"])
check("Khôi phục điểm tự động", abs(core.summarize(md, 0.5)["P"] - P0) < 1e-9)
d = next(ch for ch in md["chords"] if ch["kind"] == "line")
ev, err = core.add_manual(md, d["a"][0] + 3, d["a"][1] + 3, "endpoint", None, tol=20)
check("Đầu mút chỉ đặt ở đầu đoạn thẳng, trọng số 0,5", ev is not None and ev["x"] == d["a"][0], err or "")

# 3. thống kê E112 §15
s = core.field_stats([10.1, 9.6, 10.8, 10.3, 9.9], [110, 95, 120, 101, 99], "abrams")
check("t(n=5) = 2,776 theo E112 Bảng 7", s["t"] == 2.776)
check("95 %CI = t·s/√n", abs(s["ci"] - 2.776 * s["sd"] / math.sqrt(5)) < 1e-12)
check("G = −6,643856·log10(ℓ mm) − 3,288", abs(s["G"] - (-6.643856 * math.log10(s["l_mean"] / 1000) - 3.288)) < 1e-12)
check("ℓ = 32 µm ứng với G ≈ 6,64 (E112 Bảng 4)", abs(core.g_from_l_um(32.0) - 6.64) < 0.01,
      f"{core.g_from_l_um(32.0):.3f}")

# 4. báo cáo PDF
try:
    import grain_report
    res = {m: core.summarize(md_, 0.5) for m, md_ in core.analyze(img, 0.5)["methods"].items()}
    fbm = {m: [dict(name="tong_hop.png", um_per_px=0.5, calib="thử", res=r, image=img)] for m, r in res.items()}
    sbm = {m: core.field_stats([r["l_um"]], [r["P"]], m) for m, r in res.items()}
    pdf = os.path.join(OUT, "bao_cao_thu.pdf")
    grain_report.build_pdf(pdf, {"sample": "Thử"}, list(core.METHODS), fbm, sbm,
                           dict(sensitivity=85, focus=True, n_lines=5, mixed_hv=False,
                                enhance_display="không", enhance_detect="ảnh gốc"))
    check("Xuất PDF 3 phương pháp", os.path.getsize(pdf) > 20000)
except Exception as ex:
    check("Xuất PDF 3 phương pháp", False, str(ex))

print()
print("KẾT QUẢ:", "TẤT CẢ ĐẠT" if not fails else f"{len(fails)} LỖI: " + "; ".join(fails))
sys.exit(1 if fails else 0)
