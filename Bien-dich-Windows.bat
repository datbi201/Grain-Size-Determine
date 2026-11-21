@echo off
chcp 65001 >nul
cd /d "%~dp0"
rem Bien dich ban exe (PyInstaller) vao .._LOCAL_RUNTIME va dong zip vao .._ONEDRIVE_BACKUPeleases
py -3.12 toolsuild_windows.py %*
pause
