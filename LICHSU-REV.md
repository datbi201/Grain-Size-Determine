# Lịch sử các bản Grain Size Determine

| Bản | Ngày | Nội dung chính | Vị trí lưu |
|---|---|---|---|
| Rev 0.1.1_rust | 2026-11-21 | Bản giao diện Rust (egui) trên lõi `rust-core`, nhánh `rust-gui`: đủ chức năng Rev 0.1 (thang đo, 3 phương pháp, chỉnh ảnh, màu, sửa thủ công, cột Báo cáo, thống kê, PDF A4 nhiều phương pháp, CSV, ảnh KQ, project `.gsd` dùng chung định dạng với bản Python); xem trước PDF trong ứng dụng + In (PDFium), chữ thông số trên ảnh kết quả, chọn/xóa nhiều ảnh; exe 10,5 MB + pdfium.dll | Nhánh `rust-gui`; exe `..\02_LOCAL_RUNTIME\GrainSizeDetermine_rust`; zip `..\03_ONEDRIVE_BACKUP\releases\GrainSizeDetermine_Rev0.1.1_rust.zip` |
| Rev 0.1 | 2026-11-21 | Bản đầu tiên trong kho. Abrams / Heyn / Chord hỗn hợp (Fig. 5); quy tắc đếm E112 (1 / 1 / 1,5 / 0,5); thống kê E112 §15; nút tự chỉnh sáng/tương phản/gamma/CLAHE; chọn màu; sửa giao điểm thủ công (thêm, loại, khôi phục, hoàn tác); cột «Báo cáo» chọn ảnh đưa vào thống kê/báo cáo; báo cáo PDF A4 nhiều phương pháp có xem trước và in; lưu/mở project `.gsd`; biểu tượng mới | Kho này; exe ở `..\02_LOCAL_RUNTIME\GrainSizeDetermine`; zip ở `..\03_ONEDRIVE_BACKUP\releases\GrainSizeDetermine_Rev0.1.zip` |

## Trước khi có kho (không đánh số Rev)
| Bản | Nội dung | Vị trí |
|---|---|---|
| v1.1 "Grain Size Analyzer" | GUI Tkinter, 3 phương pháp, báo cáo PDF 1 phương pháp | `..\03_ONEDRIVE_BACKUP\legacy_versions\v1.1_GrainSizeAnalyzer_exe` (exe), `v1.1_GrainSizeAnalyzer_src` (tệp giao diện) |
| v0 kéo-thả | Script kéo thả ảnh, Abrams + Heyn, xuất CSV | `..\03_ONEDRIVE_BACKUP\legacy_versions\v0_DragDrop_script` |
