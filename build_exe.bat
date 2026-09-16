@echo off
cd /d "%~dp0"
echo ========================================================
echo   Building Claude HUD Monitor into Standalone EXE...
echo ========================================================
echo.
echo Closing any running ClaudeHUD instances...
taskkill /f /im ClaudeHUD.exe 2>nul

pyinstaller --noconsole --onefile --name "ClaudeHUD" --icon "assets\app_icon.ico" --add-data "assets;assets" main.py
echo.
echo ========================================================
echo   Build complete! Output is located at dist\ClaudeHUD.exe
echo ========================================================
pause
