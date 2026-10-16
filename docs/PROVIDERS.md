# Provider 相容性與故障排查

以本機 adapter 格式為基準，不聲稱端點為穩定公開 API。Windows 已實測 AGY CLI 1.4.0（2026-10-16）。
尚未建立完整 CLI 支援矩陣。問題回報請附平台、CLI 版本、error_code 與診斷耗時，勿附認證檔。

| Adapter | 解析欄位 | 缺失處理 |
| --- | --- | --- |
| Claude | five_hour / seven_day.utilization、resets_at；可選 seven_day_breakdown.rows | 未知窗口顯示 -- |
| Codex | rate_limit.primary_window / secondary_window 的 used_percent、reset_at、limit_window_seconds；plan_type | 不猜模型、方案或窗口 |
| AGY | `retrieveUserQuotaSummary` 的 `groups[].name`、`buckets[].id`、`remainingFraction`、`resetTime`；`retrieveUserQuota` 的 `buckets[].modelId`、`remainingFraction`、`resetTime` | Gemini 5h 取 `modelId` 含 gemini 的 bucket 中最受限者；Gemini 週額度與 Argon 週額度目前無來源，顯示 `--`；不認得的群組只在 log 記名稱 |

AGY 第三方徽章為名称含 claude/gpt 群組週窗口的最小剩餘比例，不表示每個模型均有該額度。
上游改名／缺欄位時，新增合成或脫敏 fixture 與 adapter 修正，不以預設 0/100 掩蓋差異。

## AGY 資料來源與已知限制

- 資料來源：只用 HTTP `POST {}` 讀取 `https://daily-cloudcode-pa.googleapis.com/v1internal` 的 `retrieveUserQuotaSummary`（C/G 週額度徽章）與 `retrieveUserQuota`（Gemini 5 小時）。`daily-cloudcode-pa` 是 agy 實際使用的後端；正式端點 `cloudcode-pa` 的 Gemini 數字是過期值，已不再使用
- 認證：Windows 認證管理員 `gemini:antigravity`（macOS 為 Keychain 同名項目），備援 `~/.gemini/antigravity-cli/antigravity-oauth-token`；token 只在記憶體使用
- 已知限制：Gemini 週額度與 Gemini 4 Argon 週額度（Pro 方案獨立、只有每週上限）顯示於 agy 面板，但上述公開 REST 端點都不提供；面板資料疑似來自 gRPC 的 `google.gca.aicode.v1main` 服務（`FetchQuotaStatus`／`GetUserQuotaSummary`），該服務對 REST 呼叫回 404，需 gRPC 才能存取，屬非公開介面，目前不支援。要看這兩項請以 agy 互動模式的 `/usage` 面板為準
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
