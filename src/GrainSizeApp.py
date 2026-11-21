# -*- coding: utf-8 -*-
"""
Grain Size Determine - đo cấp cỡ hạt trung bình theo ASTM E112-25 / E1382 từ ảnh kim tương.

Phương pháp: Abrams (3 vòng tròn), Heyn (đường thẳng), Chord hỗn hợp (Fig. 5: 3 vòng tròn + 2 đường chéo).
Chạy:  python src/GrainSizeApp.py   (hoặc GrainSizeDetermine.exe)
"""
import copy
import csv
import datetime
import json
import math
import os
import queue
import sys
import tempfile
import threading
import traceback
import zipfile
from collections import OrderedDict

import tkinter as tk
from tkinter import colorchooser, filedialog, messagebox, ttk

import cv2
import numpy as np

import grain_core as core

try:
    from tkinterdnd2 import DND_FILES, TkinterDnD
    HAVE_DND = True
except Exception:
    HAVE_DND = False

APP_NAME = "Grain Size Determine"
APP_VER = "0.1"
PROJ_EXT = ".gsd"
IMG_EXT = (".jpg", ".jpeg", ".png", ".bmp", ".tif", ".tiff", ".webp")
DATA_DIR = os.path.join(os.environ.get("APPDATA", os.path.expanduser("~")), "GrainSizeDetermine")
SETTINGS_FILE = os.path.join(DATA_DIR, "settings.json")
DEFAULT_COLORS = {"chord": "#00e5ff", "point": "#ff1744", "manual": "#ff00ff", "boundary": "#00c853"}
KIND_SHORT = {"cross": "Cắt biên (1)", "triple": "Điểm ba hạt (1,5)", "endpoint": "Đầu mút (0,5)"}


def resource(*parts):
    """Đường dẫn tài nguyên (chạy từ mã nguồn hoặc từ bản đóng gói PyInstaller)."""
    base = getattr(sys, "_MEIPASS", os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    p = os.path.join(base, *parts)
    if not os.path.exists(p):
        p = os.path.join(os.path.dirname(os.path.abspath(__file__)), *parts)
    return p


def set_icon(win):
    try:
        ico = resource("assets", "app.ico")
        if os.path.exists(ico):
            win.iconbitmap(default=ico)
    except Exception:
        pass


# --------------------------------------------------------------------------- #
class ScrollFrame(ttk.Frame):
    def __init__(self, parent):
        super().__init__(parent)
        self.cv = tk.Canvas(self, highlightthickness=0, width=395)
        vs = ttk.Scrollbar(self, orient="vertical", command=self.cv.yview)
        self.cv.configure(yscrollcommand=vs.set)
        vs.pack(side="right", fill="y")
        self.cv.pack(side="left", fill="both", expand=True)
        self.inner = ttk.Frame(self.cv)
        self._win = self.cv.create_window((0, 0), window=self.inner, anchor="nw")
        self.inner.bind("<Configure>", lambda e: self.cv.configure(scrollregion=self.cv.bbox("all")))
        self.cv.bind("<Configure>", lambda e: self.cv.itemconfig(self._win, width=e.width))
        self.bind("<Enter>", lambda e: self.cv.bind_all("<MouseWheel>", self._wheel))
        self.bind("<Leave>", lambda e: self.cv.unbind_all("<MouseWheel>"))

    def _wheel(self, e):
        if self.inner.winfo_reqheight() > self.cv.winfo_height():
            self.cv.yview_scroll(-1 if e.delta > 0 else 1, "units")


class Item:
    def __init__(self, path):
        self.path = path
        self.name = os.path.basename(path)
        self.size = None
        self.fov = None
        self.probed = False
        self.bar_box = None
        self.bar_px = None
        self.um_per_px = None
        self.calib = ""          # mô tả cách hiệu chuẩn
        self.data = None         # kết quả core.analyze (có thể đã sửa thủ công)
        self.undo = []
        self.include = True       # đưa vào báo cáo / thống kê
        self.state = "Chưa đo"
        self.note = ""

    def res(self, m):
        if self.data and m in self.data["methods"] and self.um_per_px:
            return core.summarize(self.data["methods"][m], self.um_per_px)
        return None


# --------------------------------------------------------------------------- #
class App:
    def __init__(self, root):
        self.root = root
        self.project_path = None
        self.dirty = False
        set_icon(root)
        root.geometry("1440x900")
        root.minsize(1150, 720)
        self.items = OrderedDict()
        self.cur = None
        self._img_cache = OrderedDict()
        self._base_cache = (None, None)
        self._disp_key = None
        self.settings = self._load_settings()
        self.last_um = float(self.settings.get("last_um", 100.0))
        self.colors = dict(DEFAULT_COLORS, **self.settings.get("colors", {}))
        self.disp = None
        self.zoom, self.ox, self.oy = 1.0, 0.0, 0.0
        self._photo = None
        self._press = None
        self.pick_mode = False
        self.pick_pts = []
        self.jobs = queue.Queue()
        self.results_q = queue.Queue()
        self.pending = 0
        self.done = 0
        threading.Thread(target=self._worker, daemon=True).start()
        self._build_style()
        self._build_menu()
        self._build_ui()
        self._bind_events()
        self._update_title()
        root.protocol("WM_DELETE_WINDOW", self.on_close)
        root.after(100, self._poll)
        if not HAVE_DND:
            self.status("Chưa có tkinterdnd2 nên chưa kéo thả được: dùng nút «Thêm ảnh…».")

    # ================================================================ UI ===
    def _build_style(self):
        st = ttk.Style()
        for t in ("vista", "xpnative", "clam"):
            if t in st.theme_names():
                st.theme_use(t)
                break
        st.configure("Treeview", rowheight=24)
        st.configure("Big.TLabel", font=("Segoe UI", 10, "bold"))
        st.configure("Accent.TButton", font=("Segoe UI", 9, "bold"))
        st.configure("Hint.TLabel", foreground="#555")

    def _build_ui(self):
        r = self.root
        tb = ttk.Frame(r, padding=(6, 4))
        tb.pack(fill="x")
        ttk.Button(tb, text="📂 Mở project", command=self.open_project).pack(side="left", padx=2)
        ttk.Button(tb, text="💾 Lưu project", command=self.save_project).pack(side="left", padx=2)
        ttk.Separator(tb, orient="vertical").pack(side="left", fill="y", padx=8)
        for text, cmd in (("➕ Thêm ảnh", self.add_files), ("📁 Thư mục", self.add_folder),
                          ("✖ Xóa chọn", self.remove_selected), ("🗑 Xóa hết", self.clear_all)):
            ttk.Button(tb, text=text, command=cmd).pack(side="left", padx=2)
        ttk.Separator(tb, orient="vertical").pack(side="left", fill="y", padx=8)
        ttk.Button(tb, text="▶ Đo ảnh chọn (F5)", style="Accent.TButton", command=self.run_selected).pack(side="left", padx=2)
        ttk.Button(tb, text="⏩ Đo tất cả", command=self.run_all).pack(side="left", padx=2)
        ttk.Separator(tb, orient="vertical").pack(side="left", fill="y", padx=8)
        ttk.Button(tb, text="📄 Báo cáo PDF…", style="Accent.TButton", command=self.report_dialog).pack(side="left", padx=2)
        ttk.Button(tb, text="💾 CSV…", command=self.export_csv).pack(side="left", padx=2)
        ttk.Button(tb, text="🖼 Lưu ảnh KQ…", command=self.save_overlays).pack(side="left", padx=2)
        ttk.Button(tb, text="❓ Hướng dẫn", command=self.show_help).pack(side="right", padx=2)

        mid = ttk.PanedWindow(r, orient="horizontal")
        left = ttk.Frame(mid)
        mid.add(left, weight=1)
        rightbox = ttk.Frame(mid, width=415)
        mid.add(rightbox, weight=0)
        sf = ScrollFrame(rightbox)
        sf.pack(fill="both", expand=True)

        # ---- thanh hiển thị
        vbar = ttk.Frame(left)
        vbar.pack(fill="x", pady=(0, 3))
        ttk.Label(vbar, text="Phương pháp:").pack(side="left")
        self.view_method = tk.StringVar(value=self.settings.get("view_method", "abrams"))
        for m in core.METHODS:
            ttk.Radiobutton(vbar, text=core.METHOD_SHORT[m], value=m, variable=self.view_method,
                            command=self.on_view_change).pack(side="left", padx=4)
        ttk.Separator(vbar, orient="vertical").pack(side="left", fill="y", padx=6)
        self.show_bound = tk.BooleanVar(value=True)
        self.show_pat = tk.BooleanVar(value=True)
        self.show_removed = tk.BooleanVar(value=False)
        ttk.Checkbutton(vbar, text="Biên hạt", variable=self.show_bound, command=self.refresh_view).pack(side="left")
        ttk.Checkbutton(vbar, text="Lưới & giao điểm", variable=self.show_pat, command=self.render).pack(side="left", padx=4)
        ttk.Checkbutton(vbar, text="Điểm đã loại", variable=self.show_removed, command=self.render).pack(side="left")
        ttk.Button(vbar, text="Vừa khung", command=self.fit).pack(side="right")
        ttk.Button(vbar, text="1:1", width=4, command=lambda: self.set_zoom(1.0)).pack(side="right", padx=4)

        self.hint_var = tk.StringVar(value="Kéo ảnh vào cửa sổ, hoặc bấm «Thêm ảnh…». Lăn chuột: phóng to · kéo: di chuyển · "
                                           "nhấp đúp: vừa khung.")
        ttk.Label(left, textvariable=self.hint_var, style="Hint.TLabel").pack(side="bottom", fill="x", pady=2)
        self.canvas = tk.Canvas(left, bg="#202020", highlightthickness=0, cursor="arrow")
        self.canvas.pack(fill="both", expand=True)

        self._build_right(sf.inner)

        # ---- đáy
        sbf = ttk.Frame(r, padding=(6, 2))
        sbf.pack(side="bottom", fill="x")
        self.status_var = tk.StringVar(value="Sẵn sàng.")
        ttk.Label(sbf, textvariable=self.status_var).pack(side="left")
        self.progress = ttk.Progressbar(sbf, length=220, mode="determinate")
        self.progress.pack(side="right")
        self.pooled_var = tk.StringVar(value="")
        ttk.Label(r, textvariable=self.pooled_var, font=("Segoe UI", 9, "bold"), foreground="#1F3B5B", justify="left",
                  padding=(8, 2)).pack(side="bottom", fill="x")
        bot = ttk.Frame(r, padding=(6, 4))
        bot.pack(side="bottom", fill="x")
        g3 = ttk.LabelFrame(bot, text=" Kết quả ảnh đang xem ", padding=6)
        g3.pack(side="right", fill="y", padx=(6, 0))
        self.res_text = tk.Text(g3, height=7, width=46, font=("Consolas", 9), relief="flat",
                                bg=self.root.cget("bg"), wrap="word", state="disabled")
        self.res_text.pack(fill="both", expand=True)
        self.res_text.tag_configure("h", font=("Consolas", 9, "bold"))
        self.res_text.tag_configure("g", font=("Consolas", 12, "bold"), foreground="#0a5")
        self.res_text.tag_configure("w", foreground="#c60")
        mid.pack(fill="both", expand=True, padx=6)

        cols = ("inc", "name", "scale", "ap", "ag", "hp", "hg", "mp", "mg", "state")
        heads = ("Báo cáo", "Ảnh", "µm/px", "Abrams P", "Abrams G", "Heyn P", "Heyn G", "Chord HH P", "Chord HH G",
                 "Trạng thái / ghi chú")
        widths = (74, 176, 66, 74, 74, 64, 64, 84, 84, 230)
        self.tree = ttk.Treeview(bot, columns=cols, show="headings", height=4, selectmode="extended")
        for c, h, w in zip(cols, heads, widths):
            self.tree.heading(c, text=h)
            self.tree.column(c, width=w, anchor="w" if c in ("name", "state") else "center")
        self.tree.heading("inc", text="Báo cáo ☑", command=self.toggle_all_include)
        sb = ttk.Scrollbar(bot, orient="vertical", command=self.tree.yview)
        self.tree.configure(yscrollcommand=sb.set)
        self.tree.pack(side="left", fill="both", expand=True)
        sb.pack(side="left", fill="y")
        if HAVE_DND:
            self.root.drop_target_register(DND_FILES)
            self.root.dnd_bind("<<Drop>>", self.on_drop)

    def _build_right(self, parent):
        s = self.settings
        # ---- 1. Thang đo
        g1 = ttk.LabelFrame(parent, text=" 1. Thang đo (scale) ", padding=6)
        g1.pack(fill="x", pady=(0, 6))
        b = ttk.Frame(g1)
        b.pack(fill="x")
        ttk.Button(b, text="🔍 Tự tìm thanh đỏ", command=self.auto_bar).pack(side="left", expand=True, fill="x")
        ttk.Button(b, text="📏 Chọn 2 điểm", command=self.start_pick).pack(side="left", expand=True, fill="x", padx=(4, 0))
        f = ttk.Frame(g1)
        f.pack(fill="x", pady=4)
        ttk.Label(f, text="Độ dài thanh (px):").grid(row=0, column=0, sticky="w")
        self.px_var = tk.StringVar()
        ttk.Entry(f, textvariable=self.px_var, width=9).grid(row=0, column=1, padx=4, pady=1)
        ttk.Label(f, text="Độ dài thực (µm):").grid(row=1, column=0, sticky="w")
        self.um_var = tk.StringVar(value=f"{self.last_um:g}")
        e = ttk.Entry(f, textvariable=self.um_var, width=10)
        e.grid(row=1, column=1, padx=4, pady=1)
        e.bind("<Return>", lambda ev: self.apply_scale(False))
        self.calc_var = tk.StringVar()
        ttk.Label(f, textvariable=self.calc_var, style="Big.TLabel").grid(row=2, column=0, columnspan=2, sticky="w")
        self.px_var.trace_add("write", lambda *a: self._update_calc())
        self.um_var.trace_add("write", lambda *a: self._update_calc())
        ttk.Button(g1, text="✔ Áp dụng cho ảnh này", style="Accent.TButton",
                   command=lambda: self.apply_scale(False)).pack(fill="x")
        ttk.Button(g1, text="✔✔ Áp dụng cho các ảnh đã chọn",
                   command=lambda: self.apply_scale(True)).pack(fill="x", pady=(3, 0))
        ttk.Separator(g1).pack(fill="x", pady=5)
        f2 = ttk.Frame(g1)
        f2.pack(fill="x")
        ttk.Label(f2, text="Hoặc nhập µm/px:").grid(row=0, column=0, sticky="w")
        self.direct_var = tk.StringVar()
        ttk.Entry(f2, textvariable=self.direct_var, width=9).grid(row=0, column=1, padx=4, pady=1)
        ttk.Button(f2, text="Đặt", width=5, command=self.apply_direct).grid(row=0, column=2)
        ttk.Label(f2, text="Hoặc Ø trường nhìn (µm):").grid(row=1, column=0, sticky="w")
        self.fov_var = tk.StringVar()
        ttk.Entry(f2, textvariable=self.fov_var, width=9).grid(row=1, column=1, padx=4, pady=1)
        ttk.Button(f2, text="Đặt", width=5, command=self.apply_fov).grid(row=1, column=2)
        self.scale_state = tk.StringVar(value="Chưa chọn ảnh.")
        ttk.Label(g1, textvariable=self.scale_state, foreground="#0a5", wraplength=320).pack(fill="x", pady=(4, 0))

        # ---- 2. Phương pháp & tham số
        g2 = ttk.LabelFrame(parent, text=" 2. Phương pháp và tham số đo ", padding=6)
        g2.pack(fill="x", pady=(0, 6))
        self.m_vars = {}
        en = s.get("methods", list(core.METHODS))
        for m in core.METHODS:
            v = tk.BooleanVar(value=m in en)
            self.m_vars[m] = v
            ttk.Checkbutton(g2, text=core.METHOD_VI[m], variable=v).pack(anchor="w")
        self.hv_var = tk.BooleanVar(value=bool(s.get("mixed_hv", False)))
        ttk.Checkbutton(g2, text="   Chord HH: thêm 2 đường 100 mm (Fig. 5 đầy đủ)", variable=self.hv_var).pack(anchor="w")
        r2 = ttk.Frame(g2)
        r2.pack(fill="x", pady=(2, 0))
        ttk.Label(r2, text="Số đường Heyn mỗi hướng:").pack(side="left")
        self.nl_var = tk.IntVar(value=int(s.get("n_lines", 5)))
        ttk.Spinbox(r2, from_=3, to=12, width=4, textvariable=self.nl_var).pack(side="left", padx=4)
        r1 = ttk.Frame(g2)
        r1.pack(fill="x", pady=(4, 0))
        ttk.Label(r1, text="Độ nhạy tách biên hạt:").pack(side="left")
        self.sens_var = tk.DoubleVar(value=float(s.get("sens", 85)))
        self.sens_lbl = ttk.Label(r1, text=f"{self.sens_var.get():.0f}")
        self.sens_lbl.pack(side="right")
        ttk.Scale(g2, from_=70, to=95, variable=self.sens_var,
                  command=lambda v: self.sens_lbl.config(text=f"{float(v):.0f}")).pack(fill="x")
        ttk.Label(g2, text="↑ tăng nếu đếm dư nhiễu · ↓ giảm nếu sót biên nhạt", style="Hint.TLabel").pack(anchor="w")
        self.focus_var = tk.BooleanVar(value=bool(s.get("focus", True)))
        ttk.Checkbutton(g2, text="Loại vùng mờ / nhòe khỏi phép đo", variable=self.focus_var).pack(anchor="w", pady=(3, 0))

        # ---- 3. Xử lý ảnh
        g4 = ttk.LabelFrame(parent, text=" 3. Xử lý ảnh (sáng / tương phản) ", padding=6)
        g4.pack(fill="x", pady=(0, 6))
        en_ = s.get("enhance", {})
        self.br_var = tk.DoubleVar(value=en_.get("brightness", 0))
        self.ct_var = tk.DoubleVar(value=en_.get("contrast", 1.0) * 100)
        self.gm_var = tk.DoubleVar(value=en_.get("gamma", 1.0) * 100)
        self.clahe_var = tk.BooleanVar(value=en_.get("clahe", False))
        self._enh_labels = {}
        for key, text, var, lo, hi, fmt in (("br", "Độ sáng", self.br_var, -100, 100, "{:+.0f}"),
                                            ("ct", "Tương phản (%)", self.ct_var, 30, 300, "{:.0f}"),
                                            ("gm", "Gamma (×100)", self.gm_var, 30, 300, "{:.0f}")):
            row = ttk.Frame(g4)
            row.pack(fill="x")
            ttk.Label(row, text=text, width=15).pack(side="left")
            lb = ttk.Label(row, text=fmt.format(var.get()), width=5)
            lb.pack(side="right")
            self._enh_labels[key] = (lb, fmt, var)
            ttk.Scale(row, from_=lo, to=hi, variable=var, command=lambda v, k=key: self._on_enh(k)).pack(
                side="left", fill="x", expand=True)
        r4 = ttk.Frame(g4)
        r4.pack(fill="x", pady=(2, 0))
        ttk.Checkbutton(r4, text="CLAHE (cân bằng sáng cục bộ)", variable=self.clahe_var,
                        command=lambda: self._on_enh(None)).pack(side="left")
        ttk.Button(r4, text="Đặt lại", width=8, command=self.reset_enh).pack(side="right")
        ttk.Button(g4, text="✨ Tự động chỉnh cho rõ biên hạt", command=self.auto_enh).pack(fill="x", pady=(3, 2))
        self.enh_detect = tk.BooleanVar(value=bool(s.get("enh_detect", False)))
        ttk.Checkbutton(g4, text="Dùng ảnh đã xử lý để tách biên tự động", variable=self.enh_detect).pack(anchor="w")

        # ---- 4. Màu
        g5 = ttk.LabelFrame(parent, text=" 4. Màu hiển thị ", padding=6)
        g5.pack(fill="x", pady=(0, 6))
        self._color_btns = {}
        for i, (key, text) in enumerate((("chord", "Đường chord"), ("point", "Giao điểm tự động"),
                                         ("manual", "Giao điểm thủ công"), ("boundary", "Biên hạt"))):
            fr = ttk.Frame(g5)
            fr.grid(row=i // 2, column=i % 2, sticky="w", padx=2, pady=2)
            btn = tk.Button(fr, width=3, bg=self.colors[key], relief="groove",
                            command=lambda k=key: self.pick_color(k))
            btn.pack(side="left")
            ttk.Label(fr, text=text).pack(side="left", padx=4)
            self._color_btns[key] = btn

        # ---- 5. Sửa giao điểm thủ công
        g6 = ttk.LabelFrame(parent, text=" 5. Sửa giao điểm thủ công ", padding=6)
        g6.pack(fill="x", pady=(0, 6))
        self.edit_var = tk.BooleanVar(value=False)
        ttk.Checkbutton(g6, text="✎ Bật chế độ sửa (phím E)", variable=self.edit_var,
                        command=self._on_edit_toggle).pack(anchor="w")
        self.kind_var = tk.StringVar(value="cross")
        kr = ttk.Frame(g6)
        kr.pack(fill="x")
        for k in ("cross", "triple", "endpoint"):
            ttk.Radiobutton(kr, text=KIND_SHORT[k], value=k, variable=self.kind_var).pack(anchor="w")
        ttk.Label(g6, style="Hint.TLabel", wraplength=320, justify="left",
                  text="Click trái gần đường chord: thêm điểm (tự hít vào chord).\n"
                       "Click phải lên điểm: loại điểm.\nShift + click trái: khôi phục điểm tự động đã loại.\n"
                       "Ctrl+Z: hoàn tác.").pack(anchor="w", pady=(2, 2))
        er = ttk.Frame(g6)
        er.pack(fill="x")
        ttk.Button(er, text="↶ Hoàn tác", command=self.undo).pack(side="left", expand=True, fill="x")
        ttk.Button(er, text="Về kết quả tự động", command=self.reset_edits).pack(side="left", expand=True, fill="x",
                                                                               padx=(4, 0))

    def _bind_events(self):
        c, r = self.canvas, self.root
        c.bind("<Configure>", lambda e: self.render() if self.disp is not None else None)
        c.bind("<MouseWheel>", self._on_wheel)
        c.bind("<ButtonPress-1>", self._on_press)
        c.bind("<B1-Motion>", self._on_drag)
        c.bind("<ButtonRelease-1>", self._on_release)
        c.bind("<Button-3>", self._on_right)
        c.bind("<Double-Button-1>", lambda e: self.fit())
        self.tree.bind("<<TreeviewSelect>>", self.on_select)
        self.tree.bind("<Button-1>", self._on_tree_click, add="+")
        self.tree.bind("<Delete>", lambda e: self.remove_selected())
        r.bind("<F5>", lambda e: self.run_selected())
        r.bind("<Control-i>", lambda e: self.add_files())
        r.bind("<Control-o>", lambda e: self.open_project())
        r.bind("<Control-s>", lambda e: self.save_project())
        r.bind("<Control-n>", lambda e: self.new_project())
        r.bind("<Control-z>", lambda e: self.undo())
        r.bind("<Escape>", lambda e: self.cancel_pick())
        r.bind("<KeyPress-e>", self._key_e)

    def _key_e(self, e):
        if isinstance(e.widget, (tk.Entry, ttk.Entry, ttk.Spinbox, tk.Text)):
            return
        self.edit_var.set(not self.edit_var.get())
        self._on_edit_toggle()

    # ========================================================== settings ===
    def _load_settings(self):
        try:
            with open(SETTINGS_FILE, "r", encoding="utf-8") as f:
                return json.load(f)
        except Exception:
            return {}

    def _save_settings(self):
        try:
            os.makedirs(DATA_DIR, exist_ok=True)
            self.settings.update(sens=float(self.sens_var.get()), focus=bool(self.focus_var.get()),
                                 n_lines=int(self.nl_var.get()), last_um=self.last_um, colors=self.colors,
                                 methods=[m for m, v in self.m_vars.items() if v.get()],
                                 mixed_hv=bool(self.hv_var.get()), enhance=self.enh_params(),
                                 enh_detect=bool(self.enh_detect.get()), view_method=self.view_method.get())
            with open(SETTINGS_FILE, "w", encoding="utf-8") as f:
                json.dump(self.settings, f, ensure_ascii=False, indent=1)
        except Exception:
            pass

    def _remember_scale(self, it):
        sc = self.settings.setdefault("scales", {})
        sc[os.path.abspath(it.path)] = {"um_per_px": it.um_per_px, "calib": it.calib,
                                        "bar_box": list(it.bar_box) if it.bar_box else None}
        self._save_settings()

    # ============================================================ project ===
    def _build_menu(self):
        mb = tk.Menu(self.root)
        fm = tk.Menu(mb, tearoff=0)
        fm.add_command(label="Project mới", accelerator="Ctrl+N", command=self.new_project)
        fm.add_command(label="Mở project…", accelerator="Ctrl+O", command=self.open_project)
        fm.add_command(label="Lưu project", accelerator="Ctrl+S", command=self.save_project)
        fm.add_command(label="Lưu project thành…", command=lambda: self.save_project(as_new=True))
        fm.add_separator()
        fm.add_command(label="Thêm ảnh…", accelerator="Ctrl+I", command=self.add_files)
        fm.add_command(label="Thêm thư mục ảnh…", command=self.add_folder)
        fm.add_separator()
        fm.add_command(label="Lập báo cáo PDF…", command=self.report_dialog)
        fm.add_command(label="Xuất CSV…", command=self.export_csv)
        fm.add_separator()
        fm.add_command(label="Thoát", command=self.on_close)
        mb.add_cascade(label="Tệp", menu=fm)
        hm = tk.Menu(mb, tearoff=0)
        hm.add_command(label="Hướng dẫn", command=self.show_help)
        hm.add_command(label="Giới thiệu", command=lambda: messagebox.showinfo(
            APP_NAME, f"{APP_NAME} Rev {APP_VER}\nĐo cấp cỡ hạt trung bình theo ASTM E112-25 / E1382-97(2023).\n"
                      "Phương pháp: Abrams, Heyn, Chord hỗn hợp (Fig. 5)."))
        mb.add_cascade(label="Trợ giúp", menu=hm)
        self.root.config(menu=mb)

    def _update_title(self):
        name = os.path.basename(self.project_path) if self.project_path else "Project chưa lưu"
        self.root.title(f"{'● ' if self.dirty else ''}{name} – {APP_NAME} Rev {APP_VER} (ASTM E112 / E1382)")

    def _set_dirty(self, v=True):
        if self.dirty != v:
            self.dirty = v
            self._update_title()

    def _ask_save(self):
        """True = được phép tiếp tục (đã lưu hoặc không cần lưu), False = hủy."""
        if not self.dirty or not self.items:
            return True
        a = messagebox.askyesnocancel(APP_NAME, "Project có thay đổi chưa lưu. Lưu trước khi tiếp tục?")
        if a is None:
            return False
        if a:
            return self.save_project()
        return True

    def new_project(self):
        if not self._ask_save():
            return
        self.tree.delete(*self.tree.get_children())
        self.items.clear()
        self.cur = None
        self.disp = None
        self.canvas.delete("all")
        self._show_results()
        self.update_pooled()
        self.project_path = None
        self._set_dirty(False)
        self._update_title()
        self.status("Project mới.")

    def save_project(self, as_new=False):
        path = self.project_path
        if as_new or not path:
            path = filedialog.asksaveasfilename(title="Lưu project", defaultextension=PROJ_EXT,
                                                initialfile="Project_CoHat" + PROJ_EXT,
                                                filetypes=[("Project Grain Size Determine", "*" + PROJ_EXT)])
            if not path:
                return False
        try:
            write_project(path, self)
        except Exception as ex:
            traceback.print_exc()
            messagebox.showerror(APP_NAME, f"Không lưu được project: {ex}")
            return False
        self.project_path = path
        self._set_dirty(False)
        self._update_title()
        self.status(f"Đã lưu project: {path}")
        return True

    def open_project(self, path=None):
        if not self._ask_save():
            return
        if not path:
            path = filedialog.askopenfilename(title="Mở project",
                                              filetypes=[("Project Grain Size Determine", "*" + PROJ_EXT),
                                                         ("Tất cả", "*.*")])
            if not path:
                return
        try:
            proj = read_project(path)
        except Exception as ex:
            traceback.print_exc()
            messagebox.showerror(APP_NAME, f"Không mở được project: {ex}")
            return
        self.tree.delete(*self.tree.get_children())
        self.items.clear()
        self.cur = None
        self.disp = None
        self.canvas.delete("all")
        s = proj.get("settings", {})
        for m, v in self.m_vars.items():
            v.set(m in s.get("methods", core.METHODS))
        self.hv_var.set(s.get("mixed_hv", False))
        self.nl_var.set(s.get("n_lines", 5))
        self.sens_var.set(s.get("sens", 85))
        self.sens_lbl.config(text=f"{self.sens_var.get():.0f}")
        self.focus_var.set(s.get("focus", True))
        e = s.get("enhance", {})
        self.br_var.set(e.get("brightness", 0))
        self.ct_var.set(e.get("contrast", 1.0) * 100)
        self.gm_var.set(e.get("gamma", 1.0) * 100)
        self.clahe_var.set(e.get("clahe", False))
        self.enh_detect.set(s.get("enh_detect", False))
        for k, c in s.get("colors", {}).items():
            if k in self.colors:
                self.colors[k] = c
                self._color_btns[k].config(bg=c)
        if s.get("view_method") in core.METHODS:
            self.view_method.set(s["view_method"])
        if proj.get("report"):
            self.settings["report"] = dict(self.settings.get("report", {}), **proj["report"])
        missing = []
        for it in proj["items"]:
            iid = f"i{len(self.items)}_{id(it)}"
            self.items[iid] = it
            if not os.path.exists(it.path):
                missing.append(it.name)
                it.state = "Không tìm thấy ảnh"
            self.tree.insert("", "end", iid=iid, values=self._row(it))
        for k, (lb, fmt, var) in self._enh_labels.items():
            lb.config(text=fmt.format(round(float(var.get()))))
        self.project_path = path
        self._set_dirty(False)
        self._update_title()
        self.update_pooled()
        if self.items:
            first = next(i for i, it in self.items.items() if os.path.exists(it.path)) if any(
                os.path.exists(it.path) for it in self.items.values()) else None
            if first:
                self.tree.selection_set(first)
        self.status(f"Đã mở project: {path} ({len(self.items)} ảnh)")
        if missing:
            messagebox.showwarning(APP_NAME, "Không tìm thấy các ảnh sau (đã di chuyển hoặc đổi tên?):\n"
                                   + "\n".join(missing[:15]) + ("\n…" if len(missing) > 15 else ""))

    def on_close(self):
        if not self._ask_save():
            return
        self._save_settings()
        self.root.destroy()

    # ============================================================= files ===
    def status(self, msg):
        self.status_var.set(msg)

    def add_files(self):
        self.add_paths(filedialog.askopenfilenames(
            title="Chọn ảnh kim tương",
            filetypes=[("Ảnh", "*.jpg *.jpeg *.png *.bmp *.tif *.tiff *.webp"), ("Tất cả", "*.*")]))

    def add_folder(self):
        d = filedialog.askdirectory(title="Chọn thư mục ảnh")
        if d:
            self.add_paths([d])

    def on_drop(self, event):
        self.add_paths(self.root.tk.splitlist(event.data))

    def add_paths(self, paths):
        files = []
        for p in paths:
            p = str(p).strip("{}")
            if os.path.isdir(p):
                files += [os.path.join(p, f) for f in sorted(os.listdir(p)) if f.lower().endswith(IMG_EXT)]
            elif os.path.isfile(p) and p.lower().endswith(IMG_EXT):
                files.append(p)
        have = {os.path.abspath(i.path) for i in self.items.values()}
        scales = self.settings.get("scales", {})
        added = []
        for f in files:
            ap = os.path.abspath(f)
            if ap in have:
                continue
            have.add(ap)
            it = Item(f)
            c = scales.get(ap)
            if c and c.get("um_per_px"):
                it.um_per_px = c["um_per_px"]
                it.calib = c.get("calib", "")
                it.bar_box = tuple(c["bar_box"]) if c.get("bar_box") else None
                it.state = "Đã có scale (chưa đo)"
            else:
                it.state = "Cần scale"
            iid = f"i{len(self.items)}_{id(it)}"
            self.items[iid] = it
            self.tree.insert("", "end", iid=iid, values=self._row(it))
            added.append(iid)
        if added:
            self._set_dirty()
            self.tree.selection_set(added[0])
            self.tree.focus(added[0])
            self.status(f"Đã thêm {len(added)} ảnh.")
        elif files:
            self.status("Các ảnh này đã có trong danh sách.")

    def remove_selected(self):
        if self.tree.selection():
            self._set_dirty()
        for iid in self.tree.selection():
            it = self.items.pop(iid, None)
            self.tree.delete(iid)
            if it is self.cur:
                self.cur = None
                self.disp = None
                self.canvas.delete("all")
                self._show_results()
        self.update_pooled()

    def clear_all(self):
        if self.items and messagebox.askyesno(APP_NAME, "Xóa toàn bộ danh sách ảnh?"):
            self.tree.delete(*self.tree.get_children())
            self.items.clear()
            self.cur = None
            self.disp = None
            self.canvas.delete("all")
            self._show_results()
            self.update_pooled()

    def get_img(self, it):
        if it.path in self._img_cache:
            self._img_cache.move_to_end(it.path)
            return self._img_cache[it.path]
        img = core.imread(it.path)
        if img is None:
            return None
        self._img_cache[it.path] = img
        while len(self._img_cache) > 3:
            self._img_cache.popitem(last=False)
        return img

    def _probe(self, it, img):
        if not it.probed:
            it.size = (img.shape[1], img.shape[0])
            it.fov = core.detect_fov(img)
            bar = core.find_red_bar(img)
            if bar:
                it.bar_px = bar[0]
                if it.bar_box is None:
                    it.bar_box = bar[1]
            it.probed = True

    # ============================================================== table ===
    def _row(self, it):
        vals = ["☑" if it.include else "☐", it.name, f"{it.um_per_px:.4f}" if it.um_per_px else "–"]
        for m in core.METHODS:
            r = it.res(m)
            vals += [f"{r['P']:g}" if r else "–", f"{r['G']:.2f}" if r and r["G"] is not None else "–"]
        edited = [core.METHOD_SHORT[m] for m, md in (it.data or {}).get("methods", {}).items() if md.get("edited")]
        note = it.state + (" · sửa TC: " + ", ".join(edited) if edited else "") + (f" · {it.note}" if it.note else "")
        return vals + [note]

    def _refresh_row(self, it):
        iid = self._iid_of(it)
        if iid and self.tree.exists(iid):
            self.tree.item(iid, values=self._row(it))

    def _iid_of(self, it):
        for k, v in self.items.items():
            if v is it:
                return k
        return None

    def on_select(self, ev=None):
        self.update_pooled()
        sel = self.tree.selection()
        if sel:
            it = self.items.get(sel[0])
            if it is not None and it is not self.cur:
                self.set_current(it)

    def set_current(self, it):
        img = self.get_img(it)
        if img is None:
            self.status(f"Không mở được ảnh: {it.name}")
            return
        self._probe(it, img)
        self.cur = it
        self.cancel_pick()
        self.px_var.set(f"{it.bar_px:.0f}" if it.bar_px else "")
        self.direct_var.set(f"{it.um_per_px:.5f}" if it.um_per_px else "")
        self.fov_var.set("")
        self._update_scale_state()
        self._show_results()
        self.refresh_view(fit=True)
        info = f"{it.name}: {it.size[0]}×{it.size[1]} px" + (f", trường nhìn tròn R = {it.fov[2]:.0f} px" if it.fov else ", toàn khung")
        if it.bar_px:
            info += f", thanh đỏ {it.bar_px:.0f} px"
        self.status(info)

    # ============================================================== scale ===
    def _update_calc(self):
        try:
            px = float(self.px_var.get().replace(",", "."))
            um = float(self.um_var.get().replace(",", "."))
            self.calc_var.set(f"= {um / px:.5f} µm/px" if px > 0 and um > 0 else "")
        except ValueError:
            self.calc_var.set("")

    def _update_scale_state(self):
        it = self.cur
        if it is None:
            self.scale_state.set("Chưa chọn ảnh.")
        elif it.um_per_px:
            self.scale_state.set(f"Scale: {it.um_per_px:.5f} µm/px · {it.calib or '—'}")
        else:
            self.scale_state.set("Ảnh chưa có scale: «Tự tìm thanh đỏ» hoặc «Chọn 2 điểm», nhập µm rồi Áp dụng.")

    def auto_bar(self):
        it = self.cur
        if it is None:
            return
        bar = core.find_red_bar(self.get_img(it))
        if not bar:
            messagebox.showinfo(APP_NAME, "Không tìm thấy thanh scale màu đỏ ở nửa dưới ảnh.\nHãy dùng «Chọn 2 điểm».")
            return
        it.bar_px, it.bar_box = bar
        self.px_var.set(f"{bar[0]:.0f}")
        self.render()
        self.status(f"Thanh đỏ dài {bar[0]:.0f} px. Nhập độ dài thực (µm) ghi trên nhãn rồi Áp dụng.")

    def start_pick(self):
        if self.cur is None:
            return
        self.edit_var.set(False)
        self.pick_mode = True
        self.pick_pts = []
        self.canvas.config(cursor="crosshair")
        self.render()
        self.status("Click vào 2 đầu của thanh scale (Esc để hủy).")

    def cancel_pick(self):
        self.pick_mode = False
        self.pick_pts = []
        self.canvas.config(cursor="tcross" if self.edit_var.get() else "arrow")
        self.render()

    def apply_scale(self, to_selected):
        it = self.cur
        if it is None:
            return
        try:
            px = float(self.px_var.get().replace(",", "."))
            um = float(self.um_var.get().replace(",", "."))
            assert px > 0 and um > 0
        except Exception:
            messagebox.showwarning(APP_NAME, "Cần độ dài thanh (px) và độ dài thực (µm) hợp lệ.")
            return
        self.last_um = um
        desc = f"thanh tỉ lệ {um:g} µm = {px:.1f} px"
        if to_selected:
            its = [self.items[i] for i in self.tree.selection() if i in self.items]
            if it not in its:
                its.append(it)
            if not self._confirm_overwrite(its):
                return
            for o in its:
                self._set_scale(o, um / px, desc if o is it else f"dùng chung scale của {it.name}", probe=True)
            self._queue(its, confirm=False)
        else:
            if not self._confirm_overwrite([it]):
                return
            self._set_scale(it, um / px, desc)
            self._queue([it], confirm=False)

    def apply_direct(self):
        it = self.cur
        if it is None:
            return
        try:
            v = float(self.direct_var.get().replace(",", "."))
            assert v > 0
        except Exception:
            messagebox.showwarning(APP_NAME, "Giá trị µm/px không hợp lệ.")
            return
        if self._confirm_overwrite([it]):
            self._set_scale(it, v, "nhập trực tiếp µm/px")
            self._queue([it], confirm=False)

    def apply_fov(self):
        it = self.cur
        if it is None:
            return
        if not it.fov:
            messagebox.showinfo(APP_NAME, "Ảnh này không có trường nhìn tròn nhận diện được.")
            return
        try:
            d = float(self.fov_var.get().replace(",", "."))
            assert d > 0
        except Exception:
            messagebox.showwarning(APP_NAME, "Đường kính trường nhìn không hợp lệ.")
            return
        if self._confirm_overwrite([it]):
            self._set_scale(it, d / (2 * it.fov[2]), f"đường kính trường nhìn {d:g} µm")
            self._queue([it], confirm=False)

    def _set_scale(self, it, um_per_px, calib, probe=False):
        if probe and not it.probed:
            img = self.get_img(it)
            if img is not None:
                self._probe(it, img)
        it.um_per_px = um_per_px
        it.calib = calib
        it.state = "Đã có scale (chưa đo)"
        self._remember_scale(it)
        self._set_dirty()
        self._refresh_row(it)
        if it is self.cur:
            self.direct_var.set(f"{um_per_px:.5f}")
            self._update_scale_state()

    # ============================================================ enhance ===
    def enh_params(self):
        return dict(brightness=round(float(self.br_var.get())), contrast=round(float(self.ct_var.get())) / 100.0,
                    gamma=round(float(self.gm_var.get())) / 100.0, clahe=bool(self.clahe_var.get()))

    def _on_enh(self, key):
        for k, (lb, fmt, var) in self._enh_labels.items():
            lb.config(text=fmt.format(round(float(var.get()))))
        if getattr(self, "_enh_job", None):
            self.root.after_cancel(self._enh_job)
        self._enh_job = self.root.after(120, self.refresh_view)
        self._set_dirty()

    def auto_enh(self):
        it = self.cur
        if it is None:
            self.status("Chọn một ảnh trước.")
            return
        e = core.auto_enhance(self.get_img(it), it.fov)
        self.br_var.set(e["brightness"])
        self.ct_var.set(e["contrast"] * 100)
        self.gm_var.set(e["gamma"] * 100)
        self.clahe_var.set(e["clahe"])
        self._on_enh(None)
        self.status("Đã tự chỉnh: " + core.enhance_desc(e) + ". Bật «Dùng ảnh đã xử lý để tách biên» rồi đo lại "
                    "nếu muốn áp dụng cho phép đo.")

    def reset_enh(self):
        self.br_var.set(0)
        self.ct_var.set(100)
        self.gm_var.set(100)
        self.clahe_var.set(False)
        self._on_enh(None)

    def pick_color(self, key):
        c = colorchooser.askcolor(color=self.colors[key], title="Chọn màu")[1]
        if c:
            self.colors[key] = c
            self._color_btns[key].config(bg=c)
            self._save_settings()
            self.refresh_view()

    # ============================================================= viewer ===
    def on_view_change(self):
        self._show_results()
        self.render()

    def _base(self, it):
        key = (it.path, tuple(sorted(self.enh_params().items())))
        if self._base_cache[0] == key:
            return self._base_cache[1]
        img = self.get_img(it)
        b = core.enhance(img, **self.enh_params())
        self._base_cache = (key, b)
        return b

    def refresh_view(self, fit=False):
        it = self.cur
        if it is None:
            return
        base = self._base(it)
        if it.data is not None:
            vis = base.copy()
            dm = core.dark_mask(it.data)
            vis[dm] = (vis[dm] * 0.4).astype(np.uint8)
            if self.show_bound.get():
                vis[core.unpack(it.data["skeleton"])] = core.hex2bgr(self.colors["boundary"])
        else:
            vis = base
        self.disp = vis
        if fit:
            self.fit()
        else:
            self.render()

    def fit(self):
        if self.disp is None:
            return
        H, W = self.disp.shape[:2]
        cw, ch = max(self.canvas.winfo_width(), 50), max(self.canvas.winfo_height(), 50)
        self.zoom = min(cw / W, ch / H)
        self.ox = (W - cw / self.zoom) / 2
        self.oy = (H - ch / self.zoom) / 2
        self.render()

    def set_zoom(self, z):
        if self.disp is None:
            return
        cw, ch = self.canvas.winfo_width(), self.canvas.winfo_height()
        ix, iy = self.ox + cw / 2 / self.zoom, self.oy + ch / 2 / self.zoom
        self.zoom = z
        self.ox, self.oy = ix - cw / 2 / z, iy - ch / 2 / z
        self.render()

    def render(self):
        self.canvas.delete("all")
        arr = self.disp
        if arr is None:
            return
        H, W = arr.shape[:2]
        cw, ch = self.canvas.winfo_width(), self.canvas.winfo_height()
        z, x0, y0 = self.zoom, self.ox, self.oy
        ix0, iy0 = max(0, int(math.floor(x0))), max(0, int(math.floor(y0)))
        ix1, iy1 = min(W, int(math.ceil(x0 + cw / z))), min(H, int(math.ceil(y0 + ch / z)))
        if ix1 > ix0 and iy1 > iy0:
            sub = arr[iy0:iy1, ix0:ix1]
            ow, oh = max(1, int(round((ix1 - ix0) * z))), max(1, int(round((iy1 - iy0) * z)))
            interp = cv2.INTER_AREA if z < 1 else (cv2.INTER_NEAREST if z >= 4 else cv2.INTER_LINEAR)
            rgb = cv2.cvtColor(cv2.resize(sub, (ow, oh), interpolation=interp), cv2.COLOR_BGR2RGB)
            self._photo = tk.PhotoImage(data=b"P6 %d %d 255\n" % (ow, oh) + rgb.tobytes())
            self.canvas.create_image((ix0 - x0) * z, (iy0 - y0) * z, anchor="nw", image=self._photo)
        self._draw_vectors()

    def _c(self, x, y):
        return (x - self.ox) * self.zoom, (y - self.oy) * self.zoom

    def _draw_vectors(self):
        it = self.cur
        if it is None:
            return
        cv = self.canvas
        if it.data is None:
            if it.fov:
                cx, cy, R = it.fov
                a, b = self._c(cx - R, cy - R)
                c, d = self._c(cx + R, cy + R)
                cv.create_oval(a, b, c, d, outline="#00bcd4", dash=(4, 4))
        if it.bar_box and (it.data is None or self.pick_mode):
            x, y, w, h = it.bar_box
            a, b = self._c(x, y)
            c, d = self._c(x + w, y + h)
            cv.create_rectangle(a - 2, b - 2, c + 2, d + 2, outline="#00e676", width=2)
            cv.create_text(a, b - 10, text="thanh scale", fill="#00e676", anchor="sw")
        md = it.data["methods"].get(self.view_method.get()) if it.data else None
        if md and self.show_pat.get():
            col = self.colors["chord"]
            w = 2 if self.zoom >= 0.5 else 1
            for ch in md["chords"]:
                if ch["kind"] == "circle":
                    a, b = self._c(ch["cx"] - ch["r"], ch["cy"] - ch["r"])
                    c, d = self._c(ch["cx"] + ch["r"], ch["cy"] + ch["r"])
                    cv.create_oval(a, b, c, d, outline=col, width=w)
                else:
                    cv.create_line(*self._c(*ch["a"]), *self._c(*ch["b"]), fill=col, width=w)
            r = 4 if self.zoom < 1.5 else 5
            for e in md["events"]:
                x, y = self._c(e["x"], e["y"])
                if e.get("removed"):
                    if self.show_removed.get() or self.edit_var.get():
                        cv.create_line(x - r, y - r, x + r, y + r, fill="#9e9e9e", width=2)
                        cv.create_line(x - r, y + r, x + r, y - r, fill="#9e9e9e", width=2)
                    continue
                c = self.colors["manual"] if e["src"] == "manual" else self.colors["point"]
                if e["kind"] == "triple":
                    cv.create_oval(x - r - 3, y - r - 3, x + r + 3, y + r + 3, outline=c, width=2)
                    cv.create_oval(x - r + 1, y - r + 1, x + r - 1, y + r - 1, fill=c, outline="")
                elif e["kind"] == "endpoint":
                    cv.create_oval(x - r - 1, y - r - 1, x + r + 1, y + r + 1, outline=c, width=2)
                elif e["src"] == "manual":
                    cv.create_rectangle(x - r, y - r, x + r, y + r, fill=c, outline="white")
                else:
                    cv.create_oval(x - r, y - r, x + r, y + r, fill=c, outline="")
        for (px, py) in [self._c(*p) for p in self.pick_pts]:
            cv.create_oval(px - 4, py - 4, px + 4, py + 4, fill="#ff1744", outline="white")
        if len(self.pick_pts) == 2:
            cv.create_line(*self._c(*self.pick_pts[0]), *self._c(*self.pick_pts[1]), fill="#ffeb3b", width=2)

    def _on_wheel(self, e):
        if self.disp is None:
            return
        nz = min(max(self.zoom * (1.18 if e.delta > 0 else 1 / 1.18), 0.02), 24.0)
        ix, iy = self.ox + e.x / self.zoom, self.oy + e.y / self.zoom
        self.zoom = nz
        self.ox, self.oy = ix - e.x / nz, iy - e.y / nz
        self.render()

    def _on_press(self, e):
        self.canvas.focus_set()
        self._press = (e.x, e.y, self.ox, self.oy, False, e.state)

    def _on_drag(self, e):
        p = self._press
        if p is None or self.disp is None:
            return
        if abs(e.x - p[0]) + abs(e.y - p[1]) > 3 or p[4]:
            self._press = p[:4] + (True, p[5])
            if not self.pick_mode:
                self.ox = p[2] - (e.x - p[0]) / self.zoom
                self.oy = p[3] - (e.y - p[1]) / self.zoom
                self.render()

    def _img_xy(self, e):
        return self.ox + e.x / self.zoom, self.oy + e.y / self.zoom

    def _on_release(self, e):
        p, self._press = self._press, None
        if p is None or p[4] or self.disp is None:
            return
        ix, iy = self._img_xy(e)
        if self.pick_mode:
            self.pick_pts.append((ix, iy))
            if len(self.pick_pts) == 2:
                (x1, y1), (x2, y2) = self.pick_pts
                self.px_var.set(f"{math.hypot(x2 - x1, y2 - y1):.1f}")
                self.pick_mode = False
                self.canvas.config(cursor="arrow")
                self.cur.bar_box = (int(min(x1, x2)), int(min(y1, y2)), int(abs(x2 - x1)) + 1, int(abs(y2 - y1)) + 1)
                self.status("Đã đo thanh scale. Nhập độ dài thực (µm) rồi bấm Áp dụng.")
            self.render()
            return
        if self.edit_var.get():
            if p[5] & 0x0001:  # Shift
                self._edit("restore", ix, iy)
            else:
                self._edit("add", ix, iy)

    def _on_right(self, e):
        if self.edit_var.get() and self.disp is not None:
            self._edit("remove", *self._img_xy(e))

    # ======================================================== manual edit ===
    def _on_edit_toggle(self):
        on = self.edit_var.get()
        if on:
            self.cancel_pick()
            if not self.cur or not self.cur.data:
                self.status("Cần đo ảnh trước khi sửa giao điểm.")
            self.show_pat.set(True)
            self.hint_var.set("CHẾ ĐỘ SỬA: click trái gần chord = thêm điểm · click phải lên điểm = loại · "
                              "Shift+click = khôi phục · Ctrl+Z = hoàn tác · E = thoát.")
        else:
            self.hint_var.set("Lăn chuột: phóng to · kéo: di chuyển · nhấp đúp: vừa khung.")
        self.canvas.config(cursor="tcross" if on else "arrow")
        self.render()

    def _edit(self, action, x, y):
        it = self.cur
        if it is None or it.data is None:
            self.status("Ảnh chưa được đo.")
            return
        m = self.view_method.get()
        md = it.data["methods"].get(m)
        if md is None:
            self.status(f"Ảnh chưa có kết quả {core.METHOD_SHORT[m]}: bật phương pháp này rồi đo lại.")
            return
        snap = (m, copy.deepcopy(md["events"]), md.get("edited", False))
        tol = max(3.0, 12.0 / self.zoom)
        if action == "add":
            valid = core.unpack(it.data["valid"])
            ev, err = core.add_manual(md, x, y, self.kind_var.get(), valid, tol)
            if err:
                self.status(err)
                return
            self.status(f"Đã thêm {KIND_SHORT[ev['kind']]} trên {ev['chord']}.")
        elif action == "remove":
            ev = core.remove_near(md, x, y, max(3.0, 10.0 / self.zoom))
            if not ev:
                self.status("Không có giao điểm nào gần vị trí click.")
                return
            self.status(f"Đã loại 1 giao điểm ({'thủ công' if ev['src'] == 'manual' else 'tự động'}) trên {ev['chord']}.")
        else:
            ev = core.restore_near(md, x, y, max(3.0, 10.0 / self.zoom))
            if not ev:
                self.status("Không có điểm đã loại nào gần vị trí click.")
                return
            self.status("Đã khôi phục 1 giao điểm tự động.")
        it.undo.append(snap)
        del it.undo[:-50]
        self._after_edit(it)

    def _after_edit(self, it):
        self._set_dirty()
        self._refresh_row(it)
        self._show_results()
        self.update_pooled()
        self.render()

    def undo(self):
        it = self.cur
        if not it or not it.undo or not it.data:
            return
        m, evs, edited = it.undo.pop()
        it.data["methods"][m]["events"] = evs
        it.data["methods"][m]["edited"] = edited
        self.view_method.set(m)
        self.status("Đã hoàn tác.")
        self._after_edit(it)

    def reset_edits(self):
        it = self.cur
        m = self.view_method.get()
        if not it or not it.data or m not in it.data["methods"]:
            return
        md = it.data["methods"][m]
        if not md.get("edited"):
            return
        if not messagebox.askyesno(APP_NAME, f"Bỏ mọi chỉnh sửa thủ công của {core.METHOD_SHORT[m]} trên ảnh này?"):
            return
        it.undo.append((m, copy.deepcopy(md["events"]), True))
        md["events"] = [dict(e, removed=False) for e in md["events"] if e["src"] == "auto"]
        md["edited"] = False
        self._after_edit(it)

    # =========================================================== analysis ===
    def _params(self):
        methods = tuple(m for m in core.METHODS if self.m_vars[m].get()) or ("abrams",)
        return dict(methods=methods, sensitivity=float(self.sens_var.get()), focus=bool(self.focus_var.get()),
                    n_lines=int(self.nl_var.get()), mixed_hv=bool(self.hv_var.get()),
                    enhance_params=self.enh_params() if self.enh_detect.get() else None)

    def _confirm_overwrite(self, its):
        ed = [i for i in its if i.data and any(md.get("edited") for md in i.data["methods"].values())]
        if not ed:
            return True
        return messagebox.askyesno(APP_NAME, f"{len(ed)} ảnh có chỉnh sửa giao điểm thủ công. Đo lại sẽ xóa các chỉnh "
                                             "sửa này. Tiếp tục?")

    def run_selected(self):
        sel = [self.items[i] for i in self.tree.selection() if i in self.items]
        if not sel and self.cur is not None:
            sel = [self.cur]
        self._queue(sel)

    def run_all(self):
        self._queue(list(self.items.values()))

    def _queue(self, its, confirm=True):
        its = [i for i in its if i is not None]
        if not its:
            return
        missing = [i for i in its if not i.um_per_px]
        its = [i for i in its if i.um_per_px]
        for i in missing:
            i.state = "Thiếu scale"
            self._refresh_row(i)
        if missing and not its:
            messagebox.showinfo(APP_NAME, "Ảnh chưa có scale. Hãy thiết lập thang đo (mục 1) trước.")
            return
        if confirm and not self._confirm_overwrite(its):
            return
        params = self._params()
        for it in its:
            it.state = "Đang đo…"
            self._refresh_row(it)
            self.pending += 1
            self.jobs.put((it, params))
        self.progress.config(maximum=max(self.pending + self.done, 1), value=self.done)
        self.status(f"Đang đo {len(its)} ảnh…" + (f" (bỏ qua {len(missing)} ảnh thiếu scale)" if missing else ""))

    def _worker(self):
        while True:
            it, params = self.jobs.get()
            try:
                img = core.imread(it.path)
                if img is None:
                    raise RuntimeError("không mở được ảnh")
                self._probe(it, img)
                out = core.analyze(img, it.um_per_px, it.fov, it.bar_box, **params)
                self.results_q.put((it, out, None))
            except Exception as ex:
                traceback.print_exc()
                self.results_q.put((it, None, str(ex)))

    def _poll(self):
        try:
            while True:
                it, out, err = self.results_q.get_nowait()
                self.pending -= 1
                self.done += 1
                if self._iid_of(it) is None:
                    continue
                if err:
                    it.state, it.note = "Lỗi", err
                else:
                    it.data = out
                    it.undo = []
                    self._set_dirty()
                    it.state = "Đã đo"
                    it.note = f"loại {out['excluded'] * 100:.0f}% vùng mờ" if out["excluded"] > 0.02 else ""
                    if self.view_method.get() not in out["methods"]:
                        self.view_method.set(next(iter(out["methods"])))
                self._refresh_row(it)
                if it is self.cur:
                    self._show_results()
                    self.refresh_view()
                self.progress.config(value=self.done)
                if self.pending <= 0:
                    self.pending = self.done = 0
                    self.progress.config(value=0)
                    self.status("Đã đo xong. Rà soát giao điểm ở chế độ sửa (E) nếu cần.")
                self.update_pooled()
        except queue.Empty:
            pass
        self.root.after(100, self._poll)

    # ============================================================ results ===
    def _show_results(self):
        it = self.cur
        t = self.res_text
        t.config(state="normal")
        t.delete("1.0", "end")
        if it is None:
            t.config(state="disabled")
            return
        m = self.view_method.get()
        t.insert("end", f"{it.name}\n", "h")
        r = it.res(m)
        if not r:
            t.insert("end", ("Chưa đo. Bấm «Đo ảnh chọn» (F5)." if it.um_per_px else "Chưa có scale.")
                     + ("" if not it.data else f"\nChưa có kết quả {core.METHOD_SHORT[m]}."))
            t.config(state="disabled")
            return
        t.insert("end", f"{core.METHOD_VI[m]} · {it.um_per_px:.4f} µm/px\n")
        if r["G"] is None:
            t.insert("end", "Không có giao điểm.\n", "w")
        else:
            t.insert("end", f"G = {r['G']:.2f}  (≈ {core.round_g(r['G']):.1f})\n", "g")
            t.insert("end", f"ℓ = {r['l_um']:.2f} µm   L = {r['L_mm']:.3f} mm   P = {r['P']:g}\n")
        t.insert("end", f"cắt {r['n_cross']} · 3 hạt {r['n_triple']} · đầu mút {r['n_end']}"
                        f" · thêm TC {r['n_manual']} · loại {r['n_removed']}\n")
        if "l_H" in r and "l_V" in r:
            t.insert("end", f"ℓ ngang/dọc = {r['l_H']:.1f}/{r['l_V']:.1f} µm (tỉ lệ {r['l_H'] / r['l_V']:.2f})\n")
        if "P<50" in r["flags"]:
            t.insert("end", "⚠ P < 50: ít giao điểm (E112 §13.1)\n", "w")
        if "P>150" in r["flags"]:
            t.insert("end", "⚠ P > 150: nên giảm độ phóng đại (E1382 §12.4.2)\n", "w")
        t.config(state="disabled")

    def _selected_or_all(self):
        """Các ảnh được đánh dấu «Báo cáo» (dùng cho thống kê, báo cáo, CSV, lưu ảnh)."""
        return [i for i in self.items.values() if i.include]

    def _on_tree_click(self, e):
        if self.tree.identify_region(e.x, e.y) == "cell" and self.tree.identify_column(e.x) == "#1":
            iid = self.tree.identify_row(e.y)
            it = self.items.get(iid)
            if it is not None:
                it.include = not it.include
                self._refresh_row(it)
                self.update_pooled()
                self._set_dirty()
                return "break"

    def toggle_all_include(self):
        its = list(self.items.values())
        v = not all(i.include for i in its)
        for i in its:
            i.include = v
            self._refresh_row(i)
        self.update_pooled()
        self._set_dirty()

    def update_pooled(self):
        its = [i for i in self._selected_or_all() if i.data]
        lines = []
        for m in core.METHODS:
            rs = [r for r in (i.res(m) for i in its) if r and r["l_um"]]
            if not rs:
                continue
            s = core.field_stats([r["l_um"] for r in rs])
            txt = f"{core.METHOD_SHORT[m]:8s} n={s['n']}: ℓ̄ = {s['l_mean']:.2f} µm"
            if s["n"] >= 2:
                txt += f" ± {s['ci']:.2f} (95%CI, %RA {s['ra']:.1f}%)"
            txt += f" → G = {s['G']:.2f}"
            lines.append(txt)
        if lines:
            lines[-1] += "    (gộp các ảnh có đánh dấu «Báo cáo»; chỉ có ý nghĩa với cùng mẫu, cùng độ phóng đại)"
        self.pooled_var.set("\n".join(lines))

    # ============================================================= export ===
    def export_csv(self):
        its = [i for i in self._selected_or_all() if i.data]
        if not its:
            messagebox.showinfo(APP_NAME, "Chưa có kết quả nào để xuất.")
            return
        p = filedialog.asksaveasfilename(title="Lưu kết quả", defaultextension=".csv", initialfile="ket_qua_co_hat.csv",
                                         filetypes=[("CSV (mở bằng Excel)", "*.csv")])
        if not p:
            return
        try:
            with open(p, "w", newline="", encoding="utf-8-sig") as fh:
                w = csv.writer(fh)
                w.writerow(["Ảnh", "µm/px", "Hiệu chuẩn", "Phương pháp", "L (mm)", "P", "Cắt", "Ba hạt", "Đầu mút",
                            "Thêm TC", "Loại TC", "ℓ (µm)", "ASTM G", "ℓ ngang", "ℓ dọc"])
                for it in its:
                    for m in core.METHODS:
                        r = it.res(m)
                        if not r or r["G"] is None:
                            continue
                        w.writerow([it.name, f"{it.um_per_px:.5f}", it.calib, core.METHOD_SHORT[m], f"{r['L_mm']:.4f}",
                                    r["P"], r["n_cross"], r["n_triple"], r["n_end"], r["n_manual"], r["n_removed"],
                                    f"{r['l_um']:.3f}", f"{r['G']:.3f}", f"{r.get('l_H', '')}", f"{r.get('l_V', '')}"])
                w.writerow([])
                w.writerow(["Thống kê", "n", "ℓ̄ (µm)", "s", "t", "95%CI", "%RA", "G", "G làm tròn"])
                for m in core.METHODS:
                    rs = [r for r in (i.res(m) for i in its) if r and r["l_um"]]
                    if rs:
                        s = core.field_stats([r["l_um"] for r in rs])
                        w.writerow([core.METHOD_SHORT[m], s["n"], f"{s['l_mean']:.3f}", s["sd"] and f"{s['sd']:.3f}",
                                    s["t"], s["ci"] and f"{s['ci']:.3f}", s["ra"] and f"{s['ra']:.2f}",
                                    f"{s['G']:.3f}", s["G_round"]])
            self.status(f"Đã lưu {p}")
        except PermissionError:
            messagebox.showerror(APP_NAME, "Không ghi được file (đang mở trong Excel?).")

    def _render_item(self, it, m, banner=True):
        img = self.get_img(it)
        base = core.enhance(img, **self.enh_params())
        r = it.res(m)
        txt = (f"{core.METHOD_SHORT[m]}  P={r['P']:g}  l={r['l_um']:.2f} um  G={r['G']:.2f}"
               if banner and r and r["G"] is not None else None)
        return core.render(base, it.data, m, self.colors, show_skeleton=self.show_bound.get(), banner=txt)

    def save_overlays(self):
        its = [i for i in self._selected_or_all() if i.data]
        if not its:
            messagebox.showinfo(APP_NAME, "Chưa có kết quả nào.")
            return
        d = filedialog.askdirectory(title="Chọn thư mục lưu ảnh kết quả")
        if not d:
            return
        n = 0
        for it in its:
            for m in it.data["methods"]:
                core.imwrite(os.path.join(d, f"{os.path.splitext(it.name)[0]}_{m}.jpg"), self._render_item(it, m))
                n += 1
        self.status(f"Đã lưu {n} ảnh vào {d}")

    # ============================================================= report ===
    def report_dialog(self):
        its = [i for i in self._selected_or_all() if i.data]
        if not its:
            messagebox.showinfo(APP_NAME, "Chưa có ảnh nào đã đo. Hãy đo trước khi lập báo cáo.")
            return
        ReportDialog(self, its)

    def make_report(self, path, info, methods, its, with_images, per_page):
        import grain_report
        if isinstance(methods, str):
            methods = [methods]
        fbm, sbm = {}, {}
        for method in methods:
            fields = []
            for it in its:
                r = it.res(method)
                if not r or r["l_um"] is None:
                    continue
                img = self._render_item(it, method, banner=False) if with_images else None
                if img is not None and max(img.shape[:2]) > 1600:
                    s = 1600 / max(img.shape[:2])
                    img = cv2.resize(img, None, fx=s, fy=s, interpolation=cv2.INTER_AREA)
                fields.append(dict(name=it.name, um_per_px=it.um_per_px, calib=it.calib, res=r, image=img))
            if fields:
                fbm[method] = fields
                sbm[method] = core.field_stats([f["res"]["l_um"] for f in fields], [f["res"]["P"] for f in fields],
                                               method)
        methods = [m for m in methods if m in fbm]
        if not methods:
            raise ValueError("Không có ảnh nào có kết quả cho phương pháp đã chọn.")
        p0 = its[0].data["params"]
        params = dict(sensitivity=p0["sensitivity"], focus=p0["focus"], n_lines=p0["n_lines"], mixed_hv=p0["mixed_hv"],
                      enhance_display=core.enhance_desc(self.enh_params()),
                      enhance_detect=("ảnh đã xử lý (" + core.enhance_desc(p0["enhance"]) + ")") if p0["enhance"]
                      else "ảnh gốc")
        legend = (f"Chú giải: đường chord – màu {self.colors['chord']}; giao điểm tự động – chấm tròn màu "
                  f"{self.colors['point']}; giao điểm thủ công – ô vuông màu {self.colors['manual']}; điểm ba hạt – "
                  "chấm có vòng bao; đầu mút – vòng tròn rỗng; vùng tối – ngoài vùng đo.")
        grain_report.build_pdf(path, info, methods, fbm, sbm, params, legend, per_page)
        return path

    def show_help(self):
        messagebox.showinfo(APP_NAME + " - Hướng dẫn", (
            "1. Kéo ảnh (hoặc thư mục) vào cửa sổ, hoặc bấm «Thêm ảnh».\n"
            "2. Chọn ảnh, thiết lập thang đo: «Tự tìm thanh đỏ» hoặc «Chọn 2 điểm» trên thanh scale, nhập µm, "
            "bấm Áp dụng (Enter) – ảnh được đo ngay theo các phương pháp đã chọn.\n"
            "3. Chọn phương pháp xem (Abrams / Heyn / Chord HH). Chỉnh sáng, tương phản để nhìn rõ biên.\n"
            "4. Rà soát: bấm E để bật chế độ sửa. Click trái gần chord để thêm giao điểm (chọn loại: cắt biên 1, "
            "điểm ba hạt 1,5, đầu mút 0,5); click phải lên điểm để loại; Shift+click để khôi phục; Ctrl+Z hoàn tác.\n"
            "5. Chọn nhiều ảnh cùng mẫu (≥ 5 trường theo E112) để xem ℓ̄, 95 %CI, %RA và G.\n"
            "6. «Báo cáo PDF» để lập báo cáo A4; «Xuất CSV» để lấy số liệu.\n\n"
            "G = −6,643856·log10(ℓ[mm]) − 3,288 (ASTM E112-25 Bảng 6)."))


# --------------------------------------------------------------------------- #
class ReportDialog(tk.Toplevel):
    FIELDS = (("report_no", "Số báo cáo"), ("date", "Ngày đo"), ("client", "Khách hàng / Dự án"),
              ("sample", "Mã mẫu"), ("material", "Vật liệu / Mác"), ("location", "Vị trí, hướng mặt cắt"),
              ("prep", "Chuẩn bị mẫu, tẩm thực"), ("equipment", "Thiết bị (kính HV, camera)"),
              ("magnification", "Độ phóng đại"), ("operator", "Người thực hiện"), ("reviewer", "Người kiểm tra"),
              ("approver", "Phê duyệt"), ("notes", "Ghi chú"))

    def __init__(self, app, its):
        super().__init__(app.root)
        self.app, self.its = app, its
        self.title("Lập báo cáo PDF (A4)")
        self.transient(app.root)
        self.resizable(False, False)
        set_icon(self)
        fr = ttk.Frame(self, padding=10)
        fr.pack(fill="both", expand=True)
        saved = app.settings.get("report", {})
        self.vars = {}
        for i, (k, lab) in enumerate(self.FIELDS):
            ttk.Label(fr, text=lab + ":").grid(row=i, column=0, sticky="w", pady=2)
            v = tk.StringVar(value=saved.get(k, ""))
            if k == "date" and not v.get():
                v.set(datetime.date.today().strftime("%d/%m/%Y"))
            if k == "report_no":
                v.set("")
            self.vars[k] = v
            ttk.Entry(fr, textvariable=v, width=52).grid(row=i, column=1, sticky="we", pady=2, padx=(8, 0))
        row = len(self.FIELDS)
        ttk.Separator(fr).grid(row=row, column=0, columnspan=2, sticky="we", pady=6)
        ttk.Label(fr, text="Phương pháp báo cáo:").grid(row=row + 1, column=0, sticky="nw")
        avail = [m for m in core.METHODS if any(i.res(m) and i.res(m)["l_um"] for i in its)]
        chosen = saved.get("methods") or [app.view_method.get()]
        mf = ttk.Frame(fr)
        mf.grid(row=row + 1, column=1, sticky="w")
        self.m_vars = {}
        for m in avail:
            v = tk.BooleanVar(value=m in chosen or len(avail) == 1)
            self.m_vars[m] = v
            ttk.Checkbutton(mf, text=core.METHOD_SHORT[m], variable=v).pack(side="left", padx=4)
        if not any(v.get() for v in self.m_vars.values()) and self.m_vars:
            next(iter(self.m_vars.values())).set(True)

        self.img_var = tk.BooleanVar(value=True)
        self.pp_var = tk.IntVar(value=int(saved.get("per_page", 2)))
        of = ttk.Frame(fr)
        of.grid(row=row + 2, column=1, sticky="w", pady=4)
        ttk.Checkbutton(of, text="Kèm ảnh minh hoạ", variable=self.img_var).pack(side="left")
        ttk.Label(of, text="   Số ảnh / trang:").pack(side="left")
        ttk.Combobox(of, values=(1, 2, 4), width=3, textvariable=self.pp_var, state="readonly").pack(side="left")
        ttk.Label(fr, text=f"Gồm {len(its)} ảnh đã đo có đánh dấu «Báo cáo» trong bảng. Kết quả chính theo phương pháp đang xem "
                           f"trên màn hình ({core.METHOD_SHORT[app.view_method.get()]}) nếu được chọn.",
                  style="Hint.TLabel", wraplength=620, justify="left").grid(row=row + 3, column=0, columnspan=2, sticky="w")
        bf = ttk.Frame(fr)
        bf.grid(row=row + 4, column=0, columnspan=2, sticky="e", pady=(8, 0))
        ttk.Button(bf, text="Hủy", command=self.destroy).pack(side="right")
        ttk.Button(bf, text="📄 Xuất PDF…", style="Accent.TButton", command=self.ok).pack(side="right", padx=6)
        ttk.Button(bf, text="👁 Xem trước", command=self.preview).pack(side="right")
        self.grab_set()

    def _methods(self):
        order = [m for m in core.METHODS if self.m_vars[m].get()] if hasattr(self, "m_vars") else []
        first = self.app.view_method.get()
        if first in order:
            order.remove(first)
            order.insert(0, first)
        return order

    def _collect(self):
        info = {k: v.get().strip() for k, v in self.vars.items()}
        keep = {k: info[k] for k in ("client", "material", "prep", "equipment", "magnification", "operator",
                                     "reviewer", "approver", "location", "sample")}
        keep["per_page"] = int(self.pp_var.get())
        keep["methods"] = self._methods()
        self.app.settings["report"] = keep
        self.app._save_settings()
        return info

    def preview(self):
        info = self._collect()
        if not self._methods():
            messagebox.showwarning(APP_NAME, "Chọn ít nhất một phương pháp.", parent=self)
            return
        tmp = os.path.join(tempfile.gettempdir(), f"GSD_preview_{os.getpid()}.pdf")
        try:
            self.config(cursor="watch")
            self.update()
            self.app.make_report(tmp, info, self._methods(), self.its, self.img_var.get(), int(self.pp_var.get()))
        except Exception as ex:
            traceback.print_exc()
            messagebox.showerror(APP_NAME, f"Lỗi khi lập báo cáo: {ex}", parent=self)
            return
        finally:
            self.config(cursor="")
        PreviewWindow(self, tmp, on_save=self.ok)

    def ok(self):
        info = self._collect()
        if not self._methods():
            messagebox.showwarning(APP_NAME, "Chọn ít nhất một phương pháp.", parent=self)
            return
        name = "BaoCao_CoHat_" + (info.get("sample") or "mau").replace(" ", "_") + ".pdf"
        p = filedialog.asksaveasfilename(parent=self, title="Lưu báo cáo PDF", defaultextension=".pdf",
                                         initialfile=name, filetypes=[("PDF", "*.pdf")])
        if not p:
            return
        try:
            self.app.make_report(p, info, self._methods(), self.its, self.img_var.get(), int(self.pp_var.get()))
        except PermissionError:
            messagebox.showerror(APP_NAME, "Không ghi được file PDF (đang mở ở chương trình khác?).", parent=self)
            return
        except Exception as ex:
            traceback.print_exc()
            messagebox.showerror(APP_NAME, f"Lỗi khi lập báo cáo: {ex}", parent=self)
            return
        self.app.status(f"Đã lưu báo cáo: {p}")
        for w in self.winfo_children():
            if isinstance(w, tk.Toplevel):
                w.destroy()
        self.destroy()
        try:
            os.startfile(p)
        except Exception:
            pass


# --------------------------------------------------------------------------- #
def _jsonable(o):
    if isinstance(o, dict):
        return {k: _jsonable(v) for k, v in o.items()}
    if isinstance(o, (list, tuple)):
        return [_jsonable(v) for v in o]
    if isinstance(o, (np.integer,)):
        return int(o)
    if isinstance(o, (np.floating,)):
        return float(o)
    if isinstance(o, np.bool_):
        return bool(o)
    return o


def write_project(path, app):
    """Project = tệp zip: project.json (thông số, scale, giao điểm, sửa thủ công) + masks.npz (biên hạt, vùng đo)."""
    pdir = os.path.dirname(os.path.abspath(path))
    items, arrays = [], {}
    for k, it in enumerate(app.items.values()):
        d = dict(path=os.path.abspath(it.path), name=it.name, um_per_px=it.um_per_px, calib=it.calib,
                 bar_box=it.bar_box, bar_px=it.bar_px, fov=it.fov, size=it.size, include=it.include,
                 state=it.state, note=it.note, data=None)
        try:
            d["rel"] = os.path.relpath(os.path.abspath(it.path), pdir)
        except ValueError:
            d["rel"] = None
        if it.data:
            dd = {kk: vv for kk, vv in it.data.items() if kk not in ("skeleton", "valid")}
            for key in ("skeleton", "valid"):
                shape, bits = it.data[key]
                arrays[f"{k}_{key}"] = bits
                dd[key + "_shape"] = list(shape)
            d["data"] = dd
        items.append(d)
    proj = dict(format="GrainSizeDetermine", version=1, app=APP_VER,
                saved=datetime.datetime.now().isoformat(timespec="seconds"),
                settings=dict(methods=[m for m, v in app.m_vars.items() if v.get()], mixed_hv=bool(app.hv_var.get()),
                              n_lines=int(app.nl_var.get()), sens=float(app.sens_var.get()),
                              focus=bool(app.focus_var.get()), enhance=app.enh_params(),
                              enh_detect=bool(app.enh_detect.get()), colors=app.colors,
                              view_method=app.view_method.get()),
                report=app.settings.get("report", {}), items=items)
    tmp = path + ".tmp"
    with zipfile.ZipFile(tmp, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("project.json", json.dumps(_jsonable(proj), ensure_ascii=False, indent=1))
        import io
        buf = io.BytesIO()
        np.savez_compressed(buf, **arrays) if arrays else np.savez_compressed(buf, empty=np.zeros(1))
        z.writestr("masks.npz", buf.getvalue())
    os.replace(tmp, path)


def read_project(path):
    import io
    pdir = os.path.dirname(os.path.abspath(path))
    with zipfile.ZipFile(path) as z:
        proj = json.loads(z.read("project.json").decode("utf-8"))
        arrays = np.load(io.BytesIO(z.read("masks.npz")))
        if proj.get("format") != "GrainSizeDetermine":
            raise ValueError("không phải tệp project của Grain Size Determine")
        out = []
        for k, d in enumerate(proj["items"]):
            p = d["path"]
            if not os.path.exists(p) and d.get("rel"):
                alt = os.path.normpath(os.path.join(pdir, d["rel"]))
                if os.path.exists(alt):
                    p = alt
            it = Item(p)
            it.name = d.get("name", it.name)
            it.um_per_px = d.get("um_per_px")
            it.calib = d.get("calib", "")
            it.bar_box = tuple(d["bar_box"]) if d.get("bar_box") else None
            it.bar_px = d.get("bar_px")
            it.fov = tuple(d["fov"]) if d.get("fov") else None
            it.size = tuple(d["size"]) if d.get("size") else None
            it.probed = it.size is not None
            it.include = d.get("include", True)
            it.state = d.get("state", "")
            it.note = d.get("note", "")
            dd = d.get("data")
            if dd:
                for key in ("skeleton", "valid"):
                    dd[key] = (tuple(dd.pop(key + "_shape")), arrays[f"{k}_{key}"])
                for md in dd["methods"].values():
                    for ch in md["chords"]:
                        for kk in ("a", "b"):
                            if kk in ch:
                                ch[kk] = tuple(ch[kk])
                if dd.get("excl"):
                    dd["excl"] = tuple(dd["excl"])
                it.data = dd
            out.append(it)
        proj["items"] = out
    return proj


class PreviewWindow(tk.Toplevel):
    """Xem trước báo cáo PDF: lật trang, lưu, in."""

    def __init__(self, parent, pdf_path, on_save=None):
        super().__init__(parent)
        import pypdfium2 as pdfium
        self.title("Xem trước báo cáo")
        self.geometry("880x980")
        self.transient(parent)
        self.pdf_path, self.on_save = pdf_path, on_save
        self.doc = pdfium.PdfDocument(pdf_path)
        self.page = 0
        bar = ttk.Frame(self, padding=6)
        bar.pack(fill="x")
        ttk.Button(bar, text="◀ Trang trước", command=lambda: self.go(-1)).pack(side="left")
        self.lbl = ttk.Label(bar, text="", width=14, anchor="center")
        self.lbl.pack(side="left", padx=6)
        ttk.Button(bar, text="Trang sau ▶", command=lambda: self.go(1)).pack(side="left")
        ttk.Button(bar, text="Đóng", command=self.close).pack(side="right")
        ttk.Button(bar, text="🖨 In…", command=self.print_pdf).pack(side="right", padx=4)
        ttk.Button(bar, text="💾 Lưu PDF…", style="Accent.TButton", command=self.save).pack(side="right")
        self.cv = tk.Canvas(self, bg="#5a5f66", highlightthickness=0)
        self.cv.pack(fill="both", expand=True)
        self.cv.bind("<Configure>", lambda e: self.show())
        self.cv.bind("<MouseWheel>", lambda e: self.go(-1 if e.delta > 0 else 1))
        self.bind("<Left>", lambda e: self.go(-1))
        self.bind("<Right>", lambda e: self.go(1))
        self.protocol("WM_DELETE_WINDOW", self.close)
        self._img = None

    def go(self, d):
        self.page = max(0, min(len(self.doc) - 1, self.page + d))
        self.show()

    def show(self):
        cw, ch = max(self.cv.winfo_width(), 100), max(self.cv.winfo_height(), 100)
        pg = self.doc[self.page]
        w, h = pg.get_size()
        sc = min((cw - 20) / w, (ch - 20) / h)
        pil = pg.render(scale=sc).to_pil().convert("RGB")
        data = b"P6 %d %d 255\n" % pil.size + pil.tobytes()
        self._img = tk.PhotoImage(data=data)
        self.cv.delete("all")
        self.cv.create_image(cw // 2, ch // 2, image=self._img)
        self.lbl.config(text=f"Trang {self.page + 1}/{len(self.doc)}")

    def print_pdf(self):
        try:
            os.startfile(self.pdf_path, "print")
        except Exception as ex:
            messagebox.showerror(APP_NAME, f"Không gửi được lệnh in (cần có chương trình đọc PDF): {ex}", parent=self)

    def save(self):
        if self.on_save:
            self.close()
            self.on_save()

    def close(self):
        try:
            self.doc.close()
        except Exception:
            pass
        self.destroy()


# --------------------------------------------------------------------------- #
def selftest(out_path):
    rng = np.random.default_rng(0)
    H, W, n = 450, 600, 120
    seeds = rng.uniform([0, 0], [W, H], size=(n, 2))
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
    img = cv2.cvtColor(g, cv2.COLOR_GRAY2BGR)
    out = core.analyze(img, 0.5)
    txt = "OK " + " ".join(f"{m}:P={core.summarize(md, 0.5)['P']},G={core.summarize(md, 0.5)['G']:.2f}"
                           for m, md in out["methods"].items())
    try:
        import grain_report
        rs = core.summarize(out["methods"]["mixed"], 0.5)
        st = core.field_stats([rs["l_um"]], [rs["P"]], "mixed")
        pdf = (out_path or "selftest") + ".pdf"
        grain_report.build_pdf(pdf, {"sample": "selftest"}, ["mixed"],
                               {"mixed": [dict(name="synthetic", um_per_px=0.5, calib="test", res=rs,
                                               image=core.render(img, out, "mixed", DEFAULT_COLORS))]},
                               {"mixed": st}, dict(sensitivity=85, focus=True, n_lines=5, mixed_hv=False,
                                        enhance_display="không", enhance_detect="ảnh gốc"))
        import pypdfium2 as pdfium
        txt += f" PDF_OK pages={len(pdfium.PdfDocument(pdf))}"
    except Exception as ex:
        txt += f" PDF_FAIL {ex}"
    if out_path:
        with open(out_path, "w", encoding="utf-8") as f:
            f.write(txt)
    print(txt)


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "--selftest":
        selftest(sys.argv[2] if len(sys.argv) > 2 else None)
        return
    try:
        import ctypes
        ctypes.windll.shcore.SetProcessDpiAwareness(1)
    except Exception:
        pass
    root = TkinterDnD.Tk() if HAVE_DND else tk.Tk()
    app = App(root)
    args = sys.argv[1:]
    if args and args[0].lower().endswith(PROJ_EXT):
        root.after(300, lambda: app.open_project(args[0]))
    elif args:
        root.after(300, lambda: app.add_paths(args))
    root.mainloop()


if __name__ == "__main__":
    main()
