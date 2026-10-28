@echo off
chcp 65001 >nul
cd /d "%~dp0"
if not exist "%~dp0faceit-bypass.exe" (
  echo Нет faceit-bypass.exe рядом с этим файлом.
  pause
  exit /b 1
)
"%~dp0faceit-bypass.exe"
