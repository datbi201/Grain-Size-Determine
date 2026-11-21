# Grain Size Determine

Phần mềm Windows đo **cấp cỡ hạt trung bình ASTM G** từ ảnh kim tương theo **ASTM E112-25** (phương pháp đoạn chắn) và **ASTM E1382-97(2023)** (phân tích ảnh bán tự động).

## Chức năng
- Phương pháp: **Abrams** (3 vòng tròn), **Heyn** (đường thẳng), **Chord hỗn hợp** (Fig. 5: 3 vòng tròn + 2 đường chéo).
- Thang đo: tự tìm thanh scale đỏ, chọn 2 điểm trên thanh scale bất kỳ, nhập µm/px hoặc đường kính trường nhìn.
- Tách biên hạt tự động; nút **Tự động** chỉnh độ sáng / tương phản / gamma / CLAHE cho rõ biên.
- **Sửa giao điểm thủ công**: thêm (cắt biên 1, điểm ba hạt 1,5, đầu mút 0,5), loại, khôi phục, hoàn tác.
- Thống kê nhiều trường theo E112 §15 (ℓ tb, s, 95 %CI, %RA, G) và đánh giá Đạt/Chưa đạt.
- **Báo cáo PDF A4** một hoặc nhiều phương pháp, có xem trước và in; xuất CSV, lưu ảnh kết quả.
- **Lưu / Mở project** (`.gsd`), hỏi lưu khi đóng.

## Chạy
- Bản exe: `..\02_LOCAL_RUNTIME\GrainSizeDetermine\GrainSizeDetermine.exe`.
- Từ mã nguồn: `Chay-GrainSizeDetermine.bat` (Python 3.12; lần đầu tự cài thư viện trong `requirements.txt`).
- Build exe: `Bien-dich-Windows.bat`.
- Kiểm thử lõi: `py -3.12 tests/test_core.py`.

## Cấu trúc
| Thư mục | Nội dung |
|---|---|
| `src/` | `GrainSizeApp.py` (giao diện), `grain_core.py` (tính toán), `grain_report.py` (báo cáo PDF) |
| `assets/` | Biểu tượng ứng dụng |
| `tools/` | `build_windows.py`, `gen_icon.py` |
| `tests/` | `test_core.py` |
| `docs/` | `PHUONG-PHAP.md` (công thức, quy tắc đếm, hiệu chỉnh) |

Xem thêm: `CLAUDE.md` (quy tắc dự án), `BAN-GIAO.md` (hiện trạng), `LICHSU-REV.md` (lịch sử bản).
