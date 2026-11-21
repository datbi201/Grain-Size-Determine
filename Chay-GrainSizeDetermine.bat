@echo off
chcp 65001 >nul
cd /d "%~dp0"
set PYV=3.12
py -%PYV% -c "import cv2,numpy,skimage,tkinterdnd2,reportlab,pypdfium2,PIL" 2>nul
if errorlevel 1 (
  echo Lan dau chay: cai thu vien can thiet cho Python %PYV% ...
  py -%PYV% -m pip install -r requirements.txt
  if errorlevel 1 ( echo Cai dat that bai. & pause & exit /b 1 )
)
start "" pyw -%PYV% src\GrainSizeApp.py %*
