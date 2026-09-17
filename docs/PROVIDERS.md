# Provider 相容性與故障排查

以本機 adapter 格式為基準，不聲稱端點為穩定公開 API。Windows 已實測 AGY CLI 1.2.4。
尚未建立完整 CLI 支援矩陣。問題回報請附平台、CLI 版本、error_code 與診斷耗時，勿附認證檔。

| Adapter | 解析欄位 | 缺失處理 |
| --- | --- | --- |
| Claude | five_hour / seven_day.utilization、resets_at；可選 seven_day_breakdown.rows | 未知窗口顯示 -- |
| Codex | rate_limit.primary_window / secondary_window 的 used_percent、reset_at、limit_window_seconds；plan_type | 不猜模型、方案或窗口 |
| AGY | command.data.groups[].name、buckets[].id/window、remaining_fraction、reset_time | Gemini 5h/week，同窗口選最受限 bucket |

AGY 第三方徽章為名称含 claude/gpt 群組週窗口的最小剩餘比例，不表示每個模型均有該額度。
上游改名／缺欄位時，新增合成或脫敏 fixture 與 adapter 修正，不以預設 0/100 掩蓋差異。

## AGY 超時

1. 在終端機確認 `agy --version`，執行 `agy --output-format json --print /quota`。
2. HUD 由 PATH 與平台預設路徑搜尋 CLI，單次最長 30 秒。
3. diagnostics.log 記錄耗時、exit code、stderr 字元數，不保存原始輸出。
4. CLI 自身失敗時先處理登入或網路。HUD 保留最後成功資料並標記 STALE，之後退避重試。

## Claude / Codex

- 401：在原 CLI 重新登入；HUD 不刷新／覆寫共享憑證。
- 429：遵守 Retry-After 與退避，不要連續手動刷新。
- schema：格式或窗口變更，未知資料顯示 --。
- network：連線未完成，等待自動重試。

目前只支援 README 列出的認證檔格式。系統鑰匙圈、自訂設定目錄、API key-only 登入不在本輪支援範圍。

## 新增 Provider

實作 BaseProvider.fetch_usage，回傳 UsageMetrics；註冊到 core/providers/__init__.py 並配置卡片與主題。
測試正常、部分缺失、空資料、無效數值、錯誤與超時；不要把網路查詢放入 UI 或測試套件。
