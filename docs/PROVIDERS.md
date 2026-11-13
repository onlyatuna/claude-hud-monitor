# Provider 相容性與故障排查

以本機 adapter 格式為基準，不聲稱端點為穩定公開 API。Windows 已實測 AGY CLI 1.4.0（2026-10-16）。
尚未建立完整 CLI 支援矩陣。問題回報請附平台、CLI 版本、error_code 與診斷耗時，勿附認證檔。

| Adapter | 解析欄位 | 缺失處理 |
| --- | --- | --- |
| Claude | five_hour / seven_day.utilization、resets_at；可選 seven_day_breakdown.rows；`limits[]`（模型別週額度，見下） | 未知窗口顯示 -- |
| Codex | rate_limit.primary_window / secondary_window 的 used_percent、reset_at、limit_window_seconds；plan_type | 不猜模型、方案或窗口 |
| AGY | `retrieveUserQuotaSummary` 的 `groups[].name`、`buckets[].id`、`remainingFraction`、`resetTime`；`retrieveUserQuota` 的 `buckets[].modelId`、`remainingFraction`、`resetTime`；statusLine 快取的 `quota.<bucket>.remaining_fraction`、`reset_time` | Gemini 5h 取 `modelId` 含 gemini 的 bucket 中最受限者；Gemini 週額度與 Argon 週額度只來自 statusLine 快取，未啟用時顯示 `--`；不認得的群組與快取 bucket 以額外列顯示（見 AGY 一節） |

AGY 第三方徽章為名称含 claude/gpt 群組週窗口的最小剩餘比例，不表示每個模型均有該額度。
上游改名／缺欄位時，新增合成或脫敏 fixture 與 adapter 修正，不以預設 0/100 掩蓋差異。

## Claude `limits[]`（模型別額度）

OAuth 用量回應的 `limits[]` 會帶出只存在於此陣列的額度，例如單一模型的週額度（`kind: weekly_scoped`、`scope.model.display_name: "Fable"`）；頂層 `seven_day_sonnet` 等欄位在這種帳號為 null。欄位形狀來自公開、已脫敏的回應紀錄，尚未以本專案維護者的帳號實測。

- `kind` 為 `session` / `weekly_all` 的項目與 `five_hour` / `seven_day` 重複，不另開列；只有在頂層 `five_hour`（或 `seven_day`）缺漏時才拿來當主列的備援。
- 其餘每個項目（只要 `percent` 是 0–100 的有限數值）都會成為一個額外列，依 API 順序排列，標題為 `<模型名大寫> 7D`（`group: weekly`）或 `… 5H`（`group: session`）；沒有模型名時改用 surface 名，再沒有就用 `kind` 大寫。同一 kind 與範圍重複時取第一個。
- 不看 `is_active`（它代表「目前是否為限制關鍵」，不代表有效）。`percent` 為 0、`resets_at` 為 null 的安靜項目照樣顯示，重設時間為 `--`。
- 每列另有穩定的 `key`（`MetricRow.key`，預設等於標題）：固定列為 `claude:5h`、`claude:7d`；模型別列為 `claude:<kind>:<模型名 slug>`，例如 `claude:weekly_scoped:fable`（slug＝小寫、連續非英數字元合成一個 `-`）。不使用陣列索引或 `model.id`（目前為 null）。`row_order` 仍以標題比對。
- 未知的 `kind` / `group` 不會造成錯誤：照上述規則顯示，並每個程序只在 log 記一次名稱（不寫入檔案）。非陣列的 `limits`、非物件的項目、缺 `percent` 或數值無效的項目一律略過；`limits[]` 的任何異常都不會丟掉有效的 `five_hour` / `seven_day`。

## 同時顯示第二個（以上）Claude 帳號

一個目錄＝一個登入（`<目錄>/.credentials.json`）；只有**分開的目錄**才能同時顯示。HUD 只讀目錄名稱與該檔的 `accessToken`（上限 64 KB），不讀 e-mail 或帳號 id，也不刷新 token（閒置帳號的 token 失效時會顯示 OFF，請用原 CLI 登入）。

- 主分頁（`claude`）永遠跟著 `claude_profile`；建議固定成一個帳號，不要用 `auto`（會在兩個帳號間跳動，也可能與額外分頁重複）。其他帳號從 右鍵選單 → 更多 → Claude 帳號 → 同時顯示 勾選，立即出現（不用重開）、取消勾選立即消失。`default`（`~/.claude`）不能當額外帳號；與主分頁相同的帳號不會列出。最多 6 個額外帳號。
- 設定檔（`config.json`，舊版執行檔會保留未知鍵）：`accounts` = `[{"id": "claude@claude-a", "provider": "claude", "profile": "claude-a", "nickname": "選填"}]`；`sidebar_tiles` = 側邊欄項目 id 的有序清單（順序＋顯示與否，缺省＝全部顯示，依 `provider_order`，額外帳號接在 Claude 後面）。無效的項目只在使用時略過，不改寫檔案。`provider_order` 仍只有三個基本 id。
- 額外帳號的 id 為 `claude@<profile 的 slug>`，額度鍵為 `claude@claude-a:5h`、`claude@claude-a:weekly_scoped:fable`；預設實例的鍵（`claude:5h`、`tray_quota`）不變。系統匣數字的選單與「自動」會列出所有帳號（分頁標題＝標籤）。
- 標籤（磚、卡片、系統匣、提示共用同一個函式）：`nickname`，否則目錄名去掉開頭的 `.`（`~/.claude-a` → `claude-a`）；與另一個實例同名時加上上層目錄。磚上最多 8 字、卡片標題最多 13 字、選單最多 16 字，超過以 `…` 截斷。
- 介面 → 側邊欄項目：每個分頁（含 AGY、Codex）可個別顯示／隱藏；隱藏不會停止抓取，系統匣數字與已儲存的 `claude:5h` 照常運作；至少保留一個。細條模式下長按磚拖曳換位，會寫入 `sidebar_tiles` 與 `provider_order`。
- 重新整理：每個實例各自排程，每次排程加上 ±25%（不低於 20 秒）的抖動，兩個帳號不會同時打用量端點；新增的帳號立刻抓取一次。
- 失敗：額外帳號的目錄不見了時，分頁維持 OFF，設定不會被刪除，可在選單取消勾選（顯示「找不到」）。

## AGY 資料來源與已知限制

- 資料來源：只用 HTTP `POST {}` 讀取 `https://daily-cloudcode-pa.googleapis.com/v1internal` 的 `retrieveUserQuotaSummary`（C/G 週額度徽章）與 `retrieveUserQuota`（Gemini 5 小時）。`daily-cloudcode-pa` 是 agy 實際使用的後端；正式端點 `cloudcode-pa` 的 Gemini 數字是過期值，已不再使用
- 認證：Windows 認證管理員 `gemini:antigravity`（macOS 為 Keychain 同名項目），備援 `~/.gemini/antigravity-cli/antigravity-oauth-token`；token 只在記憶體使用
- statusLine 快取（Gemini 週額度與 Argon 週額度的來源）：agy 互動模式會把 session JSON 從 stdin 餵給設定的 `statusLine.command`，其中 `quota` 含 `gemini-5h`、`gemini-weekly`、`3p-5h`、`3p-weekly`、`gemini-argon-wtu-weekly`（Argon，只有每週）五個 bucket（AGY 1.4.0 實測）。HUD 的 hook 只把 bucket 的 `remaining_fraction` 與 `reset_time` 寫到設定資料夾的 `agy-quota.json`，不保存 email、路徑或其他欄位
  - 啟用：右鍵選單 → 更多 → AGY 週額度掛鉤 → 啟用（或 `python main.py --install-agy-statusline`）（會先備份 `settings.json` 為 `settings.json.bak-hud-<時間>`；已有其他 statusLine 時不覆蓋）；停用：`--uninstall-agy-statusline`。啟用後需重開 agy 互動模式
  - 限制：只有 agy 互動模式開著時才會更新；agy 沒開時沿用最後一次的值，已過重置時間的 bucket 會被丟棄。設定資料夾路徑含空白時無法安裝（agy 會把含引號的指令路徑當相對路徑）
  - Argon 以徽章顯示剩餘百分比（`Argon: NN%`），與 C/G 徽章方向一致
  - 未知額度自動成列（不需改程式）：statusLine 快取裡不屬於上述五個 bucket 的 key（例如日後 Ultra 方案的 Gemini Carbon 額度），以及 `retrieveUserQuotaSummary` 中名稱不含 gemini / claude / gpt / argon 的群組，其每個有效 bucket 都會成為額外列，排在固定五列之後（先摘要群組、API 順序；再快取、依 key 排序）。`retrieveUserQuota` 的逐模型 bucket 不會成列，仍合併為 GEMINI 5H
  - 標題：快取 key 去掉開頭 `gemini-` 與 `wtu`，尾端 `weekly` / `week` / `7d` → `7D`、`5h` → `5H`，其餘大寫、`-` 換成空格（`gemini-carbon-wtu-weekly` → `CARBON 7D`）；不符形狀時整個 key 大寫、`-` 換空格。摘要 bucket 用其 `displayName` 大寫（沒有則由 `bucketId` 推導；標題重複時前面加群組名）。標題與固定列重複或已存在者略過
  - 穩定 key（托盤數字用）：`agy:<標題 slug>`（slug 規則同 Claude），例如 `agy:carbon-7d`。列消失時，托盤選擇顯示 `--`（unavailable）
  - `agy_order`：已存的排序保留仍存在的額外列位置，資料中已不存在的額外列在儲存或拖拉重排時才被丟棄；新出現的額外列排在最後。只有固定五列時行為與先前相同
  - 限制：目前沒有任何真實的 Carbon 資料（維護者為 Google AI Pro，CLI 標示 Carbon 僅 Ultra），`gemini-carbon-wtu-weekly` 這個 key 是比照 Argon 的推測
- 已知限制：agy 面板的資料可能另走 gRPC（`google.gca.aicode.v1main`，對 REST 回 404）或需 CSRF token 的本機 language server，HUD 都不使用
- 禁止事項：HUD 不呼叫 agy 執行檔。`agy --print /quota` 與 `agy -p /usage` 在無頭模式會被當成提示送進模型（每次耗約 2～3.6 萬 token），不要用來查額度

## AGY 查詢失敗

1. 確認 agy 已登入（`agy` 互動模式能開 `/usage` 面板）
2. HUD 單次 HTTP 請求逾時 8 秒；`401` 代表登入失效，請在 agy 重新登入
3. HTTP 全部失敗時 HUD 保留最後成功資料並標記 STALE，之後退避重試
4. diagnostics.log 不保存 token 或原始回應

## Claude / Codex

- 401 / 403 / 找不到憑證（`error_code` = `auth`）：顯示 OFF，不保留舊值；只有暫時性失敗（network、429、5xx、逾時）才保留最後一次的值並標記琥珀色 STALE。請在原 CLI 重新登入，HUD 不刷新／覆寫共享憑證。
- 429：遵守 Retry-After 與退避，不要連續手動刷新。
- schema：格式或窗口變更，未知資料顯示 --。
- network：連線未完成，等待自動重試。

目前只支援 README 列出的認證檔格式。系統鑰匙圈、自訂設定目錄、API key-only 登入不在本輪支援範圍。

## 新增 Provider

實作 BaseProvider.fetch_usage，回傳 UsageMetrics；註冊到 core/providers/__init__.py 並配置卡片與主題。
測試正常、部分缺失、空資料、無效數值、錯誤與超時；不要把網路查詢放入 UI 或測試套件。
