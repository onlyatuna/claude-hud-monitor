# Claude HUD Monitor (專案規格與開發規劃)

## 1. 專案背景與目標
現有的開源方案（如 `CodeZeno/Claude-Code-Usage-Monitor` 依附於系統工作列、`SlavomirDurej/claude-usage-widget` 為較肥大的 Electron 懸浮窗）缺乏硬體級 HUD（如 NVIDIA Alt+R 效能覆蓋、RivaTuner）的輕量流暢感與直覺美感。

本專案旨在打造一個專為開發者設計的 **極簡桌上型 AI 額度監控 HUD**：
- **極致輕量**：採用 Python + 原生視窗系統（Qt / PySide6），資源佔用極低。
- **硬體級 HUD 質感**：半透明黑底磨砂玻璃、無邊框、自適應暗黑風格。
- **直覺操作**：支援滑鼠自由拖曳、八向邊框自由縮放、滑鼠滾輪/右鍵選單調整透明度、視窗永遠置頂。
- **無感對接**：直接無縫讀取本地 Claude Code 認證（`~/.claude/.credentials.json`），無需手動維護金鑰。
- **隨手喚出**：支援全域快捷鍵（例如 `Alt+C` 或 `Alt+R`）一鍵顯隱。
- **獨立運行**：支援開機自啟動，並可直接打包為單一 `.exe`。

---

## 2. 系統架構設計 (SA 視角)

### 2.1 模組劃分
1. **Core Data Engine (`core/providers/`)**
   - 採用多 Provider 架構（Claude Code、Antigravity AGY、OpenAI Codex）。
   - `claude_provider.py`: 讀取本機 Claude Code OAuth Token (`~/.claude/.credentials.json`)。
   - 偽裝 Claude Code CLI User-Agent (`claude-code/x.x.x`) 請求 Anthropic Usage Endpoint。
   - 智慧快取與退避機制（避免觸發 HTTP 429）。
   - 解析 5-Hour 滾動額度、7-Day 總額度、重設時間戳記與費用拆分。

2. **UI / HUD Engine (`ui/hud_window.py`)**
   - 無邊框視窗 (`FramelessWindowHint`)、永遠置頂 (`WindowStaysOnTopHint`)。
   - 背景半透明（可動態滑桿或滾輪調節 Opacity 20%~100%）。
   - 邊界偵測實現八向視窗縮放 (Resize Grip)。
   - 現代化暗黑 HUD 介面：
     - 5-Hour Session 滾動進度條 + 倒數計時（如 "68% | 重設於 2h 45m"）。
     - 7-Day Weekly 總額度進度條。
     - 當日/當週額度消耗速率提示（警示色階：正常藍/綠 -> 警戒黃 -> 耗盡紅）。

3. **系統整合與互動 (`system/`)**
   - 全域快捷鍵註冊（一鍵喚醒/隱藏）。
   - Windows 系統匣（Tray Icon）與快捷選單。
   - 開機自啟動設定（Windows 註冊表 `Run` 機碼寫入/移除）。
   - 設定檔持久化（記錄視窗位置、大小、透明度、置頂狀態）。

---

## 3. 專案里程碑與實作規劃 (PM 視角)

| 階段 | 任務內容 | 交付產出 |
| :--- | :--- | :--- |
| **Phase 0** | 需求訪談、架構定義、環境確認 | 規格書、技術驗證 (POC) |
| **Phase 1** | 資料擷取引擎 + 核心 HUD 視窗 (MVP) | 可拖曳、半透明、即時顯示配額與倒數 |
| **Phase 2** | 右鍵選單、邊框縮放、全域快捷鍵、設定持久化 | 完整互動體驗、透明度滑桿、位置記憶 |
| **Phase 3** | 開機自啟動、系統匣整合、PyInstaller 打包 `.exe` | 獨立免安裝執行檔 + 產出交付 |
