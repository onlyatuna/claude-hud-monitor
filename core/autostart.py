import sys
import os

APP_NAME = "ClaudeHUDMonitor"
RUN_KEY_PATH = r"Software\Microsoft\Windows\CurrentVersion\Run"

def is_autostart_enabled() -> bool:
    if sys.platform == "win32":
        try:
            import winreg
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_KEY_PATH, 0, winreg.KEY_READ) as key:
                value, _ = winreg.QueryValueEx(key, APP_NAME)
                return bool(value)
        except Exception:
            return False
    elif sys.platform == "darwin":
        plist_path = os.path.expanduser(f"~/Library/LaunchAgents/com.claudehud.plist")
        return os.path.exists(plist_path)
    return False

def set_autostart(enable: bool) -> bool:
    if sys.platform == "win32":
        try:
            import winreg
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_KEY_PATH, 0, winreg.KEY_SET_VALUE) as key:
                if enable:
                    if getattr(sys, 'frozen', False):
                        cmd = f'"{sys.executable}"'
                    else:
                        pythonw = os.path.join(os.path.dirname(sys.executable), "pythonw.exe")
                        runner = pythonw if os.path.exists(pythonw) else sys.executable
                        script = os.path.abspath(sys.argv[0])
                        cmd = f'"{runner}" "{script}"'
                    winreg.SetValueEx(key, APP_NAME, 0, winreg.REG_SZ, cmd)
                else:
                    try:
                        winreg.DeleteValue(key, APP_NAME)
                    except FileNotFoundError:
                        pass
                return True
        except Exception as e:
            print(f"[AutoStart Windows] Error: {e}")
            return False
    elif sys.platform == "darwin":
        try:
            plist_dir = os.path.expanduser("~/Library/LaunchAgents")
            os.makedirs(plist_dir, exist_ok=True)
            plist_path = os.path.join(plist_dir, "com.claudehud.plist")
            if enable:
                if getattr(sys, 'frozen', False):
                    args = [sys.executable]
                else:
                    script = os.path.abspath(sys.argv[0])
                    args = [sys.executable, script]

                args_xml = "\n".join(f"        <string>{a}</string>" for a in args)
                plist_content = f"""<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.claudehud</string>
    <key>ProgramArguments</key>
    <array>
{args_xml}
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>"""
                with open(plist_path, "w", encoding="utf-8") as f:
                    f.write(plist_content)
            else:
                if os.path.exists(plist_path):
                    os.remove(plist_path)
            return True
        except Exception as e:
            print(f"[AutoStart macOS] Error: {e}")
            return False
    return False
