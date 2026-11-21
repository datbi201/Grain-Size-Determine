# Quy tắc dự án Grain Size Determine (nguồn duy nhất, đồng bộ qua git)

## 1. Dự án là gì
Phần mềm Windows (Python/Tkinter) đo cấp cỡ hạt trung bình theo ASTM E112-25 và E1382-97(2023) từ ảnh kim tương: Abrams (3 vòng tròn), Heyn (đường thẳng), Chord hỗn hợp (Fig. 5: 3 vòng tròn + 2 đường chéo). Tách biên tự động, người đo rà soát và sửa giao điểm thủ công, thống kê nhiều trường, báo cáo PDF A4, lưu/mở project `.gsd`.
Người dùng là kỹ sư ăn mòn và toàn vẹn đường ống, không phải lập trình viên: báo cáo bằng kết quả, giao diện, bảng số; không dán mã thô vào trả lời.

## 2. Cấu trúc 3 tầng (thư mục cha `Grain Sizing`)
- `01_REPO_GITHUB` (thư mục này): mã nguồn `src/`, biểu tượng `assets/`, công cụ `tools/` (build, tạo icon), kiểm thử `tests/`, tài liệu `docs/`. Có `.git`.
- `02_LOCAL_RUNTIME`: bản exe chạy thử `GrainSizeDetermine\GrainSizeDetermine.exe`. Không đưa vào git.
- `03_ONEDRIVE_BACKUP`: `releases/` (zip các bản), `legacy_versions/` (bản cũ), `samples/` (ảnh mẫu). Không đưa vào git.

## 3. Quy tắc lưu bản
- Số phiên bản: hằng `APP_VER` trong `src/GrainSizeApp.py` (bắt đầu Rev 0.1). Tăng khi phát hành bản mới.
- Kho chỉ giữ MỘT bản mới nhất. Bản cũ: chép sang `..\03_ONEDRIVE_BACKUP\legacy_versions\<tên bản>` trước khi thay. Lịch sử vẫn còn trong git.
- Mỗi bản phát hành: chạy `Bien-dich-Windows.bat` (gọi `tools/build_windows.py`): build PyInstaller, tự kiểm tra exe (`--selftest`), chép exe sang `..\02_LOCAL_RUNTIME\`, đóng zip vào `..\03_ONEDRIVE_BACKUP\releases\`.

## 4. Build và kiểm thử
- Python 3.12 (`py -3.12`); thư viện trong `requirements.txt`. Python 3.15 mặc định của máy chưa cài được scikit-image.
- Kiểm thử lõi: `py -3.12 tests/test_core.py` (độ đúng bộ đếm trên ảnh Voronoi so với đáp án hình học ±8 %, sửa thủ công, thống kê E112 §15, xuất PDF). Phải đạt hết trước khi commit thay đổi lõi.
- Thay đổi giao diện: chạy thử bằng script điều khiển cửa sổ và chụp màn hình; ghi rõ phần nào chưa thử bằng thao tác chuột thật.
- Không đổi công thức G, trọng số giao cắt (1 / 1 / 1,5 / 0,5) hay cách thống kê theo trường khi chưa có quyết định của người dùng.

## 5. Commit và đồng bộ
- Sau mỗi bản mới, sửa lỗi hoặc quyết định của người dùng: cập nhật `BAN-GIAO.md` (và `LICHSU-REV.md` nếu có bản mới) trong cùng lượt, rồi commit.
- Khi đã có remote: `git pull --rebase origin main` trước khi commit; xung đột thì dừng và báo người dùng.
- Dòng cuối commit: `Co-Authored-By` đúng model đang chạy. Không add `*.exe`, `*.zip`, `build/`, `dist/`, ảnh mẫu, project `.gsd`, PDF kết quả.
- Email tác giả: `343996192+datbi201@users.noreply.github.com` (đặt trong git config của kho).
