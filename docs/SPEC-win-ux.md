# 規格：Windows 版 UX 整理（兩種風格、表格收合、圓角）

狀態：五個決定已確認（§8），待使用者確認規格後才實作。分支 `cloud/win-ux`。範圍只有 `python/` 的 Windows 版；Rust 與 mac 凍結（不刪、不維護、不改其 README 段落）。
本文件只描述做法，**尚未實作**。標「未驗證」的項目在 Linux 虛擬機無法確認，必須由使用者在 Windows 實機驗收。

## 0. 先釐清的事實（讀程式碼的結果）

1. **repo 裡沒有「表格縮小後收合成三顆圓盤」的實作。** 全部 94 個 commit、唯一分支 `develop` 都找不到 collapse／compact 相關程式；`MIN_TABLE_W/H = 380×280` 兩個平台共用，mac 與 Windows 走同一份 `usage_table.py`。
   所以使用者在 mac 上看到的收合行為，不是這個 repo 的任何版本（可能來自另一個分支、舊版、Rust 版，或記憶有誤）。本規格的收合行為是**依使用者描述重新設計**，不是「對齊現有 mac 版」。細節見 §3，有待使用者確認。
2. **沒有切換風格的快捷鍵。** 全域快捷鍵只有 `Alt+C`（顯示／隱藏）與 `Alt+Shift+C`（穿透）。風格只能從右鍵／系統匣選單（`hud_menu.py` 的 `UI_MODES`）切換；`toggle_ui_mode()` 只剩測試與 `core/smoke_check.py` 在呼叫。因此「快捷鍵」這一項不需遷移，只要處理選單與 `toggle_ui_mode`。
3. 側邊欄固定窄欄（`sidebar_state="panel"`）用的是 `HUDWindow.sidebar_cards`，與卡片模式的 `HUDWindow.cards` 是**兩組各自獨立的 `ProviderCardWidget`**，共用同一份 `get_cards_stylesheet`。
4. Windows 的 Acrylic 只有 `vibrancy._apply_windows()`（`SetWindowCompositionAttribute`）；沒有任何圓角處理，窗口形狀是整個矩形。

## 1. 目標

1. 風格只剩兩種：**儀表表格**（`table`）、**側邊欄**（`sidebar`）。「傳統卡片」移除，其用途由側邊欄的固定窄欄承接。
2. 儀表表格的視窗可縮小，小到一定程度收合成一排三顆圓盤；放大後恢復完整表格。
3. 表格視窗在 Windows 上不露出方形模糊、拖拉縮放順手。
4. 其他 Windows 體驗問題只列出，不在本次動手（§7）。

## 2. 移除傳統卡片

### 2.1 行為

- `ui_mode` 合法值：`"table"`、`"sidebar"`。
- 選單「介面」只剩兩個風格；「佈局（橫／直）」子選單移除。橫／直的需求由側邊欄貼上下邊（橫條）或左右邊（直條）承接，已存在。
- 標題列的 `⇄` 橫直切換按鈕移除（表格模式本來就隱藏它）。
- `toggle_ui_mode()` 改為 table ⇄ sidebar（仍只供測試／smoke 使用，不新增快捷鍵）。

### 2.2 設定遷移（載入時自動、冪等、不丟其他設定）

`ConfigManager.load()` 之後做一次正規化；`ui_mode` 為 `"cards"` 或任何不認得的值時：

| 舊設定 | 新設定 |
|---|---|
| `ui_mode = "cards"` | `ui_mode = "sidebar"`、`sidebar_state = "panel"` |
| `layout_mode = "horizontal"` | `sidebar_orientation = "horizontal"` |
| `layout_mode = "vertical"` | `sidebar_orientation = "vertical"` |
| `window_x`／`window_y`（卡片的自由位置） | `sidebar_edge = "none"`、`sidebar_x`／`sidebar_y` = 原值（保留使用者放的位置，之後仍可拖到邊緣吸附） |
| 不認得的 `ui_mode`（手改壞掉） | `"sidebar"`＋`sidebar_state = "panel"`（預設值） |

- 舊鍵（`layout_mode`、`vertical_*`、`horizontal_*`）保留在檔案裡不刪，只是不再讀（降版時不會壞）；`DEFAULT_CONFIG` 移除這些鍵與 `"cards"` 註解。
- `DEFAULT_CONFIG["ui_mode"]` 新預設值：`"sidebar"`，並將 `sidebar_state` 預設改為 `"panel"`（固定窄欄）。設定檔壞掉時同樣回到此預設（§8-2）。
- 位置若落在螢幕外，沿用側邊欄既有的拉回邏輯。

### 2.3 受影響檔案

| 檔案 | 變更 |
|---|---|
| `python/ui/hud_window.py` | 刪 `self.cards`、`cards_container`、`cards_layout`、`layout_toggle_btn`、`_apply_cards_layout*`、`_fit_cards_height`、`toggle_layout_mode`、`MIN/DEF_HORIZ_*`、`MIN/DEF_VERT_*`，以及各處 `ui_mode == "cards"` 分支；`_apply_theme`／`_persist_geometry`／`_reset_geometry` 只剩 table、sidebar 兩支；`add_theme_menus`（疑似死碼，只剩測試用）一併移除並改測試 |
| `python/ui/hud_menu.py` | `UI_MODES` 去掉 cards、`LAYOUTS` 與其子選單移除 |
| `python/core/config_manager.py` | 預設值、遷移函式 |
| `python/core/smoke_check.py` | `hud.cards['agy']` → `hud.sidebar_cards['agy']`；移除 `toggle_layout_mode()` |
| `python/ui/provider_card.py`、`styles.py` | 保留（窄欄與飛出面板仍用）；`get_cards_stylesheet` 名稱可維持，避免無謂改名 |
| `python/tests/test_ui.py` | 約 14 個以 `ui_mode='cards'` 為前提的測試：能對應到窄欄的改測 `sidebar_cards`（外觀、離線、AGY 高度、列拖曳排序、`provider_order`）；只屬於卡片橫直切換的刪除（`layout_sizes`、`toggle_layout_mode`） |
| `docs/SPEC-sidebar-dock.md`、`python/PROJECT_SPEC.md`、`README.md` | 只改 Windows／風格相關描述：三種風格→兩種、移除標題列 ⇄、窄欄成為卡片的承接者、README 的 Rust/Linux/mac 段落不動 |

### 2.4 注意：窄欄高度要能隨資料長高

卡片模式有 `_fit_cards_height()`（AGY 五條時長高）。窄欄是否等價尚需確認：離屏測試顯示資料到達後視窗高度會變（523 px），與 `preferred_size()`（517 px）差 6 px，原因未查。實作時以測試鎖定「AGY 五條不被裁切」，必要時讓 `_publish` 在窄欄模式呼叫 `_dock_sidebar()`。

## 3. 儀表表格收合

### 3.1 行為（依使用者描述；是否正確見 §8 問題 1）

- 兩個狀態：**完整**（現有表格）與**收合**（一排圓盤，每個服務一顆，順序跟 `provider_order`）。
- 切換由**視窗大小**決定，沒有按鈕：寬 < 380 或高 < 280（目前的最小尺寸，改為「收合門檻」）時收合；寬 ≥ 380+16 且高 ≥ 280+16 時還原（16 px 遲滯，避免在門檻上來回閃）。
- 收合時隱藏：標題列（狀態點、標題、時間）、列標籤、圖例、重設時間、倒數、週百分比、徽章。只留 `UsageDial`（內圈 5H、外環一週、平均進度虛線與斜紋照舊），圓盤中央的百分比照舊顯示。
- 收合時所有細節放進 tooltip（沿用 `UsageDial.setToolTip` 的內容，再加服務名稱、兩個窗口的重設時間與剩餘），因為畫面上看不到。離線／過期狀態：圓盤灰化、顯示 `ERR`（沿用 `muted`）；STALE 以 tooltip 呈現。
- 收合視窗最小尺寸：每顆圓盤最小 48 px（完整模式現為 84 px），三顆加間距與邊距約 **170×80**；最大不限（放大到門檻前，圓盤等比放大）。
- 左右拖拉時圓盤保持圓形、置中；比例不合時留白而非變形。
- 各狀態的尺寸分開記住：`table_width/height`（完整）與新增 `table_compact_width/height`（收合）。從收合拉回完整時，視窗自然跟著使用者的拖拉變大，不跳回上次完整尺寸。重新啟動時依儲存尺寸所屬狀態還原。
- 配色（色階／雙色）、外觀（淺／深）、顏色可讀性維持原樣，兩種狀態共用同一份色彩邏輯。

### 3.2 做法

- `UsageTable` 增加 `set_compact(bool)`，內部以兩個子容器（完整 grid、收合 row）切換；收合 row 擁有**獨立的** `UsageDial` 實例（一個 widget 不能同時掛在兩處），`ProviderColumn.update_countdown()`／`update_metrics()` 同時餵給兩顆。
- `HUDWindow` 在 `resizeEvent`（table 模式）依 §3.1 的門檻與遲滯呼叫 `set_compact`，並調整標題列可見性、`setMinimumSize`（完整：不再硬卡 380×280，收合：170×80）。
- 門檻、遲滯、收合最小尺寸抽成純函式（如 `ui/table_layout.py: table_state(width, height, was_compact) -> bool`），不依賴 Qt，便於單元測試。
- `_calc_edge` 的 8 px 拖拉邊緣在 80 px 高的視窗上仍可用；收合時整個視窗可拖曳移動（已有 `startSystemMove`）。

### 3.3 受影響檔案

`python/ui/usage_table.py`、`python/ui/hud_window.py`、新增 `python/ui/table_layout.py`、`python/core/config_manager.py`（兩個新鍵）、`python/tests/test_ui.py`＋新增 `python/tests/test_table_layout.py`。

## 4. 表格視窗：圓角與 Acrylic（Windows）

### 4.1 問題

Acrylic 的模糊區域是整個視窗矩形，Qt 只用 QSS `border-radius` 把面板畫成圓角，四角之外仍有模糊底（mac 版用 `layer.cornerRadius + masksToBounds` 真正裁切，所以沒有這問題）。**目前推測會露出方形模糊，未驗證。**

### 4.2 可行做法比較

| 做法 | 優點 | 缺點／風險 |
|---|---|---|
| A. 視窗遮罩（`setMask`／`SetWindowRgn` 圓角區域），Win10／11 都可用 | 模糊與內容一起被裁成圓角；邏輯可在 Linux 離屏單元測試（區域四角不含、中心含） | 邊緣沒有反鋸齒，12 px 半徑下會有輕微鋸齒（可用 1 px 邊框 QSS 蓋掉大部分）；縮放時要即時更新遮罩；遮罩與 Acrylic、`WA_TranslucentBackground` 的實際互動**未驗證** |
| B. Win11 DWM 圓角：`DwmSetWindowAttribute(DWMWA_WINDOW_CORNER_PREFERENCE=33, DWMWCP_ROUND)` | 系統原生、反鋸齒 | 只有 Win11；半徑固定約 8 px，與 QSS 12 px 不符；對 `WS_EX_LAYERED`／無邊框 Qt 視窗是否生效**未驗證**；Win10 完全無效，仍需 A 作為後備 |
| C. 放棄 Acrylic，用不透明（或半透明但不模糊）面板 | 圓角由 Qt 反鋸齒繪製，最穩；拖拉最順 | 失去毛玻璃質感 |

**建議：做法 A 為主（Win10／11 一致、可部分測試），並加設定 `table_blur`（預設 `true`）讓使用者一鍵退回做法 C。** 做法 B 暫不做，等實機結果再決定。

### 4.3 拖拉縮放手感

- 已知問題（Windows 社群回報、本專案**未驗證**）：Acrylic 視窗在移動／縮放時容易卡頓。做法：滑鼠按下開始移動／縮放時，把 accent 從 Acrylic（state 4）暫時換成純色漸層（state 2／無模糊），放開後還原。純粹 Windows 專屬，只能實機驗證。
- 縮放以 `startSystemResize`（系統處理）維持；圓角遮罩在 `resizeEvent` 更新，需確認縮放過程不閃爍（**未驗證**）。

### 4.4 受影響檔案

`python/ui/vibrancy.py`（遮罩與暫停模糊的 Windows 函式，可測的幾何計算獨立成純函式）、`python/ui/hud_window.py`、`python/core/config_manager.py`（`table_blur`）、`python/ui/hud_menu.py`（外觀子選單新增「毛玻璃模糊」開關）、測試。

## 5. 測試策略（TDD，每個功能一個 commit）

順序與 commit 切分：

1. 設定遷移（`cards` → `sidebar`/`panel`、橫直與位置的對應、冪等、壞值）。
2. 移除卡片：選單只有兩種風格、`toggle_ui_mode` table⇄sidebar、窄欄承接原卡片測試（外觀、離線、AGY 高度、排序）。
3. 表格收合：`table_state` 純函式（門檻、遲滯、邊界）；`UsageTable.set_compact`（隱藏／顯示的 widget、dial 數量與順序、離線灰化）；`HUDWindow` 在 resize 後切換狀態與最小尺寸；兩組尺寸分開儲存與還原。
4. 圓角遮罩幾何（純函式）與 `table_blur` 開關；視窗 resize 後遮罩尺寸同步。
5. 文件更新。

每一步先寫失敗測試，確認紅燈再實作。完整測試指令：`QT_QPA_PLATFORM=offscreen python -B -m unittest discover -s tests`。`test_packaging` 在沒有 PyInstaller 的環境會失敗，可忽略（非本次造成）。

## 6. 驗證方式與無法在 Linux 驗證的項目

可在 Linux 驗證（離屏渲染）：
- 單元測試全數通過。
- 以 `QT_QPA_PLATFORM=offscreen` 對表格在 600×400、380×280、375×275（門檻下）、240×100、170×80 截圖，確認完整／收合的版面、圓盤不變形、深淺兩種外觀。
- 窄欄（直／橫）的截圖，確認承接了卡片的用途且 AGY 五條不被裁切。
- 圓角遮罩區域的幾何（四角不含、邊中點含）。

**無法在 Linux 驗證（一律標「未驗證」，需使用者 Windows 實機）：**
- Acrylic 模糊是否真的被圓角遮罩裁掉；遮罩邊緣鋸齒是否可接受；Win10 與 Win11 的差異。
- 拖拉、縮放的手感與卡頓、移動時暫停模糊的效果。
- 125%／150%／175% DPI 縮放下的收合門檻與圓盤清晰度；多螢幕與混合 DPI。
- 穿透模式與遮罩、模糊同時開啟的行為。
- 開機啟動、系統匣選單、從舊設定檔升級後的實際外觀。

建議的實機驗收清單會在實作完成後附在 PR 說明，逐項請使用者勾選。

## 7. 其他 Windows 體驗問題（只列出，本次不處理）

1. 選單殘留 emoji：`hud_menu.py:116` 的「⏱️」，以及 `hud_window.py` 的 `👻`（穿透標示與通知標題）、`⚠️`。與先前「選單改純文字」的決定不一致。
2. `trim_memory()`（`EmptyWorkingSet`）在啟動 2.5 秒後與每次隱藏後 150 ms 執行；顯示視窗時是否因重新換頁而短暫卡頓，**未驗證**。
3. Acrylic 疊色：QSS 面板（alpha 0.50）加 Acrylic tint（`0x99…`）兩層疊在一起，淺色主題在亮背景上可能偏灰、文字對比偏低，**未驗證**。
4. Windows 的 `setWindowOpacity`（`WS_EX_LAYERED`）與 Acrylic 並用時，透明度滑桿對模糊本身是否有效，**未驗證**。
5. 側邊欄、飛出面板與卡片的字型寫死 `Segoe UI`／`Consolas`（Windows 沒問題，但不是平台中立；mac 已凍結，僅記錄）。
6. 全域快捷鍵註冊失敗時只在系統匣提示，沒有備援入口（例如選單內顯示目前快捷鍵與重新註冊）。

## 8. 已確認的決定

1. 表格收合：照 §3 的設計（依視窗大小自動收合，不加按鈕）。
2. 預設風格：側邊欄固定窄欄（新安裝與設定壞掉時）。
3. 從 `cards` 遷移：保留原自由位置（`sidebar_edge = "none"`），之後可拖到邊緣吸附。
4. 圓角：做法 A（視窗遮罩）加 `table_blur` 開關；不做 DWM 圓角（做法 B）。
5. 收合門檻：沿用 380×280，不開放調整（不新增門檻設定鍵；仍新增 `table_compact_width/height` 記錄收合狀態的視窗尺寸）。
