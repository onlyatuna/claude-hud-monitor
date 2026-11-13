# 架構與維護規格

## 執行流程

main.py 建立 ConfigManager、HUDWindow、HUDTrayIcon 與 GlobalHotkeyManager。
HUDWindow 組合視窗與卡片；RefreshController 獨立管理查詢生命週期。

```text
QTimer / 手動刷新 / 休眠恢復
              ↓
       RefreshController
       每服務最多一個 worker
              ↓
   Claude / AGY / Codex Provider
              ↓
        UsageMetrics
              ↓ Python queue → Qt timer
       SidebarWidget（磚與固定窄欄內的 ProviderCardWidget）/ UsageTable (Modern Table)
```

## 責任界線

- core/providers/：傳輸、認證讀取、解析；不碰 Qt widget、不修改憑證。
- core/providers/base.py：共用資料契約、百分比驗證、錯誤分類、倒數。
- core/refresh_controller.py：請求序號、退避、最後成功資料與排程；可變狀態由 Qt 主執行緒更新。
- ui/hud_window.py：視窗、選單、穿透、表格／側邊欄雙風格切換（傳統卡片已移除，舊設定由 core/config_manager.py 的 migrate_style 遷移到側邊欄固定窄欄）、表格收合、圓角遮罩與休眠偵測。
- ui/provider_card.py：渲染經典卡片資料，不查網路。
- ui/usage_table.py：渲染現代表格與圓盤儀表，不查網路；平均進度虛線與超出段斜紋只表達速度，不改變顏色。視窗縮到 380×280 以下時收合成每服務一顆圓盤（每個欄位另有一顆精簡圓盤，tooltip 帶完整細節）。
- ui/table_layout.py：表格收合的門檻、遲滯與收合最小尺寸（純函式，有單元測試）。
- ui/round_mask.py：圓角視窗遮罩的幾何（純函式）；Windows 的 Acrylic 模糊整個矩形，需裁掉四角。
- ui/dock_geometry.py：側邊欄吸附／貼邊的純函式；`plan_sidebar_snap`、`sidebar_dock_target` 是「放開後會停在哪」的唯一計算，真正貼邊與預覽框共用，兩者不可能不一致。
- ui/dock_preview.py：拖曳側邊欄靠近螢幕邊緣（24 px 內）時顯示的半透明預覽框（無邊框、滑鼠穿透、不搶焦點，無擁有者視窗並固定疊在被拖曳的 HUD 正下方；是否置頂跟隨 `always_on_top`）；顏色、透明度（填色 46／邊框 235，皆為 0–255）、邊框 2 px、圓角（側邊欄 radius × 縮放）與 Rust 版相同。鎖定、表格風格、邊角縮放時不顯示。
- 放開側邊欄後不是瞬間跳到定位，而是 140 ms 三次方減速滑到 `_dock_sidebar` 算出的矩形（位置與尺寸，含橫直轉向；`dock_geometry.SnapAnimation`，16 ms 一格）；最後一格就是原本的貼邊呼叫，結果與設定值不變。目標相同、視窗隱藏、系統關閉動畫時不做動畫；按滑鼠、拖曳、縮放、切換風格會取消並停在原處，鎖定或隱藏則直接貼齊。
- ui/styles.py：側邊欄窄欄／飛出面板用的卡片樣式（深色／淺色）與表格的毛玻璃淺色／深色主題樣式表。
- ui/vibrancy.py：macOS 原生毛玻璃背景（NSVisualEffectView）、Windows Acrylic（拖曳／縮放時可暫停模糊）；其他平台自動改用不透明面板。`table_blur` 設定可關閉毛玻璃。
- core/pace.py：倒數格式、視窗長度解析、平均進度與色階門檻（純函式，具完整單元測試）。
- core/config_manager.py：設定位置、批次更新、同目錄暫存檔與原子替換。
- core/diagnostics.py：限制大小的操作紀錄，禁止傳入憑證、原始回應與 CLI stderr。
- system/hotkey.py、core/autostart.py：平台整合。
- core/tray_quota.py：系統匣圖示要顯示哪一條額度的數字（純函式，有單元測試）。設定 `tray_quota`：`off`（預設，維持原本的靜態圖示）、`auto`（已用比例最高的一條，略過離線的服務）或額度鍵（`MetricRow.key` 加上 `<provider_id>:` 前綴，已含前綴者不重複，如 `claude:5h`、`claude:weekly_scoped:fable`、`codex:WINDOW 5H`、`agy:C/G 5H`）；`tray_quota_mode`：`used`（預設）或 `remaining`。數字為整數、不含 `%`、最多兩位（已用 100 顯示 99；剩餘 0 顯示 0）；`--` 無資料、`!` 離線、琥珀色為舊資料（stale）、灰色為超過 30 分鐘未更新；選中的額度消失時設定保留不動，圖示顯示灰色 `--`，選單顯示停用的「unavailable」。圖示由 ui/tray_icon.py 以 16/20/24/32 px 灰階抗鋸齒繪製（剩餘模式在數字下方加一條橫線），僅在顯示內容改變時重繪；提示文字第一行為「Claude FABLE 7D: 84% used」。選單：系統匣選單「系統匣數字 (Tray number)」（HUD 右鍵選單不含）。
- core/providers/codex_provider.py：Codex 額度視窗依 `limit_window_seconds` 命名（`WINDOW 5H`、`WINDOW 7D`、`WINDOW 30D`…）；只有一個視窗（如免費方案的 30 天視窗、`secondary_window` 為 null）時只顯示一列，第二列為空（卡片隱藏、系統匣選單不列出）；兩個視窗皆缺才算格式錯誤。
- core/accounts.py：額外 Claude 帳號與側邊欄項目的純邏輯（無 Qt）：`accounts` / `sidebar_tiles` 的驗證（上限 6、不可重複、不可為 default／auto）、實例 id（`claude@<slug>`）、顯示/隱藏與順序的解析、共用標籤函式 `account_label`（磚 8 字／卡片 13 字／選單 16 字）。`ClaudeAccountProvider` 把 Claude 查詢固定到一個 profile 並改寫額度鍵；`RefreshController.add_provider/remove_provider` 與 `HUDWindow.apply_accounts()` 即時套用（不重開），重新整理時間每次加 ±25% 抖動。

舊 core/anthropic_client.py 已移除，新增 Claude 功能只修改 ClaudeProvider。

## 資料契約

metric1_val、metric2_val 是 0–100 的已使用比例或 None。None 對應 --；0 為有效讀值。
兩個窗口均不可用時回傳 schema 錯誤；單窗口可用時允許部分顯示。
徽章不可填入未證實的方案或模型。沒有重設時間就顯示 --。
MetricRow（`extra_metrics` 的元素）含 `key`：穩定識別字，預設等於 `title`；固定主列的 key 在 `UsageMetrics.metric1_key` / `metric2_key`（空字串表示用標題，用 `main_key(1|2)` 讀取）。Claude 固定列為 `claude:5h`、`claude:7d`，模型別額度列為 `claude:<kind>:<模型名 slug>`（如 `claude:weekly_scoped:fable`），不得由陣列索引或 `model.id` 推導。`row_order` 仍以標題比對，重排時 key 跟著該列移動。
Claude 的 `limits[]`：非 `session` / `weekly_all` 且 percent 為 0–100 有限值的項目，各自成為一個 `extra_metrics` 列（API 順序、同 key 取第一個、不看 `is_active`）；`session` / `weekly_all` 只在頂層 `five_hour` / `seven_day` 缺漏時作為主列備援。`limits` 不是陣列、項目格式錯誤或 kind 未知都不得拋出例外，也不得丟棄有效的主列資料；未知 kind / group 每程序只記一次 log。
error_code 為診斷分類；error 是使用者可讀訊息，不能包含 token 或原始 payload。
stale 與 last_success 由協調層管理，最後成功資料不可假裝是最新值。

## 排程不變條件

worker 只持有 Python queue，不持有或從背景執行緒呼叫 Qt 物件；主執行緒每 25ms 取回結果。
每個 Provider 同時最多一個 worker。手動刷新／休眠恢復使舊結果失效，原查詢結束後只補一次新查詢。
不得以清除布林鎖或 join timeout 假裝取消 worker。關閉後忽略任何晚到結果。
各服務獨立排程，AGY 慢查詢不阻塞其他服務。AGY 只走 HTTP（不呼叫 agy 執行檔）；urllib timeout 為 I/O timeout，非整體截止時間。
OS 層呼叫若卡住，該服務等待原查詢退出，不無限產生 worker；這仍是傳輸層的限制。

## 設定與平台

打包設定不得寫入 PyInstaller 解壓目錄。原始碼模式保留專案設定；測試注入暫存路徑。
排版切換前保存舊尺寸，套用新尺寸時抑制中間事件寫入；移動／縮放延遲 250ms 合併寫入。
macOS 功能需實機驗證，不以平台模擬視為已驗收。支援狀態以 README 為準。

## 驗證

執行 `python -B -m unittest discover -s tests -v` 與 `python -B main.py --smoke-test`。介面改動另可用 `python -B main.py --snapshot 資料夾` 離屏輸出各風格與狀態的 PNG（core/snapshot.py；合成資料、暫存設定、不連網），改動前後各輸出一次即可對照。
測試使用合成資料，不使用真實帳號或開機啟動設定。
發布前另做真實服務查詢、Windows 互動與重啟設定保存、macOS 實機驗證。
