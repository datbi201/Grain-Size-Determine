@echo off
chcp 65001 >nul
cd /d "%~dp0"
rem Bien dich ban Rust: exe vao ..\..\02_LOCAL_RUNTIME\GrainSizeDetermine_rust, zip vao ..\..\03_ONEDRIVE_BACKUP\releases
set CARGO_TARGET_DIR=D:\gsd-target
set PATH=%USERPROFILE%\.cargo\bin;%PATH%
cargo build --release || (pause & exit /b 1)
"%CARGO_TARGET_DIR%\release\GrainSizeDetermine_rust.exe" --selftest "%TEMP%\gsd_selftest" || (echo Tu kiem tra that bai & pause & exit /b 1)
set OUT=%~dp0..\..\02_LOCAL_RUNTIME\GrainSizeDetermine_rust
if not exist "%OUT%" mkdir "%OUT%"
copy /Y "%CARGO_TARGET_DIR%\release\GrainSizeDetermine_rust.exe" "%OUT%\" >nul
powershell -NoProfile -Command "Compress-Archive -Force -Path '%OUT%\GrainSizeDetermine_rust.exe' -DestinationPath '%~dp0..\..\03_ONEDRIVE_BACKUP\releases\GrainSizeDetermine_Rev0.1.1_rust.zip'"
echo Xong: %OUT%\GrainSizeDetermine_rust.exe
pause
