# Provider 相容性與故障排查

以本機 adapter 格式為基準，不聲稱端點為穩定公開 API。Windows 已實測 AGY CLI 1.4.0（2026-10-16）。
尚未建立完整 CLI 支援矩陣。問題回報請附平台、CLI 版本、error_code 與診斷耗時，勿附認證檔。

| Adapter | 解析欄位 | 缺失處理 |
| --- | --- | --- |
| Claude | five_hour / seven_day.utilization、resets_at；可選 seven_day_breakdown.rows | 未知窗口顯示 -- |
| Codex | rate_limit.primary_window / secondary_window 的 used_percent、reset_at、limit_window_seconds；plan_type | 不猜模型、方案或窗口 |
| AGY | `retrieveUserQuotaSummary` 的 `groups[].name`、`buckets[].id`、`remainingFraction`、`resetTime`；`retrieveUserQuota` 的 `buckets[].modelId`、`remainingFraction`、`resetTime`；statusLine 快取的 `quota.<bucket>.remaining_fraction`、`reset_time` | Gemini 5h 取 `modelId` 含 gemini 的 bucket 中最受限者；Gemini 週額度與 Argon 週額度只來自 statusLine 快取，未啟用時顯示 `--`；不認得的群組只在 log 記名稱 |

AGY 第三方徽章為名称含 claude/gpt 群組週窗口的最小剩餘比例，不表示每個模型均有該額度。
上游改名／缺欄位時，新增合成或脫敏 fixture 與 adapter 修正，不以預設 0/100 掩蓋差異。

## AGY 資料來源與已知限制

- 資料來源：只用 HTTP `POST {}` 讀取 `https://daily-cloudcode-pa.googleapis.com/v1internal` 的 `retrieveUserQuotaSummary`（C/G 週額度徽章）與 `retrieveUserQuota`（Gemini 5 小時）。`daily-cloudcode-pa` 是 agy 實際使用的後端；正式端點 `cloudcode-pa` 的 Gemini 數字是過期值，已不再使用
- 認證：Windows 認證管理員 `gemini:antigravity`（macOS 為 Keychain 同名項目），備援 `~/.gemini/antigravity-cli/antigravity-oauth-token`；token 只在記憶體使用
- statusLine 快取（Gemini 週額度與 Argon 週額度的來源）：agy 互動模式會把 session JSON 從 stdin 餵給設定的 `statusLine.command`，其中 `quota` 含 `gemini-5h`、`gemini-weekly`、`3p-5h`、`3p-weekly`、`gemini-argon-wtu-weekly`（Argon，只有每週）五個 bucket（AGY 1.4.0 實測）。HUD 的 hook 只把 bucket 的 `remaining_fraction` 與 `reset_time` 寫到設定資料夾的 `agy-quota.json`，不保存 email、路徑或其他欄位
  - 啟用：`python main.py --install-agy-statusline`（會先備份 `settings.json` 為 `settings.json.bak-hud-<時間>`；已有其他 statusLine 時不覆蓋）；停用：`--uninstall-agy-statusline`。啟用後需重開 agy 互動模式
  - 限制：只有 agy 互動模式開著時才會更新；agy 沒開時沿用最後一次的值，已過重置時間的 bucket 會被丟棄。設定資料夾路徑含空白時無法安裝（agy 會把含引號的指令路徑當相對路徑）
  - Argon 以徽章顯示剩餘百分比（`Argon: NN%`），與 C/G 徽章方向一致
- 已知限制：agy 面板的資料可能另走 gRPC（`google.gca.aicode.v1main`，對 REST 回 404）或需 CSRF token 的本機 language server，HUD 都不使用
- 禁止事項：HUD 不呼叫 agy 執行檔。`agy --print /quota` 與 `agy -p /usage` 在無頭模式會被當成提示送進模型（每次耗約 2～3.6 萬 token），不要用來查額度

## AGY 查詢失敗

1. 確認 agy 已登入（`agy` 互動模式能開 `/usage` 面板）
2. HUD 單次 HTTP 請求逾時 8 秒；`401` 代表登入失效，請在 agy 重新登入
3. HTTP 全部失敗時 HUD 保留最後成功資料並標記 STALE，之後退避重試
4. diagnostics.log 不保存 token 或原始回應

## Claude / Codex

- 401：在原 CLI 重新登入；HUD 不刷新／覆寫共享憑證。
- 429：遵守 Retry-After 與退避，不要連續手動刷新。
- schema：格式或窗口變更，未知資料顯示 --。
- network：連線未完成，等待自動重試。

目前只支援 README 列出的認證檔格式。系統鑰匙圈、自訂設定目錄、API key-only 登入不在本輪支援範圍。

## 新增 Provider

實作 BaseProvider.fetch_usage，回傳 UsageMetrics；註冊到 core/providers/__init__.py 並配置卡片與主題。
測試正常、部分缺失、空資料、無效數值、錯誤與超時；不要把網路查詢放入 UI 或測試套件。
