# Phương pháp tính và hiệu chỉnh

## Công thức và quy tắc (ASTM E112-25, E1382-97(2023))
- ℓ = L / P cho mỗi trường (ảnh); G = −6,643856·log10(ℓ[mm]) − 3,288 (E112 Bảng 6).
- Trọng số giao điểm P: cắt qua biên = 1; tiếp tuyến = 1; điểm ba hạt = 1,5; đầu mút đoạn thẳng chạm biên = 0,5 (E112 §13.3; E1382 §12.4.5).
- Nhiều trường: ℓ tb = trung bình các ℓ từng trường (không gộp ΣL/ΣP); s; 95 %CI = t·s/√n với t theo E112 Bảng 7; %RA = 95 %CI/ℓ tb·100 (E112 §15).
- Tiêu chí kiểm tra trong báo cáo: n ≥ 5; %RA ≤ 10 %; ΣP ≥ 500 (E1382 §12.4.3); mỗi trường 40–150 điểm với lưới tròn (E112 §14.3.2.1, E1382 §12.4.2) hoặc ≥ 50 điểm với Heyn (E112 §13.1).

## Lưới thử
- Abrams: 3 vòng tròn đồng tâm, chu vi 3:2:1 (250 / 166,7 / 83,3 trên 500 mm).
- Heyn: n đường ngang + n đường dọc phân bố đều; báo cáo thêm ℓ ngang/dọc.
- Chord hỗn hợp (Fig. 5): 3 vòng tròn Abrams + 2 đường chéo 150 đơn vị qua tâm; tuỳ chọn thêm 2 đường 100 đơn vị. Hình học kế thừa GSCM Rev.0.1 RC7.
- Vùng loại trừ: ngoài trường nhìn tròn, vùng mờ/nhòe (mật độ biên thấp), vùng thanh scale và nhãn chữ. Chỉ phần chord nằm trong vùng đo được tính vào L.

## Bộ đếm tự động
- Biên hạt: kênh L*, san phẳng nền, CLAHE, lọc black-hat, ngưỡng theo phân vị («độ nhạy»), làm mảnh 1 px.
- Giao điểm: các cụm biên liên tiếp dọc chord; lấp khe ≤ 1 px (GAP = 2 mẫu); cụm có điểm nối ≥ 3 nhánh trong bán kính 5 px được tính là điểm ba hạt; cụm chạm đầu đoạn hợp lệ là đầu mút.
- Hiệu chỉnh GAP/JUNCTION_R trên ảnh Voronoi có đáp án hình học (2026-11-21): sai số ℓ −3 % đến +4 % cho cả ba phương pháp (`tests/test_core.py`). Trước hiệu chỉnh, lưới tròn lệch tới −15 %.
- Ảnh thật phụ thuộc chất lượng tẩm thực; người đo phải rà soát và sửa giao điểm (E1382 §10).
