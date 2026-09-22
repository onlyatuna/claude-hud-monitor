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
       UsageTable（表格：Provider 為欄，圓盤 內圈 5 小時／外環 1 週）
```

## 責任界線

- core/providers/：傳輸、認證讀取、解析；不碰 Qt widget、不修改憑證。
- core/providers/base.py：共用資料契約、百分比驗證、錯誤分類、倒數。
- core/refresh_controller.py：請求序號、退避、最後成功資料與排程；可變狀態由 Qt 主執行緒更新。
- ui/hud_window.py：視窗、選單、穿透、主題切換與休眠偵測。
- ui/usage_table.py：渲染資料，不查網路；平均進度虛線與超出段斜紋只表達速度，不改變顏色。
- ui/styles.py：淺色／深色主題 × 色階／雙色配色的色號與樣式表。
- ui/vibrancy.py：macOS 原生毛玻璃背景（NSVisualEffectView）；其他平台改用較不透明的面板。
- core/pace.py：倒數格式、視窗長度解析、平均進度與色階門檻（純函式，可單元測試）。
- core/config_manager.py：設定位置、批次更新、同目錄暫存檔與原子替換。
- core/diagnostics.py：限制大小的操作紀錄，禁止傳入憑證、原始回應與 CLI stderr。
- system/hotkey.py、core/autostart.py：平台整合。

舊 core/anthropic_client.py 已移除，新增 Claude 功能只修改 ClaudeProvider。

## 資料契約

metric1_val、metric2_val 是 0–100 的已使用比例或 None。None 對應 --；0 為有效讀值。
兩個窗口均不可用時回傳 schema 錯誤；單窗口可用時允許部分顯示。
徽章不可填入未證實的方案或模型。沒有重設時間就顯示 --。
error_code 為診斷分類；error 是使用者可讀訊息，不能包含 token 或原始 payload。
stale 與 last_success 由協調層管理，最後成功資料不可假裝是最新值。

## 排程不變條件

worker 只持有 Python queue，不持有或從背景執行緒呼叫 Qt 物件；主執行緒每 25ms 取回結果。
每個 Provider 同時最多一個 worker。手動刷新／休眠恢復使舊結果失效，原查詢結束後只補一次新查詢。
不得以清除布林鎖或 join timeout 假裝取消 worker。關閉後忽略任何晚到結果。
各服務獨立排程，AGY 慢查詢不阻塞其他服務。CLI 有總時限；urllib timeout 為 I/O timeout，非整體截止時間。
OS 層呼叫若卡住，該服務等待原查詢退出，不無限產生 worker；這仍是傳輸層的限制。

## 設定與平台

打包設定不得寫入 PyInstaller 解壓目錄。原始碼模式保留專案設定；測試注入暫存路徑。
排版切換前保存舊尺寸，套用新尺寸時抑制中間事件寫入；移動／縮放延遲 250ms 合併寫入。
macOS 功能需實機驗證，不以平台模擬視為已驗收。支援狀態以 README 為準。

## 驗證

執行 `python -B -m unittest discover -s tests -v` 與 `python -B main.py --smoke-test`。
測試使用合成資料，不使用真實帳號或開機啟動設定。
發布前另做真實服務查詢、Windows 互動與重啟設定保存、macOS 實機驗證。
