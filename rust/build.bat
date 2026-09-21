@echo off
echo Building Claude HUD Monitor (Rust Release)...
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
