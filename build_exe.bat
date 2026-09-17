@echo off
cd /d "%~dp0"
echo ========================================================
echo   Building Claude HUD Monitor into Standalone EXE...
echo ========================================================
echo.
python -m pip install -r requirements-build.txt
if errorlevel 1 exit /b 1
python -B -m unittest discover -s tests -v
if errorlevel 1 exit /b 1

python -m PyInstaller --noconsole --onefile --name "ClaudeHUD" --icon "assets\app_icon.ico" --add-data "assets;assets" main.py
if errorlevel 1 exit /b 1
echo.
echo ========================================================
echo   Build complete! Output is located at dist\ClaudeHUD.exe
echo ========================================================
pause
