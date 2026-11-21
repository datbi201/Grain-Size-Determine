# -*- coding: utf-8 -*-
"""Biên dịch Grain Size Determine thành bản exe (PyInstaller, thư mục onedir).

  py -3.12 tools/build_windows.py            # build + tự kiểm tra + đóng zip
Kết quả:
  ..\\02_LOCAL_RUNTIME\\GrainSizeDetermine\\GrainSizeDetermine.exe
  ..\\03_ONEDRIVE_BACKUP\\releases\\GrainSizeDetermine_Rev<ver>.zip
"""
import os
import re
import shutil
import subprocess
import sys
import tempfile

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ROOT = os.path.dirname(REPO)
RUNTIME = os.path.join(ROOT, "02_LOCAL_RUNTIME")
RELEASES = os.path.join(ROOT, "03_ONEDRIVE_BACKUP", "releases")
NAME = "GrainSizeDetermine"


def version():
    t = open(os.path.join(REPO, "src", "GrainSizeApp.py"), encoding="utf-8").read()
    return re.search(r'APP_VER = "([^"]+)"', t).group(1)


def main():
    ver = version()
    work = tempfile.mkdtemp(prefix="gsd_build_")
    dist = os.path.join(work, "dist")
    sep = ";"
    cmd = [sys.executable, "-m", "PyInstaller", "--noconfirm", "--windowed", "--name", NAME,
           "--icon", os.path.join(REPO, "assets", "app.ico"),
           "--add-data", os.path.join(REPO, "assets") + sep + "assets",
           "--collect-all", "tkinterdnd2", "--collect-all", "reportlab", "--collect-all", "pypdfium2",
           "--exclude-module", "matplotlib", "--exclude-module", "pytest",
           "--distpath", dist, "--workpath", os.path.join(work, "work"), "--specpath", work,
           os.path.join(REPO, "src", "GrainSizeApp.py")]
    print(" ".join(cmd))
    subprocess.check_call(cmd, cwd=os.path.join(REPO, "src"))
    exe_dir = os.path.join(dist, NAME)
    # tự kiểm tra bản exe (phân tích ảnh tổng hợp + xuất PDF)
    st = os.path.join(work, "selftest.txt")
    subprocess.call([os.path.join(exe_dir, NAME + ".exe"), "--selftest", st])
    res = open(st, encoding="utf-8").read() if os.path.exists(st) else "KHÔNG CHẠY ĐƯỢC"
    print("Tự kiểm tra exe:", res)
    if not res.startswith("OK") or "PDF_OK" not in res:
        sys.exit("Tự kiểm tra thất bại, không chép bản build.")
    target = os.path.join(RUNTIME, NAME)
    # robocopy /MIR: đồng bộ thư mục, chịu được tệp bị khoá tạm thời bởi trình đồng bộ đám mây
    rc = subprocess.call(["robocopy", exe_dir, target, "/MIR", "/R:3", "/W:2", "/NFL", "/NDL", "/NJH", "/NP"])
    if rc >= 8:
        sys.exit(f"Chép bản build thất bại (robocopy mã {rc}).")
    os.makedirs(RELEASES, exist_ok=True)
    z = shutil.make_archive(os.path.join(RELEASES, f"{NAME}_Rev{ver}"), "zip", dist, NAME)
    print("Đã chép:", target)
    print("Đã đóng gói:", z)
    shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    main()
