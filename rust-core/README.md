# gsd_core – bản thử lõi tính bằng Rust

Port 1:1 của `src/grain_core.py` (chưa có giao diện): đọc ảnh, trường nhìn tròn, thanh scale đỏ, tách biên hạt, loại vùng mờ, lưới Abrams / Heyn / Chord hỗn hợp, đếm giao điểm (1 / 1,5 / 0,5), ℓ, G, thống kê E112 §15.

## Chạy
- Build: `cargo build --release` (trên máy này đặt `CARGO_TARGET_DIR=D:\gsd-target` vì ổ C: gần đầy).
- Dòng lệnh: `gsd_core.exe <ảnh>... [--um 0.5] [--sens 85] [--no-focus] [--no-fov] [--no-bar] [--hv]` → in JSON (P, L, ℓ, G từng phương pháp).
- Kiểm thử: `cargo test --release` (độ đúng trên ảnh Voronoi so với đáp án hình học; hình thái nhanh = vét cạn; công thức, t, 95 %CI).
- Đối chiếu với bản Python: `py -3.12 rust-core/compare_python.py "<thư mục ảnh>" D:\gsd-target\release\gsd_core.exe`.

## Kết quả thử (2026-11-21)
| Hạng mục | Kết quả |
|---|---|
| Độ đúng trên ảnh Voronoi (3 ảnh × 3 phương pháp) | sai số ℓ −5,1 % … +4,4 % (giới hạn 8 %), ngang bản Python |
| Đối chiếu Python trên 40 ảnh mẫu thật, cùng µm/px | \|ΔG\| trung bình 0,001; lớn nhất 0,019 (Chord HH); 27/40 ảnh trùng tuyệt đối cả 3 phương pháp |
| Trường nhìn tròn / thanh đỏ | cùng kết luận 40/40 |
| Thời gian 40 ảnh (cả nạp ảnh) | Rust 18,7 s; Python 31,7 s (≈ 1,7 lần nhanh hơn) |
| Kích thước | `gsd_core.exe` 1,8 MB (bản exe Python hiện ~287 MB cả thư viện) |

## Những chỗ phải tái tạo đúng hành vi OpenCV / scikit-image để khớp
- **Kênh L\***: thuật toán số nguyên `RGB2Lab_b` của OpenCV (bảng gamma 255·2³, bảng căn bậc ba 2¹⁵, hệ số × 2¹²), không phải công thức số thực – lệch ±1 ở ~12 % điểm nếu dùng số thực.
- **Giải mã JPEG**: mozjpeg (họ libjpeg-turbo như OpenCV); crate `image` cho điểm ảnh khác.
- **Gauss 8-bit**: nhân dấu phẩy tĩnh 8 bit lẻ (phân bổ sai số), lọc dọc 16 bit lẻ, làm tròn (v + 2¹⁵) >> 16.
- **CLAHE**: khi một chiều không chia hết 8, OpenCV đệm cả chiều còn lại thêm 8 hàng.
- **Làm mảnh**: bảng tra 256 mục của `skimage._fast_skeletonize` (khác Zhang–Suen cổ điển ~3 % số điểm).
- **Hình thái học**: van Herk / Gil–Werman theo từng hàng của phần tử (kết quả trùng bản vét cạn, nhanh hơn nhiều).
- Còn lại: lọc Gauss số thực của nền cho vài điểm lệch ở giới hạn cắt số nguyên → có ảnh lệch 1 giao điểm.

## Chưa làm
Giao diện (egui), báo cáo PDF, đọc/ghi project `.gsd`, sửa giao điểm thủ công, tự chỉnh sáng/tương phản, EXIF xoay ảnh.
