# 規格：側邊欄模式（貼右緣的懸浮條）

狀態：已實作（細條／窄欄／飛出面板、貼右緣、底部邊距記憶、選單切換）。實機貼齊與穿透行為待使用者在 Windows 上確認。

## 1. 目標

HUD 現在的視窗又寬又長。新增第三種 UI 模式 `sidebar`：像 IntelliJ 右側工具列一樣貼在螢幕右緣，窄、直式。**沿用現有 HUD 的懸浮特性**：置頂、可半透明、可滑鼠穿透、可蓋住其他視窗；
**不**向系統保留空間（不用 AppBar，其他視窗仍可占滿整個螢幕）。現有 `cards`、`table` 兩種模式保留不動。

## 2. 已定決策（使用者 2026-10-16 確認）

| 項目 | 決策 |
|---|---|
| 停靠 | 貼齊螢幕右緣的置頂懸浮視窗，**不使用 AppBar、不保留空間**（使用者明確不要其他視窗變窄）；透明度、滑鼠穿透、置頂沿用現有機制 |
| 形式 | 兩種狀態都做、可切換：**細條**（預設）與**固定窄欄** |
| 舊模式 | 保留 `cards`／`table`；`sidebar` 為第三種，從選單與設定切換，是否設為預設另議 |
| 視覺 | 實作者先讀 `D:\playground\.agents\skills\ui-ux-pro-max\SKILL.md`，依其方法先定 design tokens 再寫畫面 |

## 3. 兩種狀態

**細條（strip，寬 92 px）**：尺寸與擺放比照 IntelliJ 右側工具視窗列（使用者截圖：寬約 94 px、圖示在上方、下方留白）。
垂直排列三個服務按鈕（Claude／Antigravity／Codex），每個按鈕約 92×84 px，版型同 IDEA：**圖示在上、文字標籤在下**（`Claude`／`AGY`／`Codex`），
圖示外圍是細環形量表，標籤旁或下方帶該服務目前**最吃緊那一條**的已用％（顏色用 `get_progress_color`）。最上方是展開／收合與設定的小按鈕。
**高度由內容決定，不是整個螢幕高**（約 3×84＋頂端按鈕）。
- 滑鼠停留 300 ms 或點擊按鈕：向左彈出**飛出面板**（flyout，無邊框、貼在細條左邊、高度貼齊按鈕），內容是該服務的完整卡片
  （`ProviderCardWidget`，agy 含五條進度條）。滑鼠離開 600 ms 或按 Esc 收起；點按鈕可釘住（再點取消）。
- 飛出面板是浮層，收起後不留痕跡。

**固定窄欄（panel，寬 220 px，高度由內容決定）**：三個服務卡片直式堆疊，一直顯示，無飛出面板。

切換：細條頂端按鈕、右鍵選單「側邊欄 → 細條／窄欄」、設定鍵 `sidebar_state`（`"strip"`|`"panel"`）。切換時視窗寬度改變，右緣位置保持貼齊。

## 4. 停靠與視窗行為

- 視窗：無邊框、置頂、工具視窗（不出現在工作列與 Alt+Tab）、不搶焦點；**沿用現有** `opacity`（透明度）、`click_through`（滑鼠穿透）、`always_on_top`、`locked` 設定與選單，不新增也不移除
- 貼齊：啟用模式時，視窗右緣對齊所在螢幕工作區右緣（`QScreen.availableGeometry()`），**高度由內容決定**。
  預設垂直位置：**貼工作區底部（靠右下角，留 12 px 邊距）**，也就是落在 IntelliJ 右側工具列圖示下方那塊空白；使用者可垂直拖曳，位置存 `sidebar_y`（相對工作區底部的距離，避免換解析度後跑出螢幕）
- 拖曳：允許在垂直方向移動（左右固定貼右緣）；若使用者拖到靠近其他位置，放開時吸附回右緣（吸附距離 24 px）；`locked` 時不可拖
- 狀態切換（細條↔窄欄）：右緣不動、向左伸縮
- 螢幕解析度／DPI／螢幕插拔改變：重新貼齊；視窗若落在螢幕外要拉回（沿用 `_ensure_within_screen`）
- 全螢幕程式：沿用現有置頂行為，不另做處理
- 不使用任何 Windows 專屬的保留空間 API，所以 macOS／Linux 行為相同
- 多螢幕：貼目前所在螢幕的右緣；這版不提供選擇螢幕的設定

## 5. 視覺與互動要求

- 先產出 design tokens（間距、圓角、字級、色彩），亮／暗兩套，沿用 `ui/styles.py` 現有主題與 `appearance` 設定，不另開一套
- 圖示用 `QPainter` 向量繪製（或 SVG），不用 emoji；每個服務一個易辨識的簡化圖形，顏色沿用 `PROVIDER_THEMES`
- 可點區域至少 40×40 px（細條按鈕本身約 92×84）；有 hover／pressed／focus 狀態；全部可用鍵盤操作（Tab 切換、Enter 展開、Esc 收起）
- 文字對比度符合 WCAG AA；狀態不只靠顏色（同時有數字）
- 動畫：飛出面板滑入 150～200 ms，尊重系統「減少動畫」；不得造成卡頓
- 細條在所有服務錯誤時顯示 `OFFLINE` 灰態，STALE 時有琥珀色點，語意與現有卡片一致

## 6. 檔案與切分

| 任務 | 檔案 | 說明 | 建議 | 依賴 |
|---|---|---|---|---|
| T0 | （進行中）agy 五條進度條 | `provider_card.py` 等 | Codex，已派 | — |
| T1 | `python/ui/dock_geometry.py`＋`tests/test_dock_geometry.py` | 純函式：`dock_rect(work_area, width, height, y)`（貼右緣）、`snap_to_edge(rect, work_area, threshold)`、螢幕外拉回；不碰 Qt widget，可單元測試（含 125%／150% 縮放與多螢幕座標） | agy（葉節點純函式） | — |
| T2 | `python/ui/sidebar.py`＋`tests/test_sidebar.py` | 細條按鈕（圖示＋%＋環）、飛出面板、窄欄堆疊；只吃 `UsageMetrics`，不查網路；用 `QT_QPA_PLATFORM=offscreen` 測 | Codex（先讀 ui-ux-pro-max） | T0 |
| T3 | `python/ui/hud_window.py`、`core/config_manager.py`（預設鍵）、`ui/tray_icon.py`（選單）、`tests/test_ui.py` | 新增 `ui_mode="sidebar"` 整合、貼右緣與吸附、選單切換與設定持久化 | 大哥（跨檔整合） | T1、T2 |
| T4 | `docs/PROVIDERS.md` 以外的 `README.md`、`PROJECT_SPEC.md` | 使用方式與責任界線更新 | agy | T3 |

驗收：大哥跑全部測試＋離屏渲染截圖（細條、飛出、窄欄、亮／暗各一張）確認版面；貼齊、穿透與透明度的實機行為請使用者在 Windows 上確認。

## 7. 設定鍵（新增）

`ui_mode`: 新增值 `"sidebar"`；`sidebar_state`: `"strip"`（預設）| `"panel"`；`sidebar_strip_width`: 92；`sidebar_panel_width`: 220；`sidebar_bottom_offset`: 12（貼底邊距，拖曳後更新）。

## 8. 不在範圍

向系統保留空間（AppBar）、左／上／下邊停靠、自動隱藏、多螢幕選擇、自訂順序、exe 內建的 agy statusLine 啟用流程。

## 9. 風險

- 高 DPI 與多螢幕的座標換算（物理像素 vs Qt 邏輯像素）容易錯：T1 的純函式要涵蓋縮放 125%／150%
- 離屏截圖無法證明實機的貼齊、穿透與透明度，需使用者實機驗證
- 細條會蓋住其他視窗右緣約 92 px（這是使用者接受的懸浮行為），可用現有的滑鼠穿透與透明度緩解
