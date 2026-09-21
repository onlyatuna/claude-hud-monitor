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

python -m PyInstaller --noconfirm --clean ClaudeHUD.spec
if errorlevel 1 exit /b 1
echo.
echo ========================================================
echo   Build complete! Output is located at dist\ClaudeHUD.exe
echo ========================================================
pause
