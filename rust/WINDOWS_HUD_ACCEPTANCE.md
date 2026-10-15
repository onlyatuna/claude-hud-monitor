# Windows HUD 實機驗收清單

**狀態：全部「未執行」。** 這份清單只定義步驟與判定標準，不含任何已執行的結果。它對應 `rust/qtrs/PYTHON_QT_SEMANTIC_CONTRACT.md` 附錄 E.2 的 5 項（候選阻擋項目；A/B/C 分類尚未經使用者批准）。CI 通過、純函式測試與程式化訊息測試都不能代替這裡的任何一項。

這份清單屬於 HUD 的發布驗收，不屬於 qtrs／ferrokit 函式庫；之後拆 repo 時留在 HUD repo。`QTRS_*` 環境變數會在改名時一併更名，改名後以新名稱為準。

## 使用規則
1. 判定只有三種：通過、不通過、未執行。由執行者填寫；沒有實際執行就維持「未執行」。
2. 標示「Contract 沒有預期值」的步驟只記錄觀察，不判定通過。
3. 不通過時附證據：截圖、`qtrs_resize_debug.log`、`config.json` 的相關欄位。
4. 標示 `[INFERENCE]` 的預期是推論，不是 Contract 的結論。實測與推論不符時，先記錄，再回頭檢查推論。
5. 結果交回後，Contract 附錄 E.2 的「狀態」欄才會改，並且只照實填寫。

## 共同前置
| 項目 | 內容 |
|---|---|
| 受測版本 | 記錄 `git rev-parse HEAD`，以及是否有未提交的修改 |
| 建置 | `cd rust`，`cargo build --release`。執行檔名稱取自 `rust/Cargo.toml` 的 `[[bin]] name = "ClaudeHUD"`；輸出路徑 `target\release\ClaudeHUD.exe` 是 cargo 預設，未在本機核對 |
| 設定檔 | `%APPDATA%\ClaudeHUDMonitor\config.json`；exe 旁有 `config.json` 時優先（portable，見 `rust/src/config.rs` 檔頭）。測前備份 |
| 環境記錄 | Windows 版本（`winver`）、每台螢幕的解析度與縮放、哪台是主螢幕、GPU 與驅動版本 |
| 尺寸（邏輯 px，`config.rs:67-77`） | 最小值：橫向 540×125、直向 250×320、表格 380×280。HUD 沒有設最大尺寸（RC-205 記載 `set_maximum_size` 沒有 HUD 呼叫者） |
| 拖曳與縮放區 | 左鍵按在視窗內側距邊 8 邏輯 px 內是縮放（`calc_frameless_edge`，`M = 8`），其餘沒有被 widget 處理的左鍵按下是移動（`hud_window.rs:491-505`）。`locked = true` 時兩者都停用 |
| Python 對照 | 需要對照時，依 README 啟動 Python 版，在同一螢幕、同一位置操作 |
| DComp | 預設不啟用。`echo $env:QTRS_ENABLE_DCOMP` 應為空 |

## 項目 1：互動式視窗尺寸限制、視窗拖曳與多縮放
- **來源**：Contract RC-204～207 的 Residual。RC-204 實測 `WS_POPUP`（HUD 的 `LAYERED` 彈出視窗）的程式化 `SetWindowPos` 不受追蹤上下限限制，所以下限只在互動式縮放時由系統套用；CI 的比例是 1.0。
- **環境**：滑鼠。1a～1c 一台可切換縮放的螢幕；1d 兩台不同縮放的螢幕。
- **量測方法**：以 `QTRS_RESIZE_DEBUG=1` 啟動（PowerShell：`$env:QTRS_RESIZE_DEBUG="1"; .\ClaudeHUD.exe`）。每次拖曳結束時印出一份報告到 stderr，並附加到 `%TEMP%\qtrs_resize_debug.log`（`resize_debug.rs` 檔頭）。取最後一筆 `ULW ... hwnd AxB->CxD` 的 `CxD`，單位是實體像素。**不要用 `config.json` 判定下限**：`apply_size` 會把儲存值夾到最小值以上（`hud_window.rs:39-42`），看不出限制是否生效。

**1a 縮小下限**（每種縮放 × 每種模式）

| 步驟 | 預期 | 判定 |
|---|---|---|
| 1. 設定該縮放，啟動 HUD，切到該模式 | | |
| 2. 左鍵按右下角（內側 8 邏輯 px 內），往左上拖到遠小於最小值，放開 | 視窗停在下限，不再縮小 | |
| 3. 讀報告最後一筆 `hwnd` 的結果尺寸 | 見下表 | |
| 4. 左上角與四邊各重複一次 | 同上 | |

預期實體尺寸 `[INFERENCE]`：由 RC-204 的規則「邏輯最小值 × 縮放，四捨五入遠離零」推得。Contract 只給出 125% 橫向 675×156，其餘是我算的。

| 模式（邏輯最小值） | 100% | 125% | 150% |
|---|---|---|---|
| 橫向 540×125 | 540×125 | 675×156 | 810×188 |
| 直向 250×320 | 250×320 | 313×400 | 375×480 |
| 表格 380×280 | 380×280 | 475×350 | 570×420 |

小於表值代表限制沒有生效；其他差異先記錄，不直接判定。

**1b 放大**：往外拖過橫向預設 690×145，一路拖到螢幕邊。預期：沒有 HUD 設定的最大尺寸，可超過預設值。Contract 沒有預期值，只記錄停在哪裡。

**1c 視窗拖曳**

| 步驟 | 預期 | 判定 |
|---|---|---|
| 1. 在 HUD 空白處（非按鈕、非邊緣）按住左鍵拖曳 | 視窗跟著游標移動（`post_system_move` 送 `SC_DRAGMOVE`） | |
| 2. 放開 | 立刻停止移動，沒有卡在移動狀態 | |
| 3. 退出後讀 `config.json` 的 `window_x`、`window_y` | Contract 沒有預期值，只記錄與放開位置是否一致 | |
| 4. 設 `locked = true` 重做 1 與 1a | 不能拖曳也不能縮放（`calc_frameless_edge` 在 `locked` 時回傳空） | |

**1d 跨螢幕**（需要兩台不同縮放）

| 步驟 | 預期 | 判定 |
|---|---|---|
| 1. 把 HUD 從縮放 A 的螢幕拖到縮放 B 的螢幕，放開 | 視窗停在 B | |
| 2. 在 B 重做 1a（至少一種模式） | 下限用 B 的縮放換算（RC-204：每次詢問時以視窗當下 DPI 換算） | |
| 3. 來回拖 5 次，每次記錄報告最後一筆 `hwnd` 尺寸，除以當下縮放換成邏輯尺寸 | 邏輯尺寸不逐次漂移（±1 的取整除外）`[INFERENCE]`：漂移是我提出的風險，Contract 未記載 | |
| 4. 記錄跨越瞬間是否有可見的尺寸跳動或閃爍 | `WM_DPICHANGED` 的處理順序在 RC-204 是推論 `[INFERENCE]`，無法直接觀察，只能看是否出現異常 | |

## 項目 2：原生滑鼠 capture（RC-90）
- **來源**：Contract RC-90 的驗證與 Residual：`SetCapture` 只經編譯與既有測試，未以真實輸入驗證；雙擊是否也設 grab 與 capture，是否與 Qt 一致，未驗證。
- **環境**：實體滑鼠；橫向或直向模式（表格模式沒有切換按鈕）。

| 步驟 | 預期 | 判定 |
|---|---|---|
| 1. 在切換按鈕 `⇄` 上按住左鍵（不放） | Contract 沒有預期值，記錄按鈕外觀 | |
| 2. 把游標拖出 HUD 視窗至少 100 px，在外面放開 | 版面不切換；按鈕沒有卡在按下或 hover 外觀 `[INFERENCE]`：由 RC-90 的 Qt 依據（按住期間 move 與 release 交給按下的 widget）推得，未在 PySide6 實測 | |
| 3. 游標回到按鈕上 | hover 外觀正常 | |
| 4. 對照：在按鈕上按下並在按鈕內放開 | 版面切換一次 | |
| 5. 快速雙擊按鈕，記錄版面切換幾次 | Contract 沒有預期值；請在 Python 版做同樣操作並記錄次數 | |

## 項目 3：被遮蓋後的重繪（RC-86）
- **來源**：Contract RC-86 Residual：「GDI layered 退路被遮蓋後可能要等下一次變更才重繪（未驗證）」。RC-203 之後預設路徑就是 Layered，所以這個風險落在預設路徑。
- **環境**：一台螢幕即可；`always_on_top` 設為 false，否則其他視窗蓋不住 HUD。

| 步驟 | 預期 | 判定 |
|---|---|---|
| 1. 把一個最大化視窗放到 HUD 上方，完全遮蓋至少 10 秒（期間 HUD 的時鐘每秒更新） | | |
| 2. 移開遮蓋物，或把 HUD 帶到前景 | HUD 內容完整，時鐘是目前時間，沒有空白、殘影或舊畫面 | |
| 3. Win+L 鎖定再解鎖 | 同上 | |
| 4. 如果內容要等到下一次資料更新才恢復 | 記為不通過，並記錄等了多久 | |

## 項目 4：混合 DPI 的 tooltip、位置還原與渲染縮放
- **來源**：G8.8.g（tip 視窗以主螢幕 DPR 建立，再移到游標所在螢幕，`[INFERENCE]`）、G11.4.g（混合 DPI 多螢幕的工作區座標）、G10.7.i（沒有異 DPI 實機測試）。
- **環境**：兩台以上不同縮放的螢幕，其中一台是主螢幕。優先包含 175%，因為 Contract 的測試規格用的是 `DpiChanged{168,168}`。

| 步驟 | 預期 | 判定 |
|---|---|---|
| 4a. 在螢幕 A 把游標停在 `⇄` 上，等 tooltip（「切換 橫向並排 / 直式堆疊 佈局」）出現並截圖；把 HUD 拖到螢幕 B 重做 | Contract 沒有數值預期。記錄 tip 相對游標的位置、文字高度、是否被截斷、是否出現在游標所在的螢幕；與 Python 版同位置的 tip 對照 | |
| 4b. 拖到另一個縮放的螢幕後，檢查 HUD 文字 | 邊緣銳利，沒有被系統拉伸或模糊；邏輯尺寸與 1d 一致 | |
| 4c. 把 HUD 放到縮放與主螢幕不同的副螢幕，記錄螢幕與座標，退出再啟動 | 回到同一螢幕、同一位置；以 Python 版的規則為準（`hud_window.py:376-424`、`rust/src/ui/placement.rs`） | |
| 4d. 停用或拔掉副螢幕後再啟動 | 儲存位置在所有螢幕之外時，結果以 Python 版在相同條件下的結果為準；記錄兩者差異 | |

## 項目 5：DirectComposition 真實裝置與 DXGI 裝置遺失（條件式）
- **來源**：Contract RC-203 Residual。DComp 預設不啟用（`QTRS_ENABLE_DCOMP` 為 `1` 或 `true` 才啟用，`qtrs-platform/src/window.rs:1404`）。**只有在決定預設啟用 DComp 時才需要執行**；否則填「不適用」。
- **環境**：有 DirectComposition 能力的 GPU；記錄 GPU 與驅動版本。
- 已有的自動化證據只涵蓋合成故障的降級，沒有模擬真實的 DXGI `DEVICE_REMOVED` 或驅動崩潰（RC-203 驗證段）。

| 步驟 | 預期 | 判定 |
|---|---|---|
| 1. `$env:QTRS_ENABLE_DCOMP="1"` 啟動 HUD | 透明背景、不透明度、文字、拖曳與縮放與預設路徑一致；Contract 沒有逐像素預期，只記錄差異 | |
| 2. HUD 執行中按 Win+Ctrl+Shift+B（重啟顯示驅動）`[INFERENCE]`：這是一般 Windows 做法，未在本專案驗證 | 不當機、不凍結；HUD 繼續顯示（RC-203 設計：失敗時降級為 Layered 並以完整 dirty 區域重試） | |
| 3. 繼續操作 HUD | 仍可拖曳與縮放；降級後即使仍設定 `QTRS_ENABLE_DCOMP=1` 也維持 Layered（`dcomp_downgraded`） | |

目前沒有已知的日誌訊息可辨識使用中的 presenter（我只在 `qtrs-platform/src/window.rs` 看到 `dcomp_downgraded` 旗標，沒有查 `dcomp.rs` 的輸出），所以判定靠行為：持續可見、可互動。

## 結果記錄
| 項目 | 日期 | 執行者 | 受測 commit | 環境（螢幕與縮放、GPU 與驅動、Windows 版本） | 判定 | 證據路徑 |
|---|---|---|---|---|---|---|
| 1 互動式尺寸限制與拖曳 | | | | | 未執行 | |
| 2 原生滑鼠 capture | | | | | 未執行 | |
| 3 被遮蓋後重繪 | | | | | 未執行 | |
| 4 混合 DPI | | | | | 未執行 | |
| 5 DComp 與裝置遺失 | | | | | 未執行 | |
