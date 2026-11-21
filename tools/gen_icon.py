# -*- coding: utf-8 -*-
"""Tạo biểu tượng ứng dụng: tổ chức hạt kim tương cách điệu + vòng tròn chord và giao điểm.
Chạy: python tools/gen_icon.py  ->  assets/app.ico, assets/app.png"""
import os
import sys

import cv2
import numpy as np
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(os.path.dirname(HERE), "assets") if os.path.basename(HERE) == "tools" else os.path.join(HERE, "assets")


def make(size=1024):
    rng = np.random.default_rng(7)
    S = size
    n = 26
    pts = rng.uniform(0.04 * S, 0.96 * S, (n, 2))
    yy, xx = np.mgrid[0:S, 0:S]
    d = np.full((S, S), np.inf, np.float32)
    lab = np.zeros((S, S), np.int32)
    for i, (px, py) in enumerate(pts):
        di = (xx - px) ** 2 + (yy - py) ** 2
        m = di < d
        d[m] = di[m]
        lab[m] = i
    # màu hạt: vàng đồng (ferit) xen vài hạt tối (peclit)
    pal = []
    for i in range(n):
        if rng.random() < 0.18:
            pal.append((70, 85, 110))                                   # BGR tối
        else:
            k = rng.uniform(0.85, 1.12)
            pal.append(tuple(int(min(255, v * k)) for v in (95, 190, 228)))
    img = np.array(pal, np.uint8)[lab]
    b = np.zeros((S, S), bool)
    b[:-1] |= lab[:-1] != lab[1:]
    b[:, :-1] |= lab[:, :-1] != lab[:, 1:]
    b = cv2.dilate(b.astype(np.uint8), cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (17, 17))) > 0
    img[b] = (30, 38, 52)
    img = cv2.GaussianBlur(img, (0, 0), 1.2)
    # vòng tròn chord + đường chéo
    c = S // 2
    r = int(0.33 * S)
    cv2.circle(img, (c, c), r, (255, 230, 0), 34, cv2.LINE_AA)
    cv2.circle(img, (c, c), r, (255, 255, 120), 10, cv2.LINE_AA)
    # giao điểm: điểm trên vòng tròn trùng biên hạt
    th = np.linspace(0, 2 * np.pi, 4000, endpoint=False)
    xs, ys = (c + r * np.cos(th)).astype(int), (c + r * np.sin(th)).astype(int)
    on = b[ys, xs]
    hits, prev = [], False
    for i, v in enumerate(on):
        if v and not prev and (not hits or i - hits[-1] > 260) and (not hits or i + 4000 - hits[0] > 260):
            hits.append(i)
        prev = v
    for i in hits:
        cv2.circle(img, (xs[i], ys[i]), 42, (255, 255, 255), -1, cv2.LINE_AA)
        cv2.circle(img, (xs[i], ys[i]), 32, (40, 40, 235), -1, cv2.LINE_AA)
    # bo góc
    rgba = cv2.cvtColor(img, cv2.COLOR_BGR2RGBA)
    mask = np.zeros((S, S), np.uint8)
    rad = int(0.2 * S)
    cv2.rectangle(mask, (rad, 0), (S - rad, S), 255, -1)
    cv2.rectangle(mask, (0, rad), (S, S - rad), 255, -1)
    for cx, cy in ((rad, rad), (S - rad, rad), (rad, S - rad), (S - rad, S - rad)):
        cv2.circle(mask, (cx, cy), rad, 255, -1, cv2.LINE_AA)
    rgba[:, :, 3] = mask
    # viền
    edge = cv2.Canny(mask, 50, 150)
    edge = cv2.dilate(edge, np.ones((9, 9), np.uint8)) > 0
    rgba[edge & (mask > 0)] = (31, 59, 91, 255)
    return Image.fromarray(rgba)


def main():
    os.makedirs(OUT, exist_ok=True)
    im = make()
    im.resize((256, 256), Image.LANCZOS).save(os.path.join(OUT, "app.png"))
    im.save(os.path.join(OUT, "app.ico"), sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128),
                                                  (256, 256)])
    print("Đã tạo", OUT)


if __name__ == "__main__":
    main()
