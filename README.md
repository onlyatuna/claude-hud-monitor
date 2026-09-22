# ClaudeHUD Monitor

桌面 AI 額度監控 HUD，支援 Claude Code、Antigravity CLI (AGY) 與 OpenAI Codex。
支援「傳統卡片 (Classic Cards)」與「儀表表格 (Modern Table)」雙風格切換、水平橫排／垂直直排切換、主題配色切換、視窗置頂、透明度設定、系統匣常駐、額度重置倒數與滑鼠穿透。

本專案採 Monorepo 架構維護，包含 Rust 實作 (`rust/`) 與 Python 實作 (`python/`)。兩者產生的執行檔名稱統一為 `ClaudeHUD`。

---

## 實作規格與架構

| 項目 | Rust 實作 (`rust/`) | Python 實作 (`python/`) |
| :--- | :--- | :--- |
| **執行檔名** | `ClaudeHUD.exe` (Windows) / `ClaudeHUD` (macOS, Linux) | `ClaudeHUD.exe` (Windows) / `ClaudeHUD.app` (macOS) |
| **語言與版本** | Rust (2021 edition) | Python 3.12 |
| **GUI 框架** | egui 0.29 / eframe 0.29 | PySide6 (Qt 6.7+) |
| **HTTP 客戶端** | ureq 2.12 (TLS) | urllib.request (標準函式庫) |
| **行程與記憶體機制** | 原生編譯二進制，無額外 Runtime 依賴 | 內建 gc.collect() 與 Win32 EmptyWorkingSet 工作集釋放機制 |
| **打包方式** | `cargo build --release` (LTO, strip, opt-level="z") | PyInstaller 6.x (自訂 spec 排除未使用的 Qt 模組與語系) |
| **支援平台與架構** | Windows (x86_64), macOS (Universal: arm64 + x86_64), Linux (x86_64) | Windows (x86_64), macOS (arm64) |

---

## 專案結構

```text
claude-hud-monitor/
├── .github/workflows/
│   ├── ci.yml                 # CI 工作流程 (依 paths 分別執行 Python / Rust 測試)
│   └── release.yml            # 發布工作流程 (跨平台矩陣編譯雙版本產物)
├── python/                    # Python 實作原始碼與測試
│   ├── core/                  # Providers、設定管理、更新排程
│   ├── ui/                    # PySide6 介面與卡片元件
│   ├── system/                # 全域快捷鍵與系統 API 呼叫
│   ├── tests/                 # 單元測試套件
│   ├── ClaudeHUD.spec         # PyInstaller 打包規格檔
│   ├── main.py                # Python 進入點
│   └── requirements.txt
├── rust/                      # Rust 實作原始碼與測試
│   ├── src/                   # 狀態機、HTTP 客戶端、平台系統匣與視窗
│   ├── assets/                # 專案圖示與資源
│   ├── Cargo.toml             # 二進制名稱定義為 ClaudeHUD
│   └── build.rs               # Windows 資源表與 DPI 設定
├── docs/                      # 規格與相容性文件
├── LICENSE                    # AGPL-3.0
└── README.md
```

---

## 本地建置與測試

### Rust 實作 (`rust/`)

需要 Rust 1.80+ 工具鏈。

```powershell
cd rust

# 本地執行
cargo run

# 執行測試套件
cargo test

# 編譯 Release 二進制 (產物路徑: rust/target/release/ClaudeHUD.exe)
cargo build --release
```

Windows 環境可使用 `rust/build.bat` 執行打包，使用 `rust/run.bat` 或 `rust/start_silent.vbs` 啟動。

---

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
| **AGY** | 本地 API / IPC 查詢 | 已安裝且登入的 Antigravity CLI |

* 數值顯示為已使用額度百分比。
* AGY 的 `C/G 剩餘` 顯示第三方群組可用餘額。
* 預設更新週期為 60 秒。若請求失敗，採指數退避重試，若標頭含 `Retry-After` 則優先採用。

---

## 快捷鍵與操作

| 操作 | 說明 |
| :--- | :--- |
| **Alt + C** (macOS: Option + C) | 顯示／隱藏視窗 |
| **Alt + Shift + C** | 開關滑鼠穿透模式（亦可從系統匣圖示切換） |
| **標題列 ⇄** (卡片模式) | 切換水平橫排與垂直直排 |
| **右鍵選單 → 介面風格** | 在「傳統卡片 (Classic Cards)」與「儀表表格 (Modern Table)」之間切換 |
| **右鍵選單 → 配色 / 外觀** (表格模式) | 切換色階／雙色配色，以及淺色／深色／跟隨系統外觀 |
| **雙擊空白處** | 立即手動刷新各 Provider 額度 |
| **拖曳空白處 / 邊框** | 移動視窗 / 調整視窗尺寸 |
| **右鍵選單 / 系統匣圖示** | 透明度、置頂、更新頻率、開機啟動與帳號切換 |

---

## CI/CD 流程與產物清單

### 測試流程 (`.github/workflows/ci.yml`)
在發起 Pull Request 或推送到 `develop`、`main` 分支時觸發：
* 變更 `python/**` 時：執行 Python 測試矩陣 (Windows, macOS)。
* 變更 `rust/**` 時：執行 Rust 測試矩陣 (Windows, macOS, Linux)。

### 發布流程 (`.github/workflows/release.yml`)
推送到 `v*` 標籤時觸發，於 GitHub Release 發布以下檔案：
* `ClaudeHUD-Rust-Windows-x64.exe` (Rust Windows x86_64 原生執行檔)
* `ClaudeHUD-Rust-macOS-Universal.zip` (Rust macOS Universal binary: arm64 + x86_64)
* `ClaudeHUD-Rust-Linux-x64` (Rust Linux x86_64 原生執行檔)
* `ClaudeHUD-Python-Windows-x64.exe` (Python Windows x86_64 打包執行檔)
* `ClaudeHUD-Python-macOS-arm64.zip` (Python macOS arm64 打包應用程式)
* `SHA256SUMS.txt` (所有發布產物之 SHA256 校驗值清單)

---

## 開發與貢獻規範

* 分支策略：`develop` 為日常開發與 PR 目標分支，`main` 為穩定版本分支。
* 授權條款：[AGPL-3.0](LICENSE)。
