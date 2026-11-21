# -*- coding: utf-8 -*-
"""Đối chiếu lõi Rust (gsd_core.exe) với lõi Python (src/grain_core.py) trên cùng bộ ảnh.

  py -3.12 rust-core/compare_python.py <thư mục ảnh> <gsd_core.exe> [kết_quả.csv]
Cùng µm/px (0,5) cho mọi ảnh; ΔG = G_Rust − G_Python cho từng phương pháp; so trường nhìn và thanh scale.
"""
import csv
import glob
import json
import os
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(os.path.dirname(HERE), "src"))
import grain_core as core  # noqa: E402

UM = 0.5
d, exe = sys.argv[1], sys.argv[2]
out_csv = sys.argv[3] if len(sys.argv) > 3 else os.path.join(HERE, "compare_result.csv")
files = [f for f in sorted(glob.glob(os.path.join(d, "*"))) if f.lower().endswith((".jpg", ".jpeg", ".png", ".bmp", ".tif", ".webp"))]

t0 = time.time()
rust = json.loads(subprocess.run([exe, "--um", str(UM)] + files, capture_output=True, text=True, encoding="utf-8").stdout)
t_rust = time.time() - t0
rust = {os.path.basename(r["image"]): r for r in rust}

rows, t_py = [], 0.0
for f in files:
    name = os.path.basename(f)
    t1 = time.time()
    img = core.imread(f)
    fov = core.detect_fov(img)
    bar = core.find_red_bar(img)
    out = core.analyze(img, UM, fov, bar[1] if bar else None)
    dt = time.time() - t1
    t_py += dt
    r = rust.get(name, {})
    row = dict(image=name, size=f"{img.shape[1]}x{img.shape[0]}",
               fov_py="none" if not fov else f"{fov[0]:.0f},{fov[1]:.0f},{fov[2]:.0f}",
               fov_rs="none" if not r.get("fov") else ",".join(f"{v:.0f}" for v in r["fov"]),
               bar_py=bar[0] if bar else "", bar_rs=r["bar"]["len"] if r.get("bar") else "",
               excl_py=round(out["excluded"], 3), excl_rs=round(r.get("excluded", -1), 3),
               t_py=round(dt, 2), t_rs=round(r.get("t_total_s", -1), 2))
    for m in core.METHODS:
        sp = core.summarize(out["methods"][m], UM)
        sr = r.get("methods", {}).get(m, {})
        row[f"P_py_{m}"], row[f"P_rs_{m}"] = sp["P"], sr.get("P")
        row[f"G_py_{m}"] = round(sp["G"], 3) if sp["G"] is not None else None
        row[f"G_rs_{m}"] = round(sr["G"], 3) if sr.get("G") is not None else None
        row[f"dG_{m}"] = (round(sr["G"] - sp["G"], 3) if sr.get("G") is not None and sp["G"] is not None else None)
    rows.append(row)
    print(f"{name[:38]:38s} " + "  ".join(f"{m[:3]} ΔG={row['dG_' + m]:+.2f}" if row["dG_" + m] is not None else f"{m[:3]} –"
                                          for m in core.METHODS) + f"   fov {'=' if row['fov_py'] == row['fov_rs'] else '≠'}"
                                                                      f" bar {'=' if str(row['bar_py']) == str(row['bar_rs']) else '≠'}")

with open(out_csv, "w", newline="", encoding="utf-8-sig") as fh:
    w = csv.DictWriter(fh, fieldnames=list(rows[0].keys()))
    w.writeheader()
    w.writerows(rows)

print()
for m in core.METHODS:
    ds = [abs(r["dG_" + m]) for r in rows if r["dG_" + m] is not None]
    ds.sort()
    print(f"{m:7s}: n={len(ds)}  |ΔG| trung bình {sum(ds) / len(ds):.3f}  trung vị {ds[len(ds) // 2]:.3f}  "
          f"lớn nhất {ds[-1]:.3f}  ≤0,1: {sum(x <= 0.1 for x in ds)}  ≤0,25: {sum(x <= 0.25 for x in ds)}")
fov_same = sum(1 for r in rows if (r["fov_py"] == "none") == (r["fov_rs"] == "none"))
bar_same = sum(1 for r in rows if (r["bar_py"] == "") == (r["bar_rs"] == ""))
print(f"Trường nhìn tròn: cùng kết luận {fov_same}/{len(rows)};  thanh đỏ: cùng kết luận {bar_same}/{len(rows)}")
print(f"Thời gian: Python {t_py:.1f} s, Rust {t_rust:.1f} s (cả nạp ảnh) cho {len(rows)} ảnh")
print("CSV:", out_csv)
