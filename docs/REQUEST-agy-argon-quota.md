# 需求單：HUD 的 Antigravity（agy）額度改版——對齊官方面板、加入 Argon

- 提出：2026-10-16　對象：`D:\playground\claude-hud-monitor`（Python／PySide6 為主；`rust/` 另議）
- 性質：資料來源修正＋新增一個額度條＋版本更新。**這份是需求，不是實作指令**：資料來源有一處仍未查明（見 §3），要先查再做。

## 1. 背景：agy 面板的真實樣貌（使用者截圖，2026-10-16 約 18:10，Google AI **Pro** 方案）

agy（Antigravity CLI 1.4.0）的「Models & Quota」面板現在有**三組**：

| 組 | 項目 | 剩餘 | 重置倒數 |
|---|---|---|---|
| GEMINI MODELS（Gemini Flash、Gemini Pro） | Weekly | **99.26%** | 166h31m |
| | Five Hour | **97.43%** | 3h31m |
| CLAUDE AND GPT MODELS | Weekly | 84.49% | 166h36m |
| | Five Hour | 69.40% | 3h36m |
| **GEMINI 4 ARGON**（新） | **只有 Weekly** | **85.84%** | 166h36m |

面板說明文字：「For paid Pro plans, Argon quota is separate. Higher Argon quota (included as part of the larg…（截斷）」。
官方公告（Antigravity 官方帳號，2026-10-16）也寫：Pro 另有「獨立的、只有每週上限的 Argon 專屬額度」，疊在 Gemini 3.8 Flash 那一池之外。

HUD 現況（使用者截圖）：Antigravity 卡片 Gemini 5H **0%**（已用）、Weekly **5%**（已用）、徽章 `C/G: 100%`，**沒有 Argon**。

## 2. 已驗證的事實（2026-10-16 實測，可直接當契約；皆為只讀 `POST` 空物件 `{}`、`Authorization: Bearer <現有 agy 登入 token>`）

**2.1 agy 真正使用的後端是 `daily-cloudcode-pa.googleapis.com`**（agy.log 裡所有 `streamGenerateContent`／`loadCodeAssist`／`fetchAvailableModels` 都走它）。
而 HUD 現在 `AgyProvider` 的做法是「先試 daily，沒有可用資料就換 `cloudcode-pa.googleapis.com`（正式端點）」，導致：

| 端點 | `retrieveUserQuotaSummary` 回的群組 |
|---|---|
| `daily-cloudcode-pa…` | **只有** Claude and GPT models（`3p-weekly`、`3p-5h`）——沒有 Gemini 群組 |
| `cloudcode-pa…`（正式） | Gemini Models（`gemini-weekly`、`gemini-5h`）＋ Claude and GPT models |

HUD 因為 daily 沒有 Gemini 群組而判定「無可用資料」，改用正式端點的 Gemini 數字——**但那組數字是過期的/不是 agy 實際計量的**：
同一時刻正式端點回 Gemini weekly 94.84%、5h 100%，面板（＝實際）是 99.26%／97.43%。
→ **HUD 的 Gemini 兩條現在是錯的**（5H 永遠像 0% 已用、Weekly 停在舊值）。

**2.2 daily 的 `retrieveUserQuota`（沒有 Summary）回「每個模型一個 bucket」**，欄位 `modelId`、`remainingFraction`、`resetTime`、`tokenType`（`"WTUS"`）。
Gemini 5 小時視窗在這裡是對的：`gemini-3.8-flash-tiered`（及各 gemini-* 同值）的 `remainingFraction` 會隨用量下降（實測 0.974254 → 0.9649974 與面板 97.43% 一致方向）；
Claude／GPT-OSS 各 id 同值（對應 C/G 的 5 小時）。**這份沒有 weekly，也沒有任何 `argon` 的 id。**

**2.3 以下都已確認「看不到 Argon 額度」**（含帶 `{"project": "<loadCodeAssist 取得的專案>"}` 與不帶兩種）：
`retrieveUserQuotaSummary`、`retrieveUserQuota`、`fetchAvailableModels`（每個模型有 `quotaInfo`，但清單裡沒有 Argon 模型；回應約 70KB）。
`loadCodeAssist` 回 `currentTier.id = "free-tier"`（name「Antigravity」）、另有 `paidTier` 欄位——**不能用它判斷使用者是不是 Pro**。
用 Argon 實際跑一次小任務（約 2 萬 token）：Gemini 桶與 C/G 桶**都沒動** → Argon 確實記在上述端點看不到的專屬桶。

**2.4 token 位置（HUD 已在用，沿用即可）**：Windows 認證管理員 `gemini:antigravity`（JSON，`token.access_token`）；備援 `~/.gemini/antigravity-cli/antigravity-oauth-token`。

## 3. 未查明、要先查的事（需求的前置工作）

**面板的 Gemini weekly（99.26%）與 Argon weekly（85.84%）是從哪個呼叫拿到的？** 目前 2.1～2.3 的所有公開 REST 呼叫都拿不到。線索：

- agy 程式本體（`C:\Users\DJ_Linn\AppData\Local\agy\bin\agy.exe`，唯讀掃描字串即可）有這些內部 RPC 名稱：`GetUsageAndQuota`、`FetchQuotaStatus`、`GetUserQuotaSummary`、`RetrieveUserQuota`、`TriggerQuotaRefresh`、`GetFlexCreditQuota`；agy.log 有 `quota_manager.go: doRefreshQuota` 行，且 `languageServer=true`（面板可能走**本機 language server**，不是直連 Google）。
- 建議的查法（**請使用者配合一次**）：使用者在終端機執行 `agy --log-file D:\playground\claude-hud-monitor\.agy-panel.log`，進互動模式後打開面板（`/usage` 或 `/model` 的 Models & Quota 畫面）再關掉；請 agent 讀這份 log 找出面板觸發的請求與端點／埠。**用完刪掉該 log。**
- 若 log 顯示走本機 language server（localhost 埠＋CSRF token 之類），請先回報再決定要不要跟進——那是非公開介面，穩定性與風險要使用者判斷。

**抓不到來源時**：不要硬湊。退而求其次做 §4 的 R1＋R3（把 Gemini 與 C/G 修對），Argon 那條顯示「--」並在 `docs/PROVIDERS.md` 寫明原因，回報 PARTIAL。

## 4. 需求

| # | 需求 | 驗收 |
|---|---|---|
| R1 | **Gemini 與 C/G 的數字改對齊 agy 實際後端**：以 `daily-cloudcode-pa` 為主；Gemini 5H 用 daily `retrieveUserQuota` 的 `gemini-*` bucket（取最受限者）；不要再因為 daily 的 Summary 沒有 Gemini 群組就退回正式端點的舊數字。Gemini weekly 的來源待 §3 查明（查不到就標「--」，**不要**拿正式端點的舊值頂替） | 與面板同一時間比較，各項差距 ≤ 1 個百分點 |
| R2 | **新增 Argon 每週額度條**（只有 Weekly，沒有 5H）。卡片現在是 `metric1`／`metric2` 兩條＋兩個徽章；需要第三列或改版面，風格比照既有（顏色用 `get_progress_color`，剩餘轉已用的方向與現有一致）。**依賴 §3**：來源沒查出來前，此條顯示 `--` 並有 tooltip 說明 | 面板 85.84% 時，HUD 顯示 Argon 已用 14%（或與其他條一致的呈現方向） |
| R3 | **移除 CLI 備援 `agy --output-format json --print /quota`**：`/quota` 在 agy 不是指令，會被當成提示送進模型（實測每次約 3.6 萬 token）；`agy -p /usage` 在無頭模式也一樣被送進模型（約 2 萬 token）。**HUD 任何路徑都不得呼叫 agy 去跑對話或這兩個指令**；HTTP 全失敗就維持 STALE，不要退回 CLI | 搜尋全專案（含 `python/`、`rust/`、docs）不再有 `--print /quota`；`test_agy_transport.py` 對應測試改寫 |
| R4 | **多出來的未知群組不要丟**：回應裡不認得的群組（官方之後可能再加）至少在 diagnostics 記錄名稱（不記數值以外內容、不記 token） | 單元測試：塞入一個新群組不會讓整張卡變錯誤 |
| R5 | 測試：用**脫敏**的合成 fixture（照 §2.1／§2.2 的結構，數字用 §1 的值）測 `_parse_agy_json`／新增的 per-model 解析；不要把網路查詢放進測試 | `python/tests` 全綠 |
| R6 | 文件：更新 `docs/PROVIDERS.md` 的 AGY 一列與「AGY 超時」一節（去掉 `--print /quota` 說法）；寫明資料來源、已知限制（Argon／Gemini weekly 來源）與「已實測 AGY CLI 1.4.0」 | 文件與程式一致 |
| R7 | **版本更新**：升版號（建議 `v1.2.0`）、更新 `python/ClaudeHUD.spec`／打包相關版本字樣與 release note；**不要自行 build 或發佈 exe／push tag**，列出建議指令給使用者決定 | 版本字樣各處一致 |

範圍外（先不做，列在回報的「建議後續」）：`rust/src/providers/agy.rs` 的同步改版（Python 版為正式發佈線；Rust 要不要跟，請使用者決定）。

## 5. 限制與安全

- 只讀：只對 Google 端點送 `POST {}` 查額度；**不得呼叫任何會產生模型回應的方式**（不要 `agy -p`、不要 `agy --print`、不要送提示）。
- token 只在記憶體使用：不印出、不寫檔、不進 diagnostics／日誌／測試 fixture／commit；回報裡也不要貼 token 或使用者的 email、專案 ID。
- 不改使用者的 agy 設定（`~/.gemini/antigravity-cli/settings.json` 等）。
- 本專案是 git repo（`main`）：動手前 `git fetch`、開 feature 分支；commit 由使用者決定，**不要 push**。
- 這個工作區（`D:\playground`）不適用 H-ZERO 平台規範與其 skill，只取方法。

## 6. 回報時請附上

1. §3 的查證結果：面板資料來源是什麼（或「查不到」＋已試過什麼）
2. 與面板的數字對照表（同一時間：面板 vs HUD，各項）
3. 哪些需求完成、哪些沒做與原因
4. 測試結果（指令與通過數）
5. 建議使用者決定的事（Rust 版、build／發佈、是否跟進非公開介面）
