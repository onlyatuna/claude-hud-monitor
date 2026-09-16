<div align="center">

# ⚡ AI HUD Monitor

**極簡、原生、跨平台的 AI 開發者效能覆蓋儀表板 (Desktop Performance HUD)**  
*專為重度多 Agent 開發者打造，同時監控 Claude Code、Google Antigravity (AGY) 與 OpenAI Codex 即時額度水位！*

[![License: AGPL v3](https://img.shields.io/badge/License-AGPL_v3-blue.svg)](https://www.gnu.org/licenses/agpl-3.0)
[![Python Version](https://img.shields.io/badge/python-3.10+-brightgreen.svg)](https://www.python.org/)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS-lightgrey.svg)]()
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](CONTRIBUTING.md)

</div>

---

## 🎯 核心亮點

* 📊 **三合一同步並排儀表 (Simultaneous 3-in-1 Dashboard)**：
  * 同時在螢幕上呈現 **Claude Code**、**Antigravity (AGY)**、**OpenAI Codex** 的即時配額水位。
  * 獨立主題色彩、即時連線燈號、5H Session 滾動進度條、7D / AI Credits 週期指標與秒級倒數計時。
  * 隨時掌握誰的水位充足、誰面臨冷卻，輕鬆調度多 Agent 任務負載均衡。
* 📐 **直式 / 橫向雙佈局隨心切換**：
  * **💻 橫向三欄並排 (Horizontal Triple)**：左右展開三欄，高度僅約 145px，低調緊湊，適合置於螢幕頂端或工作列上方。
  * **📱 直立三層堆疊 (Vertical Stack)**：直式卡片上下堆疊，寬度僅約 280px，適合吸附於螢幕側邊。
  * 點擊標題列右上角 **`⇄`** 按鈕或滑鼠右鍵即可瞬間切換。
* 🪟 **原生八向邊框自由縮放**：無邊框設計下，滑鼠移至視窗 4 個邊緣或 4 個角落皆會自動切換系統級雙箭頭指標，隨意拉伸調整大小（直式、橫式分別獨立記憶尺寸）。
* 👻 **滑鼠點擊穿透 (Click-Through / 幽靈模式)**：
  * 搭配透明度調高後開啟穿透模式，所有滑鼠點擊、滾輪、框選將**完全穿透 HUD** 直接操作背後的 VS Code、瀏覽器或終端機！
  * 快速鍵 **`Alt + Shift + C`** 或右下角系統匣圖示可隨時解除/開啟穿透。
* ⚡ **全域快捷鍵隨手喚出**：
  * **`Alt + C`**：隨時「顯示 / 隱藏」HUD（如同 NVIDIA `Alt + R`）。
  * **`Alt + Shift + C`**：一鍵切換「滑鼠穿透模式」。
* 🔑 **無感認證**：自動無縫讀取本機已登入憑證（`~/.claude/`、`~/.gemini/`、`~/.codex/`），免手動輸入 API Key。
* ⏱️ **秒級重設倒數**：配額重設倒數計時即時跳動（`Resets in 2h 31m 15s`）。
* 🚀 **開機自動啟動**：跨平台支援 Windows 註冊表與 macOS LaunchAgents 服務開關。
* 📦 **免安裝 Python！雙平台開箱即用**：
  * **Windows 用戶**：直接執行單一免安裝 `dist\ClaudeHUD.exe`。
  * **Mac 用戶**：透過 GitHub Actions 自動雲端編譯出 `ClaudeHUD.app`，下載即用。

---

## 🎮 操作速查表

| 快捷操作 | 功能說明 |
| :--- | :--- |
| **`Alt + C`** | 全域一鍵「顯示 / 隱藏」HUD |
| **`Alt + Shift + C`** | 全域一鍵開關「滑鼠點擊穿透模式 (Ghost Mode)」 |
| **標題列 `⇄` 按鈕** | 一鍵切換「橫向三欄並排」與「直立三層堆疊」佈局 |
| **邊框 / 角落滑鼠拖曳** | 八個方向均可自由縮放視窗大小（系統自動儲存尺寸） |
| **滑鼠空白處拖曳** | 任意拖曳移動視窗（穿透模式下除外） |
| **滑鼠雙擊** | 立即手動強制同時刷新三個 AI 的最新配額數據 |
| **滑鼠右鍵點擊 HUD** | 開啟功能選單：<br>• 🔄 立即重新整理所有 AI<br>• 📐 顯示佈局 (橫向三欄 / 直立堆疊)<br>• 👻 滑鼠點擊穿透模式<br>• 📌 視窗永遠置頂<br>• 🔒 鎖定視窗位置 (防誤觸)<br>• 🌗 視窗透明度 (30% ~ 100%)<br>• ⏱️ 更新頻率 (30s ~ 300s)<br>• 🚀 開機自動啟動<br>• 📐 重設為預設尺寸與位置<br>• 👁️ 隱藏 HUD<br>• ❌ 結束程式 |
| **系統匣 (右下角圖示)** | 單擊切換 HUD 顯隱；右鍵可直接切換佈局或解除穿透模式 |

---

## 🌿 分支維護機制 (Branch Maintenance Strategy)

本專案採行嚴謹的 **Git Flow / GitHub Flow** 分支管理標準，確保代碼穩定性與持續交付：

```text
main (受保護主分支，僅存放發布版本，帶有 vX.Y.Z Tag)
  ▲
  │ (Release PR / Hotfix PR)
  ├─────────────────────────────────────────────┐
  │                                             │
develop (日常開發整合分支)               hotfix/vX.Y.Z (緊急熱修復)
  ▲                                             ▲
  │ (Feature PR / Bugfix PR)                    │
  ├──────────────────────┬──────────────────────┤
feat/feature-name      fix/bug-name           main
```

1. **`main` 分支（正式發布分支）**：
   * 嚴格受保護（Protected Branch），禁止直接 Push。
   * 僅接受來自 `develop` 的 Release PR 或緊急 `hotfix` PR。
   * 每次發布推送 `v*` Tag，會自動觸發 GitHub Actions 雲端編譯出 Windows `.exe` 與 macOS `.app` 發布至 GitHub Releases。
2. **`develop` 分支（開發整合分支）**：
   * 所有新功能開發與一般問題修復皆合併於此分支進行整合測試。
3. **主題分支（Topic Branches）**：
   * `feat/<feature-name>`：新功能或新 Provider 串接。
   * `fix/<bug-name>`：一般問題修復。
   * `hotfix/<version>`：線上版本緊急熱修復。

詳情請參閱 [CONTRIBUTING.md](CONTRIBUTING.md)。

---

## 📜 開源授權協議 (License: AGPL-3.0)

本專案基於 **[GNU Affero General Public License v3.0 (AGPL-3.0)](LICENSE)** 授權開源。

### 核心授權條款：
* **自由使用與分發**：任何人皆可免費下載、執行、分享本軟體。
* **強傳染性 Copyleft**：任何對本軟體的修改、衍生作品，或將其作為網路服務運行的衍生版本，**必須以相同的 AGPL-3.0 授權將完整原始碼公開**。
* **反商業封閉**：有效防止第三方將本開源社群成果包裝為封閉式商業付費服務而不回饋社群。

---

## 🤝 參與貢獻 (Contributing)

歡迎提交 Issue 與 Pull Request！
* 🐛 回報問題：請使用 [Bug Report 模板](.github/ISSUE_TEMPLATE/bug_report.yml)
* 💡 提出建議：請使用 [Feature Request 模板](.github/ISSUE_TEMPLATE/feature_request.yml)
* 📖 開發與 PR 規範：請詳閱 [CONTRIBUTING.md](CONTRIBUTING.md) 與 [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)
