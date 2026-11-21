# Bàn giao Grain Size Determine

Cập nhật lần cuối: 2026-11-21.

## Hiện trạng
- **Bản mới nhất: Rev 0.1** (2026-11-21), bản đầu tiên trong kho. Chi tiết chức năng: `README.md`; công thức và hiệu chỉnh: `docs/PHUONG-PHAP.md`.
- Kiểm thử lõi `tests/test_core.py`: 20/20 đạt (độ đúng bộ đếm trên ảnh Voronoi −3,2 % đến +3,5 % so với đáp án hình học; sửa thủ công; thống kê E112 §15; PDF 3 phương pháp).
- Giao diện đã chạy bằng script điều khiển cửa sổ và chụp màn hình: thêm ảnh, tự tìm thanh đỏ, áp scale, đo, nút Tự động chỉnh ảnh, bỏ chọn «Báo cáo», thêm giao điểm thủ công, lưu project → project mới → mở lại (số liệu và chỉnh sửa giữ nguyên), hộp thoại báo cáo chọn 3 phương pháp, xem trước 9 trang, hỏi lưu khi đóng.
- Bản exe `..\02_LOCAL_RUNTIME\GrainSizeDetermine\GrainSizeDetermine.exe` tự kiểm tra đạt (`--selftest`: phân tích + PDF). Zip: `..\03_ONEDRIVE_BACKUP\releases\GrainSizeDetermine_Rev0.1.zip`.
- **Chưa thử bằng thao tác chuột thật**: kéo thả ảnh từ Explorer, click sửa giao điểm, lệnh In (gửi PDF cho chương trình đọc PDF mặc định). **Chưa nghiệm thu bởi người dùng.**

## Việc đang chờ
1. Người dùng chạy thử Rev 0.1 và báo lại.
2. Kho GitHub: `https://github.com/datbi201/Grain-Size-Determine` (**Public**, nhánh `main`, tạo ngày 2026-11-21 bằng `gh`). `gh` 2.105.0 cài ở `%LOCALAPPDATA%\Programs\GitHubCLI` (đã thêm vào PATH người dùng). Trước mỗi lần commit chạy `git pull --rebase origin main`.
3. Đề xuất: kiểm chứng trên bộ ảnh có kết quả đếm tay của PTN; dịch báo cáo sang tiếng Anh nếu cần; nhãn thước bằng OCR để khỏi nhập µm.

## Môi trường build trên máy này
- Python 3.12 (`py -3.12`) đã cài đủ thư viện trong `requirements.txt` + `pyinstaller`.
- Build: `Bien-dich-Windows.bat` (hoặc `py -3.12 tools/build_windows.py`). Thư mục build tạm ở `%TEMP%`.
- Tạo lại biểu tượng: `py -3.12 tools/gen_icon.py`.
- Trình đồng bộ thư mục tạo `desktop.ini` cả trong `.git`, làm git báo `bad object refs/desktop.ini`. Gặp lỗi này thì xoá: `find .git -name desktop.ini -delete` (Git Bash) rồi chạy lại lệnh git.
- Thiết lập người dùng (màu, tham số, scale đã nhập theo ảnh) lưu ở `%APPDATA%\GrainSizeDetermine\settings.json`.

## Quyết định của người dùng
- 2026-11-20: phần mềm GUI hoàn chỉnh thay cho script kéo thả.
- 2026-11-21: chuẩn hoá theo E112-25 / E1382; kế thừa tính toán đã cân chỉnh của GSCM Rev.0.1 RC7; thêm Chord hỗn hợp (3 vòng tròn + 2 đường chéo); báo cáo PDF A4; chỉnh sáng/tương phản; chọn màu; sửa giao điểm thủ công.
- 2026-11-21: nút Tự động chỉnh ảnh; không che tối vùng thước; cột «Báo cáo» chọn ảnh đưa vào báo cáo; báo cáo chọn được cả 3 phương pháp, có xem trước; biểu tượng mới cách điệu từ ảnh kim tương; lưu/mở project, hỏi lưu khi đóng; tổ chức thư mục 3 tầng như các dự án khác; kho mới `Grain-Size-Determine`, bắt đầu Rev 0.1.
- 2026-11-21: cài `gh`; kho `Grain-Size-Determine` để **Public**.
