# PYTHON_QT_SEMANTIC_CONTRACT

`qtrs` 與 Python/PySide6 Claude HUD 之間的 **可觀察行為契約**。

- 基準：PySide6 / Qt **6.11.2**，Windows。Qt 原始碼在 repo 根目錄 `qtbase/`（與 `rust/` 同層的 `qtbase/src/...`）。
- 稽核對象：commit `701d36b`（`rust/qtrs` 五個 crate + `rust/src` 應用層）。檔案行號會漂移；以符號名稱為準，行號僅作定位。
- 本文件 **只定義行為**，不規定實作。`qtrs` 的實作方式可以和 Qt 完全不同，但 §2–§11 每一項的「qtrs required」在可觀察層面必須成立。

---

## 0. 給 Agent 的規則（先讀這一節）

1. **不得自行決定 semantic。** 實作依本文件；本文件沒寫的行為，先補一條（Qt behavior + 證據），再寫程式。
2. **「HUD 目前沒用到 / 沒測到」不是「這個行為不重要」。** 每一項 gap 的嚴重度與「HUD 是否使用」分開記錄（見 Severity）。不得因為 HUD 沒用到就刪除、降級或不寫 gap。
3. **不得把 gap 改寫成 by design。** 要標 `D`（unsupported by design）必須在該項寫出理由，並且 `qtrs` 在被誤用時要 **可見地失敗**（錯誤、panic 或明確回傳值），不得靜默丟棄。
4. **測試釘住了非 Qt 行為，或根本沒有斷言 → 必須改寫測試，不得保留。** 已知案例：`test_application_layers.rs`（`exit()` 先於 `exec()` 被釘成「立刻返回」，見 C4.1）、`test_signal_sender_tracking_and_auto_disconnection`（drop 之後沒有任何 assertion，見 C6.4）、`test_zero_timer_immediate_dispatch`（不檢查 registry 是否殘留，見 C5.3）、`test_qobject_start_and_kill_timer`（只斷言記帳，不斷言觸發，見 C5.4）。
5. **修好一個 gap 的條件** = 該項「Required test」先在修改前失敗、修改後通過，且寫進 repo。只有「能編譯」或「舊測試仍過」不算。
6. 同一個 gap 若要縮小範圍（只修一部分）→ 必須在該項保留剩餘部分的 gap 條目，不得整條關閉。
7. **同一個 root cause 在多處登錄時，以附錄 D 的 RC 為修復單位**：一次變更只能宣稱修復一個 RC，並須重新檢查該 RC 對應的**所有** gap；不得把同一處修改分別宣稱為修了 N 個 gap。

### 證據與狀態標記

| 標記 | 意義 |
|---|---|
| `[QT-SRC file:line]` | 已對照 repo 內 `qtbase/` 原始碼確認 |
| `[QT-DOC]` | Qt 文件／既有知識，**未**逐行對照原始碼 |
| `[INFERENCE]` | 推論，未驗證。實作者在依賴它之前必須先對照 `qtbase/` 確認 |
| `RAN` | 本次稽核在 Windows 實際執行程式碼重現（見附錄 C） |
| `READ` | 只讀函式本體得出，**未執行**。實作前先寫失敗測試確認 |
| `DIFF` | 與真實 PySide6 輸出逐項比對（harness） |

實作狀態：`IMPLEMENTED`（有程式且有「壞掉會失敗」的測試）／`IMPLEMENTED-UNTESTED`／`PARTIAL`／`BROKEN`（有程式但行為錯，已重現或 READ）／`ABSENT`（grep 為證）。

### Severity

| 級別 | 意義 |
|---|---|
| **P0** | Claude HUD **今天就能觀察到**與 PySide6 不同，或會靜默丟資料／卡死 |
| **P1** | 違反 PySide6 行為，但 HUD 目前沒走到那條路徑 |
| **P2** | 邊角情況 |
| **D** | 設計上不支援（須有理由，且誤用時要可見失敗） |

Gap 編號 `G<章>.<項>.<字母>`，例如 `G6.1.b`。附錄 A 有總表。

### 驗證指令

```
# qtrs workspace（在 rust/qtrs 執行）
cargo test -j 1 --workspace -- --test-threads=1
# 應用層（在 rust 執行）。唯一預期失敗：providers::agy::tests::test_fetch_usage_live_benchmark（需網路）
cargo test -j 1 -- --test-threads=1
# 與真實 PySide6 比對 layout（在 rust/qtrs 執行；需要 PySide6）
python tools/second_layer_harness/qt_layout_compare.py 7500 1
```

一律 `-j 1`、一次一個 cargo。`QT_QPA_PLATFORM=offscreen` 不可用。

---

## 1. Scope

### 1.1 範圍內

- 目標：**Rust HUD（`rust/src`，跑在 `qtrs` 上）與 Python HUD（`python/`，PySide6）在使用者可觀察層面一致**：像素、版面、互動、時序、持久化、錯誤處理。
- `qtrs` 本身要成為「可替代 PySide6 的 Qt 子集」：契約以 **Qt 的 observable behavior** 定義（object / thread / event-loop 行為），不是 API 外觀。
- 平台：**Windows 為唯一驗證平台**。`window_x11.rs`、`window_wayland.rs`、`window_cocoa.rs`、`dispatcher_unix.rs`、`dispatcher_cocoa.rs`、pynput/macOS 熱鍵都未在真機驗證；本契約對它們 **不作任何通過宣稱**。
- PySide6 版本：6.11.2。Python HUD 實際使用的 Qt 符號清單見 §12.2。

### 1.2 範圍外（需要時先在本文件新增章節）

- QtNetwork、QtSql、QtQuick、Model/View、對話框、`QMainWindow` 家族、`QGraphicsEffect`、`QPropertyAnimation`（`qtrs-core` 有 `PropertyAnimation`，但 HUD 與本契約均未涵蓋）。
- `QT_CPP_MAPPING.md` 列出但 **檔案不存在** 的 widget（見附錄 B）。

### 1.3 Python HUD 與 Rust HUD 之間「刻意」的差異

下列差異 **不算 gap**，但必須在此登記；任何新增的刻意差異也要登記，否則視為 gap。

| 差異 | Python | Rust | 依據 |
|---|---|---|---|
| 單一實例 | 無 | Windows mutex + 喚醒訊息 | `rust/src/main.rs`、`qtrs-platform/src/single_instance.rs`；`test_single_instance.rs` 3 項測試 |
| `--snapshot` 輸出 | 無 | 寫 `rust/target/snapshots/*.png` | `rust/src/main.rs` |
| 熱鍵字串可設定 | 固定 Alt+C / Alt+Shift+C | 可設定 | `rust/src/hotkey.rs` |
| `--smoke-test` | 檢查設定持久化 | 只用預設 Config | `smoke_check.py` vs `main.rs`（**這個是 gap，見 G12.5.k，不是刻意**） |
| 版面切換 | 重建 `inner_layout` | `StackedWidget` | `hud_window.py:272-347` vs `hud_window.rs`（行為差異，見 C9.6） |

> 刻意差異的前提：**可觀察結果相同**。例如單一實例是附加能力，不改變單一視窗內的任何行為。

### 1.4 契約條目格式

每一項固定五欄：**Qt behavior → qtrs required → Current implementation → Known gap → Test**，另加 **HUD usage**（Python／Rust 實際使用的位置，無則寫 grep 條件）。

---

## 2. QObject semantics

### C2.1 Parent / child、所有權、child events
- **Qt behavior** `[QT-DOC]`
  - `setParent(p)` 把 child 加入 `p->children()`；舊 parent 收到 `ChildRemoved`、新 parent 收到 `ChildAdded`，兩者走 `sendEvent`，所以 event filter 看得到。
  - 同一 parent 重設是 no-op。`setParent(nullptr)` **釋放所有權給呼叫者，不銷毀 child**。
  - `~QObject` 先從 parent 解除連結，再依序刪除 children。跨執行緒 `setParent` 會失敗。
- **qtrs required**
  - MUST：`set_parent` 雙向更新；對舊／新 parent 送 `ChildRemoved`／`ChildAdded`，且經過 notify（filter 看得到）。
  - MUST：`set_parent(child, None)` **不得銷毀 child**；所有權要能回到呼叫者（回傳 `Option<Box<dyn QObject>>` 或等價）。
  - MUST：drop parent 時依 children 順序銷毀擁有的子樹。
  - MUST：drop child 時從仍存活的 parent 解除連結，即使 parent 正在 callback 中。
  - SHOULD：拒絕自我 parent、環、跨執行緒 parent。
- **Current implementation**（`qtrs-core/src/object/qobject.rs`）
  - `IMPLEMENTED`（READ + 既有測試）：邏輯／實體（`Box`）重新掛接 `set_parent`；parent drop 級聯；`ChildAdded/Removed`。
  - `IMPLEMENTED-UNTESTED`：`remove_owned_child`；`ObjectData::drop` 解除連結並通知 parent。
- **Known gap**
  - **G2.1.a [P0, RAN]** `set_parent(owned_child, None)` **銷毀 child**。舊 parent 的 `Box` 被搬進區域變數 `transferred`，沒有新 parent 接手就在函式結尾 drop；回傳型別 `()`，呼叫者拿不回所有權。
  - **G2.1.b [P1, READ]** `add_owned_child` 不送 `ChildAdded`（Qt 在建構時帶 parent 就會送）。
  - **G2.1.c [P2, READ]** `ChildAdded/Removed` 經 `dispatch_to_object`／`event()` 直送，不經 `notify_helper`；object filter 與 application filter 看不到。
  - **G2.1.d [P1, READ]** parent 正被借用（dispatch 中）時 `with_object_mut` 回 `None`：`ObjectData::drop` 的解除連結與 `set_parent` 的 child-list 更新被**靜默略過**，留下 stale children id，事件遺失。
  - **G2.1.e [P2, READ]** 無自我 parent／環／跨執行緒檢查。
  - **G2.1.f [P2, READ]** `set_parent` 跨 registry 鎖與物件借用，非原子；`QT_CPP_MAPPING.md` 寫「atomically」不實。
  - **G2.1.g [D]** 已註冊但非 `owned_children` 的 child，parent drop 時只解除註冊、不銷毀。理由：Rust 所有權；須在文件註明「parent 只銷毀它擁有的 Box」。
- **Test**
  - 既有：`test_ownership_and_deletion_cascade`、`test_child_unlink_notifies_parent`、`test_reparent_transfers_ownership_without_split_brain`、`test_borrowed_parent_links_only_update_child_metadata`。
  - 必要：`set_parent_none_keeps_child_alive`（Drop 計數器；本次 RAN 重現失敗）；`children_destroyed_in_insertion_order`；`event_filter_on_parent_sees_child_added_removed`；`drop_child_while_parent_borrowed_leaves_no_stale_id`；`add_owned_child_sends_child_added`。
- **HUD usage**：Python `QTimer(self)`（`refresh_controller.py:35,39`）、`UsageTable(parent=self)` + `old_table.deleteLater()`（`hud_window.py:262-266`）、`widget.setParent(None)` + `deleteLater()`（`:187-189`）。Rust：`rust/src` 不用 core `set_parent`/`add_owned_child`（grep 為空）；走 `qtrs-widgets` 自己的 `set_parent_widget`。

### C2.2 deleteLater / 延遲銷毀
- **Qt behavior** `[QT-DOC]` + 事件迴圈部分見 C3.5：`deleteLater()` 投遞 `DeferredDelete`；控制權回到事件迴圈才銷毀；slot 內呼叫安全；重複呼叫只刪一次；銷毀時 cascade children、移除該物件的 posted events、斷開連線、發 `destroyed`。
- **qtrs required**
  - MUST：`delete_later` 之後物件在「下一輪回到擁有它的 loop」時才死亡（liveness 為 false、連線斷開、計時器停止）；第二次呼叫為 no-op。
  - MUST：巢狀 loop 內不得提前刪（見 C3.5 的 scope level）。
  - MUST：該物件待處理的 posted events／queued slot 被丟棄（見 C3.6）。
  - SHOULD：發出 `destroyed`。
- **Current implementation**
  - `PARTIAL`：core `delete_later(obj, loop_level)` 只設旗標並回傳 `Event`，**由呼叫者自己 post**，且 `loop_level` 要呼叫者提供（`qobject.rs` `delete_later`）。loop 端處理在 `event_loop/loop.rs` 的 `DeferredDelete` 分支，最後呼叫 `unregister_qobject`（清 liveness、解除連結、`disconnect_all_for_object`、`stop_timers_for_object`）——**不 drop 物件本體**。
  - widgets 層另有機制：`WidgetCommandQueue::post_delete`，在 `EventTreeDispatcher::dispatch_event` 結束時 flush（見 C3.5）。
- **Known gap**
  - **G2.2.a [P1, READ]** `delete_later` 不會自我 post、不會 drop；`delete_later_called`／`delete_later_loop_level` 為只寫的死欄位。
  - **G2.2.b [P1, READ]** 不清除 posted events（見 G3.6.a）。
  - **G2.2.c [P2, READ]** 無 `destroyed` 信號（grep `destroyed` 於 `core/src` 為空）。
  - **G2.2.d [P1, READ]** widgets 的 `delete_later` 在「當前 dispatch 回傳時」就刪，比 Qt 的「回到事件迴圈」早。
- **Test**：既有 `test_delete_later`、`test_safe_deferred_delete_integrated_with_event_loop`、`loop.rs::test_deferred_delete_loop_level`。必要：`deferred_delete_clears_queued_slots_timers_and_qpointer`；`delete_later_twice_posts_once`；`delete_later_on_parent_cascades_to_owned_child`。
- **HUD usage**：Python `hud_window.py:189,193,266`（重建 layout／表格）。Rust：不呼叫 core `delete_later`。

### C2.3 objectName 與階層查詢
- **Qt behavior**：`findChild` 以 `qobject_cast` 比對（子類別也算）；先檢查直接子物件再遞迴；空名稱匹配全部；`objectName()` 未設定回空字串；`setObjectName` 發 `objectNameChanged`。
- **qtrs required**：MUST 名稱與型別比對、可取得可變存取；SHOULD 優先直接子物件；SHOULD 公開 `parent()/children()/thread()`。
- **Current implementation**：`IMPLEMENTED`（`find_child_any/_id/_mut`、`find_children_any`、`QObjectExt`）；`object_name()` 回 `Option<&str>`。
- **Known gap**：**G2.3.a [P2, READ]** 型別為 exact `TypeId`（無子類別語意）；**G2.3.b [P2, READ]** 深度優先逐 child，而非先掃完直接子物件；**G2.3.c [P2, READ]** 只搜 `owned_children`；**G2.3.d [P2, READ]** 無 `objectNameChanged`；**G2.3.e [P2, READ]** `parent()/children()/thread()` 取用器不存在（欄位為 `pub`）。
- **Test**：既有 `test_object_hierarchy_search_find_child`。必要：淺層匹配優先於較早出現的深層匹配。
- **HUD usage**：Python 到處 `setObjectName` 供 QSS `#id` 選擇器（`hud_window.py:139…`、`provider_card.py:45-104`、`usage_table.py:121…`）；`findChild` 未用。Rust 走 widget 層的 `set_object_name`。

### C2.4 blockSignals / QSignalBlocker
- **Qt behavior** `[QT-DOC]`：`blockSignals(b)` 回傳先前狀態並抑制該物件**所有**信號；`QSignalBlocker` 解構時還原「先前」狀態；不需註冊。
- **qtrs required**：MUST 抑制該物件擁有的每個 `Signal`；MUST 回傳先前狀態；blocker MUST 還原先前狀態而非 false。
- **Current implementation**：`IMPLEMENTED`（`block_signals` + `SignalBlocker`；`Signal::emit` 查全域 registry）。測試 `test_raii_signal_blocker`。
- **Known gap**
  - **G2.4.a [P1, READ]** `Signal::new()` 沒有 emitter id → `block_signals` 被忽略。`with_emitter(id)` 要手動接，目前只有 `timer.rs` 與測試在用。**所有 `qtrs-widgets` 信號（`button.rs`、`action.rs`、`menu.rs` …）都是無 emitter 的**。
  - **G2.4.b [P1, READ]** 即使有 emitter id，物件若未 `register_qobject`，`query_object_signals_blocked` 回 `None`，被當成「未封鎖」。
  - **G2.4.c [P2, READ]** unregister 後封鎖靜默失效；`derive(QObject)` 不會自動綁 `#[signal]` 欄位的 emitter id。
- **Test**：既有 `test_raii_signal_blocker`。必要：未註冊 emitter 的 `block_signals(true)` 抑制發射；巢狀 blocker 還原外層狀態；derive 出來的信號遵守 block。
- **HUD usage**：Python 無（grep `blockSignals|QSignalBlocker` 為空）；Rust 無。

### C2.5 QPointer / 物件存活
- **Qt behavior**：`QPointer<T>` 在物件銷毀時自動變 null，之後保持 null。
- **qtrs required**：MUST 在 drop 或 unregister 後為 null 且保持 null。
- **Current implementation**：`IMPLEMENTED`（`Arc<AtomicBool>` liveness，與 registry record 共用）。測試 `test_qpointer_and_generational_liveness`。
- **Known gap**：**G2.5.a [P1, READ]** 重複 `register_qobject` 同一物件會把它標死：`register_object_metadata` 對被取代的 record 呼叫 `liveness.store(false)`，而新舊 record 共用同一個 `Arc`，`registered_ptr` 之後失敗。`add_owned_child` 本身會呼叫 `register_qobject`，所以「呼叫者先註冊再 `add_owned_child`」的子物件會變成無法 dispatch。**G2.5.b [P2]** `QPointer` 無法解參考，只攜帶 `id()`。
- **Test**：必要：同一物件註冊兩次後 `with_object(id, …)` 仍為 `Some`（目前回 `None`）。
- **HUD usage**：無。

### C2.6 動態／宣告屬性
- **Qt behavior** `[QT-DOC]`：宣告屬性 `setProperty` 成功回 true；未宣告名稱建立動態屬性並回 **false**；動態屬性變更送 `QDynamicPropertyChangeEvent`；設為 invalid variant 即移除；宣告屬性有 notify signal 時變更要發射。
- **qtrs required**：MUST 宣告／動態屬性可往返、唯讀被拒；SHOULD 回傳值與移除語意同 Qt；notify signal MUST 在值改變時發射一次。
- **Current implementation**：`PARTIAL`（`QObject::property/set_property`：先 meta 再動態 map；`DynamicPropertyChange` 有送）。
- **Known gap**：**G2.6.a [P2, READ]** 新動態屬性回 `true`（Qt 回 false）；**G2.6.b [P2, READ]** `Variant::Invalid` 不移除；**G2.6.c [P1, READ]** notify signal 從不發射（derive 寫死 `notify_signal = None`）；**G2.6.d [P2, READ]** 沒有 `as_qobject_any` 時 meta 路徑靜默落到動態 map。
- **Test**：必要：動態屬性變更事件次數與回傳值；Invalid 移除；notify 每次變更發射一次。
- **HUD usage**：Python `widget.setProperty("state", …)` + `unpolish/polish`（`usage_table.py:256-260`）供 `QLabel[state="muted"]`。Rust 走 widget 層 `set_property(&str,&str)`（`usage_table.rs:879`），因樣式每次 lazily 解析，不需 polish（見 C8.5）。

### C2.7 Registry、借用排他、重入
- **Qt behavior** `[QT-DOC]`：`sendEvent` 給正在處理事件的物件是允許的（可重入）；跨執行緒 `sendEvent` 是錯誤（警告但仍遞送）。
- **qtrs required**：MUST 可重入派送要嘛送達、要嘛**可見地**被拒並記載；MUST 非擁有者執行緒的派送被拒。
- **Current implementation**：`with_object*` 以 per-object `borrow_flag` CAS + 執行緒檢查；第二次借用回 `None`，`dispatch_to_object` 轉為 `false`。
- **Known gap**：**G2.7.a [P1, READ]** 對已被借用的物件送事件會**無聲丟失**（含重入 `send_event` 與 parent 自己 handler 內觸發的 `ChildAdded`）；**G2.7.b [D]** 跨執行緒直接派送不支援（`registered_ptr` 拒絕非註冊執行緒）。理由：registry 存 thread-local 原始指標；誤用回 `false`，須補上診斷（見 C7.4）；**G2.7.c [test gap]** `test_qobject_safety_and_qt6_features.rs` 的「Memory Safety & Dynamic Borrow Exclusivity Tests」標題下沒有任何測試。
- **Test**：必要：從 `event()` 內對自己 `send_event`，斷言記載的結果且無 panic／UB；非擁有者執行緒派送回 false。
- **HUD usage**：Rust HUD 不直接碰 registry。

---

## 3. Event semantics

### C3.1 QEvent 物件模型
- **Qt behavior** `[QT-SRC qcoreevent.h]`：事件建構時為 accepted；`type()` 為 `QEvent::Type` 數值；OS 來源事件 `spontaneous()` 為 true。
- **qtrs required**：MUST 新事件 accepted 且非 spontaneous；OS 來源 MUST 以 `new_spontaneous` 建；每個與 Qt 同名的 `EventType` 數值 MUST 等於 Qt 數值；HUD 會收到的事件 MUST 有對應 type。
- **Current implementation**：`IMPLEMENTED`：`Event::new/new_spontaneous/accept/ignore`；87 個 `EventType` 中 85 個與 `qcoreevent.h` 數值一致（稽核者逐一比對，`READ`）。`PARTIAL`：僅 60/87 可由 `EventKind` 到達（`Paint`(12)、`Create`、`Destroy`、`ParentChange`、`UpdateLater`、`ChildPolished`、`WindowTitleChange`、`PaletteChange`、`Clipboard`、`SockAct` 到不了）。`ABSENT`：另外 93 種 Qt 事件（`Polish`、`LanguageChange`、`StyleChange`、`FontChange`、`EnabledChange`、`ActivationChange`、`WindowStateChange`、`ApplicationActivate/Deactivate`、`LocaleChange`…）；`registerEventType`、`sendSpontaneousEvent`、`isPosted`。
- **Known gap**：**G3.1.a [P2, READ]** `Pointer`=251、`DpiChanged`=250 不是 Qt 數值（Qt：`Pointer`=218、`DevicePixelRatioChange`=222）；**G3.1.b [D]** `EventKind` 為封閉 enum，使用者自訂事件只有 `EventKind::User(Box<dyn Any>)`（恆為 type 1000）。理由：Rust 型別安全；HUD 不用；**G3.1.c [P1, READ]** 缺少的事件型別代表 filter 看不到 `Paint`／`Polish`／`LanguageChange` 等。
- **Test**：既有 `event/mod.rs::tests::*`、`test_advanced_event_system.rs::test_event_type_mapping`。必要：以表格逐一比對每個 `EventType as u32` 與 Qt 數值；每個 `EventKind` 變體有唯一 `event_type()`。
- **HUD usage**：Python 覆寫 widget handler（`hud_window.py:560-622`、`usage_table.py:160`）；`QEvent`/`postEvent`/`sendEvent`/`installEventFilter` 未用。Rust 只用 `EventKind::MetaCall`（`main.rs:44-55`）與 widgets 的 `UpdateRequest`。

### C3.2 postEvent 順序、優先序、輪次邊界
- **Qt behavior** `[QT-SRC qcoreapplication.cpp:1658-1704,1782-1934]`：`postEvent` 依優先序插入接收者所屬執行緒的佇列；`insertionOffset` 使「處理中 post 的事件」留到下一輪；接收者的執行緒尚無 loop 時事件**仍然排隊**；null receiver 警告並拒絕。
- **qtrs required**：MUST 同優先序 FIFO；MUST 較高優先序先送，**含跨輪**；MUST 處理中 post 的事件等下一輪；SHOULD 接收者執行緒尚無 loop 時排隊而非丟棄。
- **Current implementation**：`IMPLEMENTED`：輪次邊界於 pump 開始時取得（`loop.rs` 的 `insertion_offset`）；post 時喚醒。`PARTIAL`：優先序插入用 `insertion_offset.min(len)`，`insertion_offset` 設為 pump 開始時的佇列長度、**從不遞減**，已送出的事件從頭移除，所以偏移過期。
- **Known gap**
  - **G3.2.a [P1, READ]** 跨輪／下一輪的優先序錯誤。例：pump `[A,B]`，B 的 handler 先 post X(0) 再 post Y(100)，Y 落在 X 之後（Qt 先送 Y）；3 個事件 pump 完後 post L(0)、M(0)、H(100)，H 落在 M 之後（Qt 的 cleanup 會減掉 `startOffset`，H 先）。現有 `test_insertion_offset_priority_ordering` 只測 pump 期間，分辨不出。
  - **G3.2.b [P0, READ]** 目標執行緒沒有已註冊 loop 時 `post_event_to_thread` 回 `false`，呼叫端（`CoreApplication::post_event_with_priority`、`signal.rs` 的 queued 閉包、`widget.rs:296`）**忽略回傳值 → 事件靜默遺失**（Qt 會排隊）。HUD 的 `run_on_main_thread` 在 loop 註冊前被 worker 呼叫即遺失；需實測啟動競態。
  - **G3.2.c [P2, READ]** 巢狀 pump（handler 內呼叫 `process_events`）吃掉外層剩餘事件後，外層迴圈 `processed_count < max_index` 會誤送下一輪事件。
  - **G3.2.d [P2]** 無 null receiver 警告；`ObjectId(0)` 被刻意當成「無接收者」哨兵（`main.rs:46`、`timer.rs:612`）。哨兵語意必須保留並明文記載（見 C3.6）。
  - **G3.2.e [D]** 無 `sendPostedEvents(receiver, type)` 篩選式 flush、`removePostedEvents`、`hasPendingEvents`。理由須在 C3.6 一併處理（`removePostedEvents` 是 G3.6.a 的必要前置）。
- **Test**：既有 `loop.rs::test_livelock_prevention`、`test_cross_thread_wakeup`、`layered_tests.rs::test_level2_loop_livelock_protection`。必要：`priority_across_pumps`（H,L,M）；`reentrant_pump_does_not_cross_turn_boundary`；`post_before_loop_exists_is_delivered_when_loop_starts`。
- **HUD usage**：優先序不用。Rust worker → 主執行緒：`post_event_to_thread(main_thread_id, ObjectId(0), MetaCall)`（`main.rs:40-55,525-537`）；Python 用 25 ms `QTimer` 輪詢 `queue.SimpleQueue`（`refresh_controller.py:35-38`）。

### C3.3 事件壓縮
- **Qt behavior** `[QT-SRC qcoreapplication.cpp:1717-1753; qapplication.cpp:790-840]`：核心只壓 `Timer`（按 receiver + timerId）與 `Quit`（舊的留下）；widgets 壓 `UpdateRequest`、`UpdateLater`、`LayoutRequest`、`LanguageChange`，`Resize`/`Move` 保留舊事件並更新為最新值；posted `MouseMove` **不**在這一層壓縮。
- **qtrs required**：HUD 相關的壓縮（`UpdateRequest`、`LayoutRequest`、`Resize`、per-id `Timer`）MUST 與 Qt 一致；壓縮 MUST 按 receiver 區分。
- **Current implementation**：`IMPLEMENTED`（`event/compressor.rs`：`UpdateRequest`、`LayoutRequest`、`Timer`/`ZeroTimer` 按 id、`Resize`）；測試見 `compressor.rs::tests::*`、`loop.rs::test_event_compression`。
- **Known gap**：**G3.3.a [P2, READ]** `Quit` 保留**新的** exit code（Qt 保留舊的）；**G3.3.b [P2, READ]** `MouseMove`/`HoverMove` 被壓縮（Qt 不壓）；**G3.3.c [P2, READ]** `Move`/`UpdateLater`/`LanguageChange` 壓縮不存在；**G3.3.d [P2, READ]** 每次 post 都 O(queue) 掃描（Qt 以 `postedEvents` 計數把關）；**G3.3.e [D]** `Quit{exit_code}` 是 qtrs 擴充（Qt 的 Quit 不帶 code）。理由：qtrs 以 `Quit{code}` 事件攜帶 exit code 給 `exec` 的回傳值；只要 `exec` 回傳碼與 Qt 的 `exit(code)` 相同，可觀察行為不變；誤用（多個 Quit 帶不同 code）的取捨由 G3.3.a 追蹤。
- **Test**：既有如上。必要：`Move`/`UpdateLater` 壓縮（待 `EventKind` 補齊）。
- **HUD usage**：Rust 每次 `update()` post 一個 `UpdateRequest`（`widget.rs:295-300`），依賴壓縮避免渲染風暴。

### C3.4 sendEvent / notify / 事件過濾
- **Qt behavior** `[QT-SRC qcoreapplication.cpp:1275-1300]`：順序為 application filters（僅主執行緒、LIFO、跳過 null／已移除）→ 接收者的 filters（LIFO、跳過已移除，因此在迭代中移除「尚未呼叫」的 filter 是安全的且該 filter 不會被呼叫）→ `receiver->event()`。`installEventFilter` 重新安裝會移到最前。Filter 看得到**所有**事件，含 `QTimerEvent`（計時器經 `sendEvent` 送出，`qeventdispatcher_win.cpp:414-415`）。
- **qtrs required**：順序 MUST 為 app filters → object filters → `event()`；回傳 true 的 filter MUST 中止遞送且 `send_event` 回 false；迭代中移除尚未呼叫的 filter MUST 使其不被呼叫；Timer 事件 MUST 經過 filter。
- **Current implementation**：`IMPLEMENTED`：`notify_helper`（`loop.rs`）、per-thread app filter、object filter 鏈（LIFO、去重、tombstone）、`NativeEventFilter`。測試：`loop.rs::test_notify_helper_pipeline_and_safe_removal`、`layered_tests.rs::test_level1_event_filter_lifo_intercept_and_tombstone`、`event_filter.rs::tests::*`、`dispatcher_win.rs::tests::test_native_event_filter_intercept_in_dispatcher`。
- **Known gap**
  - **G3.4.a [P1, READ]** `notify_helper` 迭代 filter id 的**快照**；被前一個 filter 移除的 filter 仍會被呼叫（Qt 跳過）。現有測試只測「在自己 callback 內移除自己」。
  - **G3.4.b [P2, READ]** filter 物件正被借用（例如 parent 在 `event()` 中對它過濾的 child 送事件）時，filter 被靜默跳過，事件無過濾遞送。
  - **G3.4.c [P1, READ]** Windows 計時器遞送不經 `notify_helper`／`event()`（見 C5.2）。
  - **G3.4.d [D]** 跨執行緒 `send_event` 回 false 且不做事（Qt 會在呼叫端執行緒遞送並警告）。理由：同 G2.7.b。
  - **G3.4.e [P2, READ]** `install_event_filter` 拒絕自己與直接環（Qt 允許）。
- **Test**：必要：`filter_removed_by_earlier_filter_is_not_called`；`timer_event_passes_event_filter`。
- **HUD usage**：Python 無 `installEventFilter`；Rust 只裝 native filter（`WakeFilter`，`main.rs:57-83,322`）。

### C3.5 DeferredDelete 與輪次
- **Qt behavior** `[QT-SRC qobject.cpp:2519-2564; qcoreapplication.cpp:1870-1904,1484-1489]`：`DeferredDelete` 帶 `(loopLevel, scopeLevel)`；只有在 loop level 降到低於戳記，或明確以 `sendPostedEvents(nullptr, DeferredDelete)` 同級時才送。**scope level**：`deleteLater(); processEvents();` **不會**刪。`exec()` 結束時要 flush 剩餘的 DeferredDelete。
- **qtrs required**：MUST 不同步刪除；MUST 回到擁有它的 loop 後才銷毀，不在同一 handler 的巢狀 `processEvents` 內；MUST 最外層 `exec` 返回時 flush。
- **Current implementation**：`PARTIAL`（見 C2.2）。pump 在 `modal || (lvl>0 && loop_level>lvl)` 時延後；widgets 層另一套（dispatch 結束即刪）。
- **Known gap**：**G3.5.a [P1, READ]** 無 scope level：同 handler 內 `process_events` 會刪（`loop_level <= event_loop_level`）；**G3.5.b [P1, READ]** `EventLoop::exec` 結束沒有 cleanup flush；level 0 post 的事件只有之後在 level>0 的 pump 才會被 flush；**G3.5.c [P1, READ]** widgets 的 `delete_later` 時機比 Qt 早。
- **Test**：必要：`delete_later_not_delivered_by_process_events_in_same_handler`；`exec_exit_flushes_deferred_delete`。
- **HUD usage**：Python `deleteLater`（`hud_window.py:189,193,266`）。Rust 不用 core 這條路徑。

### C3.6 Posted events 與接收者銷毀
- **Qt behavior** `[QT-SRC qobject.cpp:195-207; qcoreapplication.cpp:1955-2002]`：`~QObject` 呼叫 `removePostedEvents(this)` 與 `unregisterTimers(this)`；因此對已刪除接收者的 queued slot **永遠不會執行**。
- **qtrs required**：MUST 對已 unregister／銷毀的接收者之事件被丟棄，**含 MetaCall**；同時 MUST 保留 `ObjectId(0)` 的「fire-and-forget、無接收者」哨兵語意——**兩者必須可區分**。
- **Current implementation**：`ABSENT` 清除佇列（grep `remove_posted|removePosted|purge` 為空；`unregister_qobject` 不碰任何佇列）。`MetaCall` 若接收者查不到（未註冊／已死／**註冊在別的執行緒**）就在 `NullObj` 替身上**照樣執行閉包**（`loop.rs` `notify_helper`，程式碼註解自己承認這偏離 Qt）。
- **Known gap**
  - **G3.6.a [P1, READ]** 對已銷毀接收者的 queued slot 仍會執行；無法區分「哨兵」與「真的死掉」。
  - **G3.6.b [P2, READ]** `unregister_qobject` → `stop_timers_for_object` 只刪 registry 項、不呼叫 Win32 `KillTimer`，孤兒 `WM_TIMER` 持續到達（被忽略，洩漏）。
- **Test**：必要：`queued_slot_not_run_after_receiver_unregistered`（註冊、排入 MetaCall、unregister、pump → 不執行）；`object_id_zero_metacall_still_runs`。
- **HUD usage**：Rust `ObjectId(0)` MetaCall（`main.rs:44-55`）、`Timer::single_shot(0)`（`main.rs:425,457`）。Python 沒有跨執行緒 queued 信號（以 `QTimer` 輪詢取代）。

---

## 4. EventLoop semantics

### C4.1 exec / exit / quit / aboutToQuit
- **Qt behavior** `[QT-SRC qcoreapplication.cpp:1450-1476,1522-1542,2052-2057,2091-2102,2138-2159; qeventloop.cpp:135-202; qguiapplication.cpp:2114-2126]`
  - `QCoreApplication::exec` 重置 `quitNow` 並跑一個全新的 `QEventLoop`；進入時移除佇列中殘留的 `Quit`。
  - `exit(code)`：`aboutToQuit` 只發一次，**在 `exit()` 內、loop 展開之前**；對每個巢狀 loop 呼叫 `exit`。
  - `quit()` 只在 `exec` 執行中才有作用；主執行緒同步送 `QEvent::Quit`，其他執行緒則 post。`QGuiApplication` 收到 `Quit` 會先關閉所有頂層視窗，任一拒絕就**取消**。
  - `exit()` 先於 `exec()` **會遺失**。
  - 最後一個視窗關閉時（`quitOnLastWindowClosed`）以**可取消的 `Quit`** 觸發。
- **qtrs required**
  - MUST `exec` 阻塞到 `exit/quit`（在 exec 開始之後）被呼叫，並回傳 code；第二次 `exec()` 是全新的 loop。
  - MUST `aboutToQuit` 只發一次，於 `exit()` 內、`exec` 返回之前。
  - MUST `quit()/exit()` 可由**任何執行緒**呼叫。
  - MUST `quit()` 先於 `exec` 不起作用。
  - SHOULD `Quit` 可被 filter 否決；`quit` 先關視窗。
- **Current implementation**（`core/event_loop/loop.rs`、`core/application/mod.rs`）
  - `IMPLEMENTED`：旗標式 `exec`、`post_quit` 與 exit code。測試 `loop.rs::test_exec_and_exit`、`test_exec_exit_method`、`layered_tests.rs::test_level3_timer_and_event_loop_exit`。
  - `IMPLEMENTED-UNTESTED`：在 `exec` 期間由 slot 呼叫 `CoreApplication::exit/quit`。
- **Known gap**
  - **G4.1.a [P1, READ]** `exit()` 先於 `exec()` 會**保留**並使 `exec` 立刻返回（`loop.rs` 註解與 `test_application_layers.rs:80-98` 釘住此行為）。Qt 會遺失。**必須改寫該測試。**
  - **G4.1.b [P1, READ]** `exit_requested` 在 `exec` 返回後不重置，第二次 `exec()` 立刻以舊 code 返回。
  - **G4.1.c [P1, READ]** worker 執行緒呼叫 `quit()/exit()` 是**靜默 no-op**（`get_thread_event_sender(ThreadId::current())` 找不到 loop，退回讀 worker 自己空的 `LOCAL_EVENT_LOOP`）。
  - **G4.1.d [P1, READ]** `aboutToQuit` 在 `exec` **返回後**才發，不是在 `exit()` 內；`exit` 無 `exec` 時不發。
  - **G4.1.e [P1, READ]** `quit()` 不是 `Quit` 事件：不關視窗、不能被否決；pump 在遞送前就鎖定 `quit_code`，filter 否決不了 posted `Quit`。
  - **G4.1.f [P2, READ]** `CoreApplication::new` 文件寫會 panic，實際沒有第二個實例檢查。
  - **G4.1.g [P1, READ]** 無獨立的巢狀 `QEventLoop`（`isRunning`、自己的 `exit`、`QEventLoopLocker`）。`EventLoop::new` 取代該執行緒已註冊的 handle 與 timer context，`Drop` 取消註冊——**在同一執行緒建立並丟棄第二個 `EventLoop` 會破壞第一個**。
  - **G4.1.h [P2]** `ExitCode::from(exit_code as u8)` 截斷（`main.rs:620`）；HUD 以 0 退出。
  - **G4.1.i [P1, READ]** `qtrs-widgets::Application` 在最後一個視窗被 `unregister_window`（drop）時直接 `quit()`，而不是在**關閉**時以可取消 `Quit`（見 C11.2）。
- **Test**：既有如上。必要：`exit_before_exec_is_noop`（並改寫 `test_application_layers.rs:80-98`）；`second_exec_is_fresh`；`quit_from_worker_thread_stops_exec`；`about_to_quit_emitted_inside_exit_before_exec_returns`；`quit_ignored_before_exec`；`creating_second_event_loop_does_not_break_first`。
- **HUD usage**：Python `app.exec()`（`main.py:100`）、`aboutToQuit.connect(on_exit)`（`:98`）、`app.quit()`（`hud_window.py:868-875`）、`setQuitOnLastWindowClosed(False)`（`main.py:48`）。Rust：`_app.exec()`（`main.rs:599`）、`Application::exit(0)`（`:407,443`）；**不連 `about_to_quit`**，清理靠 `exec` 返回後的 Drop 順序（`:600-617`）；**從不設定 `quit_on_last_window_closed(false)`**。

### C4.2 processEvents
- **Qt behavior** `[QT-SRC qcoreapplication.cpp:1356-1361; qeventdispatcher_win.cpp:479-571]`：`processEvents(flags)` 是一次 dispatcher 遍歷，回 void、不等待；Windows 上先 `sendPostedEvents()`，再排空所有訊息（每個 `WM_TIMER` 一次），`WM_QUIT` 呼叫 `quit()`；不送 DeferredDelete。
- **qtrs required**：MUST 一次呼叫送出進入時已存在的 posted events 加上待處理 OS 訊息與計時器，不阻塞；MUST 可在 `exec` 中的 slot 內巢狀呼叫。
- **Current implementation**：`IMPLEMENTED`（posted pump → dispatcher 遍歷；`dispatcher_win.rs`）。測試 `loop.rs::test_process_events_pumping`、`dispatcher_win.rs::tests::*`、`test_resize_deferred_render.rs`。
- **Known gap**
  - **G4.2.a [P1, READ]** **巢狀 `process_events` 靜默失效**：`CoreApplication::exec` 整個 `exec` 期間持有 `LOCAL_EVENT_LOOP.borrow_mut()`，`CoreApplication::process_events` 用 `try_borrow_mut` 失敗就回 false。`exec` 中途 `install_native_event_filter` 會 panic。
  - **G4.2.b [P2, READ]** 只觸發計時器的一輪回傳 false（Qt 回 true）；**G4.2.c [P2]** 無 `ExcludeUserInputEvents`／`WaitForMoreEvents`／`maxTime`（沒有工程理由說明為何不提供；需要時須補，否則在 API 上明確拒絕這些旗標）；**G4.2.d [P2, READ]** `WM_QUIT` 以 `PostQuitMessage(n)` 的 n 為 code（Qt 呼叫 `quit()`，code 0）。
- **Test**：必要：`process_events_inside_slot_during_exec_delivers`；`process_events_true_when_timer_fired`。
- **HUD usage**：Python 只在 smoke／tests 用 `processEvents`（`smoke_check.py:41`、`tests/*`）。Rust 在 `exec` 外用（`main.rs:111,204,215,229`），巢狀 bug 未觸發。

### C4.3 喚醒、跨執行緒 post、modal loop
- **Qt behavior** `[QT-DOC]`：任何執行緒的 `postEvent` 追加到接收者執行緒佇列並喚醒其 dispatcher；Win32 `wakeUp` 合併成一個訊息。
- **qtrs required**：MUST 跨執行緒 post 喚醒睡眠中的 `exec`；MUST 喚醒合併；MUST 原生 modal loop（Win32 resize、`TrackPopupMenu`）仍會 pump posted events 但**不重入**正在執行的 pump。
- **Current implementation**：`IMPLEMENTED`：`wake_up` + pending 旗標（`dispatcher_win.rs`）、modal pump 的重入保護與輪次邊界（`loop.rs`）。測試 `dispatcher_win.rs::tests::{test_wakeup_message_is_deduplicated_and_pumped_by_wnd_proc, test_wnd_proc_pump_defers_events_posted_during_dispatch}`、`loop.rs::{test_modal_pump_*, test_cross_thread_wakeup}`。
- **Known gap**：HUD 路徑上沒有；但 `Menu::exec_popup` 使用**自己的 `GetMessageW` 迴圈**而不是巢狀 `QEventLoop`（**G4.3.a [P1, READ]**）——選單開著時 posted events／計時器是否持續觸發，取決於 loop 如何喚醒該執行緒，**尚未實測**；Qt 的 `QMenu::exec` 是巢狀 `QEventLoop`（`[QT-DOC]`），期間所有計時器照常觸發。
- **Test**：必要：`timer_and_posted_event_fire_while_popup_menu_is_open`（需 Windows 真機實測；HUD 有 1 s 倒數計時器與 3 s 輪詢，選單開著時 Python 預期會繼續更新，`[QT-DOC]` 未實測）。
- **HUD usage**：Rust worker 喚醒主 loop（`main.rs:525-537`）；Python 以輪詢避開。

---

## 5. Timer semantics

### C5.1 QTimer 生命週期
- **Qt behavior** `[QT-SRC qtimer.cpp:217-228,312-334,654-682]`：`start()` 對已啟動的計時器先 stop 再以新 id 重啟；`setInterval` 對已啟動的計時器 **kill 並重啟**；單發計時器在發 `timeout` **之前**先 `stop()`；`stop()` 冪等；`isSingleShot` 在觸發時讀取；預設 `CoarseTimer`。
- **qtrs required**：`start()` MUST 重啟；`set_interval` 於啟動中 MUST 以新間隔重啟；單發 MUST 在 slot 內回報 inactive；slot 內呼叫 `stop()` MUST 安全；`set_single_shot` 於啟動中 MUST 影響下一次觸發。
- **Current implementation**（`core/timer.rs`）
  - `IMPLEMENTED`：`start`（`stop()` 後重新註冊）、`stop` 冪等、單發先清 id 再發信號（Win32 dispatcher 在遞送前 `KillTimer`）、預設 `Coarse`。測試 `timer.rs::tests::{test_timer_start_stop_and_timeout_order, test_single_shot_timer, test_timer_cancellation, test_timer_remaining_time …}`。
  - `BROKEN`：`set_interval` 只寫欄位（`timer.rs` `pub fn set_interval`），registry 項與 Win32 `SetTimer` 仍用舊間隔。
  - `BROKEN`：`set_single_shot` 啟動中只改欄位；registry 項沿用 `start` 時複製的旗標。
- **Known gap**
  - **G5.1.a [P1, RAN]** `set_interval` 對啟動中的計時器無效。重現：`set_interval(5000)`、`start()`、`set_interval(20)`，pump 400 ms → **0 次** `timeout`（Qt：約 20 次）。
  - **G5.1.b [P1, READ]** `set_single_shot` 啟動中無效。
  - **G5.1.c [P1, READ]** `Timer::start` 是 `unsafe` 並以原始位址註冊（pinning 契約）；`Timer::new` 沒有 parent 參數，沒有 `QTimer(self)` 那種「隨 parent 銷毀」的生命週期。
  - **G5.1.d [P2, READ]** 執行緒沒有 timer context 時 `start` 靜默回 `TimerId::INVALID`（Qt 會警告）。
  - **G5.1.e [P2, READ]** `timeout` 是公開 `Signal<()>` 欄位，不在 meta-object 上。
- **Test**：既有如上。必要：`set_interval_while_active_restarts`（本次 RAN 重現失敗）；`set_single_shot_while_active_fires_once`；`start_inside_own_slot_restarts_single_shot`。
- **HUD usage**：Python `QTimer(self)` 25 ms／1000 ms（`refresh_controller.py:35-44`）、250 ms 單發重啟（`hud_window.py:73-76,626,631`）、倒數 1000 ms（`:440-442`）；**從不在啟動中改 interval**。Rust：`Timer::new` + `set_interval`（啟動前）+ `start`（`main.rs:540-591`）。**差異**：Python 的 250 ms 單發、移動／縮放時重啟（debounce 儲存幾何）；Rust 用 `std::thread` 的 `ResizeDebouncer`（`config.rs:396`，只吃 resize）加 3000 ms 輪詢抓移動（`main.rs:575-591`）——行為不同，見 G12.5.d。


### C5.2 計時器事件遞送路徑
- **Qt behavior** `[QT-SRC qeventdispatcher_win.cpp:401-415]`：到期的計時器以 `QCoreApplication::sendEvent(obj, &QTimerEvent)` 遞送，application filters、object filters、`event()` 覆寫都看得到；`inTimerEvent` 防止同一計時器重入；遞送**前**先重算下次到期。
- **qtrs required**：到期計時器 MUST 走與其他事件相同的 notify 管線；同一計時器 MUST 不重入。
- **Current implementation**：重入保護與重算：`IMPLEMENTED`（`dispatcher_win.rs`）。
- **Known gap**
  - **G5.2.a [P1, READ]** Windows 路徑直接呼叫 `obj.timer_event(id)`，不建 `Event`、不經 `notify_helper`/`event()`。unix／cocoa 建了 `EventKind::Timer` 並呼叫 `event()`，仍繞過 filter。三個平台彼此不一致。對借用中或未註冊物件的 tick 被丟棄，不重試。
  - **G5.2.b [P2, READ]** `send_timer_events` 持有 registry mutex 的同時遞送；其中的 slot 若啟動／停止計時器會在同一執行緒重入 `std::sync::Mutex`（潛在死鎖，未重現；僅在 `WM_TIMER` 先於 timer context 到達時走這條路）。
- **Test**：必要：`timer_event_passes_event_filter`；`timer_event_reaches_event_override`。
- **HUD usage**：HUD 只用 `timeout` 信號，觀察不到。

### C5.3 零間隔計時器
- **Qt behavior** `[QT-SRC qeventdispatcher_win.cpp:360-363,385-387,886-901]`：Windows 上 0 ms 計時器 post `QZeroTimerEvent`，每次遞送後只要計時器還在就重新 post，因此**每輪迴圈都觸發**；停止時移除 posted event。
- **qtrs required**：非單發的 0 ms 計時器 MUST 每輪重複觸發；單發 0 ms MUST 觸發一次且**不留 registry 項**；沒有存活計時器的 idle loop MUST 阻塞而不是空轉。
- **Current implementation**：`Timer::start` 只 post **一個** `ZeroTimer`（`timer.rs`），沒有任何地方再 post（grep `ZeroTimer` 於 `rust/` 只有這個生產者）。
- **Known gap**
  - **G5.3.a [P1, RAN]** 單發 0 ms 計時器觸發後 **registry 項殘留**：重現 `fired=1 registry_len=1 next_timeout=Some(0ns)`。`EventLoop::next_timeout` 永遠回 0 → `exec` 以 100% CPU 空轉。（既有 `test_zero_timer_immediate_dispatch` 只檢查有觸發與 inactive，沒檢查 `registry.len()`。）
  - **G5.3.b [P1, READ]** 重複 0 ms 計時器只觸發一次。
- **Test**：必要：`zero_single_shot_leaves_registry_empty`（RAN 重現失敗）；`zero_timer_repeats_each_iteration`；`idle_exec_blocks_without_timers`（`next_timeout()==None`）。
- **HUD usage**：HUD 用 `Timer::single_shot(0, …)`（MetaCall 路徑，見 C5.5），**沒走**這個壞掉的路徑；因此 P1。

### C5.4 QObject::startTimer / killTimer
- **Qt behavior** `[QT-SRC qobject.cpp:195-207; qabstracteventdispatcher.cpp:95]`：`startTimer` 向執行緒的 dispatcher 註冊並遞送 `QTimerEvent`；`~QObject` 殺掉該物件的所有計時器；timer id 全域唯一。
- **qtrs required**：Windows 上 `start_timer(interval)` MUST 在運行中的 loop 內週期性呼叫 `timer_event(id)`；`kill_timer` MUST 真的停止（含 `KillTimer`）。
- **Current implementation**：`BROKEN`（Windows）。`start_object_timer` 只做 `registry.register`，`QObject::start_timer` 只加進 `ObjectData.timers`；沒有 `SetTimer`。Windows 的 `send_timer_events` 只排空**已到達的 `WM_TIMER` id**，從不輪詢 registry 到期時間。unix dispatcher 有輪詢（`registry.expired_timers`），所以只有 Windows 受影響。
- **Known gap**
  - **G5.4.a [P1, RAN]** 重現：註冊物件 `start_timer(20ms)`、pump 400 ms → `timer_event` **0 次**。到期後 `next_timeout()` 回 0，`exec` 空轉。
  - **G5.4.b [P1, READ]** `kill_timer` 不呼叫 `KillTimer`。
  - **G5.4.c [P2]** `TimerId` 是 per-registry 計數器，非全域唯一。
- **Test**：既有 `test_qobject_start_and_kill_timer` 只斷言 registry 記帳（**必須補**：實際觸發）。必要：`start_timer_fires_timer_event`、`kill_timer_stops_firing`。
- **HUD usage**：HUD 不用（Python grep `startTimer|killTimer` 為空；Rust 只有 `key_sequence_edit.rs:845` 的 release timer，**該 widget 因此在 Windows 上可能壞掉**，`[INFERENCE]` 未實測）。

### C5.5 QTimer::singleShot（靜態）
- **Qt behavior** `[QT-SRC qtimer.cpp:365-396; qtimer.h:176-183]`：`singleShot(0, fn)` 不是計時器，而是經 `invokeMethodImpl(QueuedConnection)` 排入目前執行緒，按 posted-event 順序在下一輪執行；`ms>0` 建 `QSingleShotTimer`（dispatcher 的子物件），類型 `Precise`（<2 s）／`Coarse`（≥2 s）；兩種都可在 app 存在前呼叫；帶 context 物件的版本在 context 死亡時取消。
- **qtrs required**：`single_shot(0, f)` MUST 在下一輪按 post 順序執行；`single_shot(ms, f)` MUST 於 `ms` 後觸發一次並忘記 callback；SHOULD 有 context 版本；SHOULD 在 loop 尚不存在時排隊而不是丟棄。
- **Current implementation**：`IMPLEMENTED`：0 ms → `MetaCall` 送給 `ObjectId(0)`（`timer.rs` `single_shot`），與 Qt queued-call 語意一致；`ms>0` → 隱藏 registry 項 + thread-local callback map（key 為新的 `ObjectId::next()`），固定 `Coarse`。測試 `test_single_shot_timer`、`test_qtimer_single_shot_static_api_zero_delay`。
- **Known gap**
  - **G5.5.a [P2, READ]** 無 context 版本、無取消。
  - **G5.5.b [P1, READ]** 執行緒無 loop 時 `single_shot` 靜默丟 callback（0 ms 時 `post_event_to_thread` 回 false）。
  - **G5.5.c [P2]** 類型恆為 `Coarse`（Qt <2 s 為 `Precise`）；在 20 ms < interval < 20 s 內排程數學相同，無可觀察差異。
  - **G5.5.d [P2]** callback 必須 `Send + 'static` 且只在呼叫執行緒執行。
- **Test**：必要：`single_shot_ms_fires_once_via_exec`（Windows，30 ms，斷言恰一次且 callback map 為空）；`single_shot_zero_runs_after_earlier_posted_events`（FIFO）。
- **HUD usage**：Python `QTimer.singleShot(300|2500|0|1000|0|150, …)`（`hud_window.py:108,114,215,459,613,617`）。Rust 只用 `single_shot(0, …)`（`main.rs:425,457`）與 `single_shot(2500, trim_memory)`（`:594`）。**Python 的 300／1000／150 ms 單發在 Rust 沒有對應**（見 G12.5.h）。

### C5.6 計時器類型、精度、remainingTime
- **Qt behavior** `[QT-SRC qeventdispatcher_win.cpp:302-343; qtimer.cpp:705-718]`：coarse 最長 20 s、5% 容差，低於 20 ms 轉 precise，高於 20 s 轉 very-coarse（**間隔**取整到秒）；precise 用 `timeSetEvent`，其餘 `SetCoalescableTimer`；`remainingTime` 為 `ceil`，非啟動 `-1`，逾期 `0`。
- **qtrs required**：`calculate_next_timeout` MUST 等於 Qt；`remaining_time` MUST `-1`／`0`／`ceil`（容許 1 ms）。
- **Current implementation**：`calculate_next_timeout`：`IMPLEMENTED`（測試 `test_calculate_next_timeout`）。其餘 `PARTIAL`。
- **Known gap**
  - **G5.6.a [P2, READ]** `remaining_time` 用 floor（Qt ceil）。
  - **G5.6.b [P2, READ]** Win32 一律以**原始**間隔 `SetTimer`（Qt 用調整後的）；`SetTimer` 低於 10 ms 會被夾住且 `WM_TIMER` 優先權低、忙碌時會被餓死；`timeBeginPeriod(1)` 呼叫一次、從不配對；間隔超過 `u32::MAX` 被截斷。HUD 的間隔（25／100／250／1000 ms，全部 `Coarse` 且 20 < i < 20000）排程數學相同，無可觀察差異。
- **Test**：必要：`remaining_time_ceils`；Windows 上 25 ms `Coarse` 在閒置 loop 下於 [20, 80] ms 內觸發。
- **HUD usage**：Python 全部預設 `Coarse`；Rust 同。

---

## 6. Signal / Slot semantics

### C6.1 emit、connect、disconnect、順序
- **Qt behavior** `[QT-DOC]`，括號內 `[INFERENCE]`：slot 依連線順序執行；發射期間被 disconnect／刪除的 receiver 的 slot **不再被呼叫**（`[INFERENCE]` doActivate 跳過 null receiver）；發射期間新增的連線不在本次發射中被呼叫（`[INFERENCE]`）；允許重入 emit。
- **qtrs required**：MUST 連線順序；MUST 重入 emit 與發射中 connect 安全；**MUST 在 slot 執行之前已被 disconnect 的連線不被呼叫**；MUST `disconnect` 回傳是否真的移除了東西。
- **Current implementation**（`core/signal/signal.rs`）：`Signal<T>` 為 `Arc<Mutex<…>>` 訂閱者列表；`emit` 先**快照**列表、放掉鎖再呼叫，並有 `highest_id` 保護。`IMPLEMENTED`：順序（`test_basic_emission`）、發射中 connect（`test_signal_emit_highest_id_protection`、`test_reentrancy_and_highest_id_guard`）、connect／disconnect／scoped／concurrent emit／non-Send 載荷。
- **Known gap**
  - **G6.1.a [P0, RAN]** **發射期間被 disconnect 的 slot 仍會執行**（快照在呼叫前複製、之後不再檢查）。重現：slot A 在發射中 disconnect slot B → B 仍被呼叫 1 次。
  - **G6.1.b [P0, RAN]** **兩個 Signal 的 `ConnectionId` 在全域表 `GLOBAL_CONNECTIONS` 碰撞**。每個 Signal 以自己的計數器從 1 開始編號，卻共用以 id 為 key 的全域 `HashMap`。重現：兩個 `Signal<i32>` 各以 `connect_to` 接一個 receiver，`id_a=1 id_b=1`；銷毀 receiver 1 後 `a.emit` **仍呼叫 slot**（對照組：只有一個 Signal 時正確為 0 次）。後果：receiver 銷毀時的自動斷線**靜默失效**；`Signal::disconnect(id)` 會刪掉別的 Signal 的全域記錄。`ConnectionId::next()`（全域計數器）存在但沒有 Signal 使用。
  - **G6.1.c [P2, READ]** `disconnect_receiver`／`disconnect_all` 不清 `GLOBAL_CONNECTIONS`（洩漏、stale 記錄）。
  - **G6.1.d [P1, READ]** 無 `UniqueConnection`、`SingleShotConnection`、signal-to-signal 連線（`thread/channel.rs` 的 `connect_to_signal` 是 channel 適配器，不是）。
  - **G6.1.e [D]** slot 需 `Fn(&T) + Send + Sync + 'static`（不能捕捉 `Rc`）；每個 signal 單一型別載荷（多參數用 tuple）；無 PySide 的「參數多於 slot 時截斷」。理由：Rust 型別系統；須在 `qtrs` 文件說明。
  - **G6.1.f [P1, READ]** `connect_with_type(Queued, …)` 只儲存 direct dispatcher，`emit` 時被當 direct 呼叫——「Queued」連線若不是用知道 receiver 的 `connect_*` 建立，會**靜默同步**。
- **Test**：既有如上。必要：`slot_disconnected_during_emit_is_not_called`（RAN 重現失敗）；`two_signals_each_connected_to_receiver_both_cut_on_destroy`（RAN 重現失敗）；`disconnect_on_signal_a_does_not_touch_signal_b_record`；`connect_with_type_queued_is_deferred_or_rejected`。
- **HUD usage**：Python `.connect` 用於選單 action、計時器、`colorSchemeChanged`（`hud_window.py:76,87,88,104,441,660-793`；`tray_icon.py:52-120`；`main.py:74-98`）。Rust：全部是無 receiver 的 `Signal::connect` 閉包（`main.rs:356,374,422,431,517,542,557,579`）——**所以 G6.1.b 與 C6.4 目前碰不到**；`RefreshController.updated` 有宣告有 emit 但沒有任何 connect（grep `\.updated\.connect` 為空）。

### C6.2 連線類型與執行緒解析
- **Qt behavior** `[QT-DOC]`：`AutoConnection` 在**發射時**判斷：發射執行緒 == receiver 目前所屬執行緒 → direct，否則 queued；`QueuedConnection` 永遠 post（同執行緒也一樣）；`BlockingQueuedConnection` 阻塞發射者直到 slot 執行完（同執行緒會死鎖並警告，`[INFERENCE]`）；`DirectConnection` 永遠在發射執行緒同步執行。
- **qtrs required**：MUST Auto 於發射時解析；MUST Queued 同執行緒也 post；MUST Direct 同步；MUST BlockingQueued 阻塞。
- **Current implementation**：`emit` 依 `sub.conn_type` 分支（Direct 直呼叫；Auto 且同執行緒直呼叫；其餘走 queued 路徑：複製載荷、post `MetaCall`）。`IMPLEMENTED`：同執行緒 Auto、同執行緒 Queued、跨執行緒 Queued、BlockingQueued（`test_signal_emit_direct_and_auto_same_thread`、`test_signal_emit_queued_dispatch`、`test_queued_signal_cross_thread_dispatch`、`test_blocking_queued_signal_cross_thread_synchronization`、`layered_tests::test_level3_cross_thread_queued_connection`）。**Auto 跨執行緒發射：`IMPLEMENTED-UNTESTED`**（沒有測試從第二個執行緒發射 Auto 訂閱者）。
- **Known gap**
  - **G6.2.a [P1, READ]** receiver 執行緒在**連線時**擷取，且優先於即時查詢（`signal.rs` `receiver_thread.or_else(query_object_thread)`）。`move_to_thread` 之後 Auto 仍指向舊執行緒。
  - **G6.2.b [P2, READ]** 同執行緒 BlockingQueued 直接呼叫（Qt 死鎖並警告）——安全的偏離，須記載。
  - **G6.2.c [P0, READ]** 目標執行緒無 loop 時 queued 閉包忽略 `post_event_to_thread` 回傳值 → **queued slot 靜默遺失**（見 G3.2.b）。
  - **G6.2.d [P2, READ]** BlockingQueued 無逾時；loop 存在但不跑時發射者永久卡住（post 被丟棄時 `tx` 被 drop，`rx.recv()` 回 Err 而解除）。
  - **G6.2.e [P2]** queued 需 `T: Clone + Send + 'static`。
- **Test**：必要：`auto_connection_queues_when_emitted_from_other_thread`；`auto_connection_reevaluated_after_move_to_thread`；`blocking_queued_same_thread_is_documented_or_rejected`。
- **HUD usage**：**Python 的熱鍵信號從 `threading.Thread` 發射**（`system/hotkey.py:76-104,137`），連到主執行緒的 `hud.toggle_visibility`／`toggle_click_through`（`main.py:74-88`）——依賴 AutoConnection 排入 GUI 執行緒（綁定方法的 QObject 為 receiver context；純函式 `on_hotkey_failed` 的 context 為 `[INFERENCE]`）。Rust 不用 queued 連線，改以手動 `MetaCall` post（`main.rs:40-55,471-483,514-521,525-537`）；等價前提見 C7.9。

### C6.3 sender()
- **Qt behavior** `[QT-DOC]`：在 slot 內 `sender()` 回發射者；slot 之外為 null；queued slot 也看得到；發射者已銷毀則為 null。
- **qtrs required**：MUST 直接與 queued slot 期間回發射者、之後為 `None`；SHOULD 巢狀 emit 還原外層 sender。
- **Current implementation**：thread-local 發射者 id 堆疊；`emit` 在有 emitter id 時以 `SenderGuard` 推入；queued 閉包在 receiver 執行緒重新推入。`IMPLEMENTED`（直接路徑與 emit 後彈出：`test_signal_sender_tracking_and_auto_disconnection`）。
- **Known gap**
  - **G6.3.a [P1, READ]** 無 emitter id 的 Signal（`Signal::new()`，即所有 widget 信號）不設 sender，slot 內 `sender()` 回的是外層發射者或 `None`。
  - **G6.3.b [P2, READ]** 無存活檢查。
  - **G6.3.c [P2]** queued 與巢狀路徑沒有測試。
- **Test**：必要：巢狀 emit 還原外層 sender；queued slot 在 receiver 執行緒看得到發射者；無 emitter 的 signal 在有 emitter 的 signal 的 slot 內 sender 為 `None`。
- **HUD usage**：Python／Rust 皆無（grep `sender()` 為空）。

### C6.4 receiver／emitter 銷毀時自動斷線
- **Qt behavior** `[QT-DOC]`：任一端銷毀即移除連線；之後 slot 永不被呼叫。
- **qtrs required**：MUST 在 receiver 或 emitter 銷毀後，**含已排入佇列的事件**，沒有 slot 執行。
- **Current implementation**：`PARTIAL`。`unregister_qobject`（`Drop` 與 deferred delete 都會呼叫）執行 `disconnect_all_for_object(id)`，依 `sender_id` 或 `receiver_id` 比對。只適用於經 `connect_direct_object/_to/connect_to/connect_object/connect_queued/connect_blocking_queued` 建立的連線；`connect`、`connect_with_type`、`connect_scoped` **從不**註冊全域。
- **Known gap**
  - **G6.4.a [P1, RAN]** 受 G6.1.b 影響。
  - **G6.4.b [P1, READ]** 已排入的 `MetaCall` 不被清除（G3.6.a）。
  - **G6.4.c [P2]** 無 receiver 的閉包連線（HUD 的全部）永不自動移除。
  - **G6.4.d [test gap, 已讀原始碼確認]** `test_signal_sender_tracking_and_auto_disconnection`（`test_qobject_safety_and_qt6_features.rs:214-228`）在 drop receiver 之後 `sig.emit(&2)` **沒有任何 assertion**，即使自動斷線壞了這個測試也會通過。
- **Test**：必要：修好上述測試（斷言 `recv_count` 維持 1）；以 `drop` + `unregister_qobject` 銷毀（不手動呼叫 `disconnect_all_for_object`）；銷毀 emitter 後 receiver 不被呼叫。
- **HUD usage**：Python 以 parenting 隱式達成；Rust 無。

### C6.5 MetaCall、invokeMethod、已刪除的 receiver
- **Qt behavior** `[QT-DOC]`：`~QObject` 移除該物件的 posted events，所以 queued slot 對已刪除 receiver 永不執行；`QMetaObject::invokeMethod` 接受連線類型（Auto/Direct/Queued/BlockingQueued），可呼叫 slot、invokable 與 signal。
- **qtrs required**：MUST 已銷毀 receiver 的 queued slot 不執行；SHOULD `invoke` 可指定連線類型；SHOULD invoke 一個 signal 即發射它。
- **Current implementation**：`notify_helper` 對 `MetaCall` 先試註冊的 receiver，查不到就在 `NullObj` 上執行（故意偏離 Qt）。`invoke_method` 只有 direct。`MethodInvoker` 是普通 `fn` 指標。
- **Known gap**
  - **G6.5.a [P1, READ]** 見 G3.6.a。
  - **G6.5.b [P1, READ]** HUD **依賴**這個 stub 路徑（`ObjectId(0)` 哨兵，見 C3.6）。
  - **G6.5.c [P1, READ]** `invoke_method` 無連線類型參數、無 queued 形式；signal 不可 invoke（invoker 為 `None`）；meta 層只驗參數個數不驗型別；`index_of_method` 取第一個符合者，overload 有歧義。
  - **G6.5.d [D]** 無字串式 `SIGNAL()/SLOT()`、無 `connectSlotsByName`。理由：qtrs 的連線是型別化閉包 API，沒有 moc 字串簽章；誤用在編譯期就不可能發生（不存在該 API）。
- **Test**：必要：同 C3.6；queued `invoke_method` 被延後。
- **HUD usage**：Python `@Slot(str,int,object)` 只在 `_complete`（`refresh_controller.py:98`），且是直接呼叫（`:96`），沒有經信號；`invokeMethod`/`QMetaObject` 未用。

### C6.6 Meta-object、derive、`#[signal]`
- **Qt behavior**：moc 產生含 signals/slots/properties/enums 與 superclass 鏈的 `QMetaObject`；索引穩定。
- **qtrs required**：MUST `inherits`、`class_name`、`index_of_*` 與屬性讀寫可用；SHOULD derive 宣告的 signal 能綁到 emitter。
- **Current implementation**：`IMPLEMENTED`（`MetaObject`、`derive(QObject)`；測試 `test_meta_object_hierarchy_and_inherits`、`test_meta_property_introspection_and_read_write`、`test_meta_enum_key_value_and_flags_conversions`、`test_proc_macro_derive_qobject_and_property_reflection`、`test_meta_type_system_resolution_and_registration`）。
- **Known gap**
  - **G6.6.a [P1, READ]** 無 `#[slot]`；derive 的 signal `invoker = None`，且不會用 `Signal::with_emitter(id)` 接線（→ G2.4.a）。
  - **G6.6.b [P2, READ]** derive 一律把 superclass 設為 `QOBJECT_META_OBJECT`；enums／class-infos 為空。
  - **G6.6.c [P2]** signal／property 型別名稱是 Rust 名（`String`），不是 Qt 名（`QString`）。
  - **G6.6.d [P2]** `index_of_signal` 以名稱或完整簽章比對，property 的 notify signal 不解析成索引。
- **Test**：必要：derive 出的 signal 欄位取得 emitter 並遵守 `block_signals`。
- **HUD usage**：Python `Signal(object|bool|str|())`（`RefreshController`、`GlobalHotkeyManager`）；Rust 不用 `derive(QObject)`（`RefreshController` 不是 QObject）。

### C6.7 PySide6 特有的連線語意
- **Qt/PySide behavior** `[INFERENCE — 實作前須對照 PySide6 行為實測]`：連到 QObject 綁定方法時，該 QObject 為 receiver context（連線隨其銷毀、依其執行緒決定 Auto）；連到非 QObject 可呼叫物件時，PySide 使用內部 context；slot 參數少於 signal 參數時多餘參數被截斷；`@Slot` 無可觀察效果除非走 meta 呼叫。
- **qtrs required**：SHOULD 提供「以 receiver 物件連線」的 API 作為 Python 綁定方法的對應；MUST 在文件說明閉包連線的執行緒語意（發射執行緒）。
- **Current implementation**：`connect(closure)` 永遠 Direct、在發射執行緒執行；receiver 版本為 `connect_to`／`connect_object`／`connect_queued`。
- **Known gap**：**G6.7.a [D]** 無法表達「Python 風格鬆散 slot」（任意可呼叫物件、參數個數可少於 signal）。理由：Rust 型別系統要求 slot 型別與 `Signal<T>` 的 `T` 吻合；誤用為編譯錯誤。HUD 以明確 post 補（見 C7.9）。
- **Test**：以 PySide6 實測 context 行為後補。
- **HUD usage**：見 C6.2。

---

## 7. Thread affinity semantics

> 架構事實（`READ`）：物件有兩個「執行緒」概念——`ObjectData.thread_id`（親和性）與 `ObjectRecord.registration_thread`（實體）。`with_object*` 只在 `registration_thread == 目前執行緒` 時回非 `None`。`move_to_thread` 只改前者。因此所有 QObject callback（事件、filter、有 receiver 的 MetaCall）只能在註冊它的執行緒執行。另有**兩份**每執行緒登記表：`GLOBAL_THREAD_SENDERS`（`move_to_thread` 用）與 `THREAD_EVENT_HANDLES`（`post_event_to_thread`、queued 信號、計時器用）；`EventLoop::new` 只填後者；`CoreApplication::new` 只設 TLS 旗標，**主執行緒沒有 `EventSender`**。

### C7.1 執行緒身分
- **Qt behavior** `[QT-DOC]`：`QThread::currentThread()`、`QObject::thread()`、`QCoreApplication::instance()->thread()`；主執行緒 = 建構 `QCoreApplication` 的執行緒。
- **qtrs required**：MUST 可取得目前／物件所屬執行緒；`is_main_thread()` MUST 在任何非建構 Application 的執行緒上回 false，**與呼叫先後無關**。
- **Current implementation**：`ThreadId` 包 `std::thread::ThreadId`；`main_thread_id()` 是 `get_or_init(ThreadId::current)`，**先呼叫者勝**；`set_main_thread_id` 從未被呼叫；`CoreApplication::new` 只設 TLS 旗標。
- **Known gap**
  - **G7.1.a [P1, READ]** 無 TLS context 的執行緒（pool worker、一般 `std::thread`、HUD 熱鍵執行緒）若在別人初始化之前呼叫 `is_main_thread()`，自己會變成「主執行緒」。
  - **G7.1.b [P2]** 無 `QObject::thread()`（只有自由函式 `query_object_thread`）。
  - **G7.1.c [P1]** `ThreadPool` worker 從不呼叫 `init_current`。
- **Test**：必要：`main_thread_id_is_app_thread_even_if_worker_asks_first`；`pool_worker_is_not_main`。
- **HUD usage**：Rust 把 UI 執行緒的 `ThreadId::current()` 傳進 `run_on_main_thread`（`main.rs:324,40-55`），不呼叫 `is_main_thread`；Python 無。

### C7.2 跨執行緒 post 與喚醒
- **Qt behavior** `[QT-DOC]`：`postEvent` 執行緒安全；事件進接收者執行緒的佇列並喚醒其 dispatcher；目標執行緒尚無 loop 時事件**保留**。
- **qtrs required**：MUST A 執行緒 post 到 B 的佇列時喚醒 B 並依 FIFO／優先序送出；MUST **不遺失** B 的 loop 建立之前 post 的事件；SHOULD 所有入口共用同一壓縮／優先序路徑。
- **Current implementation**：主路徑 `IMPLEMENTED`（`EventLoopHandle::post_event_with_priority`：鎖佇列、壓縮、依優先序插入、喚醒；`post_event_to_thread`）。測試 `loop.rs::test_cross_thread_wakeup`、`test_thread_system.rs::test_thread_with_event_loop`、`layered_tests.rs::test_level3_cross_thread_queued_connection`、`dispatcher_win.rs::test_wakeup_message_is_deduplicated_and_pumped_by_wnd_proc`（僅同執行緒）。
- **Known gap**
  - **G7.2.a [P0, READ]** 目標執行緒無已註冊 loop 時回 false／靜默丟（同 G3.2.b）。
  - **G7.2.b [P2, READ]** `EventSender::send` 直接推進 `q.events`，**不壓縮、無優先序、忽略 `insertion_offset`**（僅 `move_to_thread` 與 `EventLoopThreadHandle::quit/post_event` 用）。
  - **G7.2.c [P2]** 兩份登記表可能分歧。
  - **G7.2.d [P1, READ]** 無 `removePostedEvents`（→ G3.6.a）。
  - **G7.2.e [P2, READ]** `Widget::update()` 以 `ThreadId::current()` 為目的地而非 widget 親和性（`widget.rs:296`、`input_common.rs:231`、`menu.rs:1567`）；worker 呼叫 `update()` 會 post 進錯誤／不存在的佇列。
- **Test**：必要：`post_before_loop_exists_is_delivered_when_loop_starts`；`event_sender_send_applies_compression`；`destroyed_receiver_drops_pending_events`。
- **HUD usage**：Rust `run_on_main_thread`（`main.rs:40-55`，用於 `:473` 熱鍵、`:518` 主題、`:527` 刷新結果）。Python 無。

### C7.3 跨執行緒連線類型
- **Qt behavior**：同 C6.2；另外 `QFutureWatcher` 的信號在 watcher 所屬執行緒送達 `[QT-DOC]`。
- **qtrs required**：同 C6.2；`FutureWatcher`／`Promise` 的信號 MUST 在 watcher 所屬執行緒送達。
- **Current implementation**：連線類型本身見 C6.2；`thread/future.rs` 的信號在生產者執行緒以 `connect`（Direct）發射。
- **Known gap**：C6.2 的 G6.2.a–e 全部適用，另外：**G7.3.a [P1, READ]** `FutureWatcher`／`Promise` 的信號在**生產者執行緒**以普通 `connect`（Direct）發射，handler 在 worker 上跑（Qt 的 `QFutureWatcher` 信號在 watcher 所屬執行緒送達）；`finish()` 喚醒 waker 但不發 watcher 信號；`Receiver::connect_to_signal` 在發送執行緒發射。
- **Test**：必要：`future_watcher_finished_runs_on_watcher_thread`；其餘見 C6.2。
- **HUD usage**：無（HUD 不用 `FutureWatcher`；跨執行緒機制見 C7.9）。

### C7.4 接收者執行緒派送、foreign-thread sendEvent、MetaCall 替身
- **Qt behavior** `[QT-DOC]`：`sendEvent` 在**呼叫端**執行緒同步遞送（對他執行緒擁有的物件是會警告的 bug 但仍遞送）；queued `QMetaCallEvent` 對已銷毀 receiver 丟棄；receiver 存活則在其執行緒執行。
- **qtrs required**：MUST QObject callback 只在擁有者執行緒執行；foreign-thread `send_event` MUST **可見地**被拒或被 marshal，**絕不**在非擁有者執行緒執行擁有者的 callback；MUST 一個 receiver 在別的執行緒存活的 MetaCall **不得**在錯誤執行緒用替身靜默執行。
- **Current implementation**：`IMPLEMENTED-UNTESTED`（foreign-thread 情況）。`notify_helper`：`MetaCall` 先試 `with_object_mut(receiver, task)`；`None`（未註冊、已銷毀、**或註冊在別的執行緒**）就在目前執行緒的 `NullObj` 上執行閉包；非 MetaCall 事件 → `dispatch_to_object` 回 false，靜默。
- **Known gap**
  - **G7.4.a [P1, READ]** foreign-thread `send_event` 對存活物件回 false 且**無診斷**。
  - **G7.4.b [D]** `move_to_thread` 之後事件被路由到目標佇列，但 `with_object_mut` 在那邊失敗（registration_thread 未變）→ 非 MetaCall 事件被丟、MetaCall 在替身上跑；被移動的物件**實質上無法遞送**。理由：物件以 thread-local 原始指標註冊；但誤用不得靜默（須回傳錯誤）。
  - **G7.4.c [P1, READ]** stale MetaCall 對已銷毀 receiver 照跑（G3.6.a）。
- **Test**：必要：`send_event_from_foreign_thread_does_not_invoke_owner_callback`；`metacall_for_object_registered_on_other_thread_does_not_run_on_caller`（目前會失敗）。
- **HUD usage**：Rust `run_on_main_thread` 用 `ObjectId(0)`，因此恆走替身，再經 thread-local `MAIN_HUD` 到 HUD（`main.rs:36-37,48-52`）。

### C7.5 QObject::moveToThread
- **Qt behavior** `[QT-DOC]`（ThreadChange 時序、計時器重新註冊為 `[INFERENCE]`）：只能從物件所在執行緒呼叫；拒絕有 parent 的物件與 widget；在切換前對根與所有後代送 `ThreadChange`；posted events 移到新佇列；**計時器在新執行緒重新註冊**；children 跟隨；允許 `moveToThread(nullptr)`。
- **qtrs required**：MUST 拒絕有 parent（`HasParent`）與錯誤執行緒（`WrongThread`）；MUST cascade 到**所有**後代；MUST 計時器在新執行緒繼續跑；MUST 不丟佇列事件；SHOULD 送 `ThreadChange`。
- **Current implementation**：自由函式 `move_to_thread(&mut ObjectData, target, caller)`（`object/thread.rs`）。`IMPLEMENTED`：HasParent／WrongThread／單層 cascade／事件轉移／`ThreadChange`（`thread.rs::tests::*`、`test_move_to_thread_cascades_children_and_events`、`test_move_to_thread_rejects_parented_and_migrates_subtree`）。
- **Known gap**
  - **G7.5.a [P1, READ]** **計時器被停止、不遷移**（`stop_timers_for_object` 作用於呼叫執行緒的 registry，沒有人在目標重新註冊）；`ObjectData.timers` 仍列出死掉的 id。
  - **G7.5.b [P1, READ]** **事件可能遺失**：從來源佇列移除後，只有 `query_thread_sender(target)` 有值才重送；目標沒有條目就丟。且主執行緒沒有 sender，所以 **main→worker 的移動不會轉移主佇列的任何東西**。
  - **G7.5.c [P2, READ]** 轉移的事件繞過壓縮／優先序。
  - **G7.5.d [P2, READ]** 只更新一層 children 的 `ObjectData.thread_id`，更深後代只更新 registry，`connect_to` 之後讀到 stale 欄位。
  - **G7.5.e [P2, READ]** `ThreadChange` 在親和性更新**之後**送（Qt 之前，`[INFERENCE]`），且在持有同一已註冊物件 `&mut ObjectData` 時 `send_event`（aliasing 疑慮）。
  - **G7.5.f [P2]** `caller` 是參數而非從 OS 讀取。
  - **G7.5.g [P2]** target 沒有 loop 時仍回 `Ok`。
  - **G7.5.h [D]** 物件實體仍綁註冊執行緒（見 G7.4.b）。理由：同 G7.4.b（registry 存 thread-local 原始指標）；誤用（在目標執行緒存取被移動的物件）必須回傳錯誤而非靜默。
- **Test**：必要：`move_to_thread_keeps_running_timer`；`move_to_thread_three_levels_updates_all_thread_id_fields`；`move_to_thread_from_main_thread_transfers_pending_events`（用真正的 `CoreApplication::new`，不用手動 `EventSender`）；`move_to_thread_without_target_sender_does_not_drop_events`。
- **HUD usage**：Python／Rust 皆無。

### C7.6 QThread 類 API
- **Qt behavior**：`start/quit/exit/wait/isRunning/isFinished/requestInterruption/started/finished`、優先序、`run()` 內每執行緒事件迴圈；`finished` 在該執行緒發射。
- **qtrs required**：MUST 帶 loop 的 worker 收得到 posted events／queued 信號；`quit` 結束 loop；`is_finished` 反映完成；優先序 SHOULD 生效或不提供此 API。
- **Current implementation**（`thread/thread.rs`）：`spawn`、`spawn_with_event_loop`、`EventLoopThreadHandle::quit`。`IMPLEMENTED`：spawn／中斷／帶 loop 執行緒 + quit（`test_thread_spawn_and_interruption`、`test_thread_with_event_loop`）。
- **Known gap**
  - **G7.6.a [P1, READ]** `ThreadHandle::is_finished()` 在 `join(self)` 之前**恆為 false**（旗標由 builder 建立卻從未與執行緒共享，只有消耗 handle 的 `join` 會設）。
  - **G7.6.b [P1, READ]** `ThreadBuilder::priority` 存了但從不套用。
  - **G7.6.c [P1]** 無 `started/finished` 信號、`wait(timeout)`、`exit(code)`、`terminate`、`isRunning`。
  - **G7.6.d [P2]** 閉包 panic 會跳過 `clear_current`（無 guard）。
  - **G7.6.e [P2]** `EventLoopThreadHandle` 無 Drop／join，丟棄即分離。
- **Test**：必要：`thread_handle_is_finished_true_after_thread_returns_without_join`；`quit_ends_event_loop_after_queued_events_run`。
- **HUD usage**：Python 只用 `threading.Thread`；Rust 只用 `std::thread::Builder`（`refresh_controller.rs:132`、`hotkey.rs:126`、`config.rs:438`、`providers/agy.rs:259,265`）；`qtrs_core::thread::*` 在 `rust/src` 無任何使用。

### C7.7 計時器與執行緒
- **Qt behavior**：計時器屬於物件所在執行緒，該執行緒需要 dispatcher（否則警告）；從別的執行緒啟動不被支援。
- **qtrs required**：在沒有 loop 的執行緒啟動計時器 MUST 可見地失敗；MUST 擁有者檢查。
- **Current implementation**：per-thread TLS `THREAD_TIMER_CONTEXT`（`EventLoop::new` 設定）；沒有 context 時 `Timer::start` 靜默回 `INVALID`；從非擁有者執行緒啟動會註冊到**呼叫者**的 registry（無擁有者檢查）。
- **Known gap**：**G7.7.a [P2, READ]** 靜默 no-op 取代警告；**G7.7.b [P2, READ]** 無擁有者檢查。
- **Test**：必要：`timer_start_on_thread_without_loop_reports_error`；`single_shot_zero_on_worker_with_loop_runs_on_worker`。
- **HUD usage**：計時器只在 UI 執行緒使用（Python 與 Rust）。

### C7.8 ThreadPool / Future / channel
- **Qt behavior**：`QThreadPool`、`QFuture`/`QFutureWatcher`（信號在 watcher 所屬執行緒）。
- **qtrs required**：同 C7.3 的信號執行緒要求。
- **Current implementation**：`thread/pool.rs`（worker、panic 捕捉）、`future.rs`（`Promise/Future/FutureWatcher`）、`channel.rs`（`attach_event_sender` 喚醒 loop、`connect_to_signal`）。`IMPLEMENTED`：`test_thread_pool_execution_and_cancellation`、`test_future_promise_and_watcher`、`test_channel_basic_and_signal_integration`。
- **Known gap**：同 G7.3.a 與 G7.1.c；`Sender::send` 喚醒綁定的 `EventSender` 但不對任何 receiver 物件遞送，消費者仍要自己輪詢。
- **Test**：見 C7.3。
- **HUD usage**：無（HUD 用 `std::sync::mpsc`：`refresh_controller.rs:52,188`）。

### C7.9 兩個 HUD 實際如何在執行緒之間移動工作（等價要求）
- **Qt behavior（Python HUD 的做法）**：worker `threading.Thread` + `queue.SimpleQueue`（`refresh_controller.py:81-92`）→ 主執行緒 25 ms `QTimer` 排空（`:35-38`）；**規格要求 worker 從不呼叫 Qt**（`PROJECT_SPEC.md` 排程不變量）。熱鍵執行緒**直接發射 Qt 信號**（`hotkey.py:97-101,127-137`）依賴 AutoConnection。
- **qtrs required（等價條件）**：Rust 的做法與 Python 在可觀察層面等價，**僅當**：(a) 主 loop 已註冊（否則 post 被丟，G7.2.a）；(b) MetaCall 在 UI 執行緒執行（經 `THREAD_EVENT_HANDLES[main]`）；(c) 每個結果恰好被處理一次（Rust 同時有 100 ms 輪詢與 immediate callback，必須冪等）。
- **Current implementation**：provider worker（`thread::Builder`，`refresh_controller.rs:132-197`）→ `mpsc` → UI 執行緒 100 ms `poll_timer`（`main.rs:554-573`）**加上** worker 呼叫 `notify_callback` 立刻 post MetaCall（`main.rs:525-537`）。熱鍵：Win32 訊息迴圈執行緒設 `AtomicBool` 並 notify（`hotkey.rs:263-279`）→ post MetaCall（`main.rs:471-483`）。單一實例 IPC 執行緒只設 `WAKE_REQUESTED`（`main.rs:279-284`），由 1 s 計時器消費（`:546`）。
- **Known gap**
  - **G7.9.a [P2, READ；P0 主張已被讀碼推翻，待驗證]** 啟動競態：worker／熱鍵執行緒是否可能在主 loop 註冊前就 post？讀碼：`Application::new`（`main.rs:319`）在 `application/mod.rs:120` 註冊 loop，早於 provider worker（`hud_window.rs:408`）與熱鍵執行緒（`main.rs:466`）；單一實例 IPC 執行緒（`main.rs:269`，早於註冊）只寫 atomic。**這不證明所有 interleaving 皆安全**（未執行）；它只是 G3.2.b 的假設性表現，G3.2.b 修復後自然消除。**不是 `D`。**
  - **G7.9.b [P1]** 發佈設定 `panic = "abort"`（`rust/Cargo.toml`）：任何 worker panic 會終止整個行程；Python 有 `threading.excepthook`／`sys.excepthook`（`core/logger.py:64,73`），行為不同。
- **Test**：必要：`worker_post_before_main_loop_registered_is_not_lost`（App 層，模擬啟動順序）。
- **HUD usage**：見上。

---

## 8. QWidget semantics

### C8.1 可見性
- **Qt behavior** `[QT-SRC qwidget.cpp:8465-8468,5433-5434; qlayoutitem.cpp:691-693]`：`setVisible` 送 `Show/Hide`（及 `ShowToParent/HideToParent`）；對 child 的 hide／show 使 parent layout 失效（或 post `LayoutRequest`）；`QWidgetItem::isEmpty()` = 明確隱藏（`isHidden()`）或 `isWindow()`——**尚未 show 過的 widget 仍被排版**；`retainSizeWhenHidden` 可覆寫。
- **qtrs required**：隱藏 widget MUST 不佔空間也不佔 spacing；show／hide MUST **不需 app 手動呼叫 `update_layout`** 就重排 parent；隱藏 parent 的 child MUST 不繪製、不 hit-test；SHOULD 送 Show/Hide 事件、隱藏 widget 的 geometry 不被改動。
- **Current implementation**：`PARTIAL`。`set_visible` 只翻 `Cell<bool>` 並 `update()`；`item_is_empty = !is_visible()`（對應 `isHidden` 語意）；隱藏 child 被繪製與 hit-test 跳過。`ABSENT`：Show/Hide 事件（grep `EventKind::Show|EventKind::Hide` 於 `qtrs-widgets/src` 為空，雖然 core 有這兩個 kind）；`retain_size_when_hidden`。
- **Known gap**
  - **G8.1.a [P1, READ]** **show／hide 不自動重排**；HUD 以手動 `update_layout()` 補（`provider_card.rs:347-348,406-420,435`、`hud_window.rs:254,590,608,636`）。
  - **G8.1.b [P2, READ]** 隱藏 item 的 geometry 被設為 (0,0,0,0)（Qt 不動它）。
  - **G8.1.c [P1, READ]** 無 Show/Hide 事件；依賴 `showEvent` 的子類別無法實作；`Window::show/hide` 不通知 widget 樹。
- **Test**：既有無（probe 只涵蓋建構時 hidden）。必要：`hide_child_relayouts_parent_without_manual_call`；`show_hide_events_delivered`。
- **HUD usage**：Python `setVisible`（`hud_window.py:156,292,306,520,671`、`provider_card.py:50,139-193`）；Rust 同位置。

### C8.2 Enabled
- **Qt behavior** `[QT-SRC qwidget.cpp:3405-3476]`：`setEnabled` 傳遞到所有後代、清除被停用的焦點 widget、送 `EnabledChange`、重繪；QSS `:disabled` 生效。
- **qtrs required**：MUST 傳遞到後代、MUST 重繪、停用 widget MUST 不收滑鼠／鍵盤／焦點；`:disabled` SHOULD 一致。
- **Current implementation**：`PARTIAL`。`set_enabled` 只 `Cell.set`：不傳遞、不重繪、無事件、不處理焦點；`Button` 在 handler 內自己檢查；`:disabled` 從未提供給樣式解析（`pseudo_states` 是 `&[]` 或只有 hover/pressed）。
- **Known gap**：**G8.2.a [P1, READ]** 傳遞、重繪、`EnabledChange`、焦點清除、`:disabled` 全缺。
- **Test**：必要：`disable_parent_disables_children_and_repaints`；`disabled_button_ignores_press`；`qss_disabled_color`。
- **HUD usage**：Python `self.icon.setEnabled(not muted)`（`usage_table.py:323`）；Rust 自訂 icon widget 把 `set_enabled` 轉給 base（重繪視 widget 而定）。

### C8.3 幾何、最小／最大／固定大小、size policy
- **Qt behavior** `[QT-SRC qlayoutitem.cpp:578-676]`：`setMinimumSize/setMaximumSize/setFixedSize` 夾住 `resize/setGeometry` 並呼叫 `updateGeometry()`；`QWidgetItem` 的大小用 `qSmartMinSize/qSmartMaxSize`。
- **qtrs required**：MUST 對 HUD 用到的每種 widget 提供 min/max/fixed 與 `setSizePolicy`；變更 MUST 使 parent layout 失效。
- **Current implementation**：`PARTIAL`。`Widget`/`WidgetBase` **沒有** `set_minimum_size/set_maximum_size/set_fixed_size`（grep：只有 `Window::set_minimum_size`）；min/max 只來自 QSS（Label、Button、Frame、ProgressBar）或自訂 `Widget` 覆寫；trait 預設 `set_size_policy` 是**靜默 no-op**，`Label` 沒有覆寫。`Widget::size_hint` 預設 (100,30)（QWidget 為無效 (-1,-1)）。
- **Known gap**
  - **G8.3.a [P1, READ]** 無通用 min/max/fixed API。
  - **G8.3.b [P0, READ]** **`Label.set_size_policy` 被丟棄**。Python `title.setSizePolicy(Minimum, Preferred)`（`provider_card.py:39`）在 Rust 無對應呼叫（grep `set_size_policy` 於 `provider_card.rs` 為空）→ 卡片模式標題寬度行為可能不同。
  - **G8.3.c [P2, READ]** `WidgetBase::set_geometry` 不夾 min/max（只有 `item_set_geometry` 夾）。
  - **G8.3.d [P2]** 預設 size_hint 100×30 會讓忘了覆寫的自訂 widget 得到假值。
  - **G8.3.e [P1, READ]** `UsageDial`：Python `setMinimumSize(84,84)`（`usage_table.py:138`）；Rust `minimum_size()` 為 0×0，並註解稱最小值「會限制 dial」（`usage_table.rs:160-163`）——最小值不會限制上限，`[INFERENCE]` 非刻意，視窗很窄時 dial 可縮到 84 以下。
- **Test**：既有 `test_button_layout_toggle_btn_size_hint_matches_qt`、`test_label_box_model.rs::*`。必要：`label_set_size_policy_is_honoured`；`set_minimum_size_clamps_geometry_and_layout`。
- **HUD usage**：Python `setSizePolicy`（`provider_card.py:39`、`usage_table.py:139`）、`setMinimumSize`（`hud_window.py:296,327,347`、`usage_table.py:138`）；`setFixedSize/Width/Height` 無使用。Rust `set_size_policy`（`hud_window.rs:272,284`、`usage_table.rs:62,404,589,740`）。

### C8.4 滑鼠事件遞送
- **Qt behavior** `[QT-SRC qapplication.cpp:2738-2763,2745-2751,2037-2123]`：(1) 未被 accept 的滑鼠事件沿 parent 鏈上傳，直到被 accept／到視窗／`WA_NoMousePropagation`；(2) 沒按鍵的 MouseMove 只送給有 `mouseTracking` 的 widget，沒有的就**吞掉**（`res = true`，不再上傳）；(3) 按下後 move／release 隱式 grab 到被按下的 widget，即使游標移出；(4) Enter／Leave 對每個進入／離開的祖先送出，以共同祖先計算（`dispatchEnterLeave`）。
- **qtrs required**：MUST 按住按鍵時 press／release／move 送給 press 目標；MUST 未 accept 的 press／release 上傳到祖先（HUD 的拖曳／縮放依賴此）；MUST Enter／Leave 送給祖先鏈扣除共同祖先；MUST 一般 MouseMove 只給 tracking widget（含吞掉規則）。
- **Current implementation**：`PARTIAL`。`EventTreeDispatcher::dispatch_event_internal`（`hit_test.rs`）只送給 hit-test 的**單一葉節點**並回傳其 `event()` 結果——**沒有 parent fallback**；MouseMove 一律送（無 tracking 概念）；Enter/Leave 只在連續葉目標之間；grab 只存在於 popup（`PopupManager::mouse_grabber`），且只用於 press／release；無隱式 press grab；`ScrollBar` 拖曳離開 bar 後就不再跟隨；modifiers 對 press／release 恆為 0；`Button` 按下後移出再移回不會 click。`Window` 在派送的 press 回 `false` 時才退回 `mouse_press_cb`——只對 press 模擬「冒泡到頂層」。
- **Known gap**
  - **G8.4.a [P0, READ]** 無 parent 傳遞（只對 press 模擬）；HUD 的拖曳／縮放是 Python 依賴子 label／button 冒泡到視窗，Rust 靠 fallback 模擬。
  - **G8.4.b [P1]** 無 tracking 語意（Python HUD `setMouseTracking(True)`，`hud_window.py:129,140`；Rust 過度遞送，無害但不相等）。
  - **G8.4.c [P1]** 無隱式 grab。
  - **G8.4.d [P1]** Enter/Leave 非祖先鏈；既有 `test_hover_enter_leave_events_transition` 釘住「child→parent 送 Leave(child)+Enter(parent)」——**實作前先對照 `qapplication.cpp:2037-2123` 確認該序列是否為 Qt 行為，不是就改寫測試**。
  - **G8.4.e [P2]** 無 `WA_TransparentForMouseEvents`／`WA_NoMousePropagation`；視窗離開時 Leave 只送最後一個葉。
  - **G8.4.f [P1]** 右鍵 `context_menu_cb` 在 release 時觸發，與 widget 是否 accept 無關。
  - **G8.4.g [P1]** `Window` 沒有 release／double-click／move handler（見 C11.2、G12.5.f）。
- **Test**：既有 `test_widget_hit_test_and_event_dispatch`、`test_hover_enter_leave_events_transition`、`test_builtin_button_click_and_state_transition`。必要：`unaccepted_press_bubbles_to_parent`；`accepted_press_stops_bubbling`；`press_grab_routes_move_and_release_outside_widget`；`enter_leave_ancestor_chain_minus_common_ancestor`；`mousemove_without_tracking_is_swallowed`。
- **HUD usage**：Python `mousePressEvent/MoveEvent/ReleaseEvent/resizeEvent/moveEvent`（`hud_window.py:560-606`）；`enterEvent/leaveEvent/eventFilter` 無。Rust：`set_mouse_move_handler/set_mouse_press_handler/set_resize_handler`（`hud_window.rs:325,352,368`）。

### C8.5 樣式表、polish、動態屬性
- **Qt behavior** `[QT-DOC]`：`setStyleSheet` 重新 polish widget 與後代、觸發 `StyleChange` 與 `updateGeometry`；選擇器依繼承比對型別（`QFrame` 命中 `QLabel`）、`#id`、`[prop="v"]`、pseudo-state、sub-control；祖先樣式表串接、widget 自己的覆蓋；`setProperty` + `unpolish/polish` 重新評估 `[prop]` 規則。
- **qtrs required**：MUST 符合 HUD 用到的 QSS 規則（型別、`#id`、`[prop]`、`:hover`、`::chunk`、串接順序 app < 祖先 < 自己）；屬性／樣式變更 MUST 重新解析；影響尺寸時 MUST 重排。
- **Current implementation**：HUD 子集 `IMPLEMENTED`，整體 `PARTIAL`。串接 app→祖先→自己、依 specificity（`widget.rs` `resolve_style`、`style/stylesheet.rs`）；樣式於每次 `size_hint`／paint **lazily 解析**（無快取，故無 stale polish）。`selector_matches` 只比對 exact `type_name`、`*`、`QWidget`。只有 `QLabel`、`QPushButton`、`QFrame`、`QProgressBar` 會解析樣式。`attributes`（供 `[state=…]`）只有 Label 提供。
- **Known gap**
  - **G8.5.a [P1, READ]** 無繼承比對（`QFrame{}` 命不中 `QLabel`）；無 descendant/child 組合子（HUD 不用）。
  - **G8.5.b [P1, READ]** `attributes` 只有 Label；同一條規則對 Button／Frame／ProgressBar 無效。
  - **G8.5.c [P0, READ]** **樣式變更不重排**：`WidgetBase::set_style_sheet` 只標 dirty；`Label::set_text` 會 `request_layout`，但 `set_font`/`set_alignment`/style/property 不會，`Button::set_text/set_font` 也不會——需要手動 `update_layout`。
  - **G8.5.d [P0, READ]** `Window::set_style_sheet` 是**整個 Application 的**（呼叫 `Application::set_style_sheet`），Python `HUDWindow.setStyleSheet` 只作用於該子樹（`hud_window.py:246,250`）；Rust HUD 兩者都呼叫（`hud_window.rs:231-232`）→ 影響其他頂層視窗與 popup。
  - **G8.5.e [P1, READ]** `:disabled`/`:focus` 不支援。
  - **G8.5.f [P1, READ]** **QMenu 規則被解析但從不被消費**：`type_name: "QMenu"` 在原始碼中不存在；選單外觀來自寫死的 `MenuStyle`（`rust/src/ui/tray_icon.rs`），手動複製了 Python QSS 的數值；`QMenu::item:selected/:disabled` 不驅動 hover／停用色。
  - **G8.5.g [P2, READ]** `margin-*` 長手寫被解析後在 `apply_declaration` 丟棄；`margin` 只有選單消費。
  - **G8.5.h [P2, READ]** 父 widget 的 `font` 繼承未實作（`[INFERENCE]`，未對照 `qstylesheetstyle.cpp`）。
- **Test**：既有 `test_stylesheet_style.rs::*`、`test_menu_style_box_model.rs`（測 `MenuStyle`，非 QSS）。必要：`qframe_rule_matches_qlabel`；`property_change_relayouts`；`window_stylesheet_is_scoped_to_subtree`；`attribute_selector_on_button_and_frame`；**樣式表字串逐項比對**（見 C12.3）。
- **HUD usage**：Python `setStyleSheet`（`hud_window.py:149,246,250,457`、`provider_card.py:34-212`、`usage_table.py:330,368,381`）、`setProperty + polish`（`usage_table.py:258-260`）、`ui/styles.py` 全部規則；Rust `set_style_sheet`（`hud_window.rs:76,231,579,714`、`provider_card.rs:100,124`）、`set_property`（`usage_table.rs:879`）。

### C8.6 視窗屬性與旗標
- **Qt behavior** `[QT-DOC]`：`setWindowFlags/setWindowFlag` 重建原生視窗（會隱藏，Python 之後呼叫 `show()`：`hud_window.py:516,808`）；`WA_TranslucentBackground`、`WA_TransparentForMouseEvents` 是 per-widget 屬性；`setWindowOpacity`。
- **qtrs required**：MUST 建立時支援 frameless + tool + topmost + translucent；執行時切換 stays-on-top／click-through MUST 保持視窗顯示且狀態一致。
- **Current implementation**：`IMPLEMENTED-UNTESTED`，**API 為客製而非 Qt 形狀**。沒有通用 attribute／flag API（grep `set_attribute|WA_|set_window_flags` 於 `qtrs-widgets/src` 只有傳給 `Window::new` 的 `WindowFlags` bitflags）。`set_stays_on_top`、`set_click_through`、`set_opacity`、`set_cursor` 轉發到平台視窗。`hit_test` 不尊重 `WA_TransparentForMouseEvents`。
- **Known gap**
  - **G8.6.a [P2]** 無 per-widget `WA_*` 屬性（含 `WA_TransparentForMouseEvents`）。沒有工程理由；需要時須補，且在補上之前 API 不得暗示支援。
  - **G8.6.b [P1, READ]** `set_stays_on_top` 執行期路徑沒有測試（`test_hud_extensions` 只測 opacity／min size）。
  - **G8.6.c [P1]** 無 layout 導出的頂層最小尺寸（見 C9.5）。
- **Test**：既有 `test_window_opacity_and_minimum_size`、`test_layered_geometry_sync::hud_style_window_content_rect_equals_hwnd_rect_every_iteration`、`test_per_monitor_dpi_sync`。必要：`click_through_toggle_roundtrip`；`stays_on_top_toggle_keeps_window_visible`（斷言 `WS_EX_TOPMOST`／`WS_EX_TRANSPARENT`）。
- **HUD usage**：Python `hud_window.py:123-132,488,513-516,807-808`；Rust `hud_window.rs:207-215,529`。

### C8.7 繪製順序與裁剪
- **Qt behavior** `[QT-DOC]`：child 在 parent 之後、按 z 序繪製，且**被裁剪到自己的矩形與 parent 內容**。
- **qtrs required**：child MUST 被裁剪到自己的 geometry；widget `paint_event` MUST 不畫到自己矩形外。
- **Current implementation**：`PARTIAL`。`render_widget_recursive`：可見性 + dirty 相交裁剪 → `translate` → `paint_event` → children；**沒有 per-widget clip**（`set_clip_rect` 只有 dirty clip 與 `scroll.rs`、`backing_store.rs` 呼叫）。
- **Known gap**：**G8.7.a [P1, READ]** 無 child 裁剪（溢出的 label 文字、自訂 painter 不被裁）；**G8.7.b [P2]** 髒區只有整個 widget。
- **Test**：既有 `test_dirty_region_culling_skips_non_intersecting_widgets`。必要：`child_painting_is_clipped_to_its_geometry`。
- **HUD usage**：自訂 painter（`UsageDial`、icons，`usage_table.rs`）。

### C8.8 Tooltip
- **Qt behavior** `[QT-DOC]`：`setToolTip` 在 hover 一段時間後顯示。
- **qtrs required**：MUST widget 層級 tooltip（Python HUD 以 tooltip 顯示錯誤與過期資料）。
- **Current implementation**：`ABSENT` for widgets（grep `tooltip` 只有 `action.rs` 與 tray）。tray tooltip 存在。
- **Known gap**：**G8.8.a [P0, READ]** Python 在 `provider_card.py:125`、`usage_table.py:318,328,339,373-375`、`hud_window.py:155,160` 設定 tooltip；Rust 一個也沒有。
- **Test**：必要：`widget_tooltip_shows_after_hover_delay`（真機）。
- **HUD usage**：見上。

---

## 9. Layout semantics

### C9.1 Box layout（QBoxLayout）
- **Qt behavior** `[QT-SRC qboxlayout.cpp:242-340]`：`QBoxLayoutPrivate::setupGeom` + `qGeomCalc`；spacing 只在非空 item 之間；`addStretch` = `QSpacerItem(0,0,Expanding,Minimum)`（空 item，不佔 spacing）；item stretch 來自 `addWidget(w, stretch)`，否則 `QSizePolicy::horizontalStretch`。
- **qtrs required**：MUST 對扁平 H/V layout，在 §9.7 列出的 policy／stretch／min／max 組合下**逐像素**重現；MUST 維持 spacer 語意。
- **Current implementation**：`IMPLEMENTED` + `DIFF`：`BoxLayout::setup_geom`（`layout.rs`）移植 `setupGeom`；`activate` = `QBoxLayout::setGeometry`；`q_geom_calc`／`smart_min_size`／`smart_max_size`／`item_*`（`layout_engine.rs`）移植 `QWidgetItem`。
- **Known gap**
  - **G9.1.a [P1, READ]** `add_stretch(0)` 被強制成 1（`layout.rs` `stretch.max(1)`）；Qt 的 `addStretch(0)` stretch 為 0。
  - **G9.1.b [P1]** 無 `add_spacing`／`add_spacer_item`／`insert_stretch`／`set_stretch_factor`（grep 為空）。
  - **G9.1.c [P1]** 無 item 對齊（見 C9.3）。
  - **G9.1.d [P1]** 無 `heightForWidth`（grep `height_for_width|has_height` 為空）——換行 label 無法如 Qt 排版。
  - **G9.1.e [P1]** 無 `retainSizeWhenHidden`、無 RTL（`Direction` 只有 TopToBottom／LeftToRight）、無 `SizeConstraint`。
- **Test**：既有 `test_vbox_and_hbox_layout_calculation`、`test_box_layout_add_stretch`、`test_layout_stretch_minimum.rs`（6 項，用 `spacer=0`，而 layout 實際用 `-1`）、`qt_layout_compare.py`（手動，見 C9.7）。必要：`add_stretch_zero_matches_qt`（需 PySide6 參考值）。
- **HUD usage**：Python `QVBoxLayout/QHBoxLayout`（`hud_window.py:135,143,168,280,336`；`provider_card.py:25-90`；`usage_table.py:114,273-276,411`）、`addStretch`（`hud_window.py:286`、`provider_card.py:42,63,90`、`usage_table.py:123,278,285`）。Rust：對應檔案的 `BoxLayout::` 與 `add_stretch(1)`。

### C9.2 Grid layout（QGridLayout）
- **Qt behavior**：`QGridLayoutPrivate::setupLayoutData/distribute` + `distributeMultiBox`、row／col stretch、minimum width／height、span、空 row 周圍的 spacing。
- **qtrs required**：MUST 重現 HUD 用到的 row／col stretch、`setRowMinimumHeight`、span、對齊。
- **Current implementation**：`IMPLEMENTED` + `DIFF`（Probe widget、2–3 欄、span ≤ 2、row 0 的欄 stretch、row stretch）。`GridLayout::setup_layout_data`、`activate`、`find_size`、`setup_spacings`、`distribute_multi_box`、`init_empty_multi_box`。`set_row_minimum_height`／`set_column_minimum_width` HUD 有用但**不在 harness 內**（只靠 `usage_table.rs` 的固定 PySide6 數值測試 `test_grid_matches_qt_geometry`）。
- **Known gap**
  - **G9.2.a [P0, READ]** **無 per-item 對齊**。Python 傳 `AlignVCenter|AlignLeft`／`AlignHCenter` 給 `addWidget`（`usage_table.py:392,417,430`）；Rust `add_widget(widget,row,col)`／`add_widget_with_span` 沒有對齊參數（`layout.rs`），Rust 表格以 wrapper + stretch 模擬垂直置中（`usage_table.rs:1173-1209`）。對齊也會改變 `expandingDirections` 與 max size（`[QT-SRC qlayoutitem.cpp:597-600]`），`item_expanding` 沒有此邏輯。
  - **G9.2.b [P1, READ]** `Layout::add_widget_with_stretch` 對 grid **靜默忽略 stretch** 並新增一列；`Layout::set_spacing` 兩軸都設但 `spacing()` 只回水平。
  - **G9.2.c [P1]** 無 `setRowStretch`／`setColumnStretch`／`setColumnMinimumWidth` 讀回；無 `addLayout` 進格；GridLayout 沒有 `remove_widget`。
- **Test**：既有 `test_layout_stretch_minimum.rs`、`test_grid_matches_qt_geometry`、`test_columns_follow_widest_cell_hint`。必要：grid 對齊測試；harness 擴充 `setRowMinimumHeight/setColumnMinimumWidth` 與對齊旗標。
- **HUD usage**：Python `QGridLayout`（`usage_table.py:384-434`：`setHorizontalSpacing(10)`、`setVerticalSpacing(4)`、`setColumnStretch`、`setRowStretch(5,1)`、`setRowMinimumHeight(2,18)`、span `:396`）；Rust `usage_table.rs:1136-1221`。

### C9.3 巢狀 layout（addLayout）、spacer、對齊
- **Qt behavior** `[QT-SRC qboxlayout.cpp:494-495,562-572]`：`QLayout` 可以是另一個 layout 的 item：貢獻自己的 `sizeHint/minimumSize/maximumSize/expandingDirections`；未設 spacing 時繼承 parent layout 的 spacing；非頂層時 margin 為 0。
- **qtrs required**：MUST 支援巢狀 layout 且尺寸語意相同；SHOULD 支援 spacer 與對齊。
- **Current implementation**：`ABSENT` 為概念（`LayoutItem` 只持 `WidgetRef` + `stretch` + `spacer` 旗標；grep `add_layout` 為空）。以 `EmptyWidget` 包裝子 layout 來模擬：`size_hint`／`minimum_size_hint` 取自子 layout，`item_expanding` 合併其 expanding 方向。HUD 在 `provider_card.rs:194-198,217-219,234-236,255-257,272-276` 與 `hud_window.rs:261-286` 這麼做。
- **Known gap**
  - **G9.3.a [P1, INFERENCE]** wrapper 是 QWidget item：其 `maximum_size` 為 16777215，而巢狀 `QLayout` 回報其子項最大值之和（巢狀的 Fixed widget 在 Qt 中不能長大，wrapper 可以）；未做 diff。
  - **G9.3.b [P2]** wrapper 多一個 child widget 進入 hit-test／paint 樹；預設 policy Preferred 而非由 layout 導出；不繼承未設定的 spacing。
  - **G9.3.c [P0, READ]** HUD 的 `header_widget` 額外被設為 `Expanding/Fixed`（`hud_window.rs:272-275`），Python 的裸 `QHBoxLayout` 沒有這個——app 層差異。
- **Test**：既有只有間接（`usage_table.rs` 的測試）。必要：harness 擴充 box-in-box／grid-in-box，對 `addLayout` 比對；`nested_layout_max_size_matches_qt`。
- **HUD usage**：Python `addLayout`（`hud_window.py:288,345`；`provider_card.py:53,68,80,95,107`；`usage_table.py:286`）。

### C9.4 預設 spacing／margin
- **Qt behavior** `[QT-SRC qlayout.cpp:111,246-255,341-344; qboxlayout.cpp:60,243,494-495,562-572,283-289]`：spacing／margin 預設 -1 = 問 style（child 9／window 11 margin、約 6 spacing；子 layout margin 0、spacing 繼承 parent layout）；未設 spacing 時逐對 `combinedLayoutSpacing`。
- **qtrs required**：依賴 Qt 預設的 layout MUST 得到相同值（至少 Windows style）。
- **Current implementation**：具體常數：`BoxLayout::new` spacing 6、margin 0；`GridLayout::new` spacing 6/6、margin 0。無 sentinel、不問 style、無 parent 繼承。
- **Known gap**
  - **G9.4.a [P1, READ]** 依賴 Qt 預設的 layout（頂層 margin 9/11、繼承 spacing）會不同。HUD 幾乎全部明確設定。
  - **G9.4.b [P0, 已讀兩側原始碼確認]** **卡片根 layout spacing 不同**：Python `layout.setSpacing(5)`（`provider_card.py:27`）vs Rust `root_layout.set_spacing(2)`（`provider_card.rs:159`）。程式碼無註解說明；`[INFERENCE]` 會改變卡片高度。
- **Test**：必要：`default_margins_and_spacing_match_windows_style`（PySide6 參考）；`sublayout_inherits_parent_spacing`；`provider_card_geometry_matches_python`。
- **HUD usage**：Python `setContentsMargins`（`hud_window.py:136,144,169`；`provider_card.py:26`；`usage_table.py:115,274,385,412`）。

### C9.5 失效、啟用、傳遞
- **Qt behavior** `[QT-SRC qlayout.cpp:956-969,980-1081; qwidget.cpp:10571-10586]`：`updateGeometry()`／hide／show／文字變更使 parent layout 失效；`QLayout::update()` 沿 parent layout 爬到頂層 layout 並 post 一個 `LayoutRequest` 到頂層 widget，由它重新啟用（並以 `SetDefaultConstraint` 重算視窗最小尺寸）。
- **qtrs required**：任何改變 hint 的操作之後，MUST 在每個 event-loop 輪次內**一次**重排整個受影響的樹；頂層最小尺寸 MUST 跟隨 layout 最小值，除非明確設定。
- **Current implementation**：`PARTIAL`，機制刻意不同。`Widget::request_layout` 只 post 給**直接 parent**（thread-local `WidgetCommandQueue`），每次 dispatch 後與繪製前 flush，**不是** `EventKind::LayoutRequest`（widgets 從不 post 它）。box／grid／stacked 的 `activate` 在 child 大小改變時重新使子 wrapper layout 失效：傳遞**只向下**。setter（`set_margins/set_spacing/add_widget…`）**立即**重排，不像 Qt 壓縮到 `LayoutRequest`。
- **Known gap**
  - **G9.5.a [P1, READ]** 無向上傳遞：葉節點的 hint 變更不會爬到祖先 layout（Fixed/Maximum wrapper 底下的文字變更不會調整 wrapper）；HUD 以明確 `update_layout()` 補（`hud_window.rs:636,737`、`provider_card.rs:435,707`）。
  - **G9.5.b [P1, READ]** `Button::set_text/set_font`、`Label::set_font/set_alignment`、`set_style_sheet`、`set_property`、`set_visible` 不請求 layout（→ G8.1.a、G8.5.c）。
  - **G9.5.c [P2]** setter 立即重排與 Qt 壓縮不同（只有在 mutation 中讀取 geometry 的程式碼觀察得到）。
  - **G9.5.d [P1]** 頂層最小尺寸不從 layout 導出（Python HUD 明確設定 `hud_window.py:296,327,347`，所以不受影響）。
- **Test**：既有 `test_reentrant_layout_request_during_callback`、`test_multilevel_layout_traversal_with_cell`、`test_command_queue_deduplication`、`test_resize_event_observable_ordering_before_layout_activation`、`test_single_resize_pipeline`、`test_resize_deferred_render`。必要：`text_change_below_fixed_wrapper_resizes_wrapper`；`layout_min_size_sets_toplevel_min_when_not_explicit`；`button_set_text_requests_layout`。
- **HUD usage**：Python 全靠 Qt 自動重排；Rust 手動 `update_layout`（見上）。

### C9.6 StackedLayout / StackedWidget
- **Qt behavior** `[QT-SRC qstackedlayout.cpp:417-448]`：`sizeHint` = **所有**頁面 hint 的最大值（`Ignored` policy 算 0），`minimumSize` = 所有頁面 `qSmartMinSize` 的最大值；非當前頁被隱藏；`currentChanged` 信號。
- **qtrs required**：MUST 與 Qt 相同。
- **Current implementation**：`IMPLEMENTED-UNTESTED`（幾何）。`StackedLayout::size_hint`／`minimum_size`／`expanding_directions` **只用當前頁**；`activate` 把每頁都設成同一矩形並切換可見性；`set_current_index` 只在索引改變且在範圍內時發 `current_changed`。
- **Known gap**：**G9.6.a [P1, READ；決議：不在 P0 階段修，不標 D]** `[QT-SRC qstackedlayout.cpp:417-448]`：Qt 的 `sizeHint` 取**所有頁面**的最大值（`Ignored` 策略的軸取 0），`minimumSize` 取所有頁 `qSmartMinSize` 的最大值；qtrs（`stacked.rs:123-147`）只看當前頁。目前 HUD 不依賴。 頁面大小不同時，視窗 hint／最小值在切換卡片↔表格時會跳動，與 Qt 不同。**注意：Python HUD 不用 `QStackedWidget`**（grep 為空）；它重建 `inner_layout`（`hud_window.py:272-347`）。所以這是 Rust HUD 的設計偏離（`hud_window.rs:301-309,585-606`），不是移植錯誤；須決定「改成與 Python 相同的重建」或「讓 StackedLayout 符合 Qt 並證明結果等價」。**G9.6.b [P2]** `set_spacing` 為 no-op。
- **Test**：既有 `test_stacked_widget_page_switching`（索引／信號）。必要：`stacked_size_hint_is_max_over_all_pages`；`stacked_hides_non_current_page_and_hit_test_skips_it`；切換 layout 後的視窗大小與 Python 逐項比對。
- **HUD usage**：Rust `hud_window.rs:301-309,585-606`。

### C9.7 與 PySide6 的 layout 差分 harness
- **格式註記**：本項是「驗證工具」的契約，不是 Qt 行為。「現況」＝Current implementation；「涵蓋／不涵蓋」＝Known gap；「Required」＝qtrs required + Test。
- **現況（`RAN` + `DIFF`，commit `701d36b`）**：`python tools/second_layer_harness/qt_layout_compare.py <cases> <seed>`（在 `rust/qtrs` 執行；Rust 端為 `crates/qtrs-widgets/examples/layout_probe.rs`）。**今天重跑：7,500 組 × seed 1 與 seed 20260502，各 0 差異；另有 1,500 組 × seed 20260502，0 差異。**
  - 這個數字**之前不在 repo 的任何地方**（`results*.txt` 無 layout 行、CI 無此步驟、不是 `cargo test`）；現在記錄於此，並且仍然**只有手動執行**。
- **涵蓋**：合成 `Probe` widget（固定 `sizeHint`、`minimumSizeHint()=(0,0)`）；扁平 `QHBoxLayout/QVBoxLayout/QGridLayout`（2–3 欄，box 1–6 項，grid 1–7 項，span 1–2）；7 種 `Policy` 每軸獨立；hint 0–120×0–60；min／max 隨機；box stretch 0–3；grid 只對 row 0 設欄 stretch + row stretch 0/1/2；10% 隱藏項（**隱藏項不納入比對**）；單一 `spacing` 0–12；單一均勻 `margin` 0–10；容器 (0,0)、20–500 × 20–400。
- **不涵蓋（每一項都是 gap）**：巢狀 layout；`addStretch/addSpacing/addSpacerItem`；per-item 對齊；不對稱 margin；H/V 不同 grid spacing；`setColumnMinimumWidth/setRowMinimumHeight`；`QSizePolicy` stretch factor；`minimumSizeHint != 0`；自帶 layout 的 wrapper widget；隱藏項的 geometry（Rust 設 0，Qt 不動）；show／hide／文字變更後的動態重排；容器小於最小和；RTL；`sizeConstraint`；`heightForWidth`；容器偏移 ≠ 0；`StackedLayout`；真實 `Label`／`Button` 的 hint。
- **Required**：(a) harness 的結果（命令、cases、seed、差異數、commit）必須在每次宣稱 layout 一致時記錄在 repo；(b) 擴充上列「不涵蓋」中 HUD 用到的項目（巢狀、spacer、對齊、`minimumSizeHint != 0`、`setRowMinimumHeight`、真實 Label／Button）後，**才可**宣稱「HUD layout 與 Qt 一致」；在那之前只能宣稱「扁平 box／grid、Probe widget 一致」。
- **Known gap**：**G9.7.a [P1]** harness 不在 CI、不是 `cargo test`；**G9.7.b [P1]** 涵蓋範圍如上。

---

## 10. Update / Paint semantics

### C10.1 `update()` 排程與壓縮
- **Qt behavior** `[QT-DOC]`：`QWidget::update()` 向頂層視窗 post 一個被壓縮的 `UpdateRequest`；繪製在事件迴圈內執行，絕不在呼叫內；隱藏 widget 的 `update()` 不做事。
- **qtrs required**：`update()` MUST 不同步繪製；重複呼叫 MUST 合併成一次繪製；繪製期間的 `update()` MUST 恰好引發一次後續繪製；隱藏 widget 的 `update()` SHOULD 為 no-op；widget `update()` MUST **不需 app 手動呼叫 `render_and_present()`** 就到達視窗。
- **Current implementation**：`PARTIAL`。`WidgetBase::update` 設 `dirty = Some(0,0,w,h)` 並 post `UpdateRequest` 給 `window_id`（`widget.rs`）；`Window::event(UpdateRequest)` 經 `RenderState::request_render` post 一個延後的 MetaCall；重入與借用衝突狀態機已實作；`UpdateRequest` 被壓縮；無事件迴圈時就地繪製。
- **Known gap**
  - **G10.1.a [P1, READ]** `UpdateRequest` 只到達經 `unsafe Window::register()` 註冊的視窗；未註冊的接收者收到的非 MetaCall 事件被丟棄（`loop.rs`）。grep `.register()`、`register_qobject`、`bind_event_loop` 於 `rust/src` 為空 → **HUD 從不註冊視窗**，靠明確 `window.render_and_present()`（`hud_window.rs:531,637,725,738,761,775,788,831`、`main.rs:205-230`）與滑鼠事件後的 `has_pending_invalidation` 檢查繪製。從計時器或 worker 觸發的 widget `update()` **不會自行重繪**。
  - **G10.1.b [P2]** `update()` 不看可見性。
  - **G10.1.c [P2]** `Window::set_geometry`／`show`／`set_opacity`／`set_style_sheet` 同步繪製（Qt 延後）。
- **Test**：既有（皆 `#![cfg(windows)]`）`test_resize_deferred_render.rs::{update_request_and_resize_share_one_gate, update_request_alone_is_deferred_through_the_gate, registered_paint_time_update_neither_recurses_nor_ping_pongs, borrow_conflict_retries_next_turn_and_recovers, borrow_conflict_is_bounded_then_rearmed_without_losing_repaint, registered_destroyed_window_deferred_render_is_a_noop}`。必要：**未註冊視窗（HUD 的建法）** 上 `child.update()` 後 `process_events` 恰好繪製一次；隱藏 widget 的 `update()` 不排程。
- **HUD usage**：Python 只在 `usage_table.py:144` 呼叫 `update()`，從不 `repaint()`；Rust 明確 `render_and_present()`。

### C10.2 `repaint()`、`update(rect)`、`setUpdatesEnabled`
- **Qt behavior** `[QT-DOC]`：`repaint()` 立即繪製；`update(rect/region)` 只使該區域失效；`setUpdatesEnabled(false)` 抑制繪製。
- **qtrs required**：若有呼叫者需要就 MUST 提供（HUD 不需要）。
- **Current implementation**：`ABSENT`（grep `fn (repaint|update_rect|update_region|set_updates_enabled|updates_enabled)` 為空）。`update()` 一律整個 widget。
- **Known gap**：**G10.2.a [P2]** 無局部／立即重繪 API（`repaint()`、`update(rect)`）；沒有工程理由，需要時須補；**G10.2.b [P2]** 每個 widget 一個髒矩形，合併為一個外框（Qt 保留 region）。
- **Test**：必要（若實作）：`update(rect)` 只重繪相交像素。
- **HUD usage**：無。

### C10.3 繪製管線：髒區、裁剪、順序
- **Qt behavior** `[QT-DOC]`：繪製區域是髒區聯集；移動／縮放／隱藏 widget 會使 parent 中的舊與新區域失效；每個 widget 被裁剪到自己的矩形，children 被裁剪到 parent；順序為 parent 先、children 依 z 序。
- **qtrs required**：移動或隱藏 child MUST 重繪空出的區域；`paint_event` MUST 不畫到自己矩形外；children MUST 被裁剪到 parent。
- **Current implementation**：`PARTIAL`。`do_render_and_present` 清除髒矩形、設一個 clip、遞迴、呈現實體髒區；`render_widget_recursive`：`save → translate(geom) → paint_event → children → restore`，跳過隱藏與髒區外 widget。
- **Known gap**
  - **G10.3.a [P1, READ]** 無 per-widget clip（`set_clip_rect` 只有 `window.rs`、`scroll.rs`、`backing_store.rs` 呼叫）。
  - **G10.3.b [P1, READ]** `EmptyWidget::set_geometry` 只使新矩形失效：**移動不會使舊位置失效**。
  - **G10.3.c [P2]** `Painter::set_clip_rect` 取代而非相交，且忽略目前 transform（見 C10.4）。
- **Test**：既有無（沒有測試在 child 移動後斷言像素）。必要：移動彩色 child 10 px 後，舊矩形被清除；child 比 parent 大時，parent 外的像素不被動。
- **HUD usage**：兩個 HUD 都用 layout，不手動移動 child，故 gap 為潛在。

### C10.4 Painter 語意
- **Qt behavior** `[QT-DOC]` + `[INFERENCE]`：painter 帶 transform、clip（相交且經 transform）、render hints、opacity、composition mode；Antialiasing 與 `SmoothPixmapTransform` 是不同 hint；沒有 `SmoothPixmapTransform` 時縮放矩陣下的 `drawPixmap` 用最近鄰（`[INFERENCE]`）；`QPainter::rotate` 會旋轉文字。
- **qtrs required**：transform MUST 作用於 clip、文字與 pixmap；SHOULD 有 hints。
- **Current implementation**：`PARTIAL`。`save/restore`、`translate`、`scale`、`rotate`、opacity、composition 可用；初始 transform 為 DPR 縮放（`qtrs-gui/src/paint/painter.rs`）。`draw_text` 把每個字形放在 `round(transform · (pos + glyph))`，以 `font.size * dpr` 光柵化。
- **Known gap**
  - **G10.4.a [P1, READ]** `set_clip_rect` **不經 transform**（遮罩只用 DPR 縮放，文字 clip 用 `clip * dpr`）；`ScrollArea::paint_event` 在 widget translate 之後呼叫 `set_clip_rect(viewport)`，其 clip 落在視窗座標。
  - **G10.4.b [P2, READ]** `set_clip_rect` 取代而非相交：`ScrollArea` 內失去 widget 層的髒區 clip。
  - **G10.4.c [P1, READ]** `draw_text` 忽略旋轉與縮放，只轉換原點；`ProgressBar` 垂直文字先 `rotate` 再 `draw_text`，**畫出來沒有旋轉**。
  - **G10.4.d [P1, READ]** 無 render hints；`draw_pixmap` 恆為 `FilterQuality::Bilinear`，只有 hatched brush 用 Nearest。
  - **G10.4.e [P2, INFERENCE]** `Painter::begin` 只要 ClearType 開就啟用 LCD 文字，與目的地 alpha 無關；Python HUD 設 `WA_TranslucentBackground`；Qt 在 ARGB 目的地是否改用灰階，**待對照 golden**。
- **Test**：既有 `painter.rs` 內聯測試（`test_painter_transforms_and_state_stack`、`test_painter_draw_pixmap_and_clip`、`test_painter_begin_dpr_transform`）、`qtrs-gui/tests/test_painter_advanced_features.rs`、`test_lcd_text_parity.rs`（`DIFF`：固定 Qt 輸出 fixture）。必要：translate (100,0) + clip (0,0,10,10) + fill → 像素在 x=100..110；旋轉的 `draw_text` 有垂直字形範圍；縮放的 `draw_pixmap` 與 Qt golden（nearest vs bilinear）一致。
- **HUD usage**：Python `QPainter`（`usage_table.py:35,55,83,161`、`tray_icon.py:32`，Antialiasing hint）；Rust `draw_pixmap`（`usage_table.rs:566`、`menu.rs:1370`）。兩個 HUD 都沒有 `ScrollArea`；只有水平 `ProgressBar`。

### C10.5 原生視窗的整數像素放置（commit `701d36b` 的實例）
- **Qt behavior** `[QT-DOC/INFERENCE]`：每個頂層與 popup 都是自己的原生視窗，位置在整數裝置像素（`QHighDpi::toNativePixels` 取整）；視窗內容以邏輯偏移繪製，因此只有**原生視窗之間**的偏移會造成非整數；子選單是自己的視窗。
- **qtrs required**：被獨立放置的視窗之內容 MUST 以**相對於該視窗原點的整數裝置像素偏移**繪製；視窗被加寬／縮窄時 MUST 不改變其內容相對像素格線的位置（否則逐字取整會變，字距跳動）。
- **Current implementation**：`IMPLEMENTED` 於 popup 根與子選單：`Painter::translate_device`（`painter.rs`）；`present_popup` 計算 `native_root - native_win`，兩者皆經 `to_native_point`（`menu.rs`）；子選單以 `round(sg * dpr)` 平移。
- **Known gap**
  - **G10.5.a [P1, 注意]** 既有測試 `rust/src/ui/tray_icon.rs::test_menu_text_does_not_depend_on_window_offset` **只涵蓋 `translate_device` + `Menu::paint_event`**；它斷言「平移整數裝置像素 1..7 後每列位元組相同」，**沒有走 `present_popup`**，所以不是這次修復的失敗前／通過後回歸測試——**如果有人把 `present_popup` 改回邏輯偏移，這個測試不會失敗**。實機驗證（子選單向左展開，視窗 339→534 px，父選單偏移 195 px，5 列父選單文字像素差異 0）是手動的（`RAN`），沒有進 repo。
  - **G10.5.b [P2, INFERENCE]** 子選單原點是 `parent_native + round(sg.x*dpr)`，Qt 是 `round((root+sg)*dpr)`，可差 1 裝置像素（不影響字距）。
  - **G10.5.c [P1, READ]** `present_popup`／`exec_popup` 用 `primary_screen().device_pixel_ratio()`（`menu.rs:597,729`），不是 popup 所在螢幕的 DPR。
  - **G10.5.d [P2]** hit-test 用邏輯整數，繪製用裝置取整，差最多 1 裝置像素。
  - **G10.5.e [P2]** 非 Windows 的 `exec_popup` 沒有原生視窗。
- **Test**：必要：以兩個「相差非整數裝置像素」的 `root_origin` 驅動 `present_popup`，比對 backing-store 中選單區域的像素；同上於 dpr 1.25 與 1.75 並開啟子選單。
- **HUD usage**：Python `QMenu`（`hud_window.py:678-795`、`tray_icon.py:55`）；Rust `Menu::exec_popup`（`main.rs:394`）。

**逐一檢視 `translate(` 呼叫點是否可能產生同類 bug（`READ`）**

| 位置 | 偏移型別 | 同類？ |
|---|---|---|
| `menu.rs` `present_popup`（`translate_device`） | 整數裝置像素 | 否（已修） |
| `menu.rs` 子選單（`translate_device`） | 整數裝置像素 | 否（已修；與 Qt 差至多 1 px，見 G10.5.b） |
| `window.rs` `render_widget_recursive`：`translate(geom.x, geom.y)` | 整數**邏輯**，於 1.25/1.5/1.75 為非整數裝置像素 | **同類**。Qt 也以邏輯偏移在同一視窗內繪製 child，所以只有「Qt 對字形取整的方式不同」時才會相對 Qt 退化（`[INFERENCE]`）。**需要 125% 下 child label 的 golden。** |
| `scroll.rs` `translate(-scroll_x,-scroll_y)`、scrollbar translate | 整數邏輯 | 同上；HUD 不用 |
| `progress_bar.rs` `translate(tx, ty)` | 整數 | 同上，另加旋轉文字缺口（G10.4.c） |
| `label.rs`、`progress_bar.rs`：文字 x 為 `(content_w - text_w)/2.0` | **分數**邏輯文字原點 | 同類。Qt 的 `drawText(rect, AlignHCenter)` 也是分數（`[INFERENCE]`） |
| `hud_window.rs:116-121` panel painter：`RectF(0.5, 0.5, device().width()-1, …)` | `device().width()` 是 `physical/dpr` 浮點，不是 widget 的邏輯寬 | 邊角：panel 右／下緣最多差約 0.5 邏輯像素 |
| `Window::present_custom_at`（`window.rs`） | 視窗位置 `round(x*dpr)` | 已為 menu 修；**未來任何用它並以邏輯偏移繪製的 popup 都會退化** |

### C10.6 自訂 widget 的 `paintEvent`
- **Qt behavior**：子類別覆寫 `paintEvent(QPaintEvent*)`，拿到已在本地座標的 `QPainter`。
- **qtrs required**：`paint_event(&mut Painter)` MUST 以本地座標被呼叫，parent 先 child 後。
- **Current implementation**：`IMPLEMENTED`（`widget.rs`、`window.rs`）；`EmptyWidget` 另有 `paint_handler` 閉包。
- **Known gap**：**G10.6.a [P2]** 無 `QPaintEvent` 矩形；**G10.6.b [P2]** 無法防止畫到矩形外（G8.7.a）。
- **Test**：既有 `test_arc_progress_and_custom_paint.rs::{test_widget_virtual_paint_event_override, test_custom_widget_paint_handler_closure}`。
- **HUD usage**：Python `usage_table.py:160`；Rust `install_panel_painter`（`hud_window.rs:111-124`）。

### C10.7 Backing store 與 DPR 生命週期
- **Qt behavior** `[QT-DOC]`：backing store 跟隨視窗大小與**視窗所在螢幕**的 `devicePixelRatio()`；換螢幕時 DPR 切換並重繪。
- **qtrs required**：backing store 的 DPR MUST 等於視窗所在螢幕的 DPR；`DpiChanged` 之後 MUST 維持新 DPR。
- **Current implementation**：`PARTIAL`。延遲 resize + 原生尺寸對齊（`backing_store.rs`、`window.rs`）；`DpiChanged` 處理（`window.rs`）。
- **Known gap**
  - **G10.7.a [P0, READ，未實測]** `DpiChanged` 處理把 store 調成 `dpi_x/96`，然後呼叫 `do_render_and_present`，後者又以 `platform().primary_screen().device_pixel_ratio()` 重新 resize——**在與主螢幕 DPI 不同的螢幕上，store 會退回主螢幕的 DPR**。同樣的「主螢幕 DPR」假設還出現在 `Window::new`、`set_geometry`、`set_geometry_silent`、`present_custom`、`present_custom_at`、`NativeWindow::present_region`、`menu.rs`、`tray_icon.rs:281`；而 WM handler 用的是每視窗的 `GetDpiForWindow`。
  - **G10.7.b [P1, READ]** `application_device_pixel_ratio`（各螢幕最大值）只在 `Application::new` 設一次，DPI 變更或螢幕熱插拔後 stale。
  - **G10.7.c [P1, READ]** `HighDpiScaleFactorRoundingPolicy` 存了但從不讀（grep `rounding_policy` 只有存取器）；DPR 恰為 `dpi/96`，等同 Python 設的 `PassThrough`（`main.py:46`），其他 policy 被忽略。
- **Test**：既有 `test_per_monitor_dpi_sync.rs::test_per_monitor_v2_dpi_drag_propagation` 只斷言 observer callback 與 DPR 值，**從不檢查 backing store**。必要：`DpiChanged{168,168}` 並繪製後，`backing_store().device_pixel_ratio() == 1.75` 且實體大小為 `round(logical*1.75)`（在主螢幕 100% 時 READ 預測會失敗）。
- **HUD usage**：Python PassThrough（`main.py:46`），無 `devicePixelRatio()` 呼叫；Rust PerMonitorV2 manifest（`rust/build.rs:22-23`）+ `set_dpi_awareness`。

---

## 11. Window / platform semantics

> 只有 Windows 被驗證（見 §1.1）。`READ` 為預設；`RAN` 者另標。

### C11.1 視窗旗標與建立
- **Qt behavior** `[QT-DOC]`：`Frameless | Tool | StaysOnTop` 對應無邊框、tool、topmost 視窗；`WA_TranslucentBackground` 使其為 per-pixel alpha；`setWindowFlag(WindowStaysOnTopHint, v)` **重建**原生視窗並隱藏，呼叫端要再 `show()`。
- **qtrs required**：HUD 旗標 MUST 產生 `WS_POPUP | WS_EX_TOOLWINDOW | WS_EX_LAYERED`，on-top 時加 `WS_EX_TOPMOST`；topmost 切換 MUST 保持視窗可見且幾何不變。
- **Current implementation**：旗標 `IMPLEMENTED`，切換 `PARTIAL`。`NativeWindow::new`（`qtrs-platform/src/window.rs`）；HUD 請求 `FRAMELESS | CUSTOM_FRAMELESS | LAYERED | TOOL [| STAYS_ON_TOP] [| CLICK_THROUGH]`（`hud_window.rs:207-216`）；`LAYERED` 優先於 `CUSTOM_FRAMELESS`，所以不裝 NCHITTEST 設定，resize／move 走 `start_system_move/resize`（與 Python 的 `startSystemMove/Resize` 相同）；`set_stays_on_top` 就地 `SetWindowPos(HWND_TOPMOST/NOTOPMOST)`。
- **Known gap**：**G11.1.a [P2]** 無 `set_window_flags`；就地 `SetWindowPos` 保持可見，可觀察終態與 Python 的 `setWindowFlag + show()` 一致；**G11.1.b [P1]** 測試只檢查 `flags` 欄位，不檢查 `WS_EX_TOPMOST`／`WS_EX_TRANSPARENT`。
- **Test**：既有 `window.rs::test_window_flags_to_win32_styles`（建立時樣式）、`test_native_window_lifecycle_and_methods`（只查 `flags` 欄位）。必要：`set_stays_on_top(false)` 後 `GetWindowLongPtrW(GWL_EXSTYLE) & WS_EX_TOPMOST == 0`。
- **HUD usage**：Python `hud_window.py:123-128,804-812`；Rust `hud_window.rs:538-545`。

### C11.2 顯示、隱藏、關閉
- **Qt behavior** `[QT-DOC]`：`isVisible()` 反映真實狀態（含 OS 隱藏）；`showEvent/hideEvent/closeEvent` 被遞送；`close()`／Alt+F4 送 `closeEvent` 後隱藏；`quitOnLastWindowClosed(False)` 時 app 繼續跑。Python HUD：`showEvent` → `QTimer.singleShot(0, _apply_theme)`（`hud_window.py:611-613`）；`closeEvent` 儲存幾何（`:607-609`）；`hideEvent` 排程 `trim_memory`（`:615-617`）。
- **qtrs required**：`Window` MUST 回報真實可見性；關閉請求 MUST 到達 app，Alt+F4 MUST 隱藏 HUD 而不結束程式；Show／Hide 通知 MUST 到達 widget 樹。
- **Current implementation**：`PARTIAL`。`show/hide` 呼叫 `ShowWindow`；`WM_CLOSE` post `CloseRequest`/`Close` 並回 0——**從不隱藏或銷毀**；`WM_SHOWWINDOW`/`WM_PAINT` 只對 `get_window_event_binding` post `Show/Hide/Expose`，HUD 從不綁定（grep `bind_event_loop` 只有定義）；widgets crate 從不處理這些事件。
- **Known gap**
  - **G11.2.a [P1, READ]** 無 `Window::is_visible()`；Rust `HUDWindow.is_visible` 是自行追蹤的 bool（`hud_window.rs:131,518-523`）。
  - **G11.2.b [P0, READ]** `CloseRequest` 在 `WindowEventHandler` 被 `_ => {}` 吞掉（grep `CloseRequest` 於 `rust/src`、`qtrs-widgets/src` 為空）。**Alt+F4 什麼也不做**；Python 會隱藏 HUD 並儲存幾何。
  - **G11.2.c [P0, READ]** 無 `showEvent/hideEvent/closeEvent` hook：Python 的「show 時重新套用主題」「hide 時 trim_memory」沒有 Rust 對應（Rust 在 `hide()` 裡直接做，`hud_window.rs:479-485`）。
  - **G11.2.d [P2]** 無 `Expose` 重繪（分層 presenter 保留內容，非分層 `Win32DcPresenter` 視窗被遮蓋後不會重繪）。
  - **G11.2.e [P1, READ]** `Application::unregister_window` 在 drop 時、`quit_on_last_window_closed` 為 true 就呼叫 `quit`；Qt 在**關閉**時發 `lastWindowClosed`，不是銷毀時。
  - **G11.2.f [P2]** `GuiApplication::set_application_state`、`last_window_closed`、`focus_window_changed` 從不發射。
  - **G11.2.g [P1]** `main.rs` 從不 `set quit_on_last_window_closed(false)`（Python：`main.py:48`）。
- **Test**：既有 `test_application_layers.rs::{test_widget_application_window_registry_and_focus, test_core_application_exec_quit_and_about_to_quit}`（無 `closeEvent` 涵蓋）。必要：合成 `WM_CLOSE`，斷言視窗隱藏、`quit_on_last_window_closed(false)` 時 app 不結束、close hook 被呼叫；`Window::is_visible()` 跟隨 `show/hide`。
- **HUD usage**：Python `main.py:60`、`hud_window.py:861-866`、`main.py:48`；Rust `main.rs:335`、`hud_window.rs:518-523`。

### C11.3 幾何：move、resize、邏輯 vs 原生
- **Qt behavior** `[QT-DOC/INFERENCE]`：視窗幾何為裝置無關像素；原生幾何為 `QHighDpi::toNativePixels`，位置與大小**分別**取整；每次大小改變送一次 `Resize`，每次位置改變送一次 `Move`；`setMinimumSize` 由 OS 遵守。
- **qtrs required**：同上。
- **Current implementation**：主要 resize 路徑 `IMPLEMENTED`。`WM_SIZE`/`WM_MOVE`/`WM_GETMINMAXINFO`（`qtrs-platform/src/window.rs`）；`high_dpi.rs` 位置與大小獨立取整。
- **Known gap**
  - **G11.3.a [P0, READ]** DPI 混用：WM handler 用 `GetDpiForWindow`，`Window::set_geometry` 用主螢幕 DPR（見 C10.7）。
  - **G11.3.b [P2]** 位置是 `i32` 邏輯值；滑鼠位置以取整後到達 widget，Qt 給 `QPointF`。
  - **G11.3.c [P1, READ]** `NativeWindow::geometry()` 回實體 `GetWindowRect`，`Window::geometry()` 為邏輯；原生視窗內快取的 `self.geometry` 混合實體寬高與邏輯 x／y。
- **Test**：既有 `test_resize_deferred_render.rs`（全部）、`test_single_resize_pipeline.rs::{interactive_one_wm_size_one_resize_one_present, normal_resize_stays_deferred_with_a_single_pipeline}`、`test_layered_geometry_sync.rs::hud_style_window_content_rect_equals_hwnd_rect_every_iteration`、`qtrs-platform/tests/{test_layered_interactive_resize.rs, test_dcomp_interactive_resize.rs}`。必要：邏輯→原生→邏輯在 125／150／175% 對奇數尺寸往返，對照已知 Qt 值。
- **HUD usage**：Python `hud_window.py:296-354,401,415-422,855-858`；Rust `hud_window.rs:592-635,805-831`。

### C11.4 螢幕與保持視窗在螢幕內
- **Qt behavior** `[QT-DOC]`：`screens()`、`primaryScreen()`、`widget.screen()`、`availableGeometry()`；`screenChanged` per window。Python HUD 用**視窗中心所在螢幕**；還原時要求至少 50×30 可見於任一螢幕，否則停靠到主螢幕 `avail.right - w - 40, avail.top + 50`（`hud_window.py:376-424`）。
- **qtrs required**：同上，且 MUST 以相同規則還原位置。
- **Current implementation**：`PARTIAL`。`Win32Screen`（`geometry`、`available_geometry`＝`rcWork`、`dpr`、`all_screens`）；`ensure_within_screens`／`clamp_window_rect_to_screens` 支援多螢幕。
- **Known gap**
  - **G11.4.a [P0, READ]** Rust HUD 啟動時用 `primary_screen().geometry()` 與 `ensure_within_screen`（`hud_window.rs:178-193,640-662`），且與 `:823-824` 的 `available_geometry()` 不一致；**從不使用 `clamp_window_rect_to_screens`**；沒有以中心選螢幕、沒有「脫離所有螢幕就停靠右上」、沒有 50×30 門檻。
  - **G11.4.b [P1, READ]** `Window` 無 `screen()`；`screen_changed` 只在 `WM_DISPLAYCHANGE` 發射，無訂閱者。
  - **G11.4.c [P1, INFERENCE]** `Win32Screen::geometry` 以 dpr 除原點（`high_dpi::from_native_rect`），在 DPI 不同的副螢幕上不是 Qt 的虛擬桌面映射。
  - **G11.4.d [P2]** `Win32Screen::primary()` 寫死 `MonitorFromPoint(0,0)`；DPR 夾到最小 1.0。
- **Test**：既有 `test_power_events_and_screen_clamping`、`test_platform_screen_primary_and_multi_screens`（只練 helper，沒呼叫 HUD 邏輯）。必要：以「儲存位置在所有螢幕之外」與「在第二螢幕」兩種情況驅動 HUD 還原，斷言 Python 的停靠規則。
- **HUD usage**：Python `hud_window.py:376-424,856-858`；Rust `hud_window.rs:176-193,640-662,823-831`。

### C11.5 不透明度與呈現
- **Qt behavior** `[QT-DOC/INFERENCE]`：`setWindowOpacity(v)` 使整個視窗 `v` 透明；分層視窗以 `UpdateLayeredWindowIndirect` 的 `SourceConstantAlpha` 呈現。
- **qtrs required**：`set_opacity(v)` MUST 在 HUD 的分層視窗上改變可見 alpha，**不論選用哪個 surface**。
- **Current implementation**：`PARTIAL`。`NativeWindow::set_opacity` 存值，僅對**非** `LAYERED` 視窗呼叫 `SetLayeredWindowAttributes`；`Win32LayeredPresenter` 用它當 `SourceConstantAlpha`；`get_or_create_presenter` 對 `LAYERED` 視窗**先試 `DCompSurface::new`**，失敗才退回 GDI layered presenter。
- **Known gap**
  - **G11.5.a [P0（條件式：僅 DComp 可用的機器）, READ；本機 RAN：選到 Layered，未重現]** **DComp 路徑丟棄 opacity**：`present_dirty_ref(&mut self, pixmap, _opacity, dirty)`（`surface/dcomp.rs`）從不使用它；`WindowsPresenter::set_opacity` 對 `DirectComposition` 為 no-op；以寫死的 `1.0` 呈現。若 DComp 被選用，HUD 的不透明度設定**無效**（`hud_window.py:132,820` vs `hud_window.rs:223,782-788`）。
  - **G11.5.c [test gap, RAN]** `test_dcomp_*`（5 項）在本機因 `CreateDXGIFactory1 failed for IDXGIFactory2` 全部 skip，卻顯示為 passed；`window.rs:1490-1492` 對所有 `LAYERED` 視窗**無閘門地先試 DComp**，另有 `test_layered_interactive_resize::window_pipeline_*` 在本機選到 Layered。DComp 路徑在本機完全沒被測試。
  - **G11.5.b [P2]** 非分層視窗 `SetLayeredWindowAttributes` 失敗時靜默（不補 `WS_EX_LAYERED`）；DC presenter 忽略 opacity。
- **Test**：既有 `test_surface_presenter_dc_and_layered_alignment` 只斷言 `is_ok()`；`GenericWindow`/`X11NativeWindow` 的 opacity 測試只斷言儲存值。必要：以每個後端用純色 pixmap 在 0.5 opacity 呈現並讀回合成 alpha；`set_opacity` 對 DComp 視窗改變呈現 alpha。
- **HUD usage**：Python `hud_window.py:131-132,818-820`；Rust `hud_window.rs:223,782-788`、`tray_icon.rs:647-667`。

### C11.6 Click-through 與 stays-on-top 切換
- **Qt/Python behavior**：Python 在 Windows 設 `WS_EX_TRANSPARENT | WS_EX_LAYERED` 再 `SetWindowPos(SWP_FRAMECHANGED)`（`hud_window.py:474-486`）；topmost 切換後**重新斷言** click-through（`:809-810`）。
- **qtrs required**：視窗 MUST 讓滑鼠穿透；樣式位元 MUST 在 topmost 切換後保留。
- **Current implementation**：`IMPLEMENTED-UNTESTED`。`set_click_through` 在既有樣式上設／清 `WS_EX_TRANSPARENT`（HUD 視窗本來就是 `WS_EX_LAYERED`）；無 `SWP_FRAMECHANGED`；`set_stays_on_top` 不碰 click-through 樣式，所以不需重新斷言。
- **Known gap**：**G11.6.a [P2]** 非 `LAYERED` 視窗的 `set_click_through(true)` 只得 `WS_EX_TRANSPARENT`，沒有 `WS_EX_LAYERED` 時不穿透；**G11.6.b [P1]** 沒有測試斷言樣式位元。
- **Test**：必要：`set_click_through(true/false)` 前後及 `set_stays_on_top` 後斷言 `GWL_EXSTYLE & WS_EX_TRANSPARENT`。
- **HUD usage**：Python `hud_window.py:474-532,809-810`；Rust `hud_window.rs:526-536`。

### C11.7 背景（Acrylic / vibrancy）
- **Python behavior**：`vibrancy.apply` 只在 widget 可見且非 offscreen 平台時執行；呼叫 `SetWindowCompositionAttribute(WCA_ACCENT_POLICY)`，state 4、flags 2、gradient `0x99161a22`（暗）或 `0x99f0f2f8`（亮）；`clear` 設 state 0；樣式表依 apply 是否成功（`vibrant`）而不同（`vibrancy.py:75-159`、`hud_window.py:240-250`）。
- **qtrs required**：MUST 相同的 accent policy 與相同的失敗退路。
- **Current implementation**：accent policy `IMPLEMENTED`（`qtrs-platform/src/backdrop.rs`，常數相同），fallback `PARTIAL`。`BackdropType::None` 清除 accent policy，並額外呼叫 `DWMWA_USE_IMMERSIVE_DARK_MODE`、`DWMWA_SYSTEMBACKDROP_TYPE=NONE`、`DwmEnableBlurBehindWindow(false)`。
- **Known gap**：**G11.7.a [P1, READ]** HUD 在視窗可見之前呼叫 `set_backdrop`（`hud_window.rs:229`）並忽略回傳；Python 以 `isVisible()` 把關，失敗就用實心面板；Rust 一律畫 vibrant 面板色（`:88-92`）（→ G12.5.c）；**G11.7.b [P2]** `None` 比 Python 的 `clear` 多做 DWM 呼叫；**G11.7.c [P2]** macOS 路徑只對 `MockObjcRuntime` 測過。
- **Test**：既有 `test_cross_platform_backdrop_and_click_through`（Mock）、`test_backdrop_type_variants`。必要：Windows 上 `set_backdrop(Acrylic, dark)` 後 `GetWindowCompositionAttribute` 回報 state 4 與 gradient。
- **HUD usage**：Python `hud_window.py:240-250`；Rust `hud_window.rs:224-229,626-631,712`。

### C11.8 系統匣
- **Qt behavior** `[QT-DOC]`：`QSystemTrayIcon(icon)`、`setToolTip`、`setContextMenu`、`show`、`showMessage(title, msg, icon, msecs)`、`activated(reason)`；HUD 在 `Trigger` 時切換顯示；Windows 上 Qt 在右鍵時自己顯示 context menu。
- **qtrs required**：同樣的信號與訊息 API，選單 MUST 在游標處彈出。
- **Current implementation**：`IMPLEMENTED-UNTESTED`（Windows 執行期）。`TrayIcon::new/show/hide/set_tooltip/show_message` 與 `on_activated/on_context_menu_requested/on_message_clicked`（`qtrs-platform/src/tray_icon.rs`）；`tray_window_proc` 把 `NIN_SELECT | WM_LBUTTONUP` 映射為 Trigger、`WM_LBUTTONDBLCLK` 為 DoubleClick，右鍵為 Context 加選單 exec，`TaskbarCreated` 時重新加入圖示。HUD 用 `on_activated` Trigger（`main.rs:355-357`）與 `on_menu_action`（`:431-449`）。
- **Known gap**：**G11.8.a [P1]** 選單位置換算用主螢幕 DPR（`tray_icon.rs:281`）；**G11.8.b [P2]** `show_message` 只收 title／text／4 值圖示 enum／時間，不收自訂 `QIcon`（Python 傳 `tray.icon()`，`main.py:75-82`）；**G11.8.c [P0, READ；= G11.9.a 的重複登錄]** Python 的 `hotkey_failed` 訊息 Rust 沒有（→ G11.9.a）；**G11.8.d [P2]** 圖示：Python 依平台選 `.ico/.icns/.png`（`tray_icon.py:17-28`），Rust 內嵌 PNG；**G11.8.e [P2, INFERENCE]** 雙擊在 Windows 先 Trigger 兩次再 DoubleClick（如 Qt）；**G11.8.f [P2]** DBus／macOS 後端存在但未驗證。
- **Test**：既有 `tray_icon.rs` 內聯測試（`test_menu_item_constructors`、`test_menu_builder_and_hmenu_lifecycle`、`test_create_hicon_from_pixmap`、`test_tray_icon_lifecycle`、`test_tray_signals_and_window_proc_dispatch`）；應用層 `test_context_menu_parity_cards_and_table_modes`、`test_tray_and_hud_menu_unified_parity`（只比選單內容）。必要：對 tray 視窗 post `WM_LBUTTONUP` 與 `WM_LBUTTONDBLCLK`，斷言發射序列。
- **HUD usage**：Python `tray_icon.py:43-151`、`main.py:63-65,75-82`；Rust `rust/src/ui/tray_icon.rs:547-560`、`main.rs:343-357,431-449`。

### C11.9 全域熱鍵
- **Python behavior**：不用 `QShortcut`；以 ctypes `RegisterHotKey` 執行緒（`hotkey.py:34-110`）發射 `hotkey_triggered`/`clickthrough_triggered`/`hotkey_failed`/`unavailable`，經 queued 連線到 GUI 執行緒；`_safe_init_click_through` 在 click-through 熱鍵註冊失敗時停用啟動 click-through（`hud_window.py:426-437`）。
- **qtrs required**：Alt+C 與 Alt+Shift+C MUST 切換可見性與 click-through；註冊失敗 MUST 被回報，且鎖定防護（lockout guard）MUST 使用**真實註冊結果**。
- **Current implementation**：Windows 執行緒 `IMPLEMENTED-UNTESTED`（`rust/src/hotkey.rs`，id 9527/9528、`MOD_NOREPEAT`、drop 時 `WM_QUIT`）；`parse_hotkey`／`compute_ct_mods` 有單元測試。`qtrs-platform::Win32HotkeyManager` 存在但 **app 不用**。cocoa／unix／generic 的 `PlatformHotkeyManager` 只記錄 id 並回 `Ok`。
- **Known gap**
  - **G11.9.a [P0, READ]** `HotkeyManager::start` 在 Windows 即使 `RegisterHotKey` 失敗也回 `Ok`，失敗只在執行緒內 `warn!`（`hotkey.rs:235-261`）；`main.rs:496` 檢查 `hotkey.is_none()`，真實衝突時永不成立 → **鎖定防護與托盤警告不會觸發**（Python 檢查 `clickthrough_registered` 並顯示警告：`main.py:78-89`、`hud_window.py:426-437`）。
  - **G11.9.b [P1]** macOS 熱鍵明確未實作（`hotkey.rs:7-8,146-151`）；Python 用 pynput。
  - **G11.9.c [D]** `GenericHotkeyManager`／`CocoaHotkeyManager`／`UnixHotkeyManager` 為 stub，回報成功卻未註冊。理由：未驗證平台；**但 stub 回 `Ok` 違反規則 3（誤用須可見失敗）**——必須改回錯誤。
  - **G11.9.d [P2]** app 以原子旗標 + `run_on_main_thread`（`main.rs:466-483`）取代 queued 信號。
- **Test**：既有 `rust/src/hotkey.rs::{test_parse_hotkey, test_compute_ct_mods}`（Windows）、`qtrs-platform/src/hotkey.rs::{test_cocoa_hotkey_manager, test_unix_hotkey_manager}`、`test_cross_platform_hotkeys`。必要：兩次註冊相同組合，第二次 `start` 回報失敗且 app 停用啟動 click-through；對執行緒 post `WM_HOTKEY`，斷言主執行緒 callback 每次按下觸發一次。
- **HUD usage**：Python `main.py:67-90`、`hud_window.py:426-437`；Rust `main.rs:462-509`。

### C11.10 主題與色彩配置
- **Qt behavior** `[QT-DOC]`：`QGuiApplication.styleHints().colorScheme()` 與 `colorSchemeChanged`；HUD 於 `QTimer.singleShot(0, …)` 重新套用主題（`hud_window.py:104,213-220,230-238`）。
- **qtrs required**：色彩配置 MUST 反映 `AppsUseLightTheme`，變更 MUST 只發射一次。
- **Current implementation**：Windows `IMPLEMENTED-UNTESTED`。`Win32Theme::query_color_scheme` 讀 `AppsUseLightTheme`；`refresh` 只在改變時發射；視窗 proc 於 `WM_SETTINGCHANGE`/`WM_THEMECHANGED` 呼叫；app 把後續處理排到主執行緒（`main.rs:511-521`）。
- **Known gap**：**G11.10.a [P2]** 高對比與 `ShouldAppsUseDarkMode` 未驗證；**G11.10.b [P2]** `GuiApplication::new` 預設 `Palette::dark()`，不跟隨系統配置；**G11.10.c [P2]** `theme.rs` 無單元測試。
- **Test**：既有只對 cocoa／unix 的 set-and-read。必要：翻轉登錄值、呼叫 `refresh`、斷言一次 `theme_changed`。
- **HUD usage**：Python `hud_window.py:104,213-238`；Rust `main.rs:514-521`、`ui/mod.rs::resolve_is_dark`（未知配置視為暗）。

### C11.11 單一實例、剪貼簿
- **Qt behavior** `[QT-DOC]`：Qt 沒有內建單一實例保證（`QLockFile`／`QSharedMemory`／`QLocalServer` 由應用自行選用）；`QClipboard` 提供系統剪貼簿。
- **qtrs required**：單一實例：第二次啟動 MUST 不開第二個 HUD，且 MUST 不改變第一個 HUD 的任何可觀察行為（§1.3 登記為刻意差異）；剪貼簿只在 HUD 使用時才需要。
- **Current implementation**：單一實例 `IMPLEMENTED`（`main.rs:267-295`；`single_instance.rs`：Windows mutex + 註冊的喚醒訊息）；Python 沒有（grep `QLockFile|QSharedMemory|QLocalServer|Mutex` 於 `python/` 無）。剪貼簿 `IMPLEMENTED-UNTESTED`（`clipboard.rs` 3 個內聯測試）。
- **Known gap**：**G11.11.a [P2]** 第二次啟動「喚醒」第一個 HUD 後的可觀察結果（顯示？置前？）Python 沒有對應，必須定義；**G11.11.b [P2]** 剪貼簿：兩個 HUD 都不用（grep 為空），但在 Qt 行為對照完成前不得宣稱與 `QClipboard` 一致。
- **Test**：既有 `crates/qtrs-platform/tests/test_single_instance.rs`（3 項：命令轉換、guard callbacks、primary/secondary 衝突）。必要：第二次啟動後第一個 HUD 的視窗狀態（位置、大小、可見性）不變，除非喚醒命令明確要求顯示。
- **HUD usage**：Rust `main.rs:267-295`；Python 無。

### C11.12 應用層視窗 hook
- **Qt behavior（Python `main.py`）**：`setHighDpiScaleFactorRoundingPolicy(PassThrough)`、`setQuitOnLastWindowClosed(False)`、`setWindowIcon`、`aboutToQuit`。
- **qtrs required**：等價的預設 MUST 成立（見 C10.7、C11.2）。
- **Current implementation**：`PARTIAL`。policy 預設 `PassThrough`，DPR 為精確 `dpi/96`；無 `set_window_icon`（grep `WM_SETICON|set_window_icon` 為空），exe 圖示經 `build.rs` 內嵌。
- **Known gap**：**G11.12.a [P2]** 無視窗圖示 API（工作列／Alt+Tab 圖示由 exe 資源決定）。
- **Test**：必要：啟動後 `quit_on_last_window_closed == false`（併入 C11.2 的 `WM_CLOSE` 測試）；視窗圖示若實作，斷言 `WM_GETICON` 回非空。
- **HUD usage**：Python `main.py:46-51,98`；Rust `main.rs:319-322,599`。

---

## 12. Python / PySide6 compatibility requirements

> **格式註記**：§12 描述的是「Python HUD 對 qtrs 的需求面」，因此以表格與清單呈現，不套用五欄格式。需求在各表／清單內；gap 以 `G12.x.y` 編號（C12.3、C12.5）或指回 §2–§11 的 gap；必要測試在 C12.3、C12.7 與被引用的 §2–§11 條目；Qt behavior 由對應的 §2–§11 條目承擔。C12.1、C12.2、C12.4、C12.6 本身沒有獨立的測試欄。

### C12.1 啟動與應用層契約（`python/main.py`）

| Python 行為 | Rust 現況 | 狀態 |
|---|---|---|
| `setHighDpiScaleFactorRoundingPolicy(PassThrough)`（`main.py:46`） | setter 存在、預設 PassThrough、**無人讀取**（G10.7.c） | 等價（碰巧） |
| `setQuitOnLastWindowClosed(False)`（`:48`） | 從不設定；預設 true（G11.2.g） | **gap** |
| `aboutToQuit.connect(on_exit)`（`:98`） | 不連；清理靠 `exec` 返回後 Drop 順序（G4.1.d） | **gap** |
| `qInstallMessageHandler`（`:19-29,43`） | `ABSENT` | 以 `log` 取代，需決定是否等價 |
| `AppUserModelID`（`:36-41`） | `main.rs:621-631` 有 | OK |
| `setWindowIcon`（`:51,59`） | `ABSENT`，exe 資源 | G11.12.a |
| `app.palette()` → `lightness()` 決定 auto 主題（`hud_window.py:234-237`） | 以平台主題（`ui/mod.rs:111-122`） | 不同機制，需驗證等價 |
| `app.exec()`（`:100`） | `Application::exec`（`main.rs:599`） | OK |

### C12.2 Python 使用的 Qt 符號 → qtrs 對應（濃縮）

狀態：OK＝有等價且可用；PART＝部分；ABSENT＝grep 為空。

| Qt 符號 | Python 位置 | qtrs | 對應 gap |
|---|---|---|---|
| `QObject` 子類、parent | `refresh_controller.py:23,28`、`hotkey.py:8,15` | OK（需 unsafe 註冊） | C2.1 |
| `Signal(...)`、emit、connect | `refresh_controller.py`、`hotkey.py`、`hud_window.py` | PART：`connect` 只 Direct | C6.1–6.2 |
| `@Slot` | `refresh_controller.py:98` | ABSENT（Python 中亦無可觀察效果） | — |
| 非 Qt 執行緒發射信號（Auto→queued） | `hotkey.py:76-104` | PART | C6.2、C7.9 |
| `QTimer(parent)`：interval／timeout／start／stop | `refresh_controller.py:35-44`、`hud_window.py:440-442` | OK（`unsafe start`） | C5.1 |
| `QTimer.setSingleShot` debounce 250 ms | `hud_window.py:73-76,626,631` | PART：用 `ResizeDebouncer` | G12.5.d |
| `QTimer.singleShot(ms, fn)` | `hud_window.py:108,114,215,459,613,617` | OK | C5.5、G12.5.h |
| `QApplication`、`exec`、`quit`、`processEvents` | `main.py:47,100` | OK | C4.1–4.2 |
| `styleHints().colorScheme()` | `hud_window.py:104,231-233` | OK | C11.10 |
| `screens()`、`primaryScreen()`、`availableGeometry()` | `hud_window.py:379-387,407-419` | PART | C11.4 |
| `Qt.Frameless/Tool/WindowStaysOnTop` | `hud_window.py:123-127` | OK | C11.1 |
| `setWindowFlag(StaysOnTop)+show()` | `hud_window.py:807-808` | PART：就地 | C11.1 |
| `WA_TranslucentBackground` | `hud_window.py:128` | PART：恆 `LAYERED` | C11.1 |
| click-through（`WS_EX_TRANSPARENT` + `SetWindowPos`） | `hud_window.py:474-516` | PART | C11.6 |
| `setWindowOpacity` | `hud_window.py:132,820` | OK（**DComp 路徑除外**） | G11.5.a |
| `setMouseTracking` | `hud_window.py:129,140` | ABSENT | G8.4.b |
| `winId()`、`windowHandle().startSystemMove/Resize` | `hud_window.py:476,502,582-590` | OK | C11.1 |
| `mouseMoveEvent` + `setCursor` + `Qt.Edge` | `hud_window.py:539-577` | PART（邊緣 margin 8；用主螢幕 DPR；非 Windows 寫死 600×400） | — |
| `mousePressEvent` | `hud_window.py:579-593` | OK | C8.4 |
| `mouseReleaseEvent`（儲存幾何） | `hud_window.py:595-597` | **ABSENT**：Window 無 release handler | G12.5.f |
| `mouseDoubleClickEvent`（刷新） | `hud_window.py:619-622` | **ABSENT**；雙擊會重新開始視窗移動 | G12.5.f |
| `resizeEvent` | `hud_window.py:599-601` | OK | — |
| `moveEvent`、`closeEvent`、`showEvent`、`hideEvent` | `hud_window.py:603-617` | **ABSENT** on `Window` | G11.2.b–c、G12.5.f |
| `contextMenuEvent` | `hud_window.py:677-795` | OK | — |
| `show/hide/isVisible/activateWindow/close` | `hud_window.py:861-875` | PART：無 `is_visible/activate_window/close` | G11.2.a |
| `QVBoxLayout/QHBoxLayout`（margins、spacing、stretch） | `hud_window.py`、`provider_card.py` | OK | C9.1 |
| `addLayout`（巢狀） | 11 處 | ABSENT（以 `EmptyWidget` 包裝） | C9.3 |
| `QGridLayout`（span、stretch、最小高、spacing） | `usage_table.py:384-435` | PART：無對齊 | G9.2.a |
| `QSizePolicy`（Minimum/Preferred/Expanding） | `provider_card.py:39`、`usage_table.py:139` | OK（Label 的 `set_size_policy` 被丟棄） | G8.3.b |
| `QLabel`（setText/Alignment/ObjectName/Visible/Font） | 多處 | OK；`Alignment` 只有 Left/Center/Right | — |
| `QLabel.setPixmap` | `usage_table.py:118,280,323` | ABSENT（以自訂繪製 widget 取代） | — |
| `setToolTip` | `provider_card.py:125`、`usage_table.py:318,328,339,373-375`、`hud_window.py:155,160` | **ABSENT**（widget） | G8.8.a |
| `QPushButton.clicked` | `hud_window.py:158-161` | OK | — |
| `QFrame` VLine/HLine | `hud_window.py:341-343,359-361` | OK | — |
| `QProgressBar` + `::chunk` | `provider_card.py:70-74,97-101,163,171` | OK | — |
| `setProperty` + `unpolish/polish` | `usage_table.py:256-260` | PART：字串屬性、lazy 解析 | C8.5 |
| `deleteLater`、`setParent(None)`、`layout.takeAt` | `hud_window.py:172-193,266` | PART：無 `takeAt`，以 `StackedWidget` | C2.2、C9.6 |
| `QMenu`：`addAction/addMenu/addSeparator/menuAction/actions/aboutToShow/exec` | `hud_window.py:652-795`、`tray_icon.py:55-122` | OK；外觀為寫死 `MenuStyle` | G8.5.f |
| `QAction` setCheckable/Checked/Data/triggered | `tray_icon.py:70-77`、`hud_window.py:659` | OK | — |
| `QSystemTrayIcon` | `tray_icon.py:43-52,122,140-142` | PART：HUD 自己彈出 widgets `Menu`，從不 `set_menu` | C11.8 |
| `showMessage` 呼叫點 | `main.py:79-84`、`hud_window.py:430,524,840`、`tray_icon.py:150` | PART：只有「ghost paused」存在 | G12.5.n |
| `QPainter` + Antialiasing | `usage_table.py:160-246` | OK | C10.4 |
| `QPen`（寬度、樣式、cap、join、dash） | `usage_table.py:57,94-99,154-156,231` | OK | — |
| `QBrush(QPixmap tile)` hatch | `usage_table.py:32-42` | OK（`Brush::Hatched` 重現 6×6 tile） | — |
| `drawEllipse/Pie/Arc/Line/Polyline/RoundedRect`、`fillRect`、`drawPath` | `usage_table.py` | OK | — |
| `QPainterPath` arc／cubic／addText、`QPainterPathStroker` | `usage_table.py:72-74,181-228` | PART：Stroker `ABSENT`，以 `create_donut_arc_path` 取代 | — |
| `QFont`、`QFontMetricsF.horizontalAdvance/capHeight` | `usage_table.py:214-242` | OK（`DIFF`） | — |
| `QFont.setFeature('tnum')` | `usage_table.py:299` | PART：只有 `tabular_numbers` bool | — |
| `QColor.darker(110)` | `usage_table.py:88` | **ABSENT**；Rust 用原色（`usage_table.rs:690`） | G12.5.p |
| `QPixmap.setDevicePixelRatio(2)` | `usage_table.py:45-49` | OK（`with_dpr`） | — |
| `setStyleSheet`（widget 與 app 串接） | 見 C12.3 | OK（子集） | C8.5 |
| ctypes `SetWindowCompositionAttribute` | `vibrancy.py:75-144` | OK | C11.7 |
| ctypes `RegisterHotKey` 執行緒 | `hotkey.py:34-116` | OK（自有 FFI） | C11.9 |
| ctypes `EmptyWorkingSet` | `memory.py:32-39` | OK（時機不同） | G12.5.h |
| pynput（macOS）、`NSVisualEffectView`、ObjC click-through | `hotkey.py:118-149`、`vibrancy.py:38-72` | ABSENT／未驗證 | G11.9.b |
| `QThread`、`QPropertyAnimation`、`QGraphicsEffect`、`QSettings` | **HUD 不使用** | qtrs 有 `Thread`、`PropertyAnimation`、`Settings`；`QGraphicsEffect` ABSENT | 範圍外（§1.2） |

### C12.3 樣式表契約

**Python 的樣式表（`ui/styles.py` 與 widget 內聯字串）**
- 卡片模式（`styles.py:60-180`）共 18 條規則：`QWidget#CentralWidget`、`QLabel`、`QLabel#HeaderTitle/#HeaderStatus/#MetricTitle/#MetricValue/#SubDetail/#Badge`、`QProgressBar`、`QProgressBar::chunk`、`QPushButton#LayoutToggleBtn` 及其 `:hover`、`QFrame#Divider/#HorizontalDivider`、`QMenu`、`QMenu::item`、`QMenu::item:selected`、`QMenu::separator`；亮色版以顏色 regex 取代產生（`:182-196`）。
- HUD／表格視窗（`:206-239`）：`QWidget#CentralWidget`、`QLabel`、`#HeaderTitle/#HeaderStatus`、`QPushButton#LayoutToggleBtn`（+ `:hover`）、`QMenu*`（含 `::item:disabled`）。
- 表格 widget（`:244-255`）：`QLabel`、`QLabel#SectionTitle/#RowLabel/#Legend/#Cell/#Pill/#HeaderName/#HeaderBadge`、`QFrame#Separator`、動態屬性 `QLabel[state=muted]`。
- widget 內聯（`provider_card.py:34,38,66,93,115,128-133,143-145,162-163,170-171,197,212`；`hud_window.py:149,457`；`usage_table.py:330,368`）：`color`／`font-size`／`font-weight`／`letter-spacing`／`QProgressBar::chunk {background-color}`／`""`（清除）。
- 用到的屬性：`background-color`、`border`（`1px solid rgba()`／`none`）、`border-color`、`border-radius`、`color`、`font-family`（引號清單 + generic）、`font-size`（px，含 9.5／10.5）、`font-weight`（600/700/800）、`letter-spacing`（px）、`padding`（1/2/4 值與 `padding-left`）、`margin`（`4px 8px`）、`height`、`min/max-width/height`、`text-align`；色彩形式 `rgba(r,g,b,a<1)`、`#rrggbb`、`transparent`。

**qtrs 支援**：解析器 `QCssProperty::from_name`（`text/qcssparser.rs`）涵蓋上列全部屬性並含 `margin-*`、pseudo-state、`::sub-control`、`[attr=val]`、逗號選擇器、註解；解析器 `QStyleSheetStyle`（`style/stylesheet.rs`）：app→祖先→自己，再 specificity，再順序；`apply_declaration` 消費上列全部屬性，**除了 `margin-*` 長手寫**（被丟棄）。

**Gap**
- **G12.3.a [P1]** QMenu 規則被解析但不消費（G8.5.f）；Rust 卡片樣式表**丟掉了** QMenu 規則，Python 卡片樣式表有（`rust/src/ui/styles.rs:128-327`）；表格模式的 QMenu 數值是從 `get_hud_stylesheet` 複製的數字（`rust/src/ui/tray_icon.rs:258-278`）。
- **G12.3.b [P0, READ]** Rust 卡片 `QLabel#Badge` 加了 `max-height: 15px`（`styles.rs:192,291`），Python 沒有此屬性（`styles.py:116-124`）。
- **G12.3.c [P1, READ]** 表格模式面板：Python 的 `get_hud_stylesheet(theme, vibrant)` 依 `vibrant` 選半透明 `panel` 或 `panel_solid`；Rust 不接受 `vibrant`，一律以 `panel_bg_vibrant` 自繪（`styles.rs:371`、`hud_window.rs:88-92`）。
- **G12.3.d [P1]** 型別比對無繼承；Label 的 `pseudo_states` 為空（`:hover/:disabled` 對 label 不成立）。
- **G12.3.e [P2]** `font-family` 清單以一個原始字串存、查找時才拆；generic（`sans-serif`/`monospace`）被跳過而非映射到系統字型，`'Consolas', monospace` 在沒有 Consolas 時沒有等寬退路。
- **G12.3.f [P2, INFERENCE]** `font-weight: 800` 映射到 `FontWeight::Black`；列舉無 ExtraBold，字型選擇只分 regular／bold（`font_database.rs`）；Qt 的數值映射未驗證。
- **G12.3.g [P2]** `font-size` px 取整（`test_label_box_model.rs::qss_pixel_sizes_are_rounded_like_qt`）；`letter-spacing` 於 `label.rs`。
- **G12.3.h [P2, INFERENCE]** 色彩：8 位數十六進位被當 `#RRGGBBAA`（`qcssparser.rs`），Qt 的 `QColor` 形式是 `#AARRGGBB`，HUD 不用；具名顏色只有 black/white/red/green/blue；`rgba` 的 alpha ≤ 1 當分數、否則 0–255。
- Rust 以 `set_label_color`（`ui/mod.rs:86-109`）與 `set_status_dot_color`（`hud_window.rs:69-83`）模擬 Python `setStyleSheet(color: …)` 的 widget 內聯覆蓋。

**必要測試（唯一能抓到 G12.3.a–b 的方式）**：用 PySide6 腳本把 `ui/styles.py` 兩個主題的所有樣式表傾印為 `(selector, property, value)` 集合；Rust 測試解析 `rust/src/ui/styles.rs` 的樣式表並斷言集合相同。目前 `styles.rs` 沒有任何測試，`qtrs-gui/tests/test_qcssparser.rs::test_parse_full_python_cards_stylesheet` 只斷言「18 條規則」（`:285`），不比對屬性值。

### C12.4 行為需求（`python/PROJECT_SPEC.md` 與程式碼）

**資料流與責任邊界**（spec 3–36）
- provider 從不碰 Qt widget、不修改憑證；refresh controller 擁有請求序號、backoff、last-good 資料與排程，可變狀態**只在 Qt 主執行緒**被觸碰；表格的平均速度虛線與斜線填充**只表達速度，不改顏色**；診斷 log 有大小上限，且**絕不**含憑證、原始回應或 CLI stderr。

**資料契約**（38–44）
- `metric*_val` 為已用百分比 0–100 或 None；None 顯示 `--`；0 是有效讀數；兩個視窗都不可用 → schema error，一個可用 → 允許部分顯示；badge 不得帶未驗證的方案／模型名；無重設時間顯示 `--`；`error_code` 為診斷類別，`error` 是使用者可讀且不得含 token 或原始 payload；`stale`／`last_success` 由 coordinator 設定，last-good 不得冒充目前資料。

**排程不變量**（46–52）
- worker 只持有 Python queue，**從不從背景執行緒呼叫 Qt**；主執行緒每 25 ms 排空；每個 provider 最多一個 worker；手動刷新或喚醒會使舊結果失效，進行中的查詢結束後**恰好再跑一次**；**無假取消**；close 之後的遲到結果被忽略；provider 彼此獨立排程（慢的 AGY 不阻擋其他）；CLI 有總期限；OS 呼叫卡住時該 provider 等原查詢，不無限制產生 worker。

**設定與平台**（54–58）
- 打包版不得把設定寫進 PyInstaller 解壓目錄；切換 layout 前儲存舊大小，套用新大小時抑制中間事件寫入；move／resize 寫入延後並以 250 ms 合併。

**spec 未寫但程式碼有的行為**（需要列入契約）
- 熱鍵 Alt+C 切換可見性、Alt+Shift+C 切換 click-through（`main.py:67-69`）；托盤左鍵 Trigger 切換可見性（`tray_icon.py:140-142`）；視窗雙擊刷新（`hud_window.py:619-622`）；邊緣 resize margin 8 px，除非 `locked` 才可拖曳（`:539-593`）；幾何在 mouse release、close 與 250 ms 單發合併後持久化（`:595-609,624-649`）；啟動 click-through 延遲 300 ms，熱鍵失敗則停用（`:106-108,426-437`）；**喚醒偵測**：倒數計時器 tick 間隔 >15 s 就刷新（`:461-467`）；`appearance=auto` 跟隨系統色彩配置（`:104,213-238`）；視窗預設／最小（`:28-35`）：表格 380×280／450×350，水平 540×125／690×145，垂直 250×320／280×410（Rust 常數在 `config.rs`）。
- Python 的測試即可執行規格：`tests/test_ui.py` 涵蓋錯誤恢復文字清除、過期標籤、主題持久化、只在 auto 模式響應 `colorSchemeChanged`、各 layout 獨立大小、大小還原與重設；`tests/test_refresh.py` 涵蓋喚醒不重疊、backoff、stop 忽略遲到結果、`retry_after` 上限、provider 獨立。

### C12.5 Python HUD 與 Rust HUD 之間的差異（每一項必須修復或明確核准為刻意差異）

| ID | 差異 | 證據 | 處置 |
|---|---|---|---|
| G12.5.a [P0, READ；= G12.3.b] | Badge `max-height: 15px` 只在 Rust | `styles.rs:192,291` vs `styles.py:116-124` | 修復＋樣式表比對測試 |
| G12.5.b [P0, READ；= G9.4.b] | 卡片根 layout spacing 2（Rust）vs 5（Python），無註解說明 | `provider_card.rs:159` vs `provider_card.py:27`（已讀確認） | 修復或寫理由 |
| G12.5.c [P1] | 面板底色不依 Acrylic 是否成功而改變 | `hud_window.rs:88-92,229` vs `hud_window.py:240-250` | 修復 |
| G12.5.d [P0, READ] | 幾何持久化：Python 250 ms 單發於 move／resize 重啟＋mouse release 儲存；Rust 只有 resize 的 `ResizeDebouncer` + 3 s 輪詢抓移動，且無 release handler | `hud_window.py:595-609,624-649` vs `config.rs:396`、`main.rs:575-591` | 修復 |
| G12.5.e [P0, READ] | 喚醒偵測（倒數 tick 間隔 >15 s 就刷新）在 Rust 不存在 | `hud_window.py:461-467`；grep `gap|WM_POWERBROADCAST|resume` 於 `rust/src` 為空 | 修復 |
| G12.5.f [P0, READ] | `Window` 沒有 mouse-release／double-click／move／close 的 handler；雙擊在 Rust 會重新開始視窗移動，Python 是刷新 | `window.rs:1025`（platform 有發 release）；`window.rs:771-815` | 修復 |
| G12.5.g [P0, READ；= G11.2.b] | Alt+F4 / `CloseRequest` 被吞 | G11.2.b | 修復 |
| G12.5.h [P1] | 單發時序：Python 300 ms（啟動 click-through）、150 ms（hide 後 trim）、1000 ms（busy→idle 後 trim）、2500 ms（啟動後 trim）；Rust 在 `hide()` 立即 trim，且只有 2500 ms | `hud_window.py:108,114,215,459,613,617` vs `hud_window.rs:484`、`main.rs:594` | 修復或核准 |
| G12.5.i [P0, READ；= G8.8.a] | 所有 widget tooltip 缺失（錯誤與過期資料以 tooltip 顯示） | G8.8.a | 修復 |
| G12.5.j [P0, READ；= G11.9.a] | 熱鍵註冊失敗不被回報；鎖定防護失效 | G11.9.a | 修復 |
| G12.5.k [P1] | `--smoke-test` 不檢查設定持久化 | `smoke_check.py:26-29` vs `main.rs:89-114` | 修復 |
| G12.5.l [P0, READ；= G11.4.a] | 螢幕選擇／脫離螢幕還原規則 | G11.4.a | 修復 |
| G12.5.m [P1] | 托盤選單：Python 的托盤選單沒有鎖定／不透明度／間隔／重設／隱藏等項目；Rust 托盤選單是完整的 context menu | `tray_icon.py:54-122` vs `rust/src/ui/tray_icon.rs:73-240` | 需決定 |
| G12.5.n [P1] | 托盤通知：Rust 只有「ghost paused」；缺 hotkey 失敗、ghost 啟用、autostart 失敗 | `main.rs:503`、`main.rs:486-488`、`hud_window.rs:526-532`、`tray_icon.rs:686-690` | 修復 |
| G12.5.o [P1] | QMenu 外觀為寫死數值，非 QSS | G8.5.f | 修復或核准 |
| G12.5.p [P1] | `QColor.darker(110)` 缺失：Rust 用原色 | `usage_table.rs:690` vs `usage_table.py:88` | 修復（需逐像素比對） |
| G12.5.q [P1] | `UsageDial` 最小尺寸 0 vs 84 | G8.3.e | 修復 |
| G12.5.r [P1] | 版面切換：`StackedWidget` vs 重建 | C9.6 | 決定 |
| G12.5.s [P0, READ；= G8.5.d] | `Window::set_style_sheet` 為 app 全域 | G8.5.d | 修復 |
| G12.5.t [P0, READ；= G11.5.a] | DComp 路徑 opacity 無效（待實測） | G11.5.a | 實測後修復 |
| G12.5.u [P2] | 色彩／字型解析細節 | G12.3.e–h | 驗證 |
| G12.5.v [P1] | 發佈 profile `panic = "abort"` vs Python excepthook | G7.9.b | 決定 |
| G12.5.w [P2] | `rust/README.md` 仍描述 egui/eframe/reqwest | `rust/README.md:3,27,95` | 文件修正 |

### C12.6 既有的對照 harness 與未涵蓋範圍
- **`DIFF`（需要 PySide6 與真實平台 plugin）**：`qt_layout_compare.py`（layout，見 C9.7）；`qt_advance_compare.py`（`QFontMetricsF.horizontalAdvance` vs `FontMetrics::horizontal_advance_exact`）；`qt_glyph_compare.py`（字形覆蓋）；`qt_lcd_dump.py`（Qt 文字畫到不同目的地；傾印作為 `test_lcd_text_parity.rs` 的像素 fixture：`lcd_segoe12`、`lcd_jhenghei12`）。
- **寫死 Qt 測量值的測試（`cargo test` 即可跑，不需 PySide6）**：`test_qt_text_advances.rs`（僅 Windows，Qt 6.11.2）、`test_directwrite_face.rs`、`test_label_box_model.rs`、`test_layout_stretch_minimum.rs`、`test_menu_style_box_model.rs`、`usage_table.rs:1485-1578`（450×350、DPR 1.25 的 grid 幾何）。
- **視窗行為 harness（螢幕擷取 + Win32）**：`driver.py`（拖曳真實 HUD 角落並擷取桌面，偵測 stale 舊幀「second layer」）；`w32_layered.py`、`qt_layered.py`、`showwindow_cost.py`、`ulw_cost.py`、`hud_drag.py`、`hud_menu_hover.py`。`results.txt`／`results_qtrs.txt` 是**過期輸出**（qtrs:debug 在 8 個情境中 7 個重現 ghost、qtrs:release 全部不重現）。
- **視覺**：`--snapshot`（`main.rs:116-264`）寫 `rust/target/snapshots/hud_*.png`；`target/py_snapshots/py_*.png` 存在但**repo 內沒有腳本產生它們，也沒有任何東西拿它們與 Rust 輸出做差分**；`render_hud_preview.rs` 只渲染不比較。
- **效能／記憶體**：`py_mem.py`、`exe_mem.py`、`hud_mem.py`、`hud_startup.py`、`hud_paint_steady.py`、`lto_*`。
- **完全沒有 harness 涵蓋**：信號、計時器、事件迴圈語意、跨執行緒遞送；托盤、選單、action（`aboutToShow`、勾選狀態、Exit 路徑）；設定持久化時序、熱鍵註冊失敗 UX、click-through；**樣式表字串**；喚醒刷新、雙擊刷新、tooltip、多螢幕位置；`python/tests/test_ui.py` 與 `test_refresh.py` 的行為在 Rust 端沒有以**同一個情境**驅動的對應測試（`rust/src` 內的單元測試只檢查 Rust 內部）。
- **CI**：`.github/workflows/ci.yml` 只在 `rust` 目錄跑 `cargo test --all-targets`，只測應用 package；`rust/qtrs` 是獨立 workspace（`rust/Cargo.toml` 無 `[workspace]`），**qtrs 各 crate 的測試、任何 harness、任何 Python 差分都不在 CI 中**。

### C12.7 驗收閘門：什麼時候可以說「一致」

| 宣稱 | 最低證據 |
|---|---|
| 「扁平 box／grid layout 與 Qt 一致」 | C9.7 的 harness，記錄 commit／cases／seed／差異數 |
| 「HUD layout 與 Qt 一致」 | C9.7(b) 的擴充全部完成，且 provider card 幾何（G12.5.b）與 Python 逐項比對 |
| 「文字渲染一致」 | `DIFF`：`qt_advance_compare.py`、`qt_glyph_compare.py`、`test_lcd_text_parity.rs` |
| 「像素一致」 | 在 100／125／150／200% 下對 Python 與 Rust HUD 截圖逐像素差分，**並列出差異像素數與平均絕對差**；之前一次臨時量測（125%：246,594 像素中 6,685 個不同，平均絕對差 2.38）**沒有存進 repo，無法重現，不得引用為證據** |
| 「行為一致」 | C12.5 每一項關閉，或在 §1.3 登記為刻意差異 |
| 任何涉及像素的回報 | 結尾 **MANUAL WINDOWS VERIFICATION REQUIRED**，除非逐像素差分實際為零 |

---

## 附錄 A：Gap 總表

共 284 項：D 12、P0 34、P1 124、P2 111、test gap 3。依章節排序。嚴重度與驗證等級見 §0。`D` 項必須附理由，且誤用時可見失敗。P0 項的修復單位見附錄 D（root cause）。

| ID | 嚴重度／驗證 | 摘要 |
|---|---|---|
| G2.1.a | P0, RAN | `set_parent(owned_child, None)` **銷毀 child** |
| G2.1.b | P1, READ | `add_owned_child` 不送 `ChildAdded` |
| G2.1.c | P2, READ | `ChildAdded/Removed` 經 `dispatch_to_object`／`event()` 直送，不經 `notify_helper` |
| G2.1.d | P1, READ | parent 正被借用 |
| G2.1.e | P2, READ | 無自我 parent／環／跨執行緒檢查 |
| G2.1.f | P2, READ | `set_parent` 跨 registry 鎖與物件借用，非原子 |
| G2.1.g | D | 已註冊但非 `owned_children` 的 child，parent drop 時只解除註冊、不銷毀 |
| G2.2.a | P1, READ | `delete_later` 不會自我 post、不會 drop |
| G2.2.b | P1, READ | 不清除 posted events |
| G2.2.c | P2, READ | 無 `destroyed` 信號 |
| G2.2.d | P1, READ | widgets 的 `delete_later` 在「當前 dispatch 回傳時」就刪，比 Qt 的「回到事件迴圈」早 |
| G2.3.a | P2, READ | 型別為 exact `TypeId` |
| G2.3.b | P2, READ | 深度優先逐 child，而非先掃完直接子物件 |
| G2.3.c | P2, READ | 只搜 `owned_children` |
| G2.3.d | P2, READ | 無 `objectNameChanged` |
| G2.3.e | P2, READ | `parent()/children()/thread()` 取用器不存在 |
| G2.4.a | P1, READ | `Signal::new()` 沒有 emitter id → `block_signals` 被忽略 |
| G2.4.b | P1, READ | 即使有 emitter id，物件若未 `register_qobject`，`query_object_signals_blocked` 回 `None`，被當成「未封鎖」 |
| G2.4.c | P2, READ | unregister 後封鎖靜默失效 |
| G2.5.a | P1, READ | 重複 `register_qobject` 同一物件會把它標死：`register_object_metadata` 對被取代的 record 呼叫 `liveness.store |
| G2.5.b | P2 | `QPointer` 無法解參考，只攜帶 `id()` |
| G2.6.a | P2, READ | 新動態屬性回 `true` |
| G2.6.b | P2, READ | `Variant::Invalid` 不移除 |
| G2.6.c | P1, READ | notify signal 從不發射 |
| G2.6.d | P2, READ | 沒有 `as_qobject_any` 時 meta 路徑靜默落到動態 map |
| G2.7.a | P1, READ | 對已被借用的物件送事件會**無聲丟失** |
| G2.7.b | D | 跨執行緒直接派送不支援 |
| G2.7.c | test gap | `test_qobject_safety_and_qt6_features.rs` 的「Memory Safety & Dynamic Borrow Exclusivity Tes |
| G3.1.a | P2, READ | `Pointer`=251、`DpiChanged`=250 不是 Qt 數值 |
| G3.1.b | D | `EventKind` 為封閉 enum，使用者自訂事件只有 `EventKind::User(Box<dyn Any>)` |
| G3.1.c | P1, READ | 缺少的事件型別代表 filter 看不到 `Paint`／`Polish`／`LanguageChange` 等 |
| G3.2.a | P1, READ | 跨輪／下一輪的優先序錯誤 |
| G3.2.b | P0, READ | 目標執行緒沒有已註冊 loop 時 `post_event_to_thread` 回 `false`，呼叫端 |
| G3.2.c | P2, READ | 巢狀 pump |
| G3.2.d | P2 | 無 null receiver 警告 |
| G3.2.e | D | 無 `sendPostedEvents(receiver, type)` 篩選式 flush、`removePostedEvents`、`hasPendingEvents` |
| G3.3.a | P2, READ | `Quit` 保留**新的** exit code |
| G3.3.b | P2, READ | `MouseMove`/`HoverMove` 被壓縮 |
| G3.3.c | P2, READ | `Move`/`UpdateLater`/`LanguageChange` 壓縮不存在 |
| G3.3.d | P2, READ | 每次 post 都 O(queue) 掃描 |
| G3.3.e | D | `Quit{exit_code}` 是 qtrs 擴充 |
| G3.4.a | P1, READ | `notify_helper` 迭代 filter id 的**快照** |
| G3.4.b | P2, READ | filter 物件正被借用 |
| G3.4.c | P1, READ | Windows 計時器遞送不經 `notify_helper`／`event()` |
| G3.4.d | D | 跨執行緒 `send_event` 回 false 且不做事 |
| G3.4.e | P2, READ | `install_event_filter` 拒絕自己與直接環 |
| G3.5.a | P1, READ | 無 scope level：同 handler 內 `process_events` 會刪 |
| G3.5.b | P1, READ | `EventLoop::exec` 結束沒有 cleanup flush |
| G3.5.c | P1, READ | widgets 的 `delete_later` 時機比 Qt 早 |
| G3.6.a | P1, READ | 對已銷毀接收者的 queued slot 仍會執行 |
| G3.6.b | P2, READ | `unregister_qobject` → `stop_timers_for_object` 只刪 registry 項、不呼叫 Win32 `KillTimer`，孤兒 `WM |
| G4.1.a | P1, READ | `exit()` 先於 `exec()` 會**保留**並使 `exec` 立刻返回 |
| G4.1.b | P1, READ | `exit_requested` 在 `exec` 返回後不重置，第二次 `exec()` 立刻以舊 code 返回 |
| G4.1.c | P1, READ | worker 執行緒呼叫 `quit()/exit()` 是**靜默 no-op** |
| G4.1.d | P1, READ | `aboutToQuit` 在 `exec` **返回後**才發，不是在 `exit()` 內 |
| G4.1.e | P1, READ | `quit()` 不是 `Quit` 事件：不關視窗、不能被否決 |
| G4.1.f | P2, READ | `CoreApplication::new` 文件寫會 panic，實際沒有第二個實例檢查 |
| G4.1.g | P1, READ | 無獨立的巢狀 `QEventLoop` |
| G4.1.h | P2 | `ExitCode::from(exit_code as u8)` 截斷 |
| G4.1.i | P1, READ | `qtrs-widgets::Application` 在最後一個視窗被 `unregister_window` |
| G4.2.a | P1, READ | **巢狀 `process_events` 靜默失效**：`CoreApplication::exec` 整個 `exec` 期間持有 `LOCAL_EVENT_LOOP.borr |
| G4.2.b | P2, READ | 只觸發計時器的一輪回傳 false |
| G4.2.c | P2 | 無 `ExcludeUserInputEvents`／`WaitForMoreEvents`／`maxTime` |
| G4.2.d | P2, READ | `WM_QUIT` 以 `PostQuitMessage(n)` 的 n 為 code |
| G4.3.a | P1, READ | ）——選單開著時 posted events／計時器是否持續觸發，取決於 loop 如何喚醒該執行緒，**尚未實測** |
| G5.1.a | P1, RAN | `set_interval` 對啟動中的計時器無效 |
| G5.1.b | P1, READ | `set_single_shot` 啟動中無效 |
| G5.1.c | P1, READ | `Timer::start` 是 `unsafe` 並以原始位址註冊 |
| G5.1.d | P2, READ | 執行緒沒有 timer context 時 `start` 靜默回 `TimerId::INVALID` |
| G5.1.e | P2, READ | `timeout` 是公開 `Signal<()>` 欄位，不在 meta-object 上 |
| G5.2.a | P1, READ | Windows 路徑直接呼叫 `obj.timer_event(id)`，不建 `Event`、不經 `notify_helper`/`event()` |
| G5.2.b | P2, READ | `send_timer_events` 持有 registry mutex 的同時遞送 |
| G5.3.a | P1, RAN | 單發 0 ms 計時器觸發後 **registry 項殘留**：重現 `fired=1 registry_len=1 next_timeout=Some(0ns)` |
| G5.3.b | P1, READ | 重複 0 ms 計時器只觸發一次 |
| G5.4.a | P1, RAN | 重現：註冊物件 `start_timer(20ms)`、pump 400 ms → `timer_event` **0 次** |
| G5.4.b | P1, READ | `kill_timer` 不呼叫 `KillTimer` |
| G5.4.c | P2 | `TimerId` 是 per-registry 計數器，非全域唯一 |
| G5.5.a | P2, READ | 無 context 版本、無取消 |
| G5.5.b | P1, READ | 執行緒無 loop 時 `single_shot` 靜默丟 callback |
| G5.5.c | P2 | 類型恆為 `Coarse` |
| G5.5.d | P2 | callback 必須 `Send + 'static` 且只在呼叫執行緒執行 |
| G5.6.a | P2, READ | `remaining_time` 用 floor |
| G5.6.b | P2, READ | Win32 一律以**原始**間隔 `SetTimer` |
| G6.1.a | P0, RAN | **發射期間被 disconnect 的 slot 仍會執行** |
| G6.1.b | P0, RAN | **兩個 Signal 的 `ConnectionId` 在全域表 `GLOBAL_CONNECTIONS` 碰撞** |
| G6.1.c | P2, READ | `disconnect_receiver`／`disconnect_all` 不清 `GLOBAL_CONNECTIONS` |
| G6.1.d | P1, READ | 無 `UniqueConnection`、`SingleShotConnection`、signal-to-signal 連線 |
| G6.1.e | D | slot 需 `Fn(&T) + Send + Sync + 'static` |
| G6.1.f | P1, READ | `connect_with_type(Queued, …)` 只儲存 direct dispatcher，`emit` 時被當 direct 呼叫——「Queued」連線若不是用知 |
| G6.2.a | P1, READ | receiver 執行緒在**連線時**擷取，且優先於即時查詢 |
| G6.2.b | P2, READ | 同執行緒 BlockingQueued 直接呼叫 |
| G6.2.c | P0, READ | 目標執行緒無 loop 時 queued 閉包忽略 `post_event_to_thread` 回傳值 → **queued slot 靜默遺失** |
| G6.2.d | P2, READ | BlockingQueued 無逾時 |
| G6.2.e | P2 | queued 需 `T: Clone + Send + 'static` |
| G6.3.a | P1, READ | 無 emitter id 的 Signal |
| G6.3.b | P2, READ | 無存活檢查 |
| G6.3.c | P2 | queued 與巢狀路徑沒有測試 |
| G6.4.a | P1, RAN | 受 G6.1.b 影響 |
| G6.4.b | P1, READ | 已排入的 `MetaCall` 不被清除 |
| G6.4.c | P2 | 無 receiver 的閉包連線 |
| G6.4.d | test gap, 已讀原始碼確認 | `test_signal_sender_tracking_and_auto_disconnection` |
| G6.5.a | P1, READ | 見 G3.6.a |
| G6.5.b | P1, READ | HUD **依賴**這個 stub 路徑 |
| G6.5.c | P1, READ | `invoke_method` 無連線類型參數、無 queued 形式 |
| G6.5.d | D | 無字串式 `SIGNAL()/SLOT()`、無 `connectSlotsByName` |
| G6.6.a | P1, READ | 無 `#[slot]` |
| G6.6.b | P2, READ | derive 一律把 superclass 設為 `QOBJECT_META_OBJECT` |
| G6.6.c | P2 | signal／property 型別名稱是 Rust 名 |
| G6.6.d | P2 | `index_of_signal` 以名稱或完整簽章比對，property 的 notify signal 不解析成索引 |
| G6.7.a | D | 無法表達「Python 風格鬆散 slot」 |
| G7.1.a | P1, READ | 無 TLS context 的執行緒 |
| G7.1.b | P2 | 無 `QObject::thread()` |
| G7.1.c | P1 | `ThreadPool` worker 從不呼叫 `init_current` |
| G7.2.a | P0, READ | 目標執行緒無已註冊 loop 時回 false／靜默丟 |
| G7.2.b | P2, READ | `EventSender::send` 直接推進 `q.events`，**不壓縮、無優先序、忽略 `insertion_offset`** |
| G7.2.c | P2 | 兩份登記表可能分歧 |
| G7.2.d | P1, READ | 無 `removePostedEvents` |
| G7.2.e | P2, READ | `Widget::update()` 以 `ThreadId::current()` 為目的地而非 widget 親和性 |
| G7.3.a | P1, READ | `FutureWatcher`／`Promise` 的信號在**生產者執行緒**以普通 `connect` |
| G7.4.a | P1, READ | foreign-thread `send_event` 對存活物件回 false 且**無診斷** |
| G7.4.b | D | `move_to_thread` 之後事件被路由到目標佇列，但 `with_object_mut` 在那邊失敗 |
| G7.4.c | P1, READ | stale MetaCall 對已銷毀 receiver 照跑 |
| G7.5.a | P1, READ | **計時器被停止、不遷移** |
| G7.5.b | P1, READ | **事件可能遺失**：從來源佇列移除後，只有 `query_thread_sender(target)` 有值才重送 |
| G7.5.c | P2, READ | 轉移的事件繞過壓縮／優先序 |
| G7.5.d | P2, READ | 只更新一層 children 的 `ObjectData.thread_id`，更深後代只更新 registry，`connect_to` 之後讀到 stale 欄位 |
| G7.5.e | P2, READ | `ThreadChange` 在親和性更新**之後**送 |
| G7.5.f | P2 | `caller` 是參數而非從 OS 讀取 |
| G7.5.g | P2 | target 沒有 loop 時仍回 `Ok` |
| G7.5.h | D | 物件實體仍綁註冊執行緒 |
| G7.6.a | P1, READ | `ThreadHandle::is_finished()` 在 `join(self)` 之前**恆為 false** |
| G7.6.b | P1, READ | `ThreadBuilder::priority` 存了但從不套用 |
| G7.6.c | P1 | 無 `started/finished` 信號、`wait(timeout)`、`exit(code)`、`terminate`、`isRunning` |
| G7.6.d | P2 | 閉包 panic 會跳過 `clear_current` |
| G7.6.e | P2 | `EventLoopThreadHandle` 無 Drop／join，丟棄即分離 |
| G7.7.a | P2, READ | 靜默 no-op 取代警告 |
| G7.7.b | P2, READ | 無擁有者檢查 |
| G7.9.a | P2, READ；P0 主張已被讀碼推翻，待驗證 | 啟動競態：worker／熱鍵執行緒是否可能在主 loop 註冊前就 post？讀碼：`Application::new` |
| G7.9.b | P1 | 發佈設定 `panic = "abort"` |
| G8.1.a | P1, READ | **show／hide 不自動重排** |
| G8.1.b | P2, READ | 隱藏 item 的 geometry 被設為 (0,0,0,0) |
| G8.1.c | P1, READ | 無 Show/Hide 事件 |
| G8.2.a | P1, READ | 傳遞、重繪、`EnabledChange`、焦點清除、`:disabled` 全缺 |
| G8.3.a | P1, READ | 無通用 min/max/fixed API |
| G8.3.b | P0, READ | **`Label.set_size_policy` 被丟棄** |
| G8.3.c | P2, READ | `WidgetBase::set_geometry` 不夾 min/max |
| G8.3.d | P2 | 預設 size_hint 100×30 會讓忘了覆寫的自訂 widget 得到假值 |
| G8.3.e | P1, READ | `UsageDial`：Python `setMinimumSize(84,84)` |
| G8.4.a | P0, READ | 無 parent 傳遞 |
| G8.4.b | P1 | 無 tracking 語意 |
| G8.4.c | P1 | 無隱式 grab |
| G8.4.d | P1 | Enter/Leave 非祖先鏈 |
| G8.4.e | P2 | 無 `WA_TransparentForMouseEvents`／`WA_NoMousePropagation` |
| G8.4.f | P1 | 右鍵 `context_menu_cb` 在 release 時觸發，與 widget 是否 accept 無關 |
| G8.4.g | P1 | `Window` 沒有 release／double-click／move handler |
| G8.5.a | P1, READ | 無繼承比對 |
| G8.5.b | P1, READ | `attributes` 只有 Label |
| G8.5.c | P0, READ | **樣式變更不重排**：`WidgetBase::set_style_sheet` 只標 dirty |
| G8.5.d | P0, READ | `Window::set_style_sheet` 是**整個 Application 的** |
| G8.5.e | P1, READ | `:disabled`/`:focus` 不支援 |
| G8.5.f | P1, READ | **QMenu 規則被解析但從不被消費**：`type_name: "QMenu"` 在原始碼中不存在 |
| G8.5.g | P2, READ | `margin-*` 長手寫被解析後在 `apply_declaration` 丟棄 |
| G8.5.h | P2, READ | 父 widget 的 `font` 繼承未實作 |
| G8.6.a | P2 | 無 per-widget `WA_*` 屬性 |
| G8.6.b | P1, READ | `set_stays_on_top` 執行期路徑沒有測試 |
| G8.6.c | P1 | 無 layout 導出的頂層最小尺寸 |
| G8.7.a | P1, READ | 無 child 裁剪 |
| G8.7.b | P2 | 髒區只有整個 widget |
| G8.8.a | P0, READ | Python 在 `provider_card.py:125`、`usage_table.py:318,328,339,373-375`、`hud_window.py:155,16 |
| G9.1.a | P1, READ | `add_stretch(0)` 被強制成 1 |
| G9.1.b | P1 | 無 `add_spacing`／`add_spacer_item`／`insert_stretch`／`set_stretch_factor` |
| G9.1.c | P1 | 無 item 對齊 |
| G9.1.d | P1 | 無 `heightForWidth` |
| G9.1.e | P1 | 無 `retainSizeWhenHidden`、無 RTL |
| G9.2.a | P0, READ | **無 per-item 對齊** |
| G9.2.b | P1, READ | `Layout::add_widget_with_stretch` 對 grid **靜默忽略 stretch** 並新增一列 |
| G9.2.c | P1 | 無 `setRowStretch`／`setColumnStretch`／`setColumnMinimumWidth` 讀回 |
| G9.3.a | P1, INFERENCE | wrapper 是 QWidget item：其 `maximum_size` 為 16777215，而巢狀 `QLayout` 回報其子項最大值之和 |
| G9.3.b | P2 | wrapper 多一個 child widget 進入 hit-test／paint 樹 |
| G9.3.c | P0, READ | HUD 的 `header_widget` 額外被設為 `Expanding/Fixed` |
| G9.4.a | P1, READ | 依賴 Qt 預設的 layout |
| G9.4.b | P0, 已讀兩側原始碼確認 | **卡片根 layout spacing 不同**：Python `layout.setSpacing(5)` |
| G9.5.a | P1, READ | 無向上傳遞：葉節點的 hint 變更不會爬到祖先 layout |
| G9.5.b | P1, READ | `Button::set_text/set_font`、`Label::set_font/set_alignment`、`set_style_sheet`、`set_propert |
| G9.5.c | P2 | setter 立即重排與 Qt 壓縮不同 |
| G9.5.d | P1 | 頂層最小尺寸不從 layout 導出 |
| G9.6.a | P1, READ；決議：不在 P0 階段修，不標 D | `[QT-SRC qstackedlayout.cpp:417-448]`：Qt 的 `sizeHint` 取**所有頁面**的最大值 |
| G9.6.b | P2 | `set_spacing` 為 no-op |
| G9.7.a | P1 | harness 不在 CI、不是 `cargo test` |
| G9.7.b | P1 | 涵蓋範圍如上 |
| G10.1.a | P1, READ | `UpdateRequest` 只到達經 `unsafe Window::register()` 註冊的視窗 |
| G10.1.b | P2 | `update()` 不看可見性 |
| G10.1.c | P2 | `Window::set_geometry`／`show`／`set_opacity`／`set_style_sheet` 同步繪製 |
| G10.2.a | P2 | 無局部／立即重繪 API |
| G10.2.b | P2 | 每個 widget 一個髒矩形，合併為一個外框 |
| G10.3.a | P1, READ | 無 per-widget clip |
| G10.3.b | P1, READ | `EmptyWidget::set_geometry` 只使新矩形失效：**移動不會使舊位置失效** |
| G10.3.c | P2 | `Painter::set_clip_rect` 取代而非相交，且忽略目前 transform |
| G10.4.a | P1, READ | `set_clip_rect` **不經 transform** |
| G10.4.b | P2, READ | `set_clip_rect` 取代而非相交：`ScrollArea` 內失去 widget 層的髒區 clip |
| G10.4.c | P1, READ | `draw_text` 忽略旋轉與縮放，只轉換原點 |
| G10.4.d | P1, READ | 無 render hints |
| G10.4.e | P2, INFERENCE | `Painter::begin` 只要 ClearType 開就啟用 LCD 文字，與目的地 alpha 無關 |
| G10.5.a | P1, 注意 | 既有測試 `rust/src/ui/tray_icon.rs::test_menu_text_does_not_depend_on_window_offset` **只涵蓋 `tr |
| G10.5.b | P2, INFERENCE | 子選單原點是 `parent_native + round(sg.x*dpr)`，Qt 是 `round((root+sg)*dpr)`，可差 1 裝置像素 |
| G10.5.c | P1, READ | `present_popup`／`exec_popup` 用 `primary_screen().device_pixel_ratio()` |
| G10.5.d | P2 | hit-test 用邏輯整數，繪製用裝置取整，差最多 1 裝置像素 |
| G10.5.e | P2 | 非 Windows 的 `exec_popup` 沒有原生視窗 |
| G10.6.a | P2 | 無 `QPaintEvent` 矩形 |
| G10.6.b | P2 | 無法防止畫到矩形外 |
| G10.7.a | P0, READ，未實測 | `DpiChanged` 處理把 store 調成 `dpi_x/96`，然後呼叫 `do_render_and_present`，後者又以 `platform().primary |
| G10.7.b | P1, READ | `application_device_pixel_ratio` |
| G10.7.c | P1, READ | `HighDpiScaleFactorRoundingPolicy` 存了但從不讀 |
| G11.1.a | P2 | 無 `set_window_flags` |
| G11.1.b | P1 | 測試只檢查 `flags` 欄位，不檢查 `WS_EX_TOPMOST`／`WS_EX_TRANSPARENT` |
| G11.2.a | P1, READ | 無 `Window::is_visible()` |
| G11.2.b | P0, READ | `CloseRequest` 在 `WindowEventHandler` 被 `_ => {}` 吞掉 |
| G11.2.c | P0, READ | 無 `showEvent/hideEvent/closeEvent` hook：Python 的「show 時重新套用主題」「hide 時 trim_memory」沒有 Rust  |
| G11.2.d | P2 | 無 `Expose` 重繪 |
| G11.2.e | P1, READ | `Application::unregister_window` 在 drop 時、`quit_on_last_window_closed` 為 true 就呼叫 `quit` |
| G11.2.f | P2 | `GuiApplication::set_application_state`、`last_window_closed`、`focus_window_changed` 從不發射 |
| G11.2.g | P1 | `main.rs` 從不 `set quit_on_last_window_closed(false)` |
| G11.3.a | P0, READ | DPI 混用：WM handler 用 `GetDpiForWindow`，`Window::set_geometry` 用主螢幕 DPR |
| G11.3.b | P2 | 位置是 `i32` 邏輯值 |
| G11.3.c | P1, READ | `NativeWindow::geometry()` 回實體 `GetWindowRect`，`Window::geometry()` 為邏輯 |
| G11.4.a | P0, READ | Rust HUD 啟動時用 `primary_screen().geometry()` 與 `ensure_within_screen` |
| G11.4.b | P1, READ | `Window` 無 `screen()` |
| G11.4.c | P1, INFERENCE | `Win32Screen::geometry` 以 dpr 除原點 |
| G11.4.d | P2 | `Win32Screen::primary()` 寫死 `MonitorFromPoint(0,0)` |
| G11.5.a | P0（條件式：僅 DComp 可用的機器）, READ；本機 RAN：選到 Layered，未重現 | **DComp 路徑丟棄 opacity**：`present_dirty_ref(&mut self, pixmap, _opacity, dirty)` |
| G11.5.b | P2 | 非分層視窗 `SetLayeredWindowAttributes` 失敗時靜默 |
| G11.5.c | test gap, RAN | `test_dcomp_*` |
| G11.6.a | P2 | 非 `LAYERED` 視窗的 `set_click_through(true)` 只得 `WS_EX_TRANSPARENT`，沒有 `WS_EX_LAYERED` 時不穿透 |
| G11.6.b | P1 | 沒有測試斷言樣式位元 |
| G11.7.a | P1, READ | HUD 在視窗可見之前呼叫 `set_backdrop` |
| G11.7.b | P2 | `None` 比 Python 的 `clear` 多做 DWM 呼叫 |
| G11.7.c | P2 | macOS 路徑只對 `MockObjcRuntime` 測過 |
| G11.8.a | P1 | 選單位置換算用主螢幕 DPR |
| G11.8.b | P2 | `show_message` 只收 title／text／4 值圖示 enum／時間，不收自訂 `QIcon` |
| G11.8.c | P0, READ；= G11.9.a 的重複登錄 | Python 的 `hotkey_failed` 訊息 Rust 沒有 |
| G11.8.d | P2 | 圖示：Python 依平台選 `.ico/.icns/.png` |
| G11.8.e | P2, INFERENCE | 雙擊在 Windows 先 Trigger 兩次再 DoubleClick |
| G11.8.f | P2 | DBus／macOS 後端存在但未驗證 |
| G11.9.a | P0, READ | `HotkeyManager::start` 在 Windows 即使 `RegisterHotKey` 失敗也回 `Ok`，失敗只在執行緒內 `warn!` |
| G11.9.b | P1 | macOS 熱鍵明確未實作 |
| G11.9.c | D | `GenericHotkeyManager`／`CocoaHotkeyManager`／`UnixHotkeyManager` 為 stub，回報成功卻未註冊 |
| G11.9.d | P2 | app 以原子旗標 + `run_on_main_thread` |
| G11.10.a | P2 | 高對比與 `ShouldAppsUseDarkMode` 未驗證 |
| G11.10.b | P2 | `GuiApplication::new` 預設 `Palette::dark()`，不跟隨系統配置 |
| G11.10.c | P2 | `theme.rs` 無單元測試 |
| G11.11.a | P2 | 第二次啟動「喚醒」第一個 HUD 後的可觀察結果 |
| G11.11.b | P2 | 剪貼簿：兩個 HUD 都不用 |
| G11.12.a | P2 | 無視窗圖示 API |
| G12.3.a | P1 | QMenu 規則被解析但不消費 |
| G12.3.b | P0, READ | Rust 卡片 `QLabel#Badge` 加了 `max-height: 15px` |
| G12.3.c | P1, READ | 表格模式面板：Python 的 `get_hud_stylesheet(theme, vibrant)` 依 `vibrant` 選半透明 `panel` 或 `panel_sol |
| G12.3.d | P1 | 型別比對無繼承 |
| G12.3.e | P2 | `font-family` 清單以一個原始字串存、查找時才拆 |
| G12.3.f | P2, INFERENCE | `font-weight: 800` 映射到 `FontWeight::Black` |
| G12.3.g | P2 | `font-size` px 取整 |
| G12.3.h | P2, INFERENCE | 色彩：8 位數十六進位被當 `#RRGGBBAA` |
| G12.5.a | P0, READ；= G12.3.b | Badge `max-height: 15px` 只在 Rust |
| G12.5.b | P0, READ；= G9.4.b | 卡片根 layout spacing 2（Rust）vs 5（Python），無註解說明 |
| G12.5.c | P1 | 面板底色不依 Acrylic 是否成功而改變 |
| G12.5.d | P0, READ | 幾何持久化：Python 250 ms 單發於 move／resize 重啟＋mouse release 儲存；Rust 只有 resize 的 `ResizeDebouncer` |
| G12.5.e | P0, READ | 喚醒偵測（倒數 tick 間隔 >15 s 就刷新）在 Rust 不存在 |
| G12.5.f | P0, READ | `Window` 沒有 mouse-release／double-click／move／close 的 handler；雙擊在 Rust 會重新開始視窗移動，Python 是刷新 |
| G12.5.g | P0, READ；= G11.2.b | Alt+F4 / `CloseRequest` 被吞 |
| G12.5.h | P1 | 單發時序：Python 300 ms（啟動 click-through）、150 ms（hide 後 trim）、1000 ms（busy→idle 後 trim）、2500 ms |
| G12.5.i | P0, READ；= G8.8.a | 所有 widget tooltip 缺失（錯誤與過期資料以 tooltip 顯示） |
| G12.5.j | P0, READ；= G11.9.a | 熱鍵註冊失敗不被回報；鎖定防護失效 |
| G12.5.k | P1 | `--smoke-test` 不檢查設定持久化 |
| G12.5.l | P0, READ；= G11.4.a | 螢幕選擇／脫離螢幕還原規則 |
| G12.5.m | P1 | 托盤選單：Python 的托盤選單沒有鎖定／不透明度／間隔／重設／隱藏等項目；Rust 托盤選單是完整的 context menu |
| G12.5.n | P1 | 托盤通知：Rust 只有「ghost paused」；缺 hotkey 失敗、ghost 啟用、autostart 失敗 |
| G12.5.o | P1 | QMenu 外觀為寫死數值，非 QSS |
| G12.5.p | P1 | `QColor.darker(110)` 缺失：Rust 用原色 |
| G12.5.q | P1 | `UsageDial` 最小尺寸 0 vs 84 |
| G12.5.r | P1 | 版面切換：`StackedWidget` vs 重建 |
| G12.5.s | P0, READ；= G8.5.d | `Window::set_style_sheet` 為 app 全域 |
| G12.5.t | P0, READ；= G11.5.a | DComp 路徑 opacity 無效（待實測） |
| G12.5.u | P2 | 色彩／字型解析細節 |
| G12.5.v | P1 | 發佈 profile `panic = "abort"` vs Python excepthook |
| G12.5.w | P2 | `rust/README.md` 仍描述 egui/eframe/reqwest |

---

## 附錄 B：`QT_CPP_MAPPING.md` 需要更正的地方

（本次**未修改**該檔；以下是稽核發現。）

1. **工作區成員錯誤**：宣稱 `qtrs-render`（:52-57）、`qtrs-model`（:97-104）、`qtrs-network`／`qtrs-concurrent`（:159-160，「已在 workspace」）；實際 workspace 只有 `qtrs-core/gui/platform/widgets/derive`（`rust/qtrs/Cargo.toml`）。QThread／QFuture／QtConcurrent 類功能在 `qtrs-core/src/thread/`，**該檔沒有對應列**（`thread/{thread,pool,future,channel,executor,sync,task}.rs`）。
2. **不存在的 widget 檔案（約 22 個）**：`dialog.rs`、`standard_dialogs.rs`、`menu_bar.rs`、`tool_bar.rs`、`style.rs`（實為 `style/mod.rs`）、`arc_progress.rs`、`item_view.rs`、`item_widget.rs`、`tab_widget.rs`、`tab_bar.rs`、`tool_box.rs`、`splitter.rs`、`main_window.rs`、`dock_widget.rs`、`status_bar.rs`、`text_edit.rs`、`plain_text_edit.rs`、`text_browser.rs`、`combo_box.rs`、`spin_box.rs`、`slider.rs`、`dial.rs`（:112-128）。實際存在：`accessibility、action、application/、button、command、focus、frame、hit_test、input_common、key_sequence_edit、label、layout、layout_engine、layout_scheduler、menu、popup、progress_bar、scroll、size_policy、stacked、style/、widget、window`。
3. **`layout.rs`（:115）** 描述為 `VBoxLayout`/`HBoxLayout`；實際為 `BoxLayout`、`GridLayout`（grep `VBoxLayout|HBoxLayout` 於 `qtrs-widgets/src` 為空）；`layout_engine.rs`、`layout_scheduler.rs`、`size_policy.rs`、`stacked.rs` 未列。
4. **不存在的測試檔（:134-144）**：`test_item_views.rs`、`test_item_widgets.rs`、`test_widgets_containers.rs`、`test_widgets_text.rs`、`test_widgets_calendar_temporal.rs`、`test_widgets_input.rs`、`qtrs-model/tests/test_model.rs`。
5. **「declared slots 與反射 invoke 尚未支援」（:13）過時／誤導**：`MetaMethod` 帶選用 invoker，`MetaObject::invoke_method`、`QObjectExt::invoke_method` 可同步呼叫（`test_meta_method_introspection_and_invoke`）；手寫 `MetaObject` 可宣告 `MethodType::Slot`。**只有 derive 路徑**沒有 slot 與 invoker。「無 invoker」（:195）對 derive 成立；queued 呼叫以閉包形式的 `EventKind::MetaCall` 存在。
6. **「cross-thread QObject dispatch is unsupported」（:11,:203）** 對「直接派送進另一執行緒註冊的 QObject」正確，但**不完整**：跨執行緒 post 事件與 queued 信號（閉包）**確實支援**（`post_event_to_thread`，`test_cross_thread_wakeup`、`test_level3_cross_thread_queued_connection`）；請改稱「同步跨執行緒派送不支援」。同時沒有說明：外來執行緒的 MetaCall 會在**呼叫端執行緒**的 `NullObj` 替身上執行（G7.4.a）。
7. **「MetaCall 一律執行」（:207）是刻意偏離 Qt**（Qt 在 `~QObject` 清除 posted events），不是等價；且沒有「清除 posted events」功能。
8. **`thread.rs` 列（:12）**「更新 metadata／event queues」正確但不完整：還會停止計時器而不重新註冊、目標沒有 sender 時丟事件、主執行緒沒有 sender（`application/mod.rs`）、只更新一層 children 的 `thread_id`。
9. **「atomically via `with_object_mut` callbacks」（:205）錯誤**：使用數個獨立的鎖與借用（G2.1.f）；並漏掉「解除 owned child 的 parent 會 drop child」（G2.1.a）與「送給被借用 parent 的事件會被靜默丟棄」。
10. **`event/compressor.rs`（row 16）** 對應到 `qeventloop.cpp` 不正確：壓縮在 `QCoreApplicationPrivate::compressEvent`（`qcoreapplication.cpp:1717-1753`）與 `QApplicationPrivate::compressEvent`（`qapplication.cpp:790-840`）。`event_loop/loop.rs`（row 17）：posted 佇列、`notify_helper`、filter、`sendPostedEvents`、DeferredDelete 規則在 `qcoreapplication.cpp` + `QThreadData`，檔案混合了 `QEventLoop` 與 `QCoreApplication` 的職責。`timer.rs`（row 23）：排程數學對應 `qeventdispatcher_win.cpp:302-381`，0 ms／`singleShot` 邏輯對應 `qsingleshottimer.cpp` 與 `qtimer.cpp:365-396`；且未提到 `Timer::start` 是 `unsafe`（pinning 契約）、registry-only 計時器（`QObject::start_timer`）在 Windows 不被驅動（G5.4.a）、計時器遞送繞過 `event()`（G5.2.a）。
11. **`CoreApplication::new` 會 panic（P0 一節）** 不實：程式碼沒有第二實例檢查（G4.1.f）。
12. **平台**：`presenter.rs`、`surface/dcomp.rs`、`high_dpi.rs`、`single_instance.rs`、`resize_debug.rs` 不在對照表；`surface/win32.rs` 列為 `UpdateLayeredWindow`，實際 `LAYERED` 視窗**預設是 DirectComposition**（`window.rs` `get_or_create_presenter`），GDI layered 是退路；`paint/backing_store.rs` 沒有列；`painter.rs` 被描述為雙緩衝引擎，實為軟體 tiny-skia painter，沒有 render hints、clip transform、text transform（C10.4）；`hotkey.rs` 的「Win32 RegisterHotKey」只對 `Win32HotkeyManager` 成立，app 不用它，其餘 manager 為回 `Ok` 的 stub；`tray/win32.rs` 只是 `crate::tray_icon::TrayIcon` 的一行 re-export；`window.rs` 列（`QWidgetWindow`）沒有提到 `CloseRequest/Show/Hide/Expose/Move` 不被處理（G11.2）、`UpdateRequest` 僅在視窗已註冊時到達（G10.1.a）。
13. **`window.rs`（widgets）無 `close/show/hide/move/release/double-click` handler** 的限制沒有出現在文件中的任何地方。
14. **`rust/README.md`**（不是 mapping 文件）仍描述 egui/eframe/reqwest（`:3,:27,:95`）；程式碼用 qtrs 與 ureq。

---

## 附錄 C：證據紀錄與限制

### C.1 本次在 Windows 實際執行（`RAN`）— commit `701d36b`

以下 6 個重現探針寫成**拋棄式**整合測試（`qtrs-core/tests/zz_contract_probe.rs`），執行後已刪除，**未進 repo**；Required test 需由實作者在修復前重新寫成永久測試。

| 探針 | 結果 | 對應 gap |
|---|---|---|
| P1 兩個 `Signal<i32>` 各對一個 receiver `connect_to`；銷毀 receiver 1 後 `a.emit` | `id_a=1 id_b=1`；銷毀後 slot 被呼叫 **1** 次（預期 0） | G6.1.b、G6.4.a |
| P1b 對照組（只有一個 Signal） | 被呼叫 0 次（正確） | 排除測試本身問題 |
| P2 slot A 在發射中 disconnect slot B | B 仍被呼叫 **1** 次（預期 0） | G6.1.a |
| P3 單發 0 ms `Timer`，`process_events` 一次 | `fired=1 registry_len=1 next_timeout=Some(0ns)`（預期 `None`） | G5.3.a |
| P4 已註冊物件 `start_timer(20ms)`，pump 400 ms | `timer_event` **0** 次（預期 ≥ 5） | G5.4.a |
| P5 `Timer` `set_interval(5000)`、`start()`、`set_interval(20)`，pump 400 ms | `timeout` **0** 次（預期 ≥ 5） | G5.1.a |
| P6 `set_parent(owned_child, None)` | child 被 drop（預期存活） | G2.1.a |

其他 `RAN`：
- layout 差分 harness：`qt_layout_compare.py 7500 1` 與 `7500 20260502` 各 0 差異；`1500 20260502` 0 差異（C9.7）。
- 測試套件（上一階段，commit `701d36b` 時）：`rust/qtrs` workspace 全數通過；`rust` 應用 crate 68 項通過。
- popup 選單子選單實機測試（上一階段）：HUD 靠螢幕右緣，開啟子選單使視窗 339→534 px，父選單偏移 195 px，5 列父選單文字像素差異 0（C10.5；**手動，未進 repo**）。

### C.2 由本文件作者在稽核後重讀原始碼確認（非執行）
- `test_signal_sender_tracking_and_auto_disconnection` 在 drop 後沒有 assertion（`test_qobject_safety_and_qt6_features.rs:214-228`）。
- `Timer::set_interval` / `set_single_shot` 只寫欄位（`timer.rs`）。
- `ConnectionId` 由每個 Signal 自己的 `next_id` 編號、全域表以 id 為 key（`signal.rs`）。
- `set_parent` 把舊 parent 的 owned `Box` 搬進區域變數（`qobject.rs`）。
- 卡片 spacing 5 vs 2（`provider_card.py:27`、`provider_card.rs:159`）；`rust/src/ui/styles.rs` 有 `max-height: 15px`，`python/ui/styles.py` 沒有任何 `max-height`。
- `rust/src/hotkey.rs:235-240` 註冊失敗只 `warn!`；`CloseRequest` 於 `rust/src` 與 `qtrs-widgets/src` 無任何處理（grep 為空）。

### C.3 未驗證 / 限制
- 標 `READ` 的項目**沒有被執行**；其中數個稽核者自己標了 `[INFERENCE]`。實作前先寫失敗測試確認。
- 六份稽核由子代理唯讀完成；§3–§5 與 §8–§9 的 Qt 行為引用了 repo 內的 `qtbase/` 原始碼，其他章節多為 `[QT-DOC]`。行號會漂移。
- 本文件沒有對 PySide6 做新的行為實測，**除了**第 C9.7 的 layout harness；所有其他「Qt behavior」都是引用，不是本次量測。
- macOS／Linux 後端完全未驗證。
- 最需要先實測的項目：`G10.7.a`（非主螢幕 DPR，需異 DPI 雙螢幕）、`G4.3.a`（選單開著時計時器，P1）。`G11.5.a` 已在本機實測：選到 Layered，非 DComp（見 G11.5.c）。`G7.9.a` 已由讀碼降為 P2，仍待驗證。

---

## 附錄 D：P0 Root-Cause Matrix

用途：**一個 root cause 只有一個 owner、一個修復、一組回歸測試。** P0 gap 在附錄 A 與 §12 有重複登錄；修復必須以 RC 為單位，同一次變更內重新檢查該 RC 對應的**所有** gap，不得把同一處修改宣稱為「修了 N 個 gap」。

證據來源：本矩陣的 `qtrs` 位置、HUD 使用情況均為本文作者讀碼（`READ`）；標 `RAN` 者在 Windows 實際執行過。Qt 行為引用 `qtbase/` 原始碼（行號可能漂移）。**沒有 `RAN` 的 RC，實作前第一步必須是先寫出失敗的回歸測試。**

階段（見 D.3）：Phase 1 核心語意、Phase 2 widget 失效與事件、Phase 3 獨立分支、Phase 4 HUD 對齊。

### D.1 框架 root cause（qtrs）

#### RC-01 擁有權釋放：`set_parent(owned_child, None)` 銷毀 child
- **Contract gaps**：G2.1.a（P0, RAN）。相關但不同根因：G2.1.d（parent 被借用時 child-list 更新被略過）。
- **Qt behavior** `[QT-SRC qobject.cpp:2287-2345]`：`setParent_helper` 只把 child 從舊 parent 的 `children` 移除並送 `ChildRemoved`，**不刪除物件**；物件的存活由呼叫者決定。
- **qtrs root**：`qtrs-core/src/object/qobject.rs` `set_parent`：舊 parent 的 `Box` 被搬進區域變數，沒有新 parent 時於函式結尾 drop；回傳型別 `()`。
- **Evidence**：`RAN`（拋棄式探針已重現，已刪除）。
- **Required observable**：解除 parent 之後 child **仍存活**，且所有權回到呼叫者；不得靜默丟棄。API 形狀由 Phase 1 計畫決定（§2.1 已寫 `Option<Box<dyn QObject>>` 或等價）。
- **Required test**：`set_parent_none_on_owned_child_returns_ownership_and_child_survives`（修改前 FAIL：child 已 drop）；`set_parent_none_sends_child_removed_through_notify`。
- **Downstream**：任何把 child 從容器移出再重新掛接的 widget／action 操作。HUD 目前沒有呼叫（`READ`），因此屬靜默資料遺失型 P0，不是 HUD 可見型。
- **Can remove app workaround**：n/a。
- **Phase**：1。

#### RC-02 Connection 身分：`ConnectionId` 在全域表碰撞
- **Contract gaps**：G6.1.b（P0, RAN）；後果 G6.4.a（P1, RAN）、G6.1.c（stale 記錄，P2）。
- **Qt behavior** `[QT-SRC qobject.cpp:1046-1180]`：連線屬於 sender 的連線串列；`~QObject` 對**所有**以該物件為 receiver 的連線斷線（`senders` 鏈）。Qt 沒有全域的 id → 連線表。
- **qtrs root**：`qtrs-core/src/signal/signal.rs`：每個 `Signal` 自己的 `next_id` 從 1 編號，卻共用以 id 為 key 的 `GLOBAL_CONNECTIONS`。
- **Evidence**：`RAN`。兩個 `Signal<i32>` 各以 `connect_to` 接一個 receiver，`id_a=1 id_b=1`；銷毀 receiver 1 後 `a.emit` 仍呼叫 slot（對照組：單一 Signal 正確）。
- **Required observable**：receiver 銷毀後，**任何** Signal 都不得再呼叫它的 slot；`disconnect(id)` 只影響該 Signal 的該連線。
- **Required test**：`receiver_destroyed_disconnects_from_every_signal`（兩個 Signal，修改前 FAIL）；`disconnect_id_does_not_remove_other_signals_connection`；並**改寫** `test_signal_sender_tracking_and_auto_disconnection`（drop 後目前沒有任何 assertion，§0 規則 4）。
- **Downstream**：G6.4.a；G6.1.c 一併檢查。HUD 的 signal 連線都是閉包、沒有 receiver 物件（`READ`），因此是 framework P0 而非 HUD 可見型。
- **Can remove app workaround**：n/a。
- **Phase**：1。

#### RC-03 發射快照的有效性：發射中被 disconnect 的 slot 仍會執行
- **Contract gaps**：G6.1.a（P0, RAN）。
- **Qt behavior** `[QT-SRC qobject.cpp:4269 doActivate; :4330 每次迭代檢查 receiver]`：發射沿連線串列走，**每個連線在呼叫前重新檢查 `receiver`**；發射中被斷開的連線（receiver 已被清為 null）不會被呼叫。
- **qtrs root**：`signal.rs` emit：先複製 slot 快照，呼叫前不再確認連線仍有效。
- **Evidence**：`RAN`（slot A 在發射中 disconnect slot B → B 仍被呼叫 1 次）。
- **Required observable**：發射開始後被 `disconnect` 的連線，在輪到它時不得執行；發射期間**新增**的連線不執行本次發射（Qt 同）。
- **Required test**：`slot_disconnected_during_emit_is_not_called`（修改前 FAIL）；`slot_connected_during_emit_is_not_called_in_same_emit`。
- **Downstream**：RC-02（同一檔案，需同時確認連線有效性的判斷方式）。
- **Phase**：1（RC-02 之後，同一檔案）。

#### RC-04 事件投遞保證：目標執行緒尚無 loop 時事件被丟棄
- **Contract gaps**：G3.2.b、G6.2.c、G7.2.a（皆 P0, READ，**同一根因**）。相關 P1：G5.5.b、G6.2.d。**降級項**：G7.9.a（P2，見 D.4）。
- **Qt behavior** `[QT-SRC qcoreapplication.cpp:1658-1704, 1694]`：`postEvent` 把事件加入**接收者所屬執行緒**的 `postEventList`；該執行緒是否已有 event dispatcher 無關，事件先排隊，之後被處理。
- **qtrs root**：`qtrs-core/src/event_loop/loop.rs:621-632` `post_event_to_thread` 在 `THREAD_EVENT_HANDLES` 沒有該執行緒時回 `false`；呼叫端忽略回傳值：`signal.rs:522-524,629-631`、`widget.rs:296`、`timer.rs:402,610`（其中 `window.rs:153-163` 會檢查並退回同步渲染）。
- **Evidence**：`READ`。HUD 目前啟動順序在 worker 產生前已註冊 loop（`main.rs:319` → `application/mod.rs:120`），因此**不是 HUD 可見型**。
- **Required observable**：在執行緒的 loop 註冊之前 post 的事件，不遺失，於 loop 開始處理後送達，且保持 FIFO／優先序。
- **Required test**：`post_before_loop_exists_is_delivered_when_loop_starts`（Contract 已列；修改前 FAIL）；`queued_signal_emitted_before_target_loop_exists_is_delivered`；`single_shot_zero_before_loop_runs_after_earlier_posted_events`。
- **Downstream**：`Window::queue_render` 的「無 loop 就同步渲染」fallback 是否仍需要；G5.5.b、G6.2.d。
- **Can remove app workaround**：`window.rs:141-144` 的同步渲染 fallback — **未確定**，須在 RC-04 完成後檢查渲染是否仍能在無 loop 時（`--snapshot`、`--smoke-test` 路徑）運作。
- **Phase**：1（只動 `qtrs-core`，與 RC-01～03 彼此獨立）。

#### RC-05 Widget 失效協定：樣式／字型／尺寸策略變更不更新也不重排
- **Contract gaps**：G8.5.c、G8.3.b（P0, READ）。同類 P1/P2：`Widget` trait 預設 `set_size_policy`、`set_style_sheet`、`set_property` 為靜默 no-op（`widget.rs:74,205,211`；`Label`、`ScrollBar`、`ScrollArea` 未轉發 `set_size_policy`）。
- **Qt behavior** `[QT-SRC qwidget.cpp:9502-9510]`：`FontChange`／`StyleChange` 的處理做 `update(); updateGeometry(); layout->invalidate();`。`updateGeometry` `[QT-SRC qwidget.cpp:10571-10587]`：頂層視窗不做事，否則使 parent layout 失效，或對可見的 parent post `LayoutRequest`。`setStyleSheet` 經 `repolish` `[QT-SRC qwidget.cpp:2594-2632; qstylesheetstyle.cpp:2978-2998]` 觸發 `StyleChange`。
- **qtrs root**：`qtrs-widgets/src/widget.rs:366-375` `WidgetBase::set_style_sheet` 只寫 `dirty`，不 post `UpdateRequest`、不要求 layout；`label.rs` 未覆寫 `set_size_policy`。
- **Evidence**：`READ`。
- **Required observable**：不呼叫任何手動 `update_layout()`／`render_and_present()`，在樣式字級改變、size policy 改變後，經過一次事件 pump，layout 與繪製的結果與 Qt 一致。
- **Required test**：`style_sheet_font_size_change_relayouts_parent_after_one_pump`；`label_set_size_policy_changes_layout_result`（與 `qt_layout_compare.py` 對照）；`set_size_policy_on_every_widget_type_is_not_silently_dropped`。皆須修改前 FAIL。
- **Downstream**：HUD 的 6 處 `update_layout()`（`hud_window.rs:636,737`、`provider_card.rs:435,707`、`usage_table.rs:1498,1559`）、1 處手動 `LayoutScheduler`（`usage_table.rs:1594-1595`）、約 8 處 `render_and_present()`。
- **Can remove app workaround**：**是**，但只能在 RC-05 的測試通過**之後**逐一刪除，每刪一處重跑 HUD 快照與 layout harness；不得先刪。
- **Phase**：2。

#### RC-06 事件翻譯與傳遞：accept／ignore／冒泡，及 Close／Show／Hide／Move／DblClick
- **Contract gaps**：G8.4.a、G11.2.b（= G12.5.g）、G11.2.c、G12.5.f（P0, READ）。是 RC-17、RC-18 的前提。
- **Qt behavior** `[QT-SRC qapplication.cpp:2689-2762]`：滑鼠事件沿 parent 鏈送，直到某個 widget accept、碰到頂層視窗，或碰到 `WA_NoMousePropagation`。`Close` 由 `QWidgetWindow::closeEvent`（`qwidgetwindow.cpp:883`）轉為 widget 的 `closeEvent`，可 `ignore()` 取消。`close_helper` 的隱藏細節本次**未讀**，實作前須讀。
- **qtrs root**：`qtrs-widgets/src/hit_test.rs` 無 accept／冒泡；`qtrs-widgets/src/window.rs:990-1250` 的 `WindowSystemEvent` 處理沒有 `CloseRequest`、`Power`、雙擊、`Move` 的 arm；`EventKind` 已有 `Close`、`Show`、`Hide`、`Move`、`MouseButtonDblClick`（`event/mod.rs:68,85,236,242`）且 `Event` 有 `accepted`（`:754`），平台層也已產生 `CloseRequest`／`Power`（`qtrs-platform window.rs:525,1069`）。缺的是**翻譯**與**冒泡規則**。
- **Evidence**：`READ`。
- **Required observable**：子 widget 不 accept 的滑鼠事件到達 parent；視窗 Close 事件被 handler `ignore()` 後視窗保持可見，未被 ignore 時隱藏／關閉；Show／Hide 有 widget hook 且順序同 Qt。
- **Required test**：`unaccepted_press_reaches_parent_widget`；`accepted_press_stops_at_child`；`close_event_ignored_keeps_window_visible`；`close_request_without_handler_hides_window`；`show_hide_events_delivered_in_order`。
- **Downstream**：RC-11（tooltip）、RC-17（幾何持久化）、RC-18（喚醒偵測，`Power::Resume` 已存在）。
- **Can remove app workaround**：`hud_window.rs:352` 的 `set_mouse_press_handler` 拖曳／縮放模擬；`hide()` 內的 trim（`hud_window.rs:479-485`）— **只有在 HUD 以真正的 `mousePressEvent` 冒泡重寫後**。
- **Phase**：2。

#### RC-07 Layout item 對齊
- **Contract gaps**：G9.2.a（P0, READ）。
- **Qt behavior** `[QT-SRC qlayoutitem.cpp:597-600]`：對齊影響 `expandingDirections` 與最大尺寸；`addWidget(w, row, col, alignment)`。
- **qtrs root**：`qtrs-widgets/src/layout.rs`：`add_widget(widget,row,col)`／`add_widget_with_span` 無對齊參數；`item_expanding` 無對齊邏輯。
- **Evidence**：`READ`；`qt_layout_compare.py` 目前**不涵蓋對齊**（`RAN` 的 7500×2 組 0 差異不能推論此項）。
- **Required observable**：對齊的 item 在儲存格內依對齊放置，不撐滿；`expandingDirections` 與最大尺寸隨之改變。
- **Required test**：擴充 harness 加入 per-item 對齊後 0 差異；`grid_item_alignment_does_not_fill_cell`。
- **Can remove app workaround**：`usage_table.rs:1173-1209` 的 wrapper + stretch 模擬 — 只在 harness 涵蓋對齊後。
- **Phase**：3。

#### RC-08 每視窗 DPR／螢幕
- **Contract gaps**：G10.7.a、G11.3.a（P0, READ，未實測）；相關 P1：G11.8.a。
- **Qt behavior** `[QT-DOC]`：視窗的 `devicePixelRatio` 取自**所在螢幕**。須在實作前於 `qtbase/` 確認。
- **qtrs root**：`primary_screen().device_pixel_ratio()` 出現在 `qtrs-widgets/src/window.rs:261,354,468,512,645,668,914`、`menu.rs:551,597,729`、`qtrs-platform/src/window.rs:1633,1891`、`tray_icon.rs:281`；`PlatformWindow` 沒有每視窗 DPR 查詢；WM handler 用 `GetDpiForWindow`（`qtrs-platform window.rs:404`）。
- **Evidence**：`READ`；**只在異 DPI 多螢幕下可見**，本機未實測。
- **Required observable**：視窗在非主螢幕時，backing store DPR 與該螢幕一致，且在 `DpiChanged` 之後不退回主螢幕 DPR。
- **Required test**：擴充 `test_per_monitor_dpi_sync.rs`（已有 96↔168 的模擬），加入「`DpiChanged` 後呼叫 `do_render_and_present`，store DPR 仍為新值」。**無第二螢幕時只能用 fake platform，並須明說。**
- **Phase**：3（先實測）。

#### RC-09 Presenter 尊重 opacity
- **Contract gaps**：G11.5.a（= G12.5.t）；新增 G11.5.c（test gap）。
- **Qt behavior** `[QT-DOC]`：`setWindowOpacity` 對所有後端生效。
- **qtrs root**：`presenter.rs:361-363` DComp 以寫死 `1.0` 呈現；`set_opacity` 對 DComp 為 no-op（`:342-348`）；`window.rs:1490-1492` **對所有 `LAYERED` 視窗無閘門地先試 DComp**。
- **Evidence**：`READ` + `RAN`：本機 `FRAMELESS|LAYERED` 視窗選到 **Layered**；`test_dcomp_*` 5 項因 `CreateDXGIFactory1 failed for IDXGIFactory2` **全部 skip**（顯示為「通過」）。因此**本機未重現**，且 DComp 路徑在本機**完全沒被測試**。
- **Required observable**：不論選到哪個後端，`set_opacity(0.5)` 後合成 alpha 為 0.5。
- **Required test**：每個後端以純色 pixmap 在 0.5 opacity 呈現並讀回（DComp 不可用時測試必須明確標為 skipped，而非通過）。
- **待決策**：production 是否允許 DComp（程式碼目前允許）。若不允許，這是 presenter 選擇的閘門問題，不是 opacity 實作問題。
- **Phase**：3。

#### RC-10 樣式範圍與 popup 擁有關係
- **Contract gaps**：G8.5.d（= G12.5.s）。
- **Qt behavior** `[QT-SRC qwidget.cpp:2594-2632]`：`QWidget::setStyleSheet` 只作用於該 widget 及其子樹；`QMenu(parent)` 透過 parent 繼承樣式（Python `hud_window.py:678`、`tray_icon.py:55`）。
- **qtrs root**：`window.rs:547-549` `Window::set_style_sheet` 呼叫 `Application::set_style_sheet`；`Menu::new(title)`（`menu.rs:160`）無 parent；`resolve_style` 已沿 parent 鏈走（`widget.rs:391-415`）。
- **Evidence**：`READ`。HUD 自己也呼叫 `Application::set_style_sheet`（`hud_window.rs:232,579,715`），所以只改 `Window::set_style_sheet` 的範圍**不會改變 HUD 行為**；真正前提是 popup 能掛 parent。
- **Required test**：兩個頂層視窗，A 的樣式不影響 B；掛在 A 下的 `Menu` 吃 A 的樣式。
- **Depends on**：RC-05。
- **Phase**：3。

#### RC-11 Tooltip
- **Contract gaps**：G8.8.a（= G12.5.i）。
- **Qt behavior** `[QT-SRC qapplication.cpp:2731; qwidget.cpp:9381-9386]`：`toolTipWakeUp.start(delay, this)`（需要 QObject 計時器）→ 送 `ToolTip` 事件 → widget 的 `event()` 顯示 `QToolTip::showText`；無 tooltip 則 `ignore()`。
- **qtrs root**：整個缺；`EventKind::ToolTip` 存在但無人處理。
- **Evidence**：`READ`；前置 G5.4.a（`QObject::start_timer` 在 Windows 不觸發，`RAN`）。
- **Depends on**：RC-06、G5.4.a。
- **Phase**：3。

### D.2 HUD 應用層 root cause（`rust/src`，不由 qtrs 修）

| RC | 對應 gap | 位置 | 閘門（動手前必須先做） |
|---|---|---|---|
| RC-12 熱鍵註冊失敗不回報 | G11.9.a、G11.8.c、G12.5.j | `hotkey.rs:235-261`、`main.rs:496` | 無。`start` 必須回報 `RegisterHotKey` 失敗；測試：衝突的熱鍵使 `start` 回 `Err`，且 `click_through` 啟動時被關閉 |
| RC-13 螢幕選擇／還原 | G11.4.a、G12.5.l | `hud_window.rs:178-193,640-662,823-824` | 對照 Python 規則；使用已存在的 `clamp_window_rect_to_screens`（`qtrs-platform/src/screen.rs:532`） |
| RC-14 卡片根 spacing 2 vs 5 | G9.4.b、G12.5.b | `provider_card.rs:159` vs `provider_card.py:27` | **先做逐 widget rect 的 Python/Rust 幾何 diff**：若 2 是用來補 qtrs 的高度差異，則真正的 root cause 在 qtrs，不得直接改成 5 |
| RC-15 Badge `max-height: 15px` | G12.3.b、G12.5.a | `styles.rs:192,291` | 同 RC-14 的閘門 |
| RC-16 header 多餘的 `Expanding/Fixed` | G9.3.c | `hud_window.rs:272-275` | 同 RC-14 的閘門 |
| RC-17 幾何持久化 | G12.5.d | `main.rs:575-591`、`config.rs`（`ResizeDebouncer`） | 依賴 RC-06。**待決策**：Python 的 250 ms 單發重啟是否照搬（目前專案規則：不新增 timer／debounce） |
| RC-18 喚醒偵測 | G12.5.e | `main.rs` 的 `clock_timer` | 依賴 RC-06（`Power::Resume` 已存在）；先確認 Python 的「tick 間隔 >15 s」是否可由 `Power::Resume` 取代 |

### D.3 執行階段

| Phase | 內容 | 前提 |
|---|---|---|
| 0 | Contract 清理（本次已完成）；實測 G11.5.a（已測：本機為 Layered）、G10.7.a、RC-14/15/16 的幾何 diff | 無 |
| 1 | RC-01、RC-02、RC-03、RC-04；**每個 RC 一個提交**，各自附「修改前 FAIL、修改後 PASS」的測試 | RC-03 在 RC-02 之後（同一檔案）；RC-01、RC-04 與其他獨立 |
| 2 | RC-05、RC-06 | Phase 1 完成 |
| 3 | RC-07、RC-08、RC-09、RC-10、RC-11 | RC-08、RC-09 先實測；RC-10 依賴 RC-05；RC-11 依賴 RC-06 與 G5.4.a |
| 4 | RC-12 ～ RC-18；移除 RC-05/RC-06 已取代的 workaround | RC-14/15/16 先做幾何 diff；RC-17/18 依賴 RC-06 |

### D.4 未列入執行佇列的項目

| Gap | 處置 | 理由 |
|---|---|---|
| G7.9.a | 降為 **P2，待驗證**（非 `D`） | 讀碼推翻 P0 主張：`Application::new`（`main.rs:319`）在 `application/mod.rs:120` 註冊 loop，早於第一個 worker／熱鍵執行緒（`hud_window.rs:408`、`main.rs:466`）；單一實例 IPC 執行緒（`main.rs:269`）在註冊前啟動，但只寫 atomic。**不等於所有 interleaving 皆安全**；它是 RC-04 的一個假設性表現，RC-04 修復後自然消除 |
| G9.6.a | 降為 **P1**（非 `D`） | Python HUD 不使用 `QStackedLayout`（重建 layout）；Qt 的 `sizeHint`／`minimumSize` 取**所有頁面**的最大值 `[QT-SRC qstackedlayout.cpp:417-448]`，qtrs 只看當前頁（`stacked.rs:123-147`）。與 Qt 不同是事實，但目前 HUD 不依賴；**P0 階段不修**，待 qtrs 的 API 範圍擴大再處理 |
| G12.5.d 的 timer 部分 | 見 RC-17 | 不得為了 parity 而新增 timer／debounce，除非先證明它不是在補 qtrs 缺陷 |
