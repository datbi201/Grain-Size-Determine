# Grain Size Determine – bản giao diện Rust (Rev 0.1.1_rust)

Giao diện egui viết bằng Rust, dùng lõi tính `../rust-core` (khớp bản Python: |ΔG| ≤ 0,02 trên 40 ảnh mẫu).
Một tệp `GrainSizeDetermine_rust.exe` khoảng 9,8 MB, không cần cài Python hay thư viện.

## Chức năng (như bản Python Rev 0.1)
- Thêm ảnh / thư mục, kéo thả ảnh hoặc tệp `.gsd` vào cửa sổ.
- Thang đo: tự tìm thanh đỏ, chọn 2 điểm, nhập µm/px, đường kính trường nhìn; nhớ scale theo ảnh.
- Phương pháp Abrams / Heyn / Chord hỗn hợp (Fig. 5, tuỳ chọn thêm 2 đường 100 mm); độ nhạy, loại vùng mờ.
- Xử lý ảnh: độ sáng, tương phản, gamma, CLAHE, nút «Tự động»; tuỳ chọn dùng ảnh đã xử lý để tách biên.
- Màu chord / giao điểm tự động / thủ công / biên hạt.
- Sửa giao điểm thủ công (phím E): thêm (1 / 1,5 / 0,5), loại, khôi phục (Shift), hoàn tác (Ctrl+Z), về kết quả tự động.
- Cột «BC» chọn ảnh đưa vào thống kê, báo cáo, CSV; thống kê E112 §15 (ℓ tb, 95 %CI, %RA, G).
- Báo cáo PDF A4 một hoặc nhiều phương pháp (font tiếng Việt Arial của Windows); «Xem trước» mở PDF tạm bằng trình đọc PDF mặc định.
- Xuất CSV, lưu ảnh kết quả, Lưu / Mở project `.gsd` – **cùng định dạng với bản Python** (mở chéo được), hỏi lưu khi đóng.

## Build và kiểm thử
- `Bien-dich-Rust.bat`: build (đặt `CARGO_TARGET_DIR=D:\gsd-target`), tự kiểm tra, chép exe sang `..\..\02_LOCAL_RUNTIME\GrainSizeDetermine_rust\`, đóng zip vào `..\..\03_ONEDRIVE_BACKUP\releases\`.
- Tự kiểm tra không cần giao diện: `GrainSizeDetermine_rust.exe --selftest <thư mục ra> <ảnh>...` (đo, sửa thủ công, lưu và mở lại project, xuất PDF).
- Đọc project: `--readproj <tệp.gsd> <tệp kết quả.txt>`.
- Chụp giao diện để kiểm tra (không cần chuột): biến môi trường `GSD_SHOT=<png>` (kèm `GSD_SHOT_VIEW=abrams|heyn|mixed`, `GSD_SHOT_EDIT=1`, `GSD_SHOT_REPORT=1`).

## Khác bản Python
- Xem trước báo cáo mở trong trình đọc PDF của Windows (chưa có khung xem trong ứng dụng); lệnh In thực hiện từ trình đọc PDF.
- Ảnh kết quả lưu ra không có dòng chữ thông số trên ảnh (số liệu có trong báo cáo / CSV).
- CLAHE trong phần xử lý ảnh áp lên độ sáng rồi nhân tỉ lệ vào 3 kênh màu (bản Python đổi Lab ↔ BGR).
- Bảng ảnh: chọn ảnh bằng click tên; «Xóa ảnh đang xem» thay cho xoá nhiều dòng.
