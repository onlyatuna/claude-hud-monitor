# Rust / Qt 相容層規格書（qtrs）— v0.2.3

> **狀態**：治理規則（00、01、10、11）可用於試遷移，不代表規格已完整。02 已完成試遷移（72 條要求；舊 Contract C2.1–C2.7、C3.5、C6.6 已改為指標）；03 為骨架；04–09、12 只有標題。
>
> **權威來源**
> - 一章遷移完成以前，該章內容仍以 `PYTHON_QT_SEMANTIC_CONTRACT.md`（下稱「舊 Contract」）為準。
> - 例外：刻意差異登記表（01.6）與支援範圍（00.6）自 v0.2.1 起**只**以本書為準。
> - 一章遷移完成後，舊 Contract 的對應段落改成一行指標；同一條要求不得同時存在兩處。
>
> **遷移對照表**：`QTRS_SPEC_MIGRATION.md`。

## 修訂紀錄
| 版本 | 變更 |
|---|---|
| v0.2 | 大綱 00–12；00、01、02、03、10、11 骨架 |
| v0.2.3 | 治理矛盾修正：R-00001 只約束 API 返回後的狀態，回呼期間的中間狀態另立 R-00049／R-00050（PySide 實測：`ChildRemoved` 期間 child 的 parent 仍是舊 parent）；新增欄位「差異（DV）與 Qt 對照」，已核准差異的要求以有效契約判定（11.2）；R-00008 由 D 改為 B 類候選差異 DV-08（Core 要求不得標 D）；拆開複合要求（R-00002、R-00013、R-00020、R-00022、R-00025、R-00027、R-00030、R-00040 → 新增 R-00049–R-00061）；新增欄位「驗收方式與依賴」；11.7 加「可開始實作」的就緒條件；修正 v0.2.2 紀錄的節號（五問在 11.6）。同版第二輪（m2 稽核 `local/spec02_audit.md`）：再拆出 R-00062–R-00073（API 路徑、診斷輸出、widget 層、receiver 端連線、Windows 計時器、class info）；新增候選差異 DV-09（R-00048）；G2.7.b 由 D 改標 DV-07；修正 R-00066 的 `qwidget.cpp` 行號（舊引用 :941 錯誤，實際為 1557-1561、1581）；PYSIDE-RUN 補上 OS 與 dpr；原本只有 QT-SRC 的 8 條補上 PySide 實測 |
| v0.2.2 | 01.2 定義行為基線（PySide6 6.11.2 實測）與版本衝突處置；要求必須原子化（11.1），03.12 拆成五節；DV-01／DV-04 改為依實際狀態登記（01.6 加狀態欄）；缺陷必須列出受影響的 R 與嚴重度理由（01.5）；RC 可關閉多個 G，但每個 G 各自要有證據（11.3）；00.6 拆開 qtrs 系統匣與 HUD 熱鍵；區分「要求定義已確認」「實作已符合」「整體驗收通過」（10.2、11.2）；11.6 缺陷（G）固定欄位五問 |
| v0.2.1 | 新增 00.6 支援範圍；01 依據綁定環境、衝突須最小重現、未決不得改實作；嚴重度限定在支援範圍內；01.6 成為唯一的刻意差異登記表，並分成兩類；E1–E5 改為證據**類型**，驗收矩陣寫明組合；整體驗收以要求集合定義；要求 ID 改為不含章節的 `R-#####`；08／09／12 的分工 |

## 大綱
| 章 | 標題 | 角色 | 遷移來源（舊 Contract） |
|---|---|---|---|
| 00 | 專案目標與範圍 | 定義目標 | §1.1、§1.2 |
| 01 | 相容性標準與規格依據 | 定義目標 | §0、§1.3、Severity |
| 02 | 核心物件模型與生命週期（含 Meta-object、屬性） | 行為契約 | §2、C6.6 |
| 03 | 事件系統與執行模型（含 signal／slot、invokeMethod、應用程式生命週期、執行緒） | 行為契約 | §3–§7（C6.6 除外） |
| 04 | Widget 狀態與使用者互動 | 行為契約 | C8.1、C8.2、C8.4 |
| 05 | 幾何、尺寸與 Layout | 行為契約 | C8.3、§9 |
| 06 | 繪製、Painter 與文字渲染 | 行為契約 | §10 |
| 07 | 樣式系統（QSS） | 行為契約 | C8.5、C12.3 |
| 08 | 視窗、Popup 與作業系統整合（通用平台 API 語意） | 行為契約 | C8.6、C8.8、§11 |
| 09 | 公開 API 索引 | **索引，不定義語意** | C12.2 |
| 10 | 測試、驗收與相容性矩陣 | 證明方法 | §0 分層驗證、C9.7、C12.6、C12.7、附錄 C |
| 11 | 規格追蹤與變更管理 | 證明方法 | 附錄 A、附錄 D |
| 12 | HUD 應用層契約 | 行為契約（HUD） | C7.9、C12.1、C12.4、C12.5、C12.8 |

### 章節分工（避免同一行為有兩個定義處）
- **02–08**：行為語意的唯一正式定義處。
- **09**：公開符號清單。每個符號只對應到要求 ID，並列出前置條件；不重寫語意。
- **08 對 12**：
  - 08 寫 `qtrs` 提供的通用語意，例如「`TrayIcon` 右鍵時在游標處彈出 context menu」。
  - 12 只寫 HUD 怎麼使用它，例如「HUD 托盤選單的項目與順序依 `tray_icon.py:43-151`」。

---

## 00. 專案目標與範圍

### 00.1 最終使用情境
- 產品是 Claude HUD 的 Rust 版（`rust/src`），建立在 `qtrs` 上，取代 Python／PySide6 版（`python/`）。
- 一致的範圍是使用者看得到的層面：像素、版面、互動、時序、持久化、錯誤處理。

### 00.2 兩個交付物，兩種相容目標
| 交付物 | 相容目標 | Oracle |
|---|---|---|
| HUD（`rust/src`） | 與 Python HUD 在使用者可觀察層面一致 | Python HUD 在 PySide6 6.11.2 上的實際行為 |
| `qtrs`（`rust/qtrs`） | 在 00.6 列為 Core 或 Extended 的範圍內，行為與 Qt 一致 | Qt 6.11 的可觀察行為（依據條件見 01.2） |

- 相容的單位是**行為**，不是 API 外觀。Rust API 的命名與型別可以不同，但只要有對應，可觀察結果就必須相同。

### 00.3 平台
- Windows 是唯一有真機驗證的平台。
- X11、Wayland、Cocoa 的程式碼存在，也在 CI 編譯與測試；但在真機驗證以前，不做行為通過宣稱（10.6）。

### 00.4 與 Python 版的關係
- Python 版是 HUD 層的 oracle，不是 `qtrs` 的 oracle。
- Python 程式碼若明確覆寫 Qt 預設，HUD 以 Python 為準。例如 `setQuitOnLastWindowClosed(False)`（`python/main.py:48`）。此時 `qtrs` 仍須提供 Qt 預設，由 HUD 自己設定。

### 00.5 範圍的變更
- 要把某項功能移入或移出 00.6 的分類，必須修改 00.6，並在修訂紀錄寫明理由。
- 不得在個別缺陷內，以「這不在範圍內」為由關閉缺陷。

### 00.6 支援範圍與功能矩陣
**分類**
- **Core**：構成相容基線，HUD 直接依賴。
- **Extended**：`qtrs` 提供、承諾與 Qt 一致，但 HUD 不依賴。
- **D**：刻意不支援；誤用時必須可見地失敗（錯誤、panic 或明確的回傳值）。
- **Out of Scope**：不在相容性承諾內；`qtrs` 也不提供對應 API。

分類描述的是「是否承諾支援」，與缺陷嚴重度（01.4）無關。

| 功能區 | 分類 | 說明 | 章 |
|---|---|---|---|
| QObject parent／child、所有權、延遲銷毀、存活追蹤 | Core | | 02 |
| 物件名稱查詢、`blockSignals` | Core | | 02 |
| 宣告屬性、動態屬性、notify | Extended | HUD 未使用 derive 屬性 | 02 |
| Meta-object 查詢、derive、`#[signal]`、`#[slot]` | Extended | | 02 |
| 字串式 `SIGNAL()`／`SLOT()`、`connectSlotsByName` | D | G6.5.d | 02 |
| 事件物件、post／send、filter、壓縮、優先序 | Core | | 03 |
| 事件迴圈：`exec`、`processEvents`、巢狀 loop、modal pump | Core | | 03 |
| 應用程式生命週期：`exit`、`quit`、`aboutToQuit`、最後一個視窗關閉 | Core | | 03 |
| 計時器：`QTimer`、`singleShot`、`startTimer`、零間隔 | Core | | 03 |
| signal／slot 閉包連線、順序、斷線、`sender()` | Core | | 03 |
| 連線類型 Auto／Direct／Queued／BlockingQueued；Unique／SingleShot | Extended | HUD 只用閉包 Direct | 03 |
| `invokeMethod`（含 queued、overload 解析） | Extended | | 03 |
| 執行緒親和性、跨執行緒 post、`moveToThread` | Core | HUD 的 worker 結果經 MetaCall 回到 UI 執行緒 | 03 |
| `QThread` 類 API、`ThreadPool`、`Future`／`FutureWatcher` | Extended | `terminate` 為 D（RC-121） | 03 |
| 可見性、Enabled、焦點、滑鼠與鍵盤遞送、Enter／Leave | Core | | 04 |
| min／max／fixed、size policy、box／grid／stacked、巢狀 layout、heightForWidth | Core | | 05 |
| `update`、繪製順序、裁剪、dirty region | Core | | 06 |
| `repaint`、`update(rect)`、`setUpdatesEnabled` | Extended | C10.2 | 06 |
| Painter：transform、clip、render hints、pixmap、path | Core | | 06 |
| 文字度量與字形渲染（DirectWrite、ClearType） | Core | | 06 |
| QSS：HUD 用到的選擇器、pseudo-state、box model、串接 | Core | | 07 |
| QSS：其他選擇器與屬性 | Extended | | 07 |
| Widget：Label、PushButton、Frame、ProgressBar、Menu、StackedWidget、ToolTip | Core | | 04–08 |
| Widget：ScrollBar、ScrollArea 及其他已提供的 widget | Extended | 逐項列在 09 | 09 |
| 視窗旗標、顯示／隱藏／關閉、最小化、幾何、螢幕、DPI | Core | Windows | 08 |
| 不透明度、click-through、Acrylic 背景、主題 | Core | Windows | 08 |
| 系統匣（`TrayIcon`）與其選單、通知 | Core | Windows | 08 |
| 全域熱鍵（HUD 的 Alt+C、Alt+Shift+C） | 不屬於 `qtrs`：HUD 契約 | HUD 以 Win32 `RegisterHotKey` 實作（`rust/src/hotkey.rs`）；`qtrs-platform` 的 `Win32HotkeyManager` 未被使用，也不承諾 | 12 |
| 剪貼簿 | Extended | G11.11.b | 08 |
| X11、Wayland、Cocoa 後端 | Extended | 只做編譯與 CI 測試宣稱（10.6） | 08 |
| QtNetwork、QtSql、QtQuick、Model/View、對話框、`QMainWindow`、`QGraphicsEffect`、`QPropertyAnimation` | Out of Scope | | — |

---

## 01. 相容性標準與規格依據

### 01.1 依據種類
| 標記 | 意義 | 可否單獨作為修改依據 |
|---|---|---|
| `[PYSIDE-RUN]` | PySide6 實測 | 可以（須帶 01.2 的環境欄位） |
| `[QT-SRC file:line]` | 對照 `qtbase/src` 原始碼 | 可以（版本見 01.2） |
| `[PY-SRC file:line]` | Python HUD 原始碼 | 可以（限 HUD 層） |
| `[QT-DOC]` | Qt 文件或既有知識，未逐行對照 | 不可以；必須先轉成上面三種之一 |
| `[INFERENCE]` | 推論 | 不可以 |
| Rust 目前行為 | 只描述現況 | 永遠不是正確性的依據 |

### 01.2 依據必須綁定環境
**目前的版本事實**（2026-10-12 查證）：
- `qtbase/` 是 `6.11` 分支、commit `44b6f67476c3`，`QT_REPO_MODULE_VERSION` 為 **6.11.3-alpha1**。
- PySide6 是 **6.11.2**（`qVersion()` = 6.11.2）。
- 兩者**不是同一版本**，而且本機沒有 `v6.11.2` tag，無法直接比對差異。

**行為基線**（「與 Qt 一致」指的是什麼）
- `qtrs` 承諾的是 **PySide6 6.11.2 所帶的 Qt 6.11.2 runtime** 的可觀察行為，在 01.2 規則 2 記錄的環境下實測。
- `qtbase` 原始碼（commit `44b6f67`）用來**解釋機制**、找出要測的路徑與邊界；它不是行為基線。
- 兩者結論不同時：該要求標為**未決**（01.4）。記錄最小重現、6.11.2 實測結果、原始碼路徑與推定原因，以及最後採用的行為與決定者。不得直接挑選比較方便的結果。
- 只有原始碼、沒有實測時：若該段程式在 6.11.2 與 `44b6f67` 之間可能不同，依據不足，先實測。要比對兩版原始碼，須另取 `v6.11.2` tag（本機沒有）。

**規則**
1. `[QT-SRC]` 預設指 commit `44b6f67`。若改引用其他版本，必須寫成 `[QT-SRC <commit 或 tag> file:line]`。
2. `[PYSIDE-RUN]` 必須記錄以下欄位：
   - PySide6／Qt 版本；
   - platform plugin（`windows` 或 `offscreen`）；
   - 作業系統版本；
   - DPI 或縮放比例；
   - 呼叫路徑：哪一個 API、以什麼順序呼叫；
   - 腳本或 fixture 的位置。
   - 範例：`[PYSIDE-RUN 6.11.2 offscreen Win11 dpr=1.0 tests/fixtures/layout_qt_oracle.txt]`。
3. `offscreen` 的結果只能支持與平台無關的行為，例如 layout 計算、meta-object、事件順序；**不能**支持視窗、DPI、字型光柵化、系統匣這類平台行為。
4. 涉及平台行為的 `[QT-SRC]`，必須引用該平台 plugin 的原始碼（例如 `plugins/platforms/windows/`），不得把 Windows 的結論推廣到其他平台。
5. 必要時要寫出編譯組態。例如 RC-109 引用的檢查在 Qt 中只存在於 debug build。

### 01.3 依據優先序
**HUD 層**：
1. Python HUD 的實測（截圖、幾何稽核、實際執行）。
2. `[PY-SRC]`。
3. 對應 `qtrs` 要求的依據。

**`qtrs` 層**：
1. 符合 01.2 條件的 `[PYSIDE-RUN]`。
2. `[QT-SRC]`。

### 01.4 衝突與未決
1. **實測與原始碼解讀不一致時**，先做兩件事，再下結論：
   - 建立最小重現案例；
   - 逐行追出原始碼裡實際執行的路徑。

   常見原因有：讀錯路徑、版本不同（01.2）、平台 plugin 不同、編譯組態不同。結論與原因都要記錄在該要求內。
   - 先例：RC-113 只讀平台層，判定「最小化不送 Show／Hide」；RC-140 追到 `qwidget.cpp:9292-9311` 後更正了這個結論。
   - 先例：RC-142。`indexOfMethod` 由後往前掃描（`qmetaobject.cpp:740`），而 `invokeMethod` 依名稱由前往後掃描（`:1693-1708`）。
2. **衝突尚未解決時**，把該要求標為**未決**。在解決以前，不得依據它修改實作，也不得把它宣稱為已確認。
3. **既有測試釘住了非 Qt 行為時**，改寫測試，不保留。
4. **safe Rust 無法表達 Qt 語意時**（例如同一物件的可重入 `&mut`），以「保持順序的延後送達」或「可見的拒絕」取代，並登記在 01.6 的 B 類。

### 01.5 嚴重度
嚴重度只適用於 00.6 中 **Core 或 Extended** 範圍內的缺陷。

| 級別 | 條件 |
|---|---|
| P0 | Core 範圍內、HUD 今天就走得到的路徑上：使用者看得到差異，或會靜默丟資料、卡死 |
| P1 | Core 或 Extended 範圍內：違反 Qt／PySide6 行為，但 HUD 目前沒有走到 |
| P2 | Core 或 Extended 範圍內的邊角情況 |
| D | 對應 00.6 的 D：必須寫出理由，誤用時必須可見地失敗 |

- 每個缺陷（G）必須列出：受影響的要求 ID（可以多條）、可重現的影響、嚴重度理由。嚴重度依「被破壞的要求」與實際影響中最嚴重者判定。
- 一個 G 可以關聯多條 R；一個根因可以造成多個 G。
- Out of Scope 的功能不登記缺陷。
- 若發現 Out of Scope 功能的需求，先依 00.5 調整範圍，再登記缺陷。

### 01.6 刻意差異登記表（唯一的正式登記處）
**A 類：內部實作差異。** 內部做法不同，但可觀察結果相同。
- 這類不算偏差，只登記「為什麼等價」以及驗證方式。
- 尚未驗證等價的項目，視為 B 類候選。

**B 類：已核准的行為差異。** 可觀察結果確實和 Qt 或 Python 不同。
- 必須寫明核准來源、理由與適用範圍。
- 未登記在這裡的可觀察差異，一律視為缺陷。

**狀態**：「已核准」才有效。「候選」是尚未證明等價、也尚未核准的差異，在處理完以前視同缺陷；「未決」是行為本身還沒有定義。

| ID | 類 | 狀態 | 差異 | 對照 | 理由 | 適用範圍 | 核准／依據 |
|---|---|---|---|---|---|---|---|
| DV-01 | B | 已核准（部分） | 單一實例：第二次啟動不開新視窗 | Python 無 | 附加能力，不改變單一視窗內的行為 | HUD | 舊 Contract §1.3。**喚醒第一個 HUD 之後的可觀察結果（顯示？置前？）未決**（G11.11.a），不在核准範圍內 |
| DV-02 | B | 已核准 | `--snapshot` 輸出 PNG | Python 無 | 測試用附加 CLI | HUD CLI | 舊 Contract §1.3 |
| DV-03 | B | 已核准 | 熱鍵字串可設定 | Python 固定 Alt+C／Alt+Shift+C | 附加能力；預設值相同 | HUD | 舊 Contract §1.3 |
| DV-04 | A | 已核准 | 版面切換：Rust 用 `StackedWidget`，Python 重建 `inner_layout` | `hud_window.py:268-320` | 外部可觀察行為符合契約（特定切換序列之標題高度、stack 高度、元件幾何與樣式經測試驗證符合預期） | HUD | G12.5.r / RC-202。以切換取代 layout 重組；結案範圍明確限縮於行為等價驗證，不宣稱全畫面像素比較與記憶體效益 |
| DV-05 | B | 已核准 | 啟動最初約 300 ms：Rust 已套用 click-through，Python 仍可互動 | `hud_window.py:106-108,426-437` | 要對齊必須改視窗建立旗標與托盤時序，風險大於收益 | HUD 啟動 | G12.5.h 的決定（舊 Contract C12.5） |
| DV-06 | B | 已核准 | 對正在被借用的 receiver 同步 `send_event`：回 false 並印診斷；Qt 會重入遞送 | `qcoreapplication.cpp:1563-1572` | safe Rust 不能別名 `&mut`（01.4 第 4 點） | `qtrs` 核心 | RC-123 |
| DV-07 | B | 已核准 | 跨執行緒 `send_event`：被拒並印診斷（所有 build）；Qt 仍會遞送，只在 debug build 警告 | `qcoreapplication.cpp:539-548,1223-1227` | 物件只能在擁有者執行緒借用 | `qtrs` 核心 | RC-109 |
| DV-08 | B | 候選 | parent 銷毀時，非 owned 的 child 不被銷毀，變成沒有 parent；Qt 刪除所有 children | `qobject.cpp:1177-1186` | parent 只擁有 `owned_children` 的 `Box`；其他 child 的記憶體由別人擁有，parent 無權釋放 | `qtrs` 核心（R-00008、G2.1.g） | 待核准。替代方案：拒絕把非 owned 物件設為 child（改為 D 類並可見地失敗）。決定前不得改這條路徑 |
| DV-09 | B | 候選 | 以自己或後代為 parent：`reparent_owned` 回 `Err(InvalidParent)`，樹不變；Qt release build 照做，形成自我 parent 或環 | `qobject.cpp:2290-2303` | 環狀物件樹在 Rust 會造成無限遞迴與 double free；Qt 自己也標為 undefined behavior | `qtrs` 核心（R-00048、G2.1.e） | 待核准。決定前不得改這條路徑 |

- 舊 Contract §1.3 與 C12.5 的「處置」欄自 v0.2.1 起只保留指向本表的指標，不再另存內容。
- C12.5 的 G 編號仍是缺陷清單，不是登記表。

---

## 02. 核心物件模型與生命週期（試遷移 v4）

> 來源：舊 Contract C2.1–C2.7、C3.5（DeferredDelete）、C6.6。遷移後舊段落改為指標。
> 定義狀態依 11.2；`[QT-DOC]` 一律為「依據不足」。`PYSIDE-RUN` 的腳本為 `rust/qtrs/tools/pyside_probes/spec02_qobject.py`（Windows，offscreen；每個函式印出一個 R-ID 的結果）。實作狀態沿用舊 Contract 的證據；標「舊 Contract 判定」者沒有重做修改前 FAIL。
> 02.9（widget tree 與 QObject tree 的關係）在舊 Contract 沒有對應的要求，本次不新增。
> v4（spec v0.2.3）：拆開複合要求，新增 R-00049–R-00073（R-00052 未配置，不再使用）；R-00048 改依候選差異 DV-09；R-00008 改依候選差異 DV-08；R-00037–R-00039 以 DV-06／DV-07 的有效契約描述。

| ID | 節 | 範圍 | 觸發 → 結果（→ 順序） | 依據 | 差異（DV）與 Qt 對照 | 舊 C | G | RC | 定義狀態 | 驗收方式與依賴 | 實作狀態與證據 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| R-00001 | 02.1 | Core | `set_parent(c, p)` **返回後** → `p` 的 children 含 `c`、`c` 的 parent 為 `p`、舊 parent 的 children 不再含 `c`（回呼期間的中間狀態見 R-00049、R-00050） | `[QT-SRC qobject.cpp:2287-2360]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00001]` | — | C2.1 | G2.1.f | — | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 部分符合：G2.1.f（跨 registry 鎖與物件借用，非原子）開放；既有 `test_ownership_and_deletion_cascade`（舊 Contract 判定） |
| R-00049 | 02.1 | Core | 改變 parent，舊 parent 的 `ChildRemoved` 回呼期間 → 舊 parent 的 children **已不含** `c`，但 `c` 的 parent **仍是舊 parent**，新 parent 的 children 也還不含 `c` | `[QT-SRC qobject.cpp:2322-2337]`（先 `removeAt`、送 `ChildRemoved`，之後才 `parent = o`）；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00049]` | — | C2.1 | G2.1.h | — | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`）；依賴 R-00002 | 不符（讀碼）：`reparent_owned` 在所有欄位更新完才送事件（`qobject.rs:894-899`），回呼看到的 parent 已是新 parent，且新 parent 的 children 已含 `c`；非 owned 的 `set_parent` 在送 `ChildRemoved` 前已更新 registry 的 parent（`qobject.rs:756-758`） |
| R-00050 | 02.1 | Core | 改變 parent，新 parent 的 `ChildAdded` 回呼期間 → `c` 的 parent 已是新 parent、新 parent 的 children 已含 `c`、舊 parent 的 children 不含 `c` | `[QT-SRC qobject.cpp:2337-2351]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00050]` | — | C2.1 | — | — | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`）；依賴 R-00002 | 待驗證：讀碼顯示兩條路徑都在送 `ChildAdded` 前完成更新，沒有測試 |
| R-00002 | 02.1 | Core | 改變 parent → 舊 parent 收到 `ChildRemoved`，新 parent 收到 `ChildAdded`，順序為先 Removed 後 Added（順序為試遷移時依 QT-SRC 新增） | `[QT-SRC qobject.cpp:2325, 2350]`（同步 `sendEvent`）；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00002]` | — | C2.1 | — | RC-01 | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 待驗證：`reparent_owned_notifies_both_parents_through_event_filters`（RC-01）涵蓋兩個事件，未斷言順序 |
| R-00058 | 02.1 | Core | 以 `reparent_owned` 改變 owned child 的 parent → `ChildRemoved`／`ChildAdded` 經 notify 遞送，object event filter 與 application event filter 都看得到 | `[QT-SRC qobject.cpp:2325, 2350]`（`QCoreApplication::sendEvent` → `notify`） | DV-06（parent 被借用時改為 post） | C2.1 | — | RC-01 | 已確認 | E2 | 待驗證：`reparent_owned_notifies_both_parents_through_event_filters`（RC-01；RC 小節沒有這個測試的修改前 FAIL 行號） |
| R-00062 | 02.1 | Core | 以 `set_parent` 改變非 owned child 的 parent → `ChildRemoved`／`ChildAdded` 經 notify 遞送，event filter 看得到 | `[QT-SRC qobject.cpp:2325, 2350]` | DV-06（parent 被借用時改為 post） | C2.1 | G2.1.c | RC-123 | 已確認 | E2 | 不符：`set_parent` 以 `dispatch_to_object` 直送（`qobject.rs:772-788`，G2.1.c） |
| R-00003 | 02.1 | Core | 以 parent 建立 child（`add_owned_child`）→ parent 收到 `ChildAdded` | `[QT-SRC qobject.cpp:976-1011, 2347-2352]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00003]` | — | C2.1 | G2.1.b | RC-81 | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 已符合：`test_owned_child_ownership.rs::add_owned_child_sends_child_added`（RC-81 修改前 FAIL） |
| R-00004 | 02.1 | Core | 解除 parent（目標為無）→ child 不被銷毀，所有權回到呼叫者 | `[QT-SRC qobject.cpp:2287-2345]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00004]` | — | C2.1 | G2.1.a | RC-01 | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 已符合：`test_owned_child_ownership.rs::set_parent_none_on_owned_child_keeps_child_alive`（RC-01 修改前 FAIL） |
| R-00005 | 02.1 | Core | 銷毀 parent（drop 或 `delete_later`）→ 先發出自己的 `destroyed`，再依 children 順序銷毀它擁有的子樹，最後才從自己的 parent 解除（祖父收到 `ChildRemoved`） | `[QT-SRC qobject.cpp:1079-1081, 1177-1186, 2277-2281]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00005、(f)]` | — | C2.1 | — | — | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`）；依賴 R-00014 | 待驗證：沒有測試；`destroyed` 本身不存在（R-00014） |
| R-00006 | 02.1 | Core | 銷毀 child → 從仍存活的 parent 的 children 移除，不留下失效的 id | `[QT-SRC qobject.cpp:2287-2360]` | — | C2.1 | — | — | 已確認 | E2 | 待驗證：`test_ownership_and_deletion_cascade`（舊 Contract 判定） |
| R-00063 | 02.1 | Core | parent 正在 callback 中（被借用）時銷毀 child → children 的移除與 `ChildRemoved` 延後到借用結束，不遺失 | DV-06（01.6）的替代語意；`[QT-SRC qobject.cpp:2322-2326]`（對照：Qt 同步移除並送事件） | DV-06；Qt 對照：同步移除並送 `ChildRemoved` | C2.1 | G2.1.d | RC-123 | 已確認 | E2；依 DV-06 的有效契約判定 | 已符合：`qobject.rs::test_child_removed_on_drop_while_parent_borrowed_is_deferred_not_lost`（RC-123，停用防護時 FAIL left 0 right 1） |
| R-00007 | 02.1 | Core | 以其他執行緒的物件為 parent → child 仍從舊 parent 移除，結果沒有 parent | `[QT-SRC qobject.cpp:2309-2328, 2341-2345]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00007 other-thread]` | — | C2.1 | G2.1.e | RC-01 | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 不符：`set_parent` 沒有執行緒檢查（G2.1.e） |
| R-00064 | 02.1 | Core | 以其他執行緒的物件為 parent → 印出警告「QObject::setParent: Cannot set parent, new parent is in a different thread」 | `[QT-SRC qobject.cpp:2341-2344]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00064]` | — | C2.1 | G2.1.e | — | 已確認 | E2（擷取診斷輸出）＋E3 | 不符：`set_parent` 沒有執行緒檢查 |
| R-00008 | 02.1 | Core | parent 銷毀時，已登記為它的 child、但不是它擁有（非 owned）的物件 → 不被銷毀，變成沒有 parent（`parent()` 為無） | DV-08（候選，01.6）；Qt 對照依據 `[QT-SRC qobject.cpp:1177-1186]` | DV-08（候選）；Qt 對照：parent 銷毀時刪除**所有** children | C2.1 | G2.1.g | — | 未決 | 定義決定前不驗收 | 待驗證：`unregister_qobject` 只清 registry 的 parent（`qobject.rs:351-355`），child 自己的 `ObjectData.parent` 是否同步未驗證（`[INFERENCE]`）。DV-08 核准前，不得改這條路徑的實作 |
| R-00042 | 02.1 | Core | 以目前的 parent 再次 `set_parent` → 不做任何事，不送事件 | `[QT-SRC qobject.cpp:2306-2307]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py (d)]` | — | C2.1 | — | — | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 待驗證 |
| R-00048 | 02.1 | Core | 以自己或自己的後代為 parent → `qtrs` 回 `Err(InvalidParent)`，物件樹不變 | DV-09（候選，01.6）；`[QT-SRC qobject.cpp:2290-2303]`（對照）；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00048]` | DV-09（候選）；Qt 對照：release build 照做 | C2.1 | G2.1.e | RC-01 | 未決 | 定義決定前不驗收 | 待驗證：`reparent_owned` 已拒絕（`qobject.rs:822-837`）；DV-09 核准前不得改實作 |
| R-00009 | 02.2 | Core | `delete_later` → 物件在控制權回到擁有它的事件迴圈時才銷毀；在此之前仍存活 | `[QT-SRC qobject.cpp:2519-2564]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00009]` | — | C2.2、C3.5 | G2.2.a、G3.5.c | RC-92 | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 已符合：`test_delete_later`、`test_a_deferred_delete_from_the_outer_loop_waits_out_a_nested_pump`（RC-92 改寫後的測試） |
| R-00065 | 02.2 | Core | widget 的 `delete_later` → widget 在 DeferredDelete 送達時才從 widget tree 移除；在此之前仍在樹中、仍可收事件 | `[QT-SRC qobject.cpp:2519-2564]`（QWidget 沿用 QObject 的 deleteLater） | — | C2.2 | G2.2.d | RC-92 | 已確認 | E2；依賴 R-00009 | 已符合：`test_reentrancy_and_deferred.rs::test_self_deletion_during_event_dispatch`（RC-92 修改前 FAIL `:82:5 left 0 right 1`） |
| R-00010 | 02.2 | Core | 同一 handler 內 `delete_later` 後呼叫 `process_events` → 不銷毀（scope level） | `[QT-SRC qobject.cpp:2519-2564; qcoreapplication.cpp:1870-1904]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00010]` | — | C3.5 | G3.5.a | RC-93 | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`）；依賴 R-00009 | 已符合：`loop.rs::test_delete_later_inside_handler_scope_survives_nested_pump`、`test_nested_pump_inside_a_handler_keeps_its_delete_later_target_alive`（RC-93） |
| R-00011 | 02.2 | Core | 最外層 `exec` 返回 → 尚未處理的 DeferredDelete 被 flush | `[QT-SRC qcoreapplication.cpp:1484-1489]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00011]` | — | C3.5 | G3.5.b | RC-94 | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`）；依賴 R-00009 | 已符合：`test_exec_cleanup_deferred_delete.rs::test_exec_flushes_deferred_delete_requested_by_last_handler`（RC-94） |
| R-00012 | 02.2 | Core | 對同一物件呼叫兩次 `delete_later` → 只銷毀一次 | `[QT-SRC qobject.cpp:2511-2534]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00012]` | — | C2.2 | — | — | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`）；依賴 R-00009 | 待驗證：已有 `test_delete_later_posts_one_deferred_delete_and_debounces`（RC-92），未記錄修改前 FAIL |
| R-00013 | 02.2 | Core | 物件銷毀 → 送給它、尚未處理的 posted events 從佇列移除，永不遞送 | `[QT-SRC qobject.cpp:195-207; qcoreapplication.cpp:1955-2002]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00013]` | — | C2.2、C3.6 | G2.2.b | RC-88 | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 已符合：`loop.rs::test_event_posted_to_unregistered_receiver_is_removed_from_queue`（RC-88 修改前 FAIL） |
| R-00051 | 02.2 | Core | 物件銷毀 → 以它為 receiver、尚未執行的 queued slot 呼叫被丟棄，永不執行 | `[QT-SRC qobject.cpp:195-207; qcoreapplication.cpp:1955-2002]`（queued call 是送給 receiver 的 posted `MetaCall`）；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00051]` | — | C2.2、C3.6 | G2.2.b、G6.4.b | RC-88 | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 已符合：`loop.rs::test_metacall_to_receiver_unregistered_before_delivery_is_dropped`、`test_signal_direct_and_queued.rs::test_queued_signal_to_unregistered_receiver_never_runs`（RC-88 修改前 FAIL） |
| R-00014 | 02.2 | Core | QObject 銷毀 → 發出 `destroyed(QObject*)` | `[QT-SRC qobject.cpp:1079-1081]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00014]` | — | C2.2 | G2.2.c | — | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 不符：沒有 `destroyed` 信號 |
| R-00066 | 02.2 | Core | QWidget 銷毀 → 在 `~QWidget` 裡、刪除 children 之前發出 `destroyed` | `[QT-SRC widgets/kernel/qwidget.cpp:1557-1561, 1581]`（先解除封鎖並發出 `destroyed`，之後才 `deleteChildren`）；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00066]` | — | C2.2 | G2.2.c | — | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`）；依賴 R-00014 | 不符：沒有 `destroyed` 信號 |
| R-00040 | 02.2 | Core | 物件銷毀 → 它作為 sender 的連線全部斷開，之後 emit 不再呼叫 slot | `[QT-SRC qobject.cpp:195-207]`（`removePostedEvents`、`unregisterTimers`）；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00040]` | — | C2.2、C3.6 | — | RC-92 | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 待驗證：`unregister_qobject` 呼叫 `disconnect_all_for_object`（`qobject.rs:362`）；舊 Contract 的必要測試 `deferred_delete_clears_queued_slots_timers_and_qpointer` 不存在 |
| R-00071 | 02.2 | Core | 物件銷毀 → 它作為 receiver（context）的連線全部斷開，之後別人 emit 不再呼叫它的 slot | `[QT-SRC qobject.cpp:1115-1135]`（`~QObject` 逐一斷開 senders）；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00071]` | — | C2.2、C3.6 | — | RC-92 | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 待驗證：`unregister_qobject` 依 `receiver_id` 斷開（`qobject.rs:362`），沒有專門測試 |
| R-00053 | 02.2 | Core | 物件銷毀 → 它的計時器全部停止，之後不再收到 timer 事件 | `[QT-SRC qobject.cpp:195-207]`（`unregisterTimers`）；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00053]` | — | C2.2、C3.6 | — | RC-92 | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 待驗證：`stop_timers_for_object` 清除 registry 項，沒有專門測試 |
| R-00072 | 02.2 | Core | Windows：物件銷毀 → 已向作業系統登記的計時器被釋放（`KillTimer`），不再產生 `WM_TIMER` | `[QT-SRC corelib/kernel/qeventdispatcher_win.cpp:752-775, 383-391]`（`unregisterTimers` → `KillTimer`） | — | C3.6 | G3.6.b | — | 已確認 | E2（以 Win32 API 驅動，Windows CI）＋E5（10.2 平台行為） | 不符（讀碼）：`stop_timers_for_object` 只刪 registry 項，不呼叫 `KillTimer`（G3.6.b） |
| R-00043 | 02.2 | Core | 在 slot 或事件 handler 內對自己呼叫 `delete_later` → slot 結束後物件仍存活，直到 DeferredDelete 被處理才銷毀 | `[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00043]` | — | C2.2 | — | RC-92 | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`）；依賴 R-00009 | 待驗證：已有 `test_self_deletion_during_event_dispatch`（RC-92） |
| R-00044 | 02.2 | Core | `send_posted_events(None, DeferredDelete)` → 送出同一層級的 DeferredDelete | `[QT-SRC qcoreapplication.cpp:1870-1904]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py (c)]` | — | C3.5 | — | — | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 待驗證 |
| R-00015 | 02.3 | Core | 未設定名稱的物件 → `objectName` 為空 | `[QT-SRC qobject.cpp:1315-1325]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00015]` | — | C2.3 | — | — | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 待驗證：Rust 回 `Option<&str>`，`None` 是否等價待確認 |
| R-00016 | 02.3 | Core | `set_object_name` → 發出 `objectNameChanged`（值改變時） | `[QT-SRC qobject.cpp:1363-1373]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00016]` | — | C2.3 | G2.3.d | RC-172 | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 已符合：ObjectData 持有 `object_name_changed` Signal，於名稱變更時發射新名稱（RC-172） |
| R-00017 | 02.3 | Core | 依型別找 child → 子類別也算符合 | `[QT-SRC qobject.cpp:2242-2256; qmetaobject.cpp:411-414]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00017]` | — | C2.3 | G2.3.a | — | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 不符：比對 exact `TypeId` |
| R-00018 | 02.3 | Core | 遞迴找 child → 每個節點先檢查它的全部直接 children，再依序遞迴進入每個 child；結果是第一個分支裡的符合者，不一定是最淺的 | `[QT-SRC qobject.cpp:2242-2256]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00018]` | — | C2.3 | G2.3.b | RC-171 | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 已符合：依 `qt_qFindChild_helper` 採兩階段搜尋，先掃同層直接 child 再遞迴（RC-171） |
| R-00019 | 02.3 | Core | 找 child → 涵蓋所有 children，不只 parent 擁有的 | `[QT-SRC qobject.cpp:2205-2216, 2242-2256]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00019]` | — | C2.3 | G2.3.c | — | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 不符：只搜 `owned_children` |
| R-00020 | 02.3 | Core | 找 child 時省略名稱 → 符合任何名稱（包含沒有名稱的物件） | `[QT-SRC qobject.cpp:2195-2200, 2211, 2246]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00020]` | — | C2.3 | — | — | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 待驗證 |
| R-00059 | 02.3 | Core | 找 child 時名稱為空字串 → 只符合沒有名稱的物件 | `[QT-SRC qobject.cpp:2195-2200, 2211, 2246]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00059]` | — | C2.3 | — | — | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 待驗證 |
| R-00021 | 02.3 | Core | 可查詢物件的 parent、children 與所屬執行緒 | `[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00021]` | — | C2.3 | G2.3.e | RC-174 | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 已符合：ObjectData 與 QObject 提供 parent()、children_ids() 與 thread() 取用器（RC-174） |
| R-00022 | 02.4 | Core | `block_signals(true)` → 該物件的**所有** signal（含繼承來的，例如 widget 提供的 `clicked`）都不發射，直到 `block_signals(false)` | `[QT-SRC qobject.cpp:1646-1652, 4273]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00022]` | — | C2.4 | G2.4.a | RC-98 | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 部分符合：`test_button_block.rs::button_clicked_honours_block_signals`（RC-98 修改前 FAIL）；RC-98 殘餘：qtrs-core／gui／platform 與 HUD 仍有無 owner 的 `Signal::new()`，不受封鎖 |
| R-00055 | 02.4 | Core | 物件**未註冊**（未呼叫 `register_qobject`）時 `block_signals(true)` → 它的 signal 同樣不發射 | `[QT-SRC qobject.cpp:1646-1652, 4273]`（封鎖旗標屬於物件本身，`doActivate` 直接讀；Qt 沒有註冊前提） | — | C2.4 | G2.4.b | RC-97 | 已確認 | E2 | 已符合：`test_signal_owner_binding.rs::unregistered_owner_block_suppresses_emit`、`signal_blocker_suppresses_unregistered_owner`（RC-97 修改前 FAIL） |
| R-00056 | 02.4 | Core | 物件仍存活但已解除註冊時 → 先前設定的封鎖仍有效；封鎖狀態只由 `block_signals` 改變 | `[QT-SRC qobject.cpp:1646-1652, 4273]`（旗標只在 `blockSignals` 與 `~QObject` 改變） | — | C2.4 | G2.4.c | RC-97 | 已確認 | E2；依賴 R-00055 | 部分符合：`test_signal_owner_binding.rs::block_survives_unregister`（RC-97 修改前 FAIL `:85 left 1 right 0`）；G2.4.c 仍開放，開放的部分是 derive signal（R-00041） |
| R-00057 | 02.4 | Core | 已封鎖的物件被銷毀 → 仍發出 `destroyed`（銷毀時先解除封鎖） | `[QT-SRC qobject.cpp:1050]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00057]` | — | C2.2、C2.4 | G2.2.c | — | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`）；依賴 R-00014 | 不符：沒有 `destroyed` 信號（依賴 R-00014） |
| R-00023 | 02.4 | Core | `block_signals(b)` → 回傳先前的狀態 | `[QT-SRC qobject.cpp:1646-1652]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00023]` | — | C2.4 | — | — | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 待驗證：沒有斷言回傳值的測試（`test_raii_signal_blocker` 驗的是 RAII 還原，見 R-00024） |
| R-00024 | 02.4 | Core | `SignalBlocker` 結束 → 還原**先前**的狀態（巢狀時還原外層的狀態，而不是 false） | `[QT-SRC qobject.h:479-560]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00024]` | — | C2.4 | — | — | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 待驗證：舊 Contract 列為必要測試（巢狀 blocker） |
| R-00041 | 02.4 | Extended | derive 產生的 `#[signal]` 欄位 → 自動綁定 owner，因此受 `block_signals` 控制 | （moc 產生的 signal 一律以 `this` 呼叫 `QMetaObject::activate`）；`[QT-SRC qobject.cpp:4273]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00041]` | — | C2.4、C6.6 | G2.4.c | RC-97 | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`）；依賴 R-00022 | 不符：須手動 `Signal::with_owner` |
| R-00025 | 02.5 | Core | 物件銷毀 → 指向它的弱參照變成 null，之後一直是 null | `[QT-SRC qobject.cpp:1066-1077; qpointer.h:19-86]`（PySide6 沒有 `QPointer`，無法實測） | — | C2.5 | — | — | 已確認 | E2 | 待驗證：`test_qpointer_and_generational_liveness`（沒有修改前 FAIL 紀錄） |
| R-00054 | 02.5 | Core | 物件存活期間 → 弱參照一直有效；qtrs 內部的重新註冊（例如先 `register_qobject` 再 `add_owned_child`）不得使它變成 null | `[QT-SRC qobject.cpp:1066-1077]`（弱參照只在 `~QObject` 清除） | — | C2.5 | G2.5.a | RC-11a | 已確認 | E2 | 待驗證：`test_double_register_keeps_object_reachable`（RC-11a；舊 Contract 判定已修復，RC 小節沒有修改前 FAIL 行號） |
| R-00026 | 02.5 | Core | 物件存活時 → 弱參照可以取用到物件本身 | `[QT-SRC qpointer.h:79-83]`（無法實測） | — | C2.5 | G2.5.b | — | 已確認 | E2 | 不符：只帶 `id()` |
| R-00027 | 02.6 | Extended | 對宣告且可寫的屬性 `set_property` → 寫入並回 true | `[QT-SRC qobject.cpp:4480-4500]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00027]` | — | C2.6 | G2.6.d | — | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 部分符合：沒有 `as_qobject_any` 時 meta 路徑靜默落到動態 map（G2.6.d） |
| R-00060 | 02.6 | Extended | 對宣告為唯讀的屬性 `set_property` → 回 false，值不變，也不建立同名動態屬性 | `[QT-SRC qobject.cpp:4480-4500]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00060]` | — | C2.6 | — | — | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 待驗證 |
| R-00028 | 02.6 | Extended | 對未宣告的名稱 `set_property` → 建立動態屬性並回 **false** | `[QT-SRC qobject.cpp:4450-4487]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00028]` | — | C2.6 | G2.6.a | RC-173 | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 已符合：對未宣告動態屬性寫入建立屬性並回傳 false（RC-173） |
| R-00029 | 02.6 | Extended | 動態屬性的值改變 → 送一次 `DynamicPropertyChange`；值不變時不送 | `[QT-SRC qobject.cpp:4467-4487]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00029]` | — | C2.6 | — | — | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 待驗證 |
| R-00030 | 02.6 | Extended | 既有的動態屬性設為 invalid → 移除該屬性、送一次 `DynamicPropertyChange`、回 false | `[QT-SRC qobject.cpp:4456-4461]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00030]` | — | C2.6 | G2.6.b | RC-173 | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 已符合：設定 Variant::Invalid 移除屬性、送出事件並回傳 false（RC-173） |
| R-00061 | 02.6 | Extended | 不存在的名稱設為 invalid → 回 false，不送事件，不建立屬性 | `[QT-SRC qobject.cpp:4456-4461]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00030]` | — | C2.6 | G2.6.b | RC-173 | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`）；依賴 R-00030 | 已符合：不存在的名稱設為 Variant::Invalid 回 false 且不觸發事件（RC-173） |
| R-00031 | 02.6 | Extended | 有 notify 的宣告屬性：值改變時發射一次 notify，值不變時不發射 | `[QT-SRC tools/moc/generator.cpp:1209-1226]` `[限制：此行為是 moc 為 MEMBER+NOTIFY 產生的碼，PySide6 無法觀察；tools/pyside_probes/spec02_property_slot.py R-00031 實測 PySide Property 不會自動發射 notify，內建 objectName 同值不發射只是手寫 setter 的旁證；來源仍僅為 GPL-only 的 generator.cpp]` | — | C2.6 | G2.6.c | RC-100 | 已確認 | E2 | 已符合：`test_signal_owner_binding.rs::derived_property_write_emits_notify_once_per_change`（RC-100 修改前 FAIL `:162`） |
| R-00045 | 02.6 | Extended | 宣告屬性 `set_property` 後 `property` 讀回相同的值 | `[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py (e)]` | — | C2.6 | — | — | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 待驗證 |
| R-00073 | 02.6 | Extended | 動態屬性 `set_property` 後 `property` 讀回相同的值；不存在的名稱讀回 invalid | `[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py (e)]` | — | C2.6 | — | — | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 待驗證 |
| R-00032 | 02.7 | Extended | meta-object 的 superclass 鏈反映實際宣告的父類別；`inherits` 依此判定 | `[QT-SRC qmetaobject.cpp:387-395]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00032]` | — | C6.6 | G6.6.b | — | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 不符：derive 一律以 `QOBJECT_META_OBJECT` 為 superclass |
| R-00033 | 02.7 | Extended | `#[slot]` 方法登錄到 meta-object，可依名稱或簽章查詢與 invoke | `[QT-SRC corelib/kernel/qmetaobject.cpp:2774-2809]` `[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_property_slot.py R-00033]` | — | C6.6 | G6.6.a | RC-142 | 已確認 | E2 | 已符合：`test_meta_slots.rs`（7 項，RC-142 停用登錄時 FAIL） |
| R-00034 | 02.7 | Extended | signal 與 property 的型別名稱是 Qt 名稱（例如 `QString`） | `[QT-SRC qmetaobject.cpp:2180-2185, 3793]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00034]` | — | C6.6 | G6.6.c | — | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 不符：使用 Rust 名稱 |
| R-00035 | 02.7 | Extended | property 的 notify signal 可解析成 signal 索引 | `[QT-SRC qmetaobject.cpp:4270-4292]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00035]` | — | C6.6 | G6.6.d | — | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 不符 |
| R-00036 | 02.7 | Extended | meta-object 提供 enums，計數含父類別的項目 | `[QT-SRC qmetaobject.cpp:599-606]`；`[PYSIDE-RUN 6.11.2 offscreen Win11-26200 dpr=1.0 tools/pyside_probes/spec02_qobject.py R-00036]` | — | C6.6 | G6.6.b | — | 已確認 | E2＋E3（期望值取自 `spec02_qobject.py`） | 不符：皆為空 |
| R-00067 | 02.7 | Extended | meta-object 提供 class info，計數含父類別的項目 | `[QT-SRC qmetaobject.cpp:637-644]`（`classInfoCount` 加總 superdata）。PySide 探針結果不確定（基底類別的 class info 未被登記，`spec02_qobject.py R-00067`），不能作為依據 | — | C6.6 | G6.6.b | — | 依據不足 | 先補依據（C++ 或修正後的 PySide 探針） | 待驗證：class info 為空 |
| R-00046 | 02.7 | Extended | meta-object 的 `class_name` 回傳宣告的類別名稱 | `[QT-DOC]` | — | C6.6 | — | — | 依據不足 | 先補依據（PySide 實測或 QT-SRC） | 待驗證：既有 `test_meta_object_hierarchy_and_inherits`（舊 Contract 判定） |
| R-00047 | 02.7 | Extended | 方法、signal、property 的索引在程式執行期間穩定，依宣告順序 | `[QT-DOC]` | — | C6.6 | — | — | 依據不足 | 先補依據（PySide 實測或 QT-SRC） | 待驗證 |
| R-00037 | 02.8 | Core | 對正在處理事件（被借用中）的物件同步 `send_event` → 回 false，事件不遞送 | DV-06（01.6）；`[QT-SRC qcoreapplication.cpp:1563-1572]`（對照） | DV-06；Qt 對照：重入遞送（`qcoreapplication.cpp:1563-1572`） | C2.7 | G2.7.a、G2.7.c | RC-123 | 已確認 | E2；依 DV-06 的有效契約判定 | 部分符合：`qobject.rs::test_sync_send_to_borrowed_receiver_is_rejected_visibly`（RC-123，停用防護時 FAIL）；差異登記為 DV-06；G2.7.c（測試標題下無測試）待清理 |
| R-00068 | 02.8 | Core | 對被借用中的物件同步 `send_event` 被拒時 → 印出診斷訊息（可見地失敗，不靜默） | DV-06（01.6） | DV-06；Qt 對照：沒有這種失敗 | C2.7 | G2.7.a | RC-123 | 已確認 | E2（擷取診斷輸出）；依 DV-06 的有效契約判定 | 待驗證：沒有斷言診斷輸出的測試 |
| R-00038 | 02.8 | Core | 對正被借用的 receiver post 的事件 → 不遺失，借用結束後送達 | DV-06（01.6）的替代語意；`[QT-SRC qcoreapplication.cpp:1955-2002]`（posted 事件依序送達） | DV-06；Qt 對照：無借用限制，post 的事件本來就之後送達 | C2.7 | G2.7.a | RC-123 | 已確認 | E2；依 DV-06 的有效契約判定；依賴 R-00037 | 已符合：`qobject.rs::test_child_added_to_borrowed_parent_is_deferred_not_lost`（RC-123 修改前 FAIL）；`loop.rs::metacall_to_registered_borrowed_receiver_is_deferred_not_run_on_stub` |
| R-00070 | 02.8 | Core | 對正被借用的 receiver 延後的多個事件 → 借用結束後依原 post 順序送達 | `[QT-SRC qcoreapplication.cpp:1955-2002]`（posted 事件依序送達）；DV-06 | DV-06 | C2.7 | G2.7.a | RC-123 | 已確認 | E2；依 DV-06 的有效契約判定；依賴 R-00038 | 待驗證：既有測試沒有斷言順序 |
| R-00039 | 02.8 | Core | 從非擁有者執行緒同步派送（`send_event`）→ 回 false，不在錯誤的執行緒執行擁有者的 callback | DV-07（01.6）；`[QT-SRC qcoreapplication.cpp:539-548, 1223-1227]`（對照） | DV-07；Qt 對照：照常遞送，只在 debug build 警告（`qcoreapplication.cpp:539-548, 1223-1227`） | C2.7、C7.4 | G2.7.b | RC-109 | 已確認 | E2；依 DV-07 的有效契約判定 | 待驗證：`send_event_from_foreign_thread_is_rejected_without_running_the_owner_callback` 是防護（停用檢查時仍通過），不是修改前 FAIL；診斷訊息未被斷言 |
| R-00069 | 02.8 | Core | 從非擁有者執行緒同步派送被拒時 → 印出診斷訊息 | DV-07（01.6） | DV-07；Qt 對照：只在 debug build 警告 | C2.7、C7.4 | G2.7.b | RC-109 | 已確認 | E2（擷取診斷輸出）；依 DV-07 的有效契約判定 | 待驗證：診斷訊息未被斷言 |

> 未遷移為要求的舊內容：C2.3「可取得可變存取」屬 API 形狀，列入 09 索引；舊 Contract 中寫成必要測試、但 repo 內不存在的測試名稱，已在對應列的實作狀態註明。

**實作註記**（不具規範效力）
- 02.1 所有權：core 的擁有者是 `ObjectData.owned_children: Vec<Box<dyn QObject>>`；owned child 的 `set_parent` 回 `Err(OwnedByParent)`，須改用 `reparent_owned(child_id, new_parent)`（RC-01）。
- 02.8：每物件 `borrow_flag`；同步派送遇到借用中的 receiver 時回 false，posted 事件留在佇列（RC-123）。
- 02.9：core 的 `ObjectData` 與 widgets 的 `WidgetRef`（`Rc<RefCell<Box<dyn Widget>>>`）是兩套樹；只要不影響對外行為，本書不規定採用哪一套。

## 03. 事件系統與執行模型（骨架）

| 節 | 要求範圍 | 範圍分類 | 遷移來源 | 相關 RC |
|---|---|---|---|---|
| 03.1 | 事件物件：type 值、accept／ignore、spontaneous | Core | C3.1 | RC-129 |
| 03.2 | `postEvent`：順序、優先序、輪次邊界 | Core | C3.2、C4.3 | RC-68 |
| 03.3 | 事件壓縮 | Core | C3.3 | — |
| 03.4 | `sendEvent`／`notify`／event filter 的順序 | Core | C3.4 | RC-129 |
| 03.5 | 接收者銷毀時的 posted events | Core | C3.6 | RC-123 |
| 03.6 | 事件迴圈：`exec`、`processEvents`、巢狀 loop、喚醒、modal pump | Core | C4.2、C4.3 | RC-104、RC-105 |
| 03.7 | 應用程式生命週期：`exit`、`quit`、`aboutToQuit`、最後一個視窗關閉 | Core | C4.1、C11.2 | RC-106、RC-113、RC-115、RC-141 |
| 03.8 | 計時器 | Core | §5 | RC-84、RC-118 |
| 03.9 | signal／slot：順序、connect／disconnect、`sender()`、自動斷線 | Core | C6.1、C6.3、C6.4 | RC-02、RC-119 |
| 03.10 | 連線類型與執行緒解析 | Extended | C6.2、C7.3 | RC-108 |
| 03.11 | `invokeMethod`：解析、型別檢查、連線類型 | Extended | C6.5 | RC-120、RC-142 |
| 03.12 | 執行緒身分與物件的 thread affinity | Core | C7.1、C7.4 | RC-82、RC-109 |
| 03.13 | 跨執行緒 post 與喚醒 | Core | C7.2 | — |
| 03.14 | `moveToThread`：前置條件與結果 | Core | C7.5 | RC-107 |
| 03.15 | `QThread` 類 API 的生命週期 | Extended | C7.6、C7.7 | RC-121 |
| 03.16 | `ThreadPool`、Future／Watcher 的完成通知 | Extended | C7.8 | RC-131 |
| 03.17 | panic 邊界：callback 失敗後 loop 的狀態 | Core | G12.5.v | RC-128 |

**固定原則**
- 節是分類，不是驗收單位。一節之內可以有多條要求，每條各自有 ID、範圍分類與狀態。
- 每條要求都寫成「**觸發 → 可觀察結果 → 順序**」三段。
- 內部機制（例如 `WidgetCommandQueue`、`insertion_offset`、`QUIT_GATE`）只寫在「實作註記」小段。實作註記不具規範效力，重構時可以直接改寫。

## 04–08（只有標題）
- **04 Widget 狀態與使用者互動**：可見性、Enabled、焦點、滑鼠與鍵盤遞送、狀態變更事件、parent 對 child 的影響。
- **05 幾何、尺寸與 Layout**：min／max／fixed、size policy、size hint、box／grid／stacked、巢狀 layout、預設 margin 與 spacing、heightForWidth、失效與重算、頂層最小尺寸。
- **06 繪製、Painter 與文字渲染**：`update` 與 `repaint`、dirty region、裁剪、繪製順序、Painter 狀態與 render hints、文字度量與字形、DPR 與 backing store。
- **07 樣式系統（QSS）**：選擇器、型別繼承、pseudo-state、串接與優先序、polish 時機、box model。
- **08 視窗、Popup 與作業系統整合**：建立旗標、顯示／隱藏／關閉、最小化、幾何、螢幕與 DPI、不透明度、click-through、背景效果、tooltip、選單與 popup、系統匣、熱鍵、主題、單一實例、macOS AppKit。

## 09. 公開 API 索引（只有標題）
- 每列：符號 → 範圍分類（00.6）→ 要求 ID → 前置條件 → 無效輸入時的結果。
- 不重寫語意；語意以 02–08 為準。

---

## 10. 測試、驗收與相容性矩陣

### 10.1 證據類型（不是單一階梯）
| 類型 | 內容 | 能證明什麼 | 不能證明什麼 |
|---|---|---|---|
| E1 讀碼 | 只讀函式本體 | 疑似缺陷，或解釋現象的成因 | 任何「已修復」或「一致」 |
| E2 針對性測試 | 永久測試：修改前 FAIL、修改後 PASS，記錄確切行號 | 該測試輸入下，該要求成立 | 其他輸入、真實平台 |
| E3 Oracle 對照 | 與 PySide6 逐項比對（環境依 01.2） | 在 oracle 涵蓋的輸入空間內一致 | 涵蓋範圍以外的輸入；offscreen 無法代表的平台行為 |
| E4 整合矩陣 | workspace 與 HUD 全套測試、clippy、fmt、CI 三平台 | 指定 commit 通過列明的驗證矩陣，結果與已知例外已完整記錄 | 測試沒有涵蓋的行為沒有回歸 |
| E5 真實環境 | Windows 真實視窗、截圖差分或手動操作（條件依 10.5） | 在記錄的環境下，使用者看得到的結果一致 | 其他環境 |

### 10.2 驗收矩陣：宣稱需要哪些證據
三種宣稱分開，不得互相代替：
- **要求定義已確認**：這條要求描述的行為有足夠依據（01.2、01.3），沒有未決衝突。這只說明「Qt 應該怎麼做」，與 Rust 是否做到無關。
- **實作已符合**：Rust 對這條要求的實作通過下表規定的證據。
- **整體驗收通過**：10.3。

| 宣稱 | 必要證據組合 |
|---|---|
| 要求定義已確認 | 符合 01.2 的 `[PYSIDE-RUN]`；或 `[QT-SRC]`（且依 01.2 不需實測） |
| 實作已符合：與平台無關的語意（例如事件順序、meta-object） | 要求定義已確認＋E2；行為由實測定義時另加 E3 |
| 實作已符合：平台行為（例如視窗、tooltip、DPI） | 要求定義已確認＋E2（以平台 API 驅動的測試）＋E5 |
| 某個 RC 完成 | 它關聯的每個 G 各自的關閉證據（11.3）＋E4 |
| layout 與 Qt 一致 | E3：layout oracle 全數無差異，記錄 commit、案例數、seed |
| 文字渲染一致 | E3（文字 harness）＋E5（10.5 的環境） |
| 像素一致 | E5：在 10.5 的環境下逐像素差分，列出差異像素數與平均絕對差 |
| 涉及像素、但差分不為零的回報 | 結尾標註 **MANUAL WINDOWS VERIFICATION REQUIRED** |

### 10.3 整體驗收（例如「HUD 行為一致」「qtrs Core 相容」）
1. **先定義本次驗收的要求集合**。例如：
   - 「HUD 行為一致」= 12 章的全部要求，加上 HUD 依賴的所有 Core 要求；
   - 「qtrs Core 相容」= 00.6 中 Core 範圍的全部要求。
2. **滿足以下全部條件，才能宣稱通過**：
   - 集合內每一條要求的定義已確認，而且實作已符合（10.2）；
   - 沒有未登記在 01.6 的差異；
   - 沒有開放中的 P0；
   - 沒有缺少證據的「完成」宣稱；
   - 標為未決（01.4）的要求必須逐條列出，不得計入「通過」。

### 10.4 測試規則
- **修改前 FAIL**：暫時停用修正，記錄 `檔案:行 left/right`，再精確還原。修改前就會通過的測試，只算「釘住」（pin），不算證據。
- **永久測試的門檻**：必須能抓到使用者看得到的錯誤（行為、邊界、不變條件、狀態轉換、優先序、錯誤處理）。不收只驗接線、轉傳、預設值或原始碼文字的測試。
- **Oracle 工具**：
  - layout：`qtrs-widgets/tests/layout_qt_oracle.rs`，fixture 317 案例，seed 20260502，`[PYSIDE-RUN 6.11.2 offscreen]`。
  - 文字：`qt_advance_compare.py`、`qt_glyph_compare.py`、`test_lcd_text_parity.rs`。
  - 幾何：`rust/tools/geometry_audit/`。
- **間歇性失敗**：
  - 先單獨重跑失敗項，再在修改前的 commit 上重跑，判斷是否與本次修改有關，並記錄結果。不得為了刷綠而反覆重跑整套。
  - 已知不穩定的測試：`test_layered_interactive_resize`、`test_system_move_release`、agy live benchmark（需要網路）、real-window tooltip。新增項目時要附重現條件。

### 10.5 E5 的環境條件（每次 E5 都要記錄）
| 欄位 | 內容 |
|---|---|
| 作業系統 | Windows 版本與 build |
| 縮放 | 100／125／150／200%；多螢幕時記錄每個螢幕的縮放 |
| 字型 | 系統字型、ClearType 開或關 |
| 主題 | 淺色或深色、`AppsUseLightTheme`、Acrylic 是否成功 |
| 視窗 | HUD 版面模式、視窗大小、位置 |
| 資料 | provider 資料來源（固定 fixture 或停用 fetch）；時鐘文字的處理方式（遮罩或固定） |
| 擷取 | 擷取範圍、方法（`--snapshot` 或桌面擷取） |
| 差分規則 | 逐像素比較；記錄差異像素數、平均絕對差、遮罩區域 |

### 10.6 平台矩陣
| 平台 | 編譯／CI | 測試 | 真機 | 可宣稱 |
|---|---|---|---|---|
| Windows | CI | CI＋本機 | 部分（逐項記錄 E5） | 依 10.2 |
| Linux（X11／Wayland） | CI | CI | 無 | 只能宣稱「可編譯、指定測試通過」 |
| macOS（Cocoa） | CI | CI＋AppKit 主執行緒檢查（C11.13） | 無 | 同上；`--known-open` 列出的檢查不算通過 |

### 10.7 驗證指令與流程
沿用舊 Contract §0「驗證指令」與「分層驗證流程」（每個 RC → 每批 → push → CI）。遷移本章時，把指令搬到這一節，舊 Contract 改為指標。

---

## 11. 規格追蹤與變更管理

### 11.1 ID：身分與位置分開
| ID | 代表 | 格式 | 規則 |
|---|---|---|---|
| 要求 ID | 一條行為要求 | `R-#####`（例如 `R-00142`），依建立順序編號 | 永不重用、永不重新編號；廢止時保留並註記 |
| 缺陷 ID | 實作與要求之間的差距 | 沿用 `G<章>.<項>.<字母>` | 不改；既有 commit 與測試都引用它 |
| RC ID | 一個根因、一次修復 | 沿用 `RC-<n>` | 不改 |
| 差異 ID | 刻意差異 | `DV-##` | 見 01.6 |

**原子化**：一條要求只描述一個行為主題，只有一個範圍分類，可以獨立判定狀態。若一條要求需要「部分完成」才能描述，就表示它應該拆開。不同的失敗模式（例如「未註冊」與「解除註冊後」）各自成一條。

**回呼期間的狀態**：要求若涉及同步回呼（事件、signal），要分開寫「API 返回後的狀態」與「回呼執行期間看得到的狀態」；後者以 PySide 實測為準，不得假設原子性。

每條要求固定帶以下欄位。章節只是欄位，不是身分的一部分：

| 欄位 | 範例 |
|---|---|
| ID | `R-00142` |
| 章節 | `03.11` |
| 範圍分類 | Extended |
| 觸發 → 結果 → 順序 | — |
| 依據 | `[QT-SRC qmetaobject.cpp:1693-1708]` |
| 差異（DV）與 Qt 對照 | 適用 01.6 的差異時填 DV ID 與 Qt 的對照行為；否則 `—` |
| 舊 Contract | `C6.5` |
| 缺陷 | `G6.5.c` |
| RC | `RC-120`、`RC-142` |
| 定義狀態 | 已確認：`[QT-SRC qmetaobject.cpp:1693-1708]` |
| 驗收方式與依賴 | `E2＋E3`；依賴 `R-00014` |
| 實作狀態與證據 | 已符合：E2（`test_meta_invoke_overload_signal.rs:119`）＋E4（CI run id） |

### 11.2 要求狀態（兩個獨立欄位）
**定義狀態**（規格本身）
| 狀態 | 條件 |
|---|---|
| 已確認 | 依據符合 01.2、01.3，沒有未決衝突 |
| 依據不足 | 只有 `[QT-DOC]`、`[INFERENCE]`，或依 01.2 需要實測但還沒測 |
| 未決 | 依據之間有衝突尚未解決（01.4）；不得據此改實作 |

**實作狀態**（Rust）
| 狀態 | 條件 | 對應舊標記 |
|---|---|---|
| 已符合 | 定義已確認，且達到 10.2 的證據組合，沒有開放的 G | `IMPLEMENTED` |
| 部分符合 | 有開放的 G，或有已登記的殘餘 | `PARTIAL` |
| 待驗證 | 有實作，但只有 E1，或沒有會失敗的測試 | `IMPLEMENTED-UNTESTED`、`READ` |
| 不符 | 已重現或讀碼確認行為錯誤 | `BROKEN`、`ABSENT` |
| 未支援 | 00.6 的 D，必須可見地失敗 | `D` |

定義狀態不是「已確認」時，實作狀態最多只能是「待驗證」。

**已核准差異（01.6 B 類）**：
- 要求的「觸發 → 結果」寫 `qtrs` 承諾的結果（有效契約）；Qt 的行為寫在「差異（DV）與 Qt 對照」欄。
- 實作狀態依有效契約判定，不依原始 Qt 行為判定。
- 依據欄同時列 DV ID 與 Qt 對照的出處；DV 是「候選」時，定義狀態為「未決」。
- `D` 只用於 00.6 中刻意不提供的功能；Core／Extended 要求不得標 `D`。

### 11.3 RC 規則
- 一個 RC 對應一個根因。一個根因可以解釋並修復多個 G，但 RC 必須逐一列出關聯的 G，每個 G 各自提供符合 10.2 的關閉證據。不得只因為根因修好了，就自動關閉所有相關的 G。
- RC 內要重新檢查該根因對應的**全部** G；沒有證據的 G 保持開放。
- RC 小節的固定欄位：依據（含 01.2 的環境）、根因、修正、驗證（修改前 FAIL 與修改後 PASS 的確切行號）、HUD 影響、殘餘。
- 只修了一部分：保留剩餘部分的 G，不得整條關閉。

### 11.4 變更影響分析
- 修改公開符號以前，先用 LSP 查所有參照。
- 修改核心路徑（`WidgetBase`、焦點、事件派送、生命週期）的 RC，要提早跑 E4，不等批次。
- **要求本身的內容改變時**（不是實作改變）：保留舊內容並註記理由，列出受影響的 G、RC、測試與 DV；被影響的測試在同一次變更內改寫。
- **範圍分類改變時**：依 00.5 處理。

### 11.5 紀錄
- 每批：commit hash、CI run id、三平台結果、annotation 數（目標 0 個 warning）。
- 疑慮與未決事項：集中記錄。處理時寫回對應的 R、G、RC 或 DV。

### 11.6 缺陷（G）的固定欄位
每個 G 都要能回答以下問題；回答不了的 G 不得進入修復：
1. 違反哪些正式要求（R）？
2. 正確行為由什麼依據支持（01.2）？
3. 如何重現，哪個測試抓得到？
4. 修復後要跑哪些驗收（10.2）？
5. 還有哪些相關要求或殘餘，不能一併宣稱完成？

### 11.7 遷移流程
1. **盤點**：`QTRS_SPEC_MIGRATION.md` 列出每個舊 C 條目的目標章節、G、RC、目前狀態與遷移狀態。只盤點，不改內容。
2. **試遷移**：先遷移 02 章。每條要求都要有 ID、觸發、可觀察結果、順序、依據與驗收方式。完成後檢查三件事：
   - 舊 Contract 的重複描述是否消除；
   - G 與 RC 的關聯是否保留；
   - 每條要求是否有明確的驗收方式。
   **就緒條件**（一條要求可以開始改實作，必須全部滿足）：
   - 只有一個行為主題（11.1）；
   - 定義狀態為「已確認」；
   - 「驗收方式與依賴」寫明證據類型（10.1），以及是否需要 PySide oracle（E3）；
   - 依賴的其他要求（事件、焦點、layout、繪製等）已列出；
   - 舊 Contract、G、RC、測試、DV 的關係可追溯，沒有在遷移中遺失的未結案問題。
   未決或依據不足的要求，只阻擋**依賴它的**實作與驗收；與它無關的工作照常進行。
3. **雙向盤點**：
   - 舊 → 新：舊 Contract 的每條要求都已遷移成 R、明確廢止，或改為指標；每個 G 都對應到至少一條 R。
   - 新 → 舊：新章沒有新增未經決策的行為要求；新增的要求必須有依據，並在遷移紀錄中標為「新增」。
4. **其餘各章**：流程確認可用後，各章可以平行遷移。每章完成時，同一次變更就把舊 Contract 的對應段落改成指標。

---

## 12. HUD 應用層契約（只有標題）

refresh controller 與 worker 邊界、持久化與設定、診斷 log、`--smoke-test`、托盤與選單的項目與順序、幾何稽核。來源：C7.9、C12.1、C12.4、C12.5、C12.8。
