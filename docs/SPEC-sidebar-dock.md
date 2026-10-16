# 規格：側邊欄模式（AppBar 停靠）

狀態：草案，待使用者審閱。審過才派工。

## 1. 目標

HUD 現在的視窗又寬又長。新增第三種 UI 模式 `sidebar`：像 Windows 工作列／IntelliJ 右側工具列一樣貼在螢幕右緣，
窄、直式、不擋工作，並且真的向系統保留空間（其他視窗最大化時不會被蓋住）。現有 `cards`、`table` 兩種模式保留不動。

## 2. 已定決策（使用者 2026-10-16 確認）

| 項目 | 決策 |
|---|---|
| 停靠 | 真正的 Windows AppBar（`SHAppBarMessage`），靠右（`ABE_RIGHT`） |
| 形式 | 兩種狀態都做、可切換：**細條**（預設）與**固定窄欄** |
| 舊模式 | 保留 `cards`／`table`；`sidebar` 為第三種，從選單與設定切換，是否設為預設另議 |
| 視覺 | 實作者先讀 `D:\playground\.agents\skills\ui-ux-pro-max\SKILL.md`，依其方法先定 design tokens 再寫畫面 |

## 3. 兩種狀態

**細條（strip，約 56 px 寬）**：像 IntelliJ 右側工具列。垂直排列三個服務按鈕（Claude／Antigravity／Codex），每個按鈕＝向量繪製的小圖示＋
該服務目前**最吃緊那一條**的已用％（顏色用 `get_progress_color`）＋細環形量表。最上方是展開／收合與設定的小按鈕。
- 滑鼠停留 300 ms 或點擊按鈕：向左彈出**飛出面板**（flyout，無邊框、貼在細條左邊、高度貼齊按鈕），內容是該服務的完整卡片
  （`ProviderCardWidget`，agy 含五條進度條）。滑鼠離開 600 ms 或按 Esc 收起；點按鈕可釘住（再點取消）。
- 保留的螢幕空間＝細條寬度；飛出面板是浮層，**不**改變保留空間。

**固定窄欄（panel，約 220 px 寬）**：三個服務卡片直式堆疊，一直顯示，無飛出面板。保留的螢幕空間＝窄欄寬度。

切換：細條頂端按鈕、右鍵選單「側邊欄 → 細條／窄欄」、設定鍵 `sidebar_state`（`"strip"`|`"panel"`）。切換時重新向系統申請空間。

## 4. AppBar 行為（Windows）

- 啟用 sidebar 模式時：`ABM_NEW` → `ABM_QUERYPOS`／`ABM_SETPOS`（`ABE_RIGHT`，主螢幕，寬＝目前狀態寬度，高＝螢幕工作區高度）→ 移動視窗
- 狀態切換、DPI 或螢幕解析度改變（`ABN_POSCHANGED`）：重算並重新 `ABM_SETPOS`
- 離開模式、關閉程式、例外結束：一定 `ABM_REMOVE`（`closeEvent` 與 `atexit` 都要）；程式崩潰後系統會自動清除，但仍要避免殘留
- 有全螢幕程式（`ABN_FULLSCREENAPP`）：降到最底層／隱藏，結束後恢復
- 視窗屬性：無邊框、工具視窗（不出現在工作列與 Alt+Tab）、不搶焦點；`click_through`、`locked`、拖曳縮放在此模式停用（位置固定）
- 非 Windows 或 AppBar 註冊失敗：退化為貼齊右緣的置頂浮動視窗（不保留空間），並在診斷記錄一行原因
- 多螢幕：貼主螢幕右緣；設定鍵 `sidebar_screen`（預設 `"primary"`）預留，這版只支援主螢幕

## 5. 視覺與互動要求

- 先產出 design tokens（間距、圓角、字級、色彩），亮／暗兩套，沿用 `ui/styles.py` 現有主題與 `appearance` 設定，不另開一套
- 圖示用 `QPainter` 向量繪製（或 SVG），不用 emoji；每個服務一個易辨識的簡化圖形，顏色沿用 `PROVIDER_THEMES`
- 可點區域至少 40×40 px；有 hover／pressed／focus 狀態；全部可用鍵盤操作（Tab 切換、Enter 展開、Esc 收起）
- 文字對比度符合 WCAG AA；狀態不只靠顏色（同時有數字）
- 動畫：飛出面板滑入 150～200 ms，尊重系統「減少動畫」；不得造成卡頓
- 細條在所有服務錯誤時顯示 `OFFLINE` 灰態，STALE 時有琥珀色點，語意與現有卡片一致

## 6. 檔案與切分

| 任務 | 檔案 | 說明 | 建議 | 依賴 |
|---|---|---|---|---|
| T0 | （進行中）agy 五條進度條 | `provider_card.py` 等 | Codex，已派 | — |
| T1 | `python/system/appbar.py`＋`tests/test_appbar.py` | ctypes 包裝 `SHAppBarMessage`；純函式 `compute_edge_rect(screen_rect, work_area, width)` 與狀態機可單元測試；`register()`／`set_pos()`／`remove()`；非 Windows 為 no-op；不碰 Qt widget | Codex | — |
| T2 | `python/ui/sidebar.py`＋`tests/test_sidebar.py` | 細條按鈕（圖示＋%＋環）、飛出面板、窄欄堆疊；只吃 `UsageMetrics`，不查網路；用 `QT_QPA_PLATFORM=offscreen` 測 | Codex（先讀 ui-ux-pro-max） | T0 |
| T3 | `python/ui/hud_window.py`、`core/config_manager.py`（預設鍵）、`ui/tray_icon.py`（選單）、`tests/test_ui.py` | 新增 `ui_mode="sidebar"` 整合、AppBar 生命週期、選單切換與設定持久化 | 大哥（跨檔整合） | T1、T2 |
| T4 | `docs/PROVIDERS.md` 以外的 `README.md`、`PROJECT_SPEC.md` | 使用方式與責任界線更新 | agy | T3 |

驗收：大哥跑全部測試＋離屏渲染截圖（細條、飛出、窄欄、亮／暗各一張）確認版面；AppBar 保留空間的實機行為請使用者在 Windows 上確認（我無法目視或用其他視窗驗證）。

## 7. 設定鍵（新增）

`ui_mode`: 新增值 `"sidebar"`；`sidebar_state`: `"strip"`（預設）| `"panel"`；`sidebar_strip_width`: 56；`sidebar_panel_width`: 220。

## 8. 不在範圍

左／上／下邊停靠、自動隱藏、多螢幕選擇、自訂順序、macOS 的原生停靠（退化為浮動視窗）、exe 內建的 agy statusLine 啟用流程。

## 9. 風險

- AppBar 是 Windows 老 API，崩潰或強制結束可能殘留保留空間（需啟動時清理的保險：啟動時對同視窗 class 先 `ABM_REMOVE` 一次）
- 高 DPI 與多螢幕的座標換算（物理像素 vs Qt 邏輯像素）容易錯：T1 的純函式要涵蓋縮放 125%／150%
- 離屏截圖無法證明 AppBar 的真實保留空間，需使用者實機驗證
