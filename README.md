# AI HUD Monitor

Python / PySide6 桌面 AI 額度 HUD，同時顯示 Claude Code、Antigravity CLI（AGY）與 Codex。
支援橫直排版、置頂、透明度、系統匣、倒數與滑鼠穿透。

## 開發與本地測試

驗證基準為 Python 3.12，建議使用獨立虛擬環境。

```powershell
python -m venv .venv
.venv\Scripts\Activate.ps1
python -m pip install -r requirements-build.txt
python -B -m unittest discover -s tests -v
python main.py
```

macOS 使用 `source .venv/bin/activate`。Windows 使用 `build_exe.bat`，macOS 使用 `bash build_mac.sh` 打包。
GitHub Windows 產物為 `ClaudeHUD-Windows.exe`；本地腳本產物為 `dist/ClaudeHUD.exe`。
本地修改尚未發布，詳見 [驗證紀錄](docs/LOCAL_VALIDATION.md)。

## 額度資料來源

| 服務 | 查詢方式 | 必要條件 |
| --- | --- | --- |
| Claude | 本機 OAuth → `/api/oauth/usage` | `~/.claude/.credentials.json` 含有效 access token |
| Codex | 本機 OAuth → `/backend-api/wham/usage` | `~/.codex/auth.json` 含有效 access token |
| AGY | `agy --output-format json --print /quota` | 已安裝且登入相容的 AGY CLI |

Adapter 依賴服務回應／CLI 格式，不保證所有登入儲存方式或未來版本相容。
不需手動输入 API Key；HUD 不更新或寫入認證檔。只存於系統鑰匙圈等登入方式，目前可能無法使用。

- 百分比為**已使用額度**；AGY 的 `C/G 剩餘` 徽章代表第三方群組剩餘額度。
- `--` 表示沒有有效資料，不能解讀為 0%。Codex 窗口依回應長度顯示，未知時為 PRIMARY／SECONDARY。
- `STALE` 表示查詢失敗後保留的最後成功資料；副文字顯示資料時間，滑鼠停留卡片可查看錯誤。
- Codex 顯示回應中的方案，不推測目前模型。AGY 同窗口多個 bucket 採已用比例最高者。
- 預設每 60 秒更新；失敗採退避，最高 15 分鐘，HTTP Retry-After 較長時優先遵守。手動刷新可立即重試。

格式與排查：[Provider 相容性](docs/PROVIDERS.md)。

## 操作與平台狀態

| 操作 | 功能 |
| --- | --- |
| Alt+C（macOS 為 Option+C） | 顯示／隱藏 |
| Alt+Shift+C | 開關穿透，亦可由系統匣解除 |
| 標題列 ⇄ | 橫直排版 |
| 雙擊空白處 | 立即刷新 |
| 拖曳邊框／空白處 | 縮放／移動 |
| 右鍵與系統匣 | 透明度、置頂、更新頻率、開機啟動等 |

Windows 為本輪驗證平台。macOS 快捷鍵、穿透與 LaunchAgent 已有實作及模擬測試，**仍需實機驗證**。
macOS 快捷鍵使用 pynput，需要系統輔助使用／輸入監控權限；未授權時使用系統匣。
CI macOS 目標為 Apple Silicon arm64，不宣稱 Universal 或 Intel 支援。Linux 尚不在支援範圍。

## 設定與診斷

原始碼模式保留專案內 `config.json`；打包後使用：

- Windows：`%APPDATA%/ClaudeHUDMonitor/config.json`
- macOS：`~/Library/Application Support/ClaudeHUDMonitor/config.json`

`diagnostics.log` 在設定檔旁，輪替上限為 256 KiB × 3 份。只記錄耗時、退出碼等操作資訊，不記錄 token 或回應內容。
舊單檔版暫存目錄中的設定不保證能找回。詳見 [架構規格](PROJECT_SPEC.md)。

## 貢獻與發布

`main` 與 `develop` 維持既有保護分支流程。修正從 `develop` 建立 topic branch，PR 以 `develop` 為目標。
本地測試不需要推送、建立 Tag 或發布。CI 保留既有建置工作，增加 PR 觸發與測試；`v*` Tag 發布流程保留。
參閱 [CONTRIBUTING.md](CONTRIBUTING.md)、[SECURITY.md](SECURITY.md) 與 [行為準則](CODE_OF_CONDUCT.md)。

授權：[AGPL-3.0](LICENSE)。
