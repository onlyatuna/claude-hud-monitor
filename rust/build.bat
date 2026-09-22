@echo off
echo Building Claude HUD Monitor (Rust Release)...
taskkill /F /IM ClaudeHUD.exe >nul 2>&1
cargo build --release
if %ERRORLEVEL% equ 0 (
    echo.
    echo ========================================================
    echo Build Successful!
    echo Output: target\release\ClaudeHUD.exe
    echo ========================================================
) else (
    echo Build failed!
)
pause
