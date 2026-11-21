@echo off
chcp 65001 >nul
cd /d "%~dp0"
rem Bien dich ban Rust: exe + pdfium.dll vao ..\..\02_LOCAL_RUNTIME\GrainSizeDetermine_rust, zip vao ..\..\03_ONEDRIVE_BACKUP\releases
set CARGO_TARGET_DIR=D:\gsd-target
set PATH=%USERPROFILE%\.cargo\bin;%PATH%
cargo build --release || (pause & exit /b 1)
set OUT=%~dp0..\..\02_LOCAL_RUNTIME\GrainSizeDetermine_rust
if not exist "%OUT%" mkdir "%OUT%"
rem pdfium.dll (xem truoc PDF trong ung dung): lay tu vendor\ neu co, neu khong lay tu pypdfium2 cua Python 3.12
set PDFIUM=%~dp0vendor\pdfium.dll
if not exist "%PDFIUM%" for /f "delims=" %%P in ('py -3.12 -c "import pypdfium2_raw,os;print(os.path.join(os.path.dirname(pypdfium2_raw.__file__),'pdfium.dll'))" 2^>nul') do set PDFIUM=%%P
if exist "%PDFIUM%" (copy /Y "%PDFIUM%" "%OUT%\" >nul & copy /Y "%PDFIUM%" "%CARGO_TARGET_DIR%\release\" >nul) else (echo CANH BAO: khong tim thay pdfium.dll - Xem truoc se mo bang trinh doc PDF cua Windows)
"%CARGO_TARGET_DIR%\release\GrainSizeDetermine_rust.exe" --selftest "%TEMP%\gsd_selftest" || (echo Tu kiem tra that bai & pause & exit /b 1)
copy /Y "%CARGO_TARGET_DIR%\release\GrainSizeDetermine_rust.exe" "%OUT%\" >nul
copy /Y "%~dp0THIRD-PARTY-NOTICES.txt" "%OUT%\" >nul
powershell -NoProfile -Command "Compress-Archive -Force -Path '%OUT%\*' -DestinationPath '%~dp0..\..\03_ONEDRIVE_BACKUP\releases\GrainSizeDetermine_Rev0.1.1_rust.zip'"
echo Xong: %OUT%
pause
