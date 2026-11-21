# -*- coding: utf-8 -*-
"""
grain_report.py - Báo cáo kết quả đo cỡ hạt, PDF khổ A4 (reportlab).

Nội dung theo ASTM E112-25 §18 và ASTM E1382-97(2023) §15: thông tin mẫu, phương pháp,
xử lý ảnh, kết quả từng trường, thống kê (ℓ tb, s, 95 %CI, %RA), cấp hạt G, ảnh minh hoạ.
Một báo cáo có thể gồm một hoặc nhiều phương pháp (Abrams, Heyn, Chord hỗn hợp).
"""
import datetime
import io
import os

import cv2
from reportlab.lib import colors
from reportlab.lib.enums import TA_CENTER, TA_LEFT
from reportlab.lib.pagesizes import A4
from reportlab.lib.styles import ParagraphStyle
from reportlab.lib.units import mm
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont
from reportlab.pdfgen import canvas as rl_canvas
from reportlab.platypus import Image, KeepTogether, PageBreak, Paragraph, SimpleDocTemplate, Spacer, Table, TableStyle

import grain_core as core

APP_TITLE = "Grain Size Determine"
_FONT, _FONT_B, _FONT_I = "Helvetica", "Helvetica-Bold", "Helvetica-Oblique"


def _register_fonts():
    global _FONT, _FONT_B, _FONT_I
    fdir = os.path.join(os.environ.get("WINDIR", r"C:\Windows"), "Fonts")
    for reg, bold, ital in (("arial.ttf", "arialbd.ttf", "ariali.ttf"), ("tahoma.ttf", "tahomabd.ttf", "tahoma.ttf"),
                            ("times.ttf", "timesbd.ttf", "timesi.ttf")):
        try:
            pdfmetrics.registerFont(TTFont("VN", os.path.join(fdir, reg)))
            pdfmetrics.registerFont(TTFont("VN-B", os.path.join(fdir, bold)))
            pdfmetrics.registerFont(TTFont("VN-I", os.path.join(fdir, ital)))
            _FONT, _FONT_B, _FONT_I = "VN", "VN-B", "VN-I"
            return
        except Exception:
            continue


_register_fonts()

NAVY = colors.HexColor("#1F3B5B")
LIGHT = colors.HexColor("#E8EEF5")
GRID = colors.HexColor("#9AA8B8")
LT = "ℓ<sub>tb</sub>"


def num(v, d=2):
    if v is None:
        return "–"
    return f"{v:.{d}f}".replace(".", ",")


def _styles():
    P = ParagraphStyle
    return dict(
        title=P("t", fontName=_FONT_B, fontSize=15, leading=19, textColor=NAVY, alignment=TA_LEFT),
        sub=P("s", fontName=_FONT, fontSize=9, leading=12, textColor=colors.HexColor("#405060")),
        h=P("h", fontName=_FONT_B, fontSize=11, leading=14, textColor=NAVY, spaceBefore=8, spaceAfter=4),
        h2=P("h2", fontName=_FONT_B, fontSize=10, leading=13, textColor=NAVY, spaceBefore=6, spaceAfter=3),
        b=P("b", fontName=_FONT, fontSize=9, leading=12),
        bs=P("bs", fontName=_FONT, fontSize=8, leading=10),
        cell=P("c", fontName=_FONT, fontSize=8, leading=10),
        cellb=P("cb", fontName=_FONT_B, fontSize=8, leading=10),
        cellc=P("cc", fontName=_FONT, fontSize=8, leading=10, alignment=TA_CENTER),
        cap=P("cap", fontName=_FONT_I, fontSize=8, leading=10, alignment=TA_CENTER),
        big=P("big", fontName=_FONT_B, fontSize=13, leading=17, textColor=NAVY, alignment=TA_CENTER),
        note=P("n", fontName=_FONT_I, fontSize=7.5, leading=9.5, textColor=colors.HexColor("#505a66")))


def _esc(t):
    return (str(t) if t is not None else "").replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def _tbl(data, widths, header=True, zebra=True, center_from=1, font=8):
    t = Table(data, colWidths=widths, repeatRows=1 if header else 0)
    st = [("FONT", (0, 0), (-1, -1), _FONT, font), ("GRID", (0, 0), (-1, -1), 0.4, GRID),
          ("VALIGN", (0, 0), (-1, -1), "MIDDLE"),
          ("TOPPADDING", (0, 0), (-1, -1), 2), ("BOTTOMPADDING", (0, 0), (-1, -1), 2),
          ("LEFTPADDING", (0, 0), (-1, -1), 3), ("RIGHTPADDING", (0, 0), (-1, -1), 3)]
    if header:
        st += [("BACKGROUND", (0, 0), (-1, 0), NAVY), ("TEXTCOLOR", (0, 0), (-1, 0), colors.white),
               ("FONT", (0, 0), (-1, 0), _FONT_B, font)]
    if center_from is not None:
        st.append(("ALIGN", (center_from, 0), (-1, -1), "CENTER"))
    if zebra:
        for r in range(1 if header else 0, len(data)):
            if r % 2 == 0:
                st.append(("BACKGROUND", (0, r), (-1, r), LIGHT))
    t.setStyle(TableStyle(st))
    return t


class _NumberedCanvas(rl_canvas.Canvas):
    footer_text = ""

    def __init__(self, *a, **k):
        super().__init__(*a, **k)
        self._saved = []

    def showPage(self):
        self._saved.append(dict(self.__dict__))
        self._startPage()

    def save(self):
        n = len(self._saved)
        for st in self._saved:
            self.__dict__.update(st)
            w, h = A4
            self.setStrokeColor(GRID)
            self.setLineWidth(0.5)
            self.line(18 * mm, 14 * mm, w - 18 * mm, 14 * mm)
            self.setFont(_FONT, 7.5)
            self.setFillColor(colors.HexColor("#505a66"))
            self.drawString(18 * mm, 10 * mm, self.footer_text)
            self.drawRightString(w - 18 * mm, 10 * mm, f"Trang {self._pageNumber}/{n}")
            super().showPage()
        super().save()


def method_text(method, n_lines, mixed_hv):
    if method == "abrams":
        return ("<b>Abrams</b> – ba vòng tròn đồng tâm, chu vi tỉ lệ 3:2:1 (250 / 166,7 / 83,3 mm trên tổng 500 mm "
                "của Fig. 5, E112 §14.3), đặt tại tâm trường đo.")
    if method == "heyn":
        return (f"<b>Heyn</b> – lưới {n_lines} đường thẳng ngang và {n_lines} đường thẳng dọc phân bố đều trên trường "
                "đo (E112 §13); có thêm ℓ theo phương ngang/dọc để đánh giá dị hướng.")
    return ("<b>Chord hỗn hợp</b> – mẫu lưới Fig. 5 của E112: ba vòng tròn đồng tâm (chu vi 250 / 166,7 / 83,3 đơn vị) "
            "và hai đường chéo dài 150 đơn vị qua tâm" + (", cùng hai đường 100 đơn vị ngang/dọc" if mixed_hv else "")
            + "; mọi đoạn chắn của một trường gộp chung ℓ = ΣL/ΣP.")


def build_pdf(path, info, methods, fields_by_method, stats_by_method, params, colors_legend=None,
              images_per_page=2):
    """
    methods          : danh sách phương pháp, ví dụ ["abrams", "mixed"]
    fields_by_method : {method: [dict(name, um_per_px, calib, res, image)]}
    stats_by_method  : {method: core.field_stats(...)}
    """
    if isinstance(methods, str):
        methods = [methods]
    S = _styles()
    doc = SimpleDocTemplate(path, pagesize=A4, leftMargin=18 * mm, rightMargin=18 * mm, topMargin=15 * mm,
                            bottomMargin=20 * mm, title="Báo cáo đo cỡ hạt ASTM E112",
                            author=info.get("operator", ""), subject=info.get("sample", ""))
    W = A4[0] - 36 * mm
    story = []
    all_fields = {f["name"]: f for m in methods for f in fields_by_method[m]}

    # ---- tiêu đề
    hr = Table([[Paragraph(f"Số BC: <b>{_esc(info.get('report_no') or '…')}</b>", S["b"])],
                [Paragraph(f"Ngày: <b>{_esc(info.get('date') or datetime.date.today().strftime('%d/%m/%Y'))}</b>",
                           S["b"])]], colWidths=[52 * mm])
    hr.setStyle(TableStyle([("BOX", (0, 0), (-1, -1), 0.6, NAVY), ("LEFTPADDING", (0, 0), (-1, -1), 5)]))
    title = [Paragraph("BÁO CÁO KẾT QUẢ XÁC ĐỊNH CỠ HẠT TRUNG BÌNH", S["title"]),
             Paragraph("Phương pháp đoạn chắn (intercept) theo ASTM E112-25; phân tích ảnh bán tự động theo "
                       "ASTM E1382-97 (2023)", S["sub"])]
    ht = Table([[title, hr]], colWidths=[W - 54 * mm, 54 * mm])
    ht.setStyle(TableStyle([("VALIGN", (0, 0), (-1, -1), "TOP"), ("LINEBELOW", (0, 0), (-1, 0), 1.2, NAVY),
                            ("BOTTOMPADDING", (0, 0), (-1, -1), 6)]))
    story += [ht, Spacer(1, 4)]

    # ---- 1. Thông tin mẫu
    story.append(Paragraph("1. Thông tin mẫu và điều kiện thử", S["h"]))

    def kv(k, v):
        return [Paragraph(k, S["cellb"]), Paragraph(_esc(v) or "–", S["cell"])]
    rows = [kv("Khách hàng / Dự án", info.get("client")) + kv("Mã mẫu", info.get("sample")),
            kv("Vật liệu / Mác", info.get("material")) + kv("Vị trí, hướng mặt cắt", info.get("location")),
            kv("Chuẩn bị mẫu, tẩm thực", info.get("prep")) + kv("Thiết bị", info.get("equipment")),
            kv("Độ phóng đại", info.get("magnification")) + kv("Số trường đo", str(len(all_fields)))]
    t = Table(rows, colWidths=[33 * mm, W / 2 - 33 * mm, 33 * mm, W / 2 - 33 * mm])
    t.setStyle(TableStyle([("GRID", (0, 0), (-1, -1), 0.4, GRID), ("BACKGROUND", (0, 0), (0, -1), LIGHT),
                           ("BACKGROUND", (2, 0), (2, -1), LIGHT), ("VALIGN", (0, 0), (-1, -1), "MIDDLE")]))
    story.append(t)

    # ---- 2. Phương pháp
    story.append(Paragraph("2. Phương pháp thử", S["h"]))
    calibs = sorted({f.get("calib", "") for f in all_fields.values() if f.get("calib")})
    sc = [f["um_per_px"] for f in all_fields.values()]
    nl, hv = params.get("n_lines", 5), params.get("mixed_hv", False)
    rows = [["Tiêu chuẩn áp dụng", "ASTM E112-25 (§13 Heyn, §14.3 Abrams, Fig. 5, §15 thống kê); ASTM E1382-97 (2023)"],
            ["Lưới thử (test pattern)", "<br/>".join(method_text(m, nl, hv) for m in methods)],
            ["Quy tắc đếm giao điểm P", "Cắt qua biên hạt = 1; tiếp tuyến = 1; điểm ba hạt = 1,5; đầu mút đoạn thẳng "
                                        "chạm biên = 0,5 (E112 §13.3; E1382 §12.4.5)."],
            ["Tính toán", f"ℓ = L/P cho từng trường; {LT} = trung bình các trường; G = −6,643856·log<sub>10</sub>"
                          f"({LT} [mm]) − 3,288 (E112 Bảng 6); 95 %CI = t·s/√n; %RA = 95 %CI/{LT}·100 (E112 §15)."],
            ["Hiệu chuẩn thang đo", _esc(", ".join(calibs) if calibs else "theo thanh tỉ lệ trên ảnh")
             + (f"; {min(sc):.4f} – {max(sc):.4f} µm/px" if sc else "")],
            ["Phát hiện biên hạt", f"Tự động (lọc black-hat trên kênh L*, ngưỡng phân vị "
                                   f"{params.get('sensitivity', 85):.0f}, làm mảnh 1 px)"
             + ("; loại vùng mờ/nhòe" if params.get("focus") else "")
             + "; người đo rà soát, thêm/bớt giao điểm thủ công."],
            ["Xử lý ảnh (E1382 §15.4)", "Hiển thị: " + _esc(params.get("enhance_display", "không"))
             + ". Ảnh dùng để tách biên: " + _esc(params.get("enhance_detect", "ảnh gốc")) + "."]]
    story.append(_tbl([[Paragraph(a, S["cellb"]), Paragraph(b, S["cell"])] for a, b in rows],
                      [42 * mm, W - 42 * mm], header=False, zebra=False, center_from=None))

    # ---- 3. Tóm tắt kết quả (nhiều phương pháp)
    story.append(Paragraph("3. Tóm tắt kết quả", S["h"]))
    sm = [["Phương pháp", "n", "ΣP", "ℓ tb (µm)", "95 %CI (µm)", "%RA", "G", "G làm tròn"]]
    for m in methods:
        s = stats_by_method[m]
        fs = fields_by_method[m]
        sm.append([Paragraph(core.METHOD_VI[m], S["cell"]), str(s["n"]), num(sum(f["res"]["P"] for f in fs), 1),
                   num(s["l_mean"]), ("± " + num(s["ci"])) if s.get("ci") else "–",
                   (num(s["ra"], 1) + " %") if s.get("ra") is not None else "–", num(s["G"]),
                   num(s["G_round"], 1)])
    story.append(_tbl(sm, [62 * mm, 10 * mm, 17 * mm, 18 * mm, 22 * mm, 15 * mm, 13 * mm, W - 157 * mm]))
    main = methods[0]
    sg = stats_by_method[main]
    gb = Table([[Paragraph(f"CẤP CỠ HẠT TRUNG BÌNH ASTM  G = {num(sg['G_round'], 1)}", S["big"])],
                [Paragraph(f"{LT} = {num(sg['l_mean'])} µm"
                           + (f" ± {num(sg['ci'])} µm (95 %CI), %RA = {num(sg['ra'], 1)} %" if sg.get('ci') else "")
                           + f" · theo {core.METHOD_VI[main]}"
                           + (" (phương pháp chính; các phương pháp khác để đối chiếu)" if len(methods) > 1 else ""),
                           S["cellc"])]], colWidths=[W])
    gb.setStyle(TableStyle([("BOX", (0, 0), (-1, -1), 1.2, NAVY), ("BACKGROUND", (0, 0), (-1, -1), LIGHT),
                            ("TOPPADDING", (0, 0), (-1, -1), 4), ("BOTTOMPADDING", (0, 0), (-1, -1), 4)]))
    story += [Spacer(1, 5), gb]

    # ---- 4. Chi tiết từng phương pháp
    for k, m in enumerate(methods, 1):
        fs, s = fields_by_method[m], stats_by_method[m]
        story.append(Paragraph(f"4.{k}. Kết quả từng trường – {core.METHOD_VI[m]}", S["h"]))
        hdr = ["TT", "Trường / ảnh", "µm/px", "L (mm)", "P", "Cắt / 3 hạt / Đầu mút", "Thêm / Bớt TC", "ℓ (µm)", "G"]
        data = [hdr]
        for i, f in enumerate(fs, 1):
            r = f["res"]
            data.append([str(i), Paragraph(_esc(f["name"]), S["cell"]), num(f["um_per_px"], 4), num(r["L_mm"], 3),
                         num(r["P"], 1), f"{r['n_cross']} / {r['n_triple']} / {r['n_end']}",
                         f"{r['n_manual']} / {r['n_removed']}", num(r["l_um"]), num(r["G"])])
        story.append(_tbl(data, [8 * mm, 46 * mm, 15 * mm, 15 * mm, 13 * mm, 28 * mm, 20 * mm, 15 * mm, W - 160 * mm]))
        srows = [["Đại lượng", "Giá trị", "Đại lượng", "Giá trị"],
                 ["Số trường n", str(s["n"]), "Hệ số t (95 %)", num(s.get("t"), 3)],
                 [Paragraph(f"{LT} (µm)", S["cell"]), num(s["l_mean"], 3), "Độ lệch chuẩn s (µm)", num(s.get("sd"), 3)],
                 ["95 %CI (µm)", ("± " + num(s["ci"], 3)) if s.get("ci") else "–", "%RA",
                  (num(s["ra"], 1) + " %") if s.get("ra") is not None else "–"],
                 [Paragraph(f"G (từ {LT})", S["cell"]), num(s["G"]), "Khoảng G ứng với 95 %CI",
                  (num(s["G_lo"]) + " – " + num(s["G_hi"])) if s.get("G_lo") is not None and s.get("G_hi") is not None
                  else "–"]]
        blk = [Spacer(1, 4), _tbl(srows, [42 * mm, W / 2 - 42 * mm, 42 * mm, W / 2 - 42 * mm], center_from=None),
               Spacer(1, 4)]
        crow = [["Tiêu chí theo tiêu chuẩn", "Giá trị", "Đánh giá"]]
        for name, ok, val in s.get("checks", []):
            crow.append([Paragraph(_esc(name), S["cell"]), val,
                         Paragraph(f"<font color='{'#1B7F3A' if ok else '#B3261E'}'><b>{'Đạt' if ok else 'Chưa đạt'}"
                                   "</b></font>", S["cellc"])])
        blk.append(_tbl(crow, [W - 62 * mm, 36 * mm, 26 * mm], center_from=1))
        story.append(KeepTogether(blk))
    story.append(Paragraph("P tính theo trọng số; “Thêm / Bớt TC” là số giao điểm người đo thêm vào / loại bỏ so với kết "
                           "quả phát hiện tự động.", S["note"]))

    notes = (info.get("notes") or "").strip()
    story.append(Spacer(1, 4))
    story.append(Paragraph("Ghi chú: " + (_esc(notes) + ". " if notes else "")
                           + "Kết quả chỉ đại diện cho các trường ảnh đã đo. Biên hạt được phát hiện tự động và đã được "
                             "người đo rà soát; độ chính xác phụ thuộc chất lượng tẩm thực và ảnh (E1382 §10). "
                             "G làm tròn tới 0,1.", S["note"]))
    story.append(Spacer(1, 8))
    sig = Table([[Paragraph("<b>Người thực hiện</b>", S["cellc"]), Paragraph("<b>Người kiểm tra</b>", S["cellc"]),
                  Paragraph("<b>Phê duyệt</b>", S["cellc"])], ["", "", ""],
                 [Paragraph(_esc(info.get("operator", "")), S["cellc"]),
                  Paragraph(_esc(info.get("reviewer", "")), S["cellc"]),
                  Paragraph(_esc(info.get("approver", "")), S["cellc"])]],
                colWidths=[W / 3] * 3, rowHeights=[None, 16 * mm, None])
    story.append(KeepTogether([sig]))

    # ---- 5. Ảnh minh hoạ
    per = max(1, int(images_per_page))
    cols = 1 if per <= 2 else 2
    maxw = W if cols == 1 else (W - 6 * mm) / 2
    maxh = (A4[1] - 62 * mm) / (2 if per >= 2 else 1) - 14 * mm
    first = True
    for k, m in enumerate(methods, 1):
        imgs = [f for f in fields_by_method[m] if f.get("image") is not None]
        if not imgs:
            continue
        story.append(PageBreak() if first else Spacer(1, 8))
        first = False
        story.append(Paragraph(f"5.{k}. Ảnh các trường đo – {core.METHOD_VI[m]}", S["h"]))
        if colors_legend and k == 1:
            story += [Paragraph(colors_legend, S["bs"]), Spacer(1, 4)]
        cells = []
        for i, f in enumerate(fields_by_method[m], 1):
            if f.get("image") is None:
                continue
            img = f["image"]
            h, w = img.shape[:2]
            sc_ = min(maxw / w, maxh / h)
            ok, buf = cv2.imencode(".jpg", img, [cv2.IMWRITE_JPEG_QUALITY, 88])
            r = f["res"]
            cells.append((Image(io.BytesIO(buf.tobytes()), width=w * sc_, height=h * sc_),
                          Paragraph(f"Trường {i}: {_esc(f['name'])} – P = {num(r['P'], 1)}, ℓ = {num(r['l_um'])} µm, "
                                    f"G = {num(r['G'])}", S["cap"])))
        if cols == 1:
            for im, cap in cells:
                story.append(KeepTogether([im, Spacer(1, 2), cap, Spacer(1, 6)]))
        else:
            rows = []
            for j in range(0, len(cells), 2):
                pair = cells[j:j + 2]
                rows.append([[p[0], p[1]] for p in pair] + ([""] if len(pair) == 1 else []))
            t = Table(rows, colWidths=[maxw + 3 * mm] * 2)
            t.setStyle(TableStyle([("VALIGN", (0, 0), (-1, -1), "TOP"), ("ALIGN", (0, 0), (-1, -1), "CENTER"),
                                   ("BOTTOMPADDING", (0, 0), (-1, -1), 8)]))
            story.append(t)

    _NumberedCanvas.footer_text = (f"{APP_TITLE} · ASTM E112-25 / E1382 · Mã mẫu: {info.get('sample', '') or '–'}"
                                   f" · Số BC: {info.get('report_no', '') or '–'}")
    doc.build(story, canvasmaker=_NumberedCanvas)
    return path
