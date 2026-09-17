# 本地修正驗證紀錄

基準：v1.0.1 / 6f7d67e。工作分支：fix/provider-reliability。狀態：本地開發，未發布。

| 順序 | 修正 | 驗證 |
| --- | --- | --- |
| 1 | 缺失配額不再顯示 0% | 空資料、部分窗口、無效數字、AGY bucket |
| 2 | 打包設定持久路徑與原子寫入 | frozen 路徑、重載、失敗保留原檔 |
| 3 | AGY 超時、診斷、退避、舊資料 | transport mock、排程與 stale |
| 4 | 移除 Codex 假模型／方案 | 徽章與窗口長度 |
| 5 | 查詢競態 | 延遲回傳、重複刷新、關閉後忽略 |
| 6 | 錯誤與倒數殘留 | Qt offscreen 卡片測試 |
| 7 | macOS 實作與支援聲明 | 快捷鍵與 plist 模擬；實機待驗證 |

## 驗證指令

```powershell
python -B -m unittest discover -s tests -v
python -B main.py --smoke-test
```

smoke-test 使用合成資料與暫存設定，不讀取憑證、不註冊快捷鍵、不修改開機啟動。
打包後亦可執行 `dist/local/ClaudeHUD-Local.exe --smoke-test` 並確認 exit code 0。

## 本機服務查詢：2026-09-17

Windows / Python 3.12.2，各查詢一次均成功：Claude 0.44s、Codex 0.62s、AGY 4.17s（CLI 1.2.4）。
僅代表本機帳號該次成功，不代表長期穩定性或其他登入環境。

## 發布前人工驗收

- Windows 桌面：顯隱、穿透解除、拖曳、休眠恢復、重啟設定。
- macOS：權限拒絕／授權、Option+C、Option+Shift+C、穿透、LaunchAgent、arm64 打包。
- 遠端 CI：只修改本地 workflow，未推送觸發；既有保護分支未變更。
- macOS pynput 依賴仍為範圍版本，完整平台 lockfile 待 macOS 驗證後產生。

## 自動化結果

26 個回歸測試通過，執行緒生命週期修正後完整套件連續 10 輪通過。
Windows PyInstaller 單檔測試版可執行離線 smoke-test；原生 Windows 字型的合成資料畫面已檢查。
本地測試版：dist/local/ClaudeHUD-Local.exe。未提交、未推送、未建立 Tag 或發布。
中文檔案以 UTF-8 寫入，另有回歸檢查防止錯誤管線編碼造成問號。
