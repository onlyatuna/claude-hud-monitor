# ClaudeHUD Monitor

桌面 AI 額度監控 HUD，支援 Claude Code、Antigravity CLI (AGY) 與 OpenAI Codex。
支援「傳統卡片 (Classic Cards)」、「儀表表格 (Modern Table)」與「側邊欄 (Sidebar)」三種風格切換（傳統卡片已併入側邊欄固定窄欄）、水平橫排／垂直直排切換、主題配色切換、視窗置頂、透明度設定、系統匣常駐、額度重置倒數與滑鼠穿透。

本專案為 Python (PySide6) 實作 (`python/`)，產生的執行檔名稱為 `ClaudeHUD`。

---

## 實作規格與架構

| 項目 | Python 實作 (`python/`) |
| :--- | :--- |
| **執行檔名** | `ClaudeHUD.exe` (Windows) / `ClaudeHUD.app` (macOS) |
| **語言與版本** | Python 3.12 |
| **GUI 框架** | PySide6 (Qt 6.7+) |
| **HTTP 客戶端** | urllib.request (標準函式庫) |
| **行程與記憶體機制** | 內建 gc.collect() 與 Win32 EmptyWorkingSet 工作集釋放機制 |
| **打包方式** | PyInstaller 6.x (自訂 spec 排除未使用的 Qt 模組與語系) |
| **支援平台與架構** | Windows (x86_64), macOS (arm64) |

---

## 專案結構

```text
claude-hud-monitor/
├── .github/workflows/
│   ├── ci.yml                 # CI 工作流程 (變更 python/ 時執行 Python 測試)
│   └── release.yml            # 發布工作流程 (編譯 Windows / macOS 打包產物)
├── python/                    # Python 實作原始碼與測試
│   ├── core/                  # Providers、設定管理、更新排程
│   ├── ui/                    # PySide6 介面與卡片元件
│   ├── system/                # 全域快捷鍵與系統 API 呼叫
│   ├── tests/                 # 單元測試套件
│   ├── ClaudeHUD.spec         # PyInstaller 打包規格檔
│   ├── main.py                # Python 進入點
│   └── requirements.txt
├── docs/                      # 規格與相容性文件
├── LICENSE                    # AGPL-3.0
└── README.md
```

---

## 本地建置與測試

### Python 實作 (`python/`)

需要 Python 3.12 環境。

```powershell
cd python

python -m venv .venv
.venv\Scripts\Activate.ps1    # macOS: source .venv/bin/activate

pip install -r requirements-build.txt

# 執行回歸測試
python -B -m unittest discover -s tests -v

# 啟動應用程式
python main.py
```

Windows 環境可使用 `python/build_exe.bat` 產出 `dist/ClaudeHUD.exe`；macOS 環境可使用 `python/build_mac.sh` 產出 `dist/ClaudeHUD.app`。

---

## 額度資料來源

| 服務 | 查詢途徑 | 認證與必要條件 |
| :--- | :--- | :--- |
| **Claude** | 本機 OAuth → `/api/oauth/usage` | `~/.claude/.credentials.json` 包含有效 access token (支援多帳號切換) |
| **Codex** | 本機 OAuth → `/backend-api/wham/usage` | `~/.codex/auth.json` 包含有效 access token |
| **AGY** | Google 端點 `daily-cloudcode-pa` 的額度查詢（5 小時、C/G）＋ agy statusLine 快取（Gemini 週額度、Argon 週額度） | 已安裝且登入的 Antigravity CLI；週額度與 Argon 需啟用 statusLine，見下方 |

* 數值顯示為已使用額度百分比。
* AGY 的 `C/G 剩餘` 顯示 Claude/GPT 群組的每週剩餘；`Argon` 顯示 Gemini 4 Argon（Pro 方案獨立額度）的每週剩餘。

### 啟用 AGY 週額度與 Argon（一次性設定）

Gemini 週額度與 Argon 週額度沒有公開 API，agy 只會在互動模式把它們推給 `statusLine` 指令。HUD 提供一支只寫入額度的 hook：

```
cd python
python main.py --install-agy-statusline      # 先備份 ~/.gemini/antigravity-cli/settings.json 再寫入
python main.py --uninstall-agy-statusline    # 移除
```

安裝後重開 agy 互動模式即可。注意：

* 需要 `python`（Windows）或 `python3` 在 PATH 上，agy 會用它執行 hook。
* 只有 agy 互動模式開著時才會更新；agy 沒開時沿用最後一次的值，已過重置時間的項目會顯示 `--`。
* 已設定其他 statusLine 時不會覆蓋；設定資料夾路徑含空白時無法安裝。
* hook 只保存各項額度的剩餘比例與重置時間，不保存 email、路徑或 token。
* 未啟用時，Weekly 與 Argon 顯示 `--`，其餘（5 小時、C/G）不受影響。
* 預設更新週期為 60 秒。若請求失敗，採指數退避重試，若標頭含 `Retry-After` 則優先採用。

---

## 快捷鍵與操作

| 操作 | 說明 |
| :--- | :--- |
| **Alt + C** (macOS: Option + C) | 顯示／隱藏視窗 |
| **Alt + Shift + C** | 開關滑鼠穿透模式（亦可從系統匣圖示切換） |
| **系統匣選單 → 介面風格** | 在「儀表表格」與「側邊欄」之間切換（舊設定的「傳統卡片」會自動改成側邊欄固定窄欄，位置與橫直排保留） |
| **縮小儀表表格視窗** | 縮到 380×280 以下會收合成每個服務一顆圓盤（最小約 174×62，細節在滑過圓盤的提示裡）；拉大回 396×296 以上恢復完整表格。兩種尺寸分開記住 |
| **右鍵選單 → 外觀 → 毛玻璃模糊** (表格) | Windows 的 Acrylic 毛玻璃；表格視窗會裁成圓角，拖曳或縮放時暫停模糊以免卡頓。關閉則改為不模糊的圓角面板 |
| **側邊欄** | 貼在螢幕右緣的懸浮窄條（不保留螢幕空間，可半透明、滑鼠穿透）。細條 92 px：點或停留在服務圖示會向左彈出完整卡片，再點可釘住；頂端按鈕切換成 220 px 固定窄欄；可自由拖曳並吸附到螢幕任一邊（貼上下邊變橫條），Ctrl＋滾輪或拖拉邊角縮放，按住磚再拖曳可換服務順序；所有服務的詳細頁面都能直接拖拉整條換順序（前兩條為磚上顯示的主要兩條）；用量 ≥ 90% 時磚上出現警示三角，視窗重設時磚會短暫亮一下，閒置時自動淡化（可在「行為」關閉） |
| **右鍵選單 → 配色 / 外觀** (表格模式) | 切換色階／雙色配色，以及淺色／深色／跟隨系統外觀 |
| **雙擊空白處** | 立即手動刷新各 Provider 額度 |
| **拖曳空白處 / 邊框** | 移動視窗 / 調整視窗尺寸 |
| **右鍵選單 / 系統匣圖示** | 透明度、置頂、更新頻率、開機啟動與帳號切換 |

---

## CI/CD 流程與產物清單

### 測試流程 (`.github/workflows/ci.yml`)
在發起 Pull Request 或推送到 `develop`、`main` 分支時觸發：
* 變更 `python/**` 時：執行 Python 測試矩陣 (Windows, macOS)。

### 發布流程 (`.github/workflows/release.yml`)
推送到 `v*` 標籤時觸發，於 GitHub Release 發布以下檔案：
* `ClaudeHUD-Python-Windows-x64.exe` (Python Windows x86_64 打包執行檔)
* `ClaudeHUD-Python-macOS-arm64.zip` (Python macOS arm64 打包應用程式)
* `SHA256SUMS.txt` (所有發布產物之 SHA256 校驗值清單)

---

## 開發與貢獻規範

* 分支策略：`develop` 為日常開發與 PR 目標分支，`main` 為穩定版本分支。
* 授權條款：[AGPL-3.0](LICENSE)。

儀表表格與側邊欄（Python 版）共用「外觀」設定：跟隨系統／淺色／深色。可從右鍵或系統匣選單切換，重啟後保留；選擇跟隨系統時，執行中也會自動更新。
