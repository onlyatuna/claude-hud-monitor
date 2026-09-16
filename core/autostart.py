import sys
import os
import winreg

APP_NAME = "ClaudeHUDMonitor"
RUN_KEY_PATH = r"Software\Microsoft\Windows\CurrentVersion\Run"

def get_launch_command() -> str:
    # If compiled with PyInstaller / cx_Freeze
    if getattr(sys, 'frozen', False):
        exe_path = sys.executable
        return f'"{exe_path}"'
    else:
        # Running as python script; use pythonw to avoid console window if possible
        python_exe = sys.executable
        # Try to find pythonw.exe in same directory
        dir_name = os.path.dirname(python_exe)
        pythonw = os.path.join(dir_name, "pythonw.exe")
        runner = pythonw if os.path.exists(pythonw) else python_exe
        script_path = os.path.abspath(sys.argv[0])
        return f'"{runner}" "{script_path}"'

def is_autostart_enabled() -> bool:
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_KEY_PATH, 0, winreg.KEY_READ) as key:
            value, _ = winreg.QueryValueEx(key, APP_NAME)
            return bool(value)
    except FileNotFoundError:
        return False
    except Exception as e:
        print(f"[AutoStart] Query error: {e}")
        return False

def set_autostart(enable: bool) -> bool:
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_KEY_PATH, 0, winreg.KEY_SET_VALUE) as key:
            if enable:
                cmd = get_launch_command()
                winreg.SetValueEx(key, APP_NAME, 0, winreg.REG_SZ, cmd)
                print(f"[AutoStart] Enabled: {cmd}")
            else:
                try:
                    winreg.DeleteValue(key, APP_NAME)
                    print("[AutoStart] Disabled")
                except FileNotFoundError:
                    pass
            return True
    except Exception as e:
        print(f"[AutoStart] Set error: {e}")
        return False
