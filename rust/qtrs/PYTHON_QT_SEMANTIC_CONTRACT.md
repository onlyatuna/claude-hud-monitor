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
  - MUST：解除 parent **不得銷毀 child**；所有權要能回到呼叫者。qtrs 的形狀（RC-01）：對 owned child，`set_parent` 回 `Err(ReparentError::OwnedByParent)`；以 id 為參數的 `reparent_owned(child_id, new_parent) -> Result<Option<Box<dyn QObject>>, ReparentError>` 移動或釋放 `Box`（`None` 目標回傳 `Some(box)`）。
  - MUST：drop parent 時依 children 順序銷毀擁有的子樹。
  - MUST：drop child 時從仍存活的 parent 解除連結，即使 parent 正在 callback 中。
  - SHOULD：拒絕自我 parent、環、跨執行緒 parent。
- **Current implementation**（`qtrs-core/src/object/qobject.rs`）
  - `IMPLEMENTED`（READ + 既有測試）：邏輯／實體（`Box`）重新掛接 `set_parent`；parent drop 級聯；`ChildAdded/Removed`。
  - `IMPLEMENTED-UNTESTED`：`ObjectData::drop` 解除連結並通知 parent。（`remove_owned_child` 已於 RC-01 移除，由 `reparent_owned` 取代。）
- **Known gap**
  - **G2.1.a [P0, RAN；已修復：RC-01]** `set_parent(owned_child, None)` **銷毀 child**。舊 parent 的 `Box` 被搬進區域變數 `transferred`，沒有新 parent 接手就在函式結尾 drop；回傳型別 `()`，呼叫者拿不回所有權。
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
  - **G3.2.b [P0, READ；已修復：RC-04]** 目標執行緒沒有已註冊 loop 時 `post_event_to_thread` 回 `false`，呼叫端（`CoreApplication::post_event_with_priority`、`signal.rs` 的 queued 閉包、`widget.rs:296`）**忽略回傳值 → 事件靜默遺失**（Qt 會排隊）。HUD 的 `run_on_main_thread` 在 loop 註冊前被 worker 呼叫即遺失；需實測啟動競態。
  - **G3.2.c [P2, READ]** 巢狀 pump（handler 內呼叫 `process_events`）吃掉外層剩餘事件後，外層迴圈 `processed_count < max_index` 會誤送下一輪事件。
  - **G3.2.d [P2]** 無 null receiver 警告；`ObjectId(0)` 被刻意當成「無接收者」哨兵（`main.rs:46`、`timer.rs:612`）。哨兵語意必須保留並明文記載（見 C3.6）。
  - **G3.2.e [D]** 無 `sendPostedEvents(receiver, type)` 篩選式 flush、`removePostedEvents`、`hasPendingEvents`。理由須在 C3.6 一併處理（`removePostedEvents` 是 G3.6.a 的必要前置）。
  - **G3.2.f [P2, READ]** RC-04 之後，投遞給「永遠不會建立 loop 的執行緒」的事件會一直留在 pending 佇列直到行程結束（Qt 在執行緒結束時釋放 `QThreadData` 的 postEventList）。已套用與 live 佇列相同的壓縮，所以重複的 `UpdateRequest` 不會累積，但 `MetaCall` 等不可壓縮事件會。需要執行緒結束的清理掛鉤（`ThreadContext`），尚未實作。
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
  - **G5.1.f [P1, RAN；已修復：RC-11a]** 對已啟動（或 `stop` 後再 `start`）的計時器重啟，`timeout` **永遠不再觸發**。根因不在 `Timer`：`register_object_metadata` 重新註冊同一物件時，把被取代的舊 record 的 `liveness` 設為 false，但新舊 record 共用同一個 `Arc<AtomicBool>`，等於把活著的物件標為死亡（所有 `QPointer` 變 null，`with_object_mut` 拒絕遞送 `timer_event`）。修復：只有舊 record 與新物件不共用 liveness（id 被別的物件重用）時才標死。測試 `qtrs-core/tests/test_timer_restart.rs`（5 項，修復前 5/5 FAIL）。同一缺陷也影響任何重複 `register_qobject` 的物件。
- **Test**：既有如上。必要：`set_interval_while_active_restarts`（本次 RAN 重現失敗）；`set_single_shot_while_active_fires_once`；`start_inside_own_slot_restarts_single_shot`。
- **HUD usage**：Python `QTimer(self)` 25 ms／1000 ms（`refresh_controller.py:35-44`）、250 ms 單發重啟（`hud_window.py:73-76,626,631`）、倒數 1000 ms（`:440-442`）；**從不在啟動中改 interval**。Rust：`Timer::new` + `set_interval`（啟動前）+ `start`（`main.rs:540-591`）。**差異**：Python 的 250 ms 單發、移動／縮放時重啟（debounce 儲存幾何）；Rust 用 `std::thread` 的 `ResizeDebouncer`（`config.rs:396`，只吃 resize）加 3000 ms 輪詢抓移動（`main.rs:575-591`）——行為不同，見 G12.5.d。**（RC-17 之後：輪詢已移除，移動改由 window event 觸發同一個 `ResizeDebouncer`。）**


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
  - **G6.1.a [P0, RAN；已修復：RC-03]** **發射期間被 disconnect 的 slot 仍會執行**（快照在呼叫前複製、之後不再檢查）。重現：slot A 在發射中 disconnect slot B → B 仍被呼叫 1 次。
  - **G6.1.b [P0, RAN；已修復：RC-02]** **兩個 Signal 的 `ConnectionId` 在全域表 `GLOBAL_CONNECTIONS` 碰撞**。每個 Signal 以自己的計數器從 1 開始編號，卻共用以 id 為 key 的全域 `HashMap`。重現：兩個 `Signal<i32>` 各以 `connect_to` 接一個 receiver，`id_a=1 id_b=1`；銷毀 receiver 1 後 `a.emit` **仍呼叫 slot**（對照組：只有一個 Signal 時正確為 0 次）。後果：receiver 銷毀時的自動斷線**靜默失效**；`Signal::disconnect(id)` 會刪掉別的 Signal 的全域記錄。`ConnectionId::next()`（全域計數器）存在但沒有 Signal 使用。
  - **G6.1.c [P2, READ]** `disconnect_receiver`／`disconnect_all` 不清 `GLOBAL_CONNECTIONS`（洩漏、stale 記錄）。RC-02 之後 id 全域唯一，stale 記錄只剩**洩漏**，不再造成錯誤斷線；仍未修。
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
  - **G6.2.c [P0, READ；已修復：RC-04]** 目標執行緒無 loop 時 queued 閉包忽略 `post_event_to_thread` 回傳值 → **queued slot 靜默遺失**（見 G3.2.b）。
  - **G6.2.d [P2, READ]** BlockingQueued 無逾時；loop 存在但不跑時發射者永久卡住。RC-04 之後，目標執行緒尚無 loop 時事件改為排隊，發射者會**阻塞到目標 loop 啟動並處理**（Qt 同樣如此）；先前是事件被丟棄、`tx` 被 drop、`rx.recv()` 回 Err 而解除。若目標執行緒永遠不建 loop，發射者永久阻塞。目前只有測試使用 BlockingQueued（grep 確認）。
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
  - **G6.4.a [P1, RAN；已修復：隨 RC-02]** 受 G6.1.b 影響。
  - **G6.4.b [P1, READ]** 已排入的 `MetaCall` 不被清除（G3.6.a）。
  - **G6.4.c [P2]** 無 receiver 的閉包連線（HUD 的全部）永不自動移除。
  - **G6.4.d [test gap, 已修復：RC-02]** `test_signal_sender_tracking_and_auto_disconnection`（`test_qobject_safety_and_qt6_features.rs:214-228`）在 drop receiver 之後 `sig.emit(&2)` **沒有任何 assertion**，即使自動斷線壞了這個測試也會通過。
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
  - **G7.2.a [P0, READ；已修復：RC-04]** 目標執行緒無已註冊 loop 時回 false／靜默丟（同 G3.2.b）。
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
  - **G8.1.a [P1, READ]** **show／hide 不自動重排**（**已修復：RC-26**：`WidgetBase::set_visible` 改呼叫 `update_geometry`，與 Qt 一樣只在可見性真的改變時請求 parent 重排；`Menu` 為彈出視窗不在此列）；HUD 以手動 `update_layout()` 補（`provider_card.rs:347-348,406-420,435`、`hud_window.rs:254,590,608,636`）。
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
  - **G8.3.b [P0, READ；已修復：RC-05]** **`Label.set_size_policy` 被丟棄**。Python `title.setSizePolicy(Minimum, Preferred)`（`provider_card.py:39`）在 Rust 無對應呼叫（grep `set_size_policy` 於 `provider_card.rs` 為空）→ 卡片模式標題寬度行為可能不同。
  - **G8.3.c [P2, READ]** `WidgetBase::set_geometry` 不夾 min/max（只有 `item_set_geometry` 夾）。
  - **G8.3.d [P2]** 預設 size_hint 100×30 會讓忘了覆寫的自訂 widget 得到假值。
  - **G8.3.e [P1, READ；已修復：RC-29]** （修復前：） `UsageDial`：Python `setMinimumSize(84,84)`（`usage_table.py:138`）；Rust `minimum_size()` 為 0×0，並註解稱最小值「會限制 dial」（`usage_table.rs:160-163`）——最小值不會限制上限，`[INFERENCE]` 非刻意，視窗很窄時 dial 可縮到 84 以下。 RC-29：`UsageDial::minimum_size()` 改為 84×84（Python 同值），policy 維持 Expanding/Expanding、`size_hint` 維持 84×84；註解改正為「最小值只是下限，上限由最大值決定」。
- **Test**：既有 `test_button_layout_toggle_btn_size_hint_matches_qt`、`test_label_box_model.rs::*`。必要：`label_set_size_policy_is_honoured`；`set_minimum_size_clamps_geometry_and_layout`。
- **HUD usage**：Python `setSizePolicy`（`provider_card.py:39`、`usage_table.py:139`）、`setMinimumSize`（`hud_window.py:296,327,347`、`usage_table.py:138`）；`setFixedSize/Width/Height` 無使用。Rust `set_size_policy`（`hud_window.rs:272,284`、`usage_table.rs:62,404,589,740`）。

### C8.4 滑鼠事件遞送
- **Qt behavior** `[QT-SRC qapplication.cpp:2738-2763,2745-2751,2037-2123]`：(1) 未被 accept 的滑鼠事件沿 parent 鏈上傳，直到被 accept／到視窗／`WA_NoMousePropagation`；(2) 沒按鍵的 MouseMove 只送給有 `mouseTracking` 的 widget，沒有的就**吞掉**（`res = true`，不再上傳）；(3) 按下後 move／release 隱式 grab 到被按下的 widget，即使游標移出；(4) Enter／Leave 對每個進入／離開的祖先送出，以共同祖先計算（`dispatchEnterLeave`）。
- **qtrs required**：MUST 按住按鍵時 press／release／move 送給 press 目標；MUST 未 accept 的 press／release 上傳到祖先（HUD 的拖曳／縮放依賴此）；MUST Enter／Leave 送給祖先鏈扣除共同祖先；MUST 一般 MouseMove 只給 tracking widget（含吞掉規則）。
- **Current implementation**：`PARTIAL`。`EventTreeDispatcher::dispatch_event_internal`（`hit_test.rs`）只送給 hit-test 的**單一葉節點**並回傳其 `event()` 結果——**沒有 parent fallback**；MouseMove 一律送（無 tracking 概念）；Enter/Leave 只在連續葉目標之間；grab 只存在於 popup（`PopupManager::mouse_grabber`），且只用於 press／release；無隱式 press grab；`ScrollBar` 拖曳離開 bar 後就不再跟隨；modifiers 對 press／release 恆為 0；`Button` 按下後移出再移回不會 click。`Window` 在派送的 press 回 `false` 時才退回 `mouse_press_cb`——只對 press 模擬「冒泡到頂層」。
- **Known gap**
  - **G8.4.a [P0, READ；已修復：RC-06]** 無 parent 傳遞（只對 press 模擬）；HUD 的拖曳／縮放是 Python 依賴子 label／button 冒泡到視窗，Rust 靠 fallback 模擬。
  - **G8.4.b [P1]** 無 tracking 語意（Python HUD `setMouseTracking(True)`，`hud_window.py:129,140`；Rust 過度遞送，無害但不相等）。
  - **G8.4.c [P1]** 無隱式 grab。
  - **G8.4.d [P1]** Enter/Leave 非祖先鏈；既有 `test_hover_enter_leave_events_transition` 釘住「child→parent 送 Leave(child)+Enter(parent)」——**實作前先對照 `qapplication.cpp:2037-2123` 確認該序列是否為 Qt 行為，不是就改寫測試**。
  - **G8.4.e [P2]** 無 `WA_TransparentForMouseEvents`／`WA_NoMousePropagation`；視窗離開時 Leave 只送最後一個葉。
  - **G8.4.f [P1]** 右鍵 `context_menu_cb` 在 release 時觸發，與 widget 是否 accept 無關。
  - **G8.4.g [P1；已修復：RC-06]** `Window` 沒有 release／double-click／move handler（見 C11.2、G12.5.f）。
  - **G8.4.h [P1, READ]** RC-06 之後仍存在的滑鼠傳遞限制：(1) `MouseMove` 不沿 parent 傳遞——Qt 的傳遞迴圈對 move 依賴 buttons 狀態與 `hasMouseTracking`（`qapplication.cpp:2735-2740`），qtrs 兩者都沒有（G8.4.b、G8.4.c）；(2) 雙擊只有 Win32 平台層會產生（視窗類別 `CS_DBLCLKS`，`qwindowswindowclassdescription.cpp:67`）；X11／Wayland／Cocoa 的第二次點擊仍是一般 `MousePress`——Qt 是在通用層用 `mouseDoubleClickInterval`／`mouseDoubleClickDistance` 判斷（`qguiapplication.cpp:2401-2425`），qtrs 沒有這一層；(3) 沒有 `WA_NoMousePropagation`（G8.4.e）。
  - **G8.4.i [P2]** `Window::set_mouse_press_handler`／`set_mouse_move_handler`／`set_context_menu_handler` 與新的 `set_window_event_handler` 是兩套並存的視窗層 handler API；press 與 move 的語意（未被 widget 處理才呼叫）與 `set_window_event_handler` 一致，但沒有合併。
- **Test**：既有 `test_widget_hit_test_and_event_dispatch`、`test_hover_enter_leave_events_transition`、`test_builtin_button_click_and_state_transition`。必要：`unaccepted_press_bubbles_to_parent`；`accepted_press_stops_bubbling`；`press_grab_routes_move_and_release_outside_widget`；`enter_leave_ancestor_chain_minus_common_ancestor`；`mousemove_without_tracking_is_swallowed`。
- **HUD usage**：Python `mousePressEvent/MoveEvent/ReleaseEvent/resizeEvent/moveEvent`（`hud_window.py:560-606`）；`enterEvent/leaveEvent/eventFilter` 無。Rust：`set_mouse_move_handler/set_mouse_press_handler/set_resize_handler`（`hud_window.rs:325,352,368`）。

### C8.5 樣式表、polish、動態屬性
- **Qt behavior** `[QT-DOC]`：`setStyleSheet` 重新 polish widget 與後代、觸發 `StyleChange` 與 `updateGeometry`；選擇器依繼承比對型別（`QFrame` 命中 `QLabel`）、`#id`、`[prop="v"]`、pseudo-state、sub-control；祖先樣式表串接、widget 自己的覆蓋；`setProperty` + `unpolish/polish` 重新評估 `[prop]` 規則。
- **qtrs required**：MUST 符合 HUD 用到的 QSS 規則（型別、`#id`、`[prop]`、`:hover`、`::chunk`、串接順序 app < 祖先 < 自己）；屬性／樣式變更 MUST 重新解析；影響尺寸時 MUST 重排。
- **Current implementation**：HUD 子集 `IMPLEMENTED`，整體 `PARTIAL`。串接 app→祖先→自己、依 specificity（`widget.rs` `resolve_style`、`style/stylesheet.rs`）；樣式於每次 `size_hint`／paint **lazily 解析**（無快取，故無 stale polish）。`selector_matches` 只比對 exact `type_name`、`*`、`QWidget`。只有 `QLabel`、`QPushButton`、`QFrame`、`QProgressBar` 會解析樣式。`attributes`（供 `[state=…]`）只有 Label 提供。
- **Known gap**
  - **G8.5.a [P1, READ]** 無繼承比對（`QFrame{}` 命不中 `QLabel`）；無 descendant/child 組合子（HUD 不用）。
  - **G8.5.b [P1, READ]** `attributes` 只有 Label；同一條規則對 Button／Frame／ProgressBar 無效。
  - **G8.5.c [P0, READ；已修復：RC-05]** **樣式變更不重排**：`WidgetBase::set_style_sheet` 只標 dirty；`Label::set_text` 會 `request_layout`，但 `set_font`/`set_alignment`/style/property 不會，`Button::set_text/set_font` 也不會——需要手動 `update_layout`。
  - **G8.5.d [P0, READ；已修復：RC-10]** `Window::set_style_sheet` 是**整個 Application 的**（呼叫 `Application::set_style_sheet`），Python `HUDWindow.setStyleSheet` 只作用於該子樹（`hud_window.py:246,250`）；Rust HUD 兩者都呼叫（`hud_window.rs:231-232`）→ 影響其他頂層視窗與 popup。
  - **G8.5.e [P1, READ]** `:disabled`/`:focus` 不支援。
  - **G8.5.f [P1, READ]** **QMenu 規則被解析但從不被消費**：`type_name: "QMenu"` 在原始碼中不存在；選單外觀來自寫死的 `MenuStyle`（`rust/src/ui/tray_icon.rs`），手動複製了 Python QSS 的數值；`QMenu::item:selected/:disabled` 不驅動 hover／停用色。
  - **G8.5.g [P2, READ]** `margin-*` 長手寫被解析後在 `apply_declaration` 丟棄；`margin` 只有選單消費。
  - **G8.5.h [P2, READ]** 父 widget 的 `font` 繼承未實作（`[INFERENCE]`，未對照 `qstylesheetstyle.cpp`）。
  - **G8.5.i [P1, READ]** RC-05 之後仍存在的失效傳播限制：(1) `updateGeometry` 只要求**直接 parent** 的 layout 重排（`LayoutScheduler::invalidate(parent)`）；parent 自己的 size hint 因此改變時，不會再往祖先傳（Qt 的 `QLayout::invalidate` 會一路到最上層 layout 並對它 post `LayoutRequest`）；(2) widget 自己的 layout 在 `style_changed` 時只標 dirty，要靠 parent 的 `BoxLayout::activate` 順手重排（`child_layout.is_dirty()`）；沒有 parent 的 root，或 parent 沒有 layout 時，不會被排程（**G8.5.i(2) 的 root 部分 = RC-05 residual prerequisite，由 RC-10 一併消化**：沒有 parent 的 root 在 render 前由 `LayoutScheduler::activate_if_dirty` 排程，沒有新增 layout 子系統；「parent 沒有 layout」的情形未驗證，仍開放）。(3) `Application::set_style_sheet` 不通知既有 widget（RC-10 已修復）；`Application::set_font` 見 G8.5.j。
  - **G8.5.j [P2, READ；實測]** `Application::set_font` 只寫 `GLOBAL_FONT`：沒有任何 widget 讀 `Application::font()`，也沒有 `FontChange`／`ApplicationFontChange`；各 widget 在建構時寫死字型（Label／Button 13、ProgressBar 12、Menu 12），也沒有「明確設定 vs. 沿用」的 resolve mask，所以新建的 widget 也不會用 app font。實測：既有 Label 的 size hint 在 `Application::set_font(40pt)` 後仍是 35→35；新建 Label 的字型為 13.0 而非 40.0。Qt：`QApplication::setFont` 對所有非 window 的 widget 送 `ApplicationFontChange`，`resolveFont()` 重新解析（`qapplication.cpp:1352-1386`、`qwidget.cpp:4763-4829, 9265`）。**HUD 不受影響**：Python 從未呼叫 `QApplication.setFont`（`grep` 全 `python/`），只用 widget 區域的 `setFont` 與 QSS。依使用者決定，**不在 RC-10 處理**。
  - **G8.5.k [P2, READ]** HUD 的選單沒有 parent：Rust `Menu::new("")`（`tray_icon.rs:308`）；Python `QMenu(self.hud_window)`（`tray_icon.py:55`）、`QMenu(self)`（`hud_window.py:678`）。Qt 的 QMenu 沿 parent 鏈取得 sheet（`qstylesheetstyle.cpp:1654`）。qtrs 的 parent 鏈解析已可用（RC-10 的 `a_menu_takes_the_sheet_of_the_window_it_hangs_under_and_not_another`），但 `Menu` 不消費解析結果（G8.5.f），所以即使掛 parent 也沒有可見差異；兩者要一起處理。
  - **G8.5.l [P2, READ]** `StyleChange` 通知以 `try_borrow` 走訪：`style_changed_below` 跳過目前被 mutably borrow 的 widget（正在被驅動的那個），`Application::set_style_sheet` 在某個 root 被借用時，整棵樹不會被通知。與 `resolve_style` 遇到被借用祖先就停止走訪同類（`widget.rs:463`）。未實測。
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
- **Current implementation**：`IMPLEMENTED`（RC-11c；Win32 真實視窗驗證）。`Widget::tool_tip/set_tool_tip/tool_tip_duration/always_show_tool_tips`、`EventKind::ToolTip{x,y,global_x,global_y}`（`QHelpEvent` 形狀，無 `text`）、`qtrs-widgets/src/tooltip.rs`（`ToolTip::show_text/hide_text/is_visible/text/geometry`、純函式 `place_tip`／`expire_time_ms`）、`EventTreeDispatcher::dispatch_mouse_move`。
- **Known gap**：**G8.8.a [P0, READ；已修復：RC-11c + HUD 接線]** Python 在 `provider_card.py:125`、`usage_table.py:318,328,339,373-375`、`hud_window.py:155,160` 設定 tooltip；Rust HUD 已在 `provider_card.rs`（卡片 `container`）、`usage_table.rs`（5 個 value cell + header + dial + m2_val 的 run-out 備註）、`hud_window.rs`（`ghost_label`、`layout_toggle_btn`）設定相同文字（測試 `ui/tool_tip_tests.rs` 7 項，修前全部失敗；真實 `ClaudeHUD.exe` Windows smoke：停留約 1.3 s 後出現 `WS_EX_NOACTIVATE|TOPMOST` 的 tip 視窗、未搶前景、移開與點擊後消失、子 widget 退回父層 tooltip；tip 外觀未取得像素，仍須 MANUAL WINDOWS VERIFICATION）；**G8.8.b [P2, READ]** 無 `QToolTip::showText` 的 `rect` 參數（`setTipRect`，游標離開該矩形即隱藏）與 `QToolTip::font/palette/setFont/setPalette`；**G8.8.c [P2, READ]** 游標大小固定為 `QPlatformCursor` 預設的 16×16 邏輯像素（偏移 `(2,16)`）。Qt 的 `placeTip` 取 `cursor->size()`（Windows 為 `QWindowsCursor::size()`，由登錄檔 `CursorBaseSize` 與 DPI 算出，`qwindowscursor.cpp:675-692`），再經 `QHighDpi::fromNativePixels` 除以 DPR（`qtooltip.cpp:325-328`）；qtrs 兩步都沒做。因此 tip 相對游標的偏移只有在「預設 100% DPI 且登錄值剛好得 16」時才可能與 Qt 一致 `[INFERENCE]`；DPR≠1 或自訂游標大小時偏移必然不同，須手動驗證。**位置幾何與 Qt 的一致性只由 `place_tip` 純函式測試（手算自 `qtooltip.cpp:349-361`）支持，不涵蓋游標大小；真實視窗測試只證明接線，不是 Qt 幾何 parity 證明**；**G8.8.d [P1, READ]** tip 內容是單行 `Label`：沒有自動換行（Qt 在比螢幕寬時換行，`qtooltip.cpp:152-157`）、沒有 rich text（`Qt::mightBeRichText`）、字串中的換行未驗證。Python 的 tooltip 是否含換行／HTML 待 Phase 4 核對；**G8.8.e [P2, READ]** `set_tool_tip` 不送 `ToolTipChange`（無 `EventKind`、無 `changeEvent`）；`StatusTip`、`WhatsThis` 未實作；`Action::tool_tip` 未接到 menu／toolbar；**G8.8.f [P2, READ]** tip 視窗只重用一個實例（Qt 每次新建並 `deleteLater`）；無淡入淡出；`WindowActivate/Deactivate/ActivationChange` 不存在（G11.1.d），tip 因 `FocusIn/FocusOut` 而隱藏，不因啟用狀態改變；**G8.8.g [P2, `[INFERENCE]`]** 混合 DPI：tip 視窗以主螢幕 DPR 建立，再移到游標所在螢幕；未在異質 DPI 實機驗證。
- **Test**：`qtrs-widgets/tests/test_tooltip.rs`（26 項：純函式 `place_tip`／`expire_time_ms`；喚醒延遲、取消、fall-asleep、冒泡與座標、`showText` 更換／位置／存活；真實視窗：按鈕狀態、非活動視窗與 `WA_AlwaysShowToolTips`、不搶前景）。tip 外觀（顏色、字型、圓角）不自動斷言，須手動驗證。
- **HUD usage**：見上；HUD 尚未設定 tooltip（Phase 4）。

---

## 9. Layout semantics

### C9.1 Box layout（QBoxLayout）
- **Qt behavior** `[QT-SRC qboxlayout.cpp:242-340]`：`QBoxLayoutPrivate::setupGeom` + `qGeomCalc`；spacing 只在非空 item 之間；`addStretch` = `QSpacerItem(0,0,Expanding,Minimum)`（空 item，不佔 spacing）；item stretch 來自 `addWidget(w, stretch)`，否則 `QSizePolicy::horizontalStretch`。
- **qtrs required**：MUST 對扁平 H/V layout，在 §9.7 列出的 policy／stretch／min／max 組合下**逐像素**重現；MUST 維持 spacer 語意。
- **Current implementation**：`IMPLEMENTED` + `DIFF`：`BoxLayout::setup_geom`（`layout.rs`）移植 `setupGeom`；`activate` = `QBoxLayout::setGeometry`；`q_geom_calc`／`smart_min_size`／`smart_max_size`／`item_*`（`layout_engine.rs`）移植 `QWidgetItem`。
- **Known gap**
  - **G9.1.a [P1, READ；已修復：RC-30]** （修復前：） `add_stretch(0)` 被強制成 1（`layout.rs` `stretch.max(1)`）；Qt 的 `addStretch(0)` stretch 為 0。 RC-30：兩處 `stretch.max(1)`（`Layout::add_stretch` 預設實作與 `BoxLayout::add_stretch`）改為原值傳入；`test_box_layout_add_stretch_zero.rs` 4 項以 PySide6 參考值比對。
  - **G9.1.b [P1；已修復：RC-31]** （修復前：） 無 `add_spacing`／`add_spacer_item`／`insert_stretch`／`set_stretch_factor`（grep 為空）。 RC-31：`BoxLayout` 新增 `add_spacing`／`insert_spacing`／`add_spacer_item`／`insert_spacer_item`／`insert_stretch`／`set_stretch_factor`／`set_stretch`／`stretch`，以及公開的 `SpacerItem`（`QSpacerItem` 的 min／max／hint／expanding）；`LayoutItem::spacer` 由 `bool` 改為 `Option<SpacerItem>`。`set_stretch_factor(QLayout*)` 不適用（qtrs 子 layout 掛在 widget 上，以 widget 版本處理）。
  - **G9.1.c [P1；= G9.2.a；已修復：RC-07]** 無 item 對齊（見 C9.3）。
  - **G9.1.d [P1]** 無 `heightForWidth`（grep `height_for_width|has_height` 為空）——換行 label 無法如 Qt 排版。
  - **G9.1.e [P1]** 無 `retainSizeWhenHidden`、無 RTL（`Direction` 只有 TopToBottom／LeftToRight）、無 `SizeConstraint`。
- **Test**：既有 `test_vbox_and_hbox_layout_calculation`、`test_box_layout_add_stretch`、`test_layout_stretch_minimum.rs`（6 項，用 `spacer=0`，而 layout 實際用 `-1`）、`qt_layout_compare.py`（手動，見 C9.7）。`add_stretch_zero_matches_qt` 已由 RC-30 的 `test_box_layout_add_stretch_zero.rs` 補上。 G9.1.b API 由 RC-31 的 `test_box_layout_spacing.rs`（8 項）以 PySide6 參考值比對。
- **HUD usage**：Python `QVBoxLayout/QHBoxLayout`（`hud_window.py:135,143,168,280,336`；`provider_card.py:25-90`；`usage_table.py:114,273-276,411`）、`addStretch`（`hud_window.py:286`、`provider_card.py:42,63,90`、`usage_table.py:123,278,285`）。Rust：對應檔案的 `BoxLayout::` 與 `add_stretch(1)`。

### C9.2 Grid layout（QGridLayout）
- **Qt behavior**：`QGridLayoutPrivate::setupLayoutData/distribute` + `distributeMultiBox`、row／col stretch、minimum width／height、span、空 row 周圍的 spacing。
- **qtrs required**：MUST 重現 HUD 用到的 row／col stretch、`setRowMinimumHeight`、span、對齊。
- **Current implementation**：`IMPLEMENTED` + `DIFF`（Probe widget、2–3 欄、span ≤ 2、row 0 的欄 stretch、row stretch）。`GridLayout::setup_layout_data`、`activate`、`find_size`、`setup_spacings`、`distribute_multi_box`、`init_empty_multi_box`。`set_row_minimum_height`／`set_column_minimum_width` HUD 有用但**不在 harness 內**（只靠 `usage_table.rs` 的固定 PySide6 數值測試 `test_grid_matches_qt_geometry`）。
- **Known gap**
  - **G9.2.a [P0, READ；已修復：RC-07]** **無 per-item 對齊**。Python 傳 `AlignVCenter|AlignLeft`／`AlignHCenter` 給 `addWidget`（`usage_table.py:392,417,430`）；Rust `add_widget(widget,row,col)`／`add_widget_with_span` 沒有對齊參數（`layout.rs`），Rust 表格以 wrapper + stretch 模擬垂直置中（`usage_table.rs:1173-1209`）。對齊也會改變 `expandingDirections` 與 max size（`[QT-SRC qlayoutitem.cpp:597-600]`），`item_expanding` 沒有此邏輯。
  - **G9.2.b [P1, READ]** `Layout::add_widget_with_stretch` 對 grid **靜默忽略 stretch** 並新增一列；`Layout::set_spacing` 兩軸都設但 `spacing()` 只回水平。
  - **G9.2.c [P1]** 無 `setRowStretch`／`setColumnStretch`／`setColumnMinimumWidth` 讀回；無 `addLayout` 進格；GridLayout 沒有 `remove_widget`。
  - **G9.2.d [P1, READ]** RC-07 之後仍存在的對齊限制：(1) 沒有 `heightForWidth`（G9.1.d）時，垂直對齊的 widget 以 size hint 高度為準；Qt 的 `QWidgetItem::setGeometry` 在 `hasHeightForWidth()` 時改用 `heightForWidth(寬度)`（`qlayoutitem.cpp:443-446`）；(2) 沒有 layout 自身的對齊（`QLayout::setAlignment(Qt::Alignment)` 影響 `maximumSize` 與 `setGeometry`，`qgridlayout.cpp:1216,1324`、`qboxlayout.cpp:620,743`）；(3) 沒有 RTL，所以 `AlignAbsolute` 與 `QStyle::visualAlignment` 的翻轉不存在；(4) `QLayout::setAlignment(QLayout*, …)`（巢狀 layout 的對齊）隨 `addLayout`（C9.3）一起缺；(5) `ItemAlignment` 與 `Label` 的文字 `Alignment`、`qtrs_gui::TextAlignment` 是三個互不相通的型別，Qt 只有一個 `Qt::Alignment`。
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
  - **G9.5.a [P1, READ；已修復：RC-28]** （修復前：）無向上傳遞：葉節點的 hint 變更不會爬到祖先 layout（Fixed/Maximum wrapper 底下的文字變更不會調整 wrapper）；HUD 以明確 `update_layout()` 補（現行位置 `hud_window.rs:741,822`、`provider_card.rs:472`；`provider_card.rs:807` 為測試）。RC-28：`LayoutScheduler::activate_pending` 在 layout 真的重跑後呼叫擁有者的 `update_geometry()`（= `qlayout.cpp:1131` 的 `mw->updateGeometry()`），每次 flush 往上一層，到視窗 root（無 parent）為止。HUD 的手動 `update_layout()` 未移除（另案）。
  - **G9.5.b [P1, READ]** `Button::set_text/set_font`、`Label::set_font/set_alignment`、`set_style_sheet`、`set_property`、`set_visible` 不請求 layout（→ G8.1.a、G8.5.c）。
  - **G9.5.c [P2]** setter 立即重排與 Qt 壓縮不同（只有在 mutation 中讀取 geometry 的程式碼觀察得到）。
  - **G9.5.d [P1]** 頂層最小尺寸不從 layout 導出（Python HUD 明確設定 `hud_window.py:296,327,347`，所以不受影響）。
- **Test**：既有 `test_reentrant_layout_request_during_callback`、`test_multilevel_layout_traversal_with_cell`、`test_command_queue_deduplication`、`test_resize_event_observable_ordering_before_layout_activation`、`test_single_resize_pipeline`、`test_resize_deferred_render`。必要：`text_change_below_fixed_wrapper_resizes_wrapper`；`layout_min_size_sets_toplevel_min_when_not_explicit`；`button_set_text_requests_layout`。
- **HUD usage**：Python 全靠 Qt 自動重排；Rust 手動 `update_layout`（見上）。

### C9.6 StackedLayout / StackedWidget
- **Qt behavior** `[QT-SRC qstackedlayout.cpp:417-448]`：`sizeHint` = **所有**頁面 hint 的最大值（`Ignored` policy 算 0），`minimumSize` = 所有頁面 `qSmartMinSize` 的最大值；非當前頁被隱藏；`currentChanged` 信號。
- **qtrs required**：MUST 與 Qt 相同。
- **Current implementation**：`IMPLEMENTED-UNTESTED`（幾何）。`StackedLayout::size_hint`／`minimum_size`／`expanding_directions` **只用當前頁**；`activate` 把每頁都設成同一矩形並切換可見性；`set_current_index` 只在索引改變且在範圍內時發 `current_changed`。
- **Known gap**：**G9.6.a [P1, READ；決議：不在 P0 階段修，不標 D]** `[QT-SRC qstackedlayout.cpp:417-448]`：Qt 的 `sizeHint` 取**所有頁面**的最大值（`Ignored` 策略的軸取 0），`minimumSize` 取所有頁 `qSmartMinSize` 的最大值；qtrs（`stacked.rs:123-147`）只看當前頁。目前 HUD 不依賴。 頁面大小不同時，視窗 hint／最小值在切換卡片↔表格時會跳動，與 Qt 不同。**注意：Python HUD 不用 `QStackedWidget`**（grep 為空）；它重建 `inner_layout`（`hud_window.py:272-347`）。所以這是 Rust HUD 的設計偏離（`hud_window.rs:301-309,585-606`），不是移植錯誤；須決定「改成與 Python 相同的重建」或「讓 StackedLayout 符合 Qt 並證明結果等價」。**G9.6.b [P2]** `set_spacing` 為 no-op。 **已修復：RC-24**。`[QT-SRC qstackedlayout.cpp:417-436,438-448]` `sizeHint`＝所有頁 `widget->sizeHint()` 的逐分量最大值（某軸 policy 為 `Ignored` 則該軸以 0 計），`minimumSize`＝所有頁 `qSmartMinSize` 的最大值，與目前頁無關。PySide6 實測（`QStackedLayout`，頁面覆寫 `sizeHint`／`minimumSizeHint`）：頁 100×50＋60×80 → hint (100,80)，目前頁 0 或 1 皆同；明確 min (30,10)＋(20,40) → min (30,40)；Fixed 頁（hint 100×50、minHint 70×30）即使非目前頁 → min (100,50)；Ignored 水平的頁 → hint (60,50)；兩軸 Ignored → 該頁對 hint 與 min 皆為 0；空 → (0,0)。修復：`stacked.rs` 的 `size_hint`／`minimum_size` 改為遍歷所有頁（沿用 qtrs 既有「layout 的 size_hint 含 margins」慣例，與 Qt 的 `QLayout::totalSizeHint` 一致）。測試 `tests/test_stacked_layout_hint.rs` 5 項，其中 3 項修復前 FAIL（hint (100,50) vs (100,80)；min (30,10) vs (30,40)；Ignored (100,50) vs (60,50)）。**HUD 影響**：HUD 的 `StackedWidget` 同時裝著卡片頁與表格頁，stack 的 hint 現在含兩者；HUD 測試（含 RC-16 的 header／卡片幾何）全數仍過，並新增 `test_default_vertical_window_layout_matches_pyside6_with_the_table_page_in_the_stack`（預設 280×410：header 33、卡片 109、間距 122，取自 PySide6）。**未修**：`expanding_directions` 仍只看目前頁（Qt `QStackedLayout` 沒有覆寫，用 `QLayout` 的預設）；`activate` 仍給每一頁 geometry（Qt `StackOne` 只給目前頁，`qstackedlayout.cpp:453-467`）；Qt `sizeHint` 不含 margins 而 qtrs 含（margins 預設 0，HUD 為 0）。
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
  - **G10.7.a [P0, READ；已修復：RC-08 implementation complete / real heterogeneous-DPI verification pending]** `DpiChanged` 處理把 store 調成 `dpi_x/96`，然後呼叫 `do_render_and_present`，後者又以 `platform().primary_screen().device_pixel_ratio()` 重新 resize——**在與主螢幕 DPI 不同的螢幕上，store 會退回主螢幕的 DPR**。同樣的「主螢幕 DPR」假設還出現在 `Window::new`、`set_geometry`、`set_geometry_silent`、`present_custom`、`present_custom_at`、`NativeWindow::present_region`、`menu.rs`、`tray_icon.rs:281`；而 WM handler 用的是每視窗的 `GetDpiForWindow`。
  - **G10.7.b [P1, READ]** `application_device_pixel_ratio`（各螢幕最大值）只在 `Application::new` 設一次，DPI 變更或螢幕熱插拔後 stale。
  - **G10.7.c [P1, READ]** `HighDpiScaleFactorRoundingPolicy` 存了但從不讀（grep `rounding_policy` 只有存取器）；DPR 恰為 `dpi/96`，等同 Python 設的 `PassThrough`（`main.py:46`），其他 policy 被忽略。
  - **G10.7.d [P1, READ]** RC-08 之後視窗**內**的換算都用視窗自己的 DPR，但**視窗之間沒有共同的邏輯座標系**：`Window::new` 以主螢幕 DPR 決定原生位置（視窗尚未存在，無法先問它的螢幕），之後 `set_geometry` 以視窗 DPR 換位置。Qt 以 `QHighDpiScaling` 的螢幕原點映射處理（`qhighdpiscaling.cpp`）。僅在異 DPI 多螢幕下可見。
  - **G10.7.e [P1, READ]** 彈出選單的螢幕夾限仍取 `primary_screen().available_geometry()`（`menu.rs:551`），選單不會出現在非主螢幕；同時 `tray_icon.rs:281` 以主螢幕 DPR 換算游標位置（G11.8.a 未修）。
  - **G10.7.f [P2, READ]** `hit_test.rs:321` 的 `DpiChanged` 分支以固定的 `old_dpr = 1.0` 呼叫 `propagate_dpi_change_recursive`；`Window` 的兩條路徑已改用視窗快取的 DPR。
  - **G10.7.g [P1, READ]** 視窗 DPR 只在建立時與 `DpiChanged`（`WM_DPICHANGED`）更新；Qt 另在 `handleWindowScreenChanged`／`QPlatformWindow::handleScreenChanged` 重算（`qguiapplication.cpp:3431,3504`、`qplatformwindow.cpp:824`）並送 `QEvent::DevicePixelRatioChange`。螢幕熱插拔與跨螢幕移動而不改 DPI 的情形未涵蓋。
  - **G10.7.h [P2, READ]** `PlatformWindow::device_pixel_ratio` 只有 Windows（`GetDpiForWindow`）是每視窗；`GenericWindow`、Cocoa、Wayland、X11 回傳主螢幕 DPR（等於修改前的行為）。後三者在本機**無法編譯**，未驗證。
  - **G10.7.i [test gap]** 沒有異 DPI 實機測試：RC-08 的測試以 fake platform（主螢幕 1.0、視窗所在螢幕 2.0）驗證，Win32 的 `GetDpiForWindow` 路徑只在單一 DPI 下被其他測試走過。
- **Test**：既有 `test_per_monitor_dpi_sync.rs::test_per_monitor_v2_dpi_drag_propagation` 只斷言 observer callback 與 DPR 值，**從不檢查 backing store**。必要：`DpiChanged{168,168}` 並繪製後，`backing_store().device_pixel_ratio() == 1.75` 且實體大小為 `round(logical*1.75)`（在主螢幕 100% 時 READ 預測會失敗）。
- **HUD usage**：Python PassThrough（`main.py:46`），無 `devicePixelRatio()` 呼叫；Rust PerMonitorV2 manifest（`rust/build.rs:22-23`）+ `set_dpi_awareness`。

---

## 11. Window / platform semantics

> 只有 Windows 被驗證（見 §1.1）。`READ` 為預設；`RAN` 者另標。

### C11.1 視窗旗標與建立
- **Qt behavior** `[QT-DOC]`：`Frameless | Tool | StaysOnTop` 對應無邊框、tool、topmost 視窗；`WA_TranslucentBackground` 使其為 per-pixel alpha；`setWindowFlag(WindowStaysOnTopHint, v)` **重建**原生視窗並隱藏，呼叫端要再 `show()`。
- **qtrs required**：HUD 旗標 MUST 產生 `WS_POPUP | WS_EX_TOOLWINDOW | WS_EX_LAYERED`，on-top 時加 `WS_EX_TOPMOST`；topmost 切換 MUST 保持視窗可見且幾何不變。
- **Current implementation**：旗標 `IMPLEMENTED`，切換 `PARTIAL`。`NativeWindow::new`（`qtrs-platform/src/window.rs`）；HUD 請求 `FRAMELESS | CUSTOM_FRAMELESS | LAYERED | TOOL [| STAYS_ON_TOP] [| CLICK_THROUGH]`（`hud_window.rs:207-216`）；`LAYERED` 優先於 `CUSTOM_FRAMELESS`，所以不裝 NCHITTEST 設定，resize／move 走 `start_system_move/resize`（與 Python 的 `startSystemMove/Resize` 相同）；`set_stays_on_top` 就地 `SetWindowPos(HWND_TOPMOST/NOTOPMOST)`。
- **Known gap**：**G11.1.a [P2]** 無 `set_window_flags`；就地 `SetWindowPos` 保持可見，可觀察終態與 Python 的 `setWindowFlag + show()` 一致；**G11.1.b [P1]** 測試只檢查 `flags` 欄位，不檢查 `WS_EX_TOPMOST`／`WS_EX_TRANSPARENT`；**G11.1.c [P2, READ]** X11／Wayland／Cocoa 後端是模擬：沒有真實 X server／Wayland compositor／AppKit 連線（Cocoa 走 `MockObjcRuntime`）。RC-11b 之後 `is_active` 與 `WindowFlags::TOOLTIP` 在這些後端上由注入的事件或 mock 狀態驅動（X11 `FocusIn/Out`、Wayland `KeyboardEnter/Leave`、Cocoa `isKeyWindow`），未對真實系統驗證；X11 沒有 window type／override-redirect，Wayland 沒有 popup role。`WindowSystemEvent::MouseMove.buttons` 在 Win32 來自 `wParam` 的 `MK_*`（真實）；X11／Wayland／Cocoa 後端由視窗物件記錄自己看到的 press／release（模擬事件沒有按鈕狀態遮罩）。Wayland 以 keyboard focus 為 active，qtwayland 原始碼不在 `qtbase/`，`[INFERENCE]`；**G11.1.d [P1, READ；已修復：RC-27]** （修復前：）`Application::active_window()` 從不被設定（`set_active_window` 只有測試呼叫），沒有 `WindowActivate`／`WindowDeactivate`／`ActivationChange` 遞送給 widget，`QWidget::isActiveWindow` 不存在。平台層 `PlatformWindow::is_active` 已可用（RC-11b），但尚未接到 toolkit 層；RC-11c 的「只在 active window 顯示 tooltip」需要它。RC-27 之後：`FocusIn`／`FocusOut` 設定／清除 `Application::active_window()`，送 `WindowActivate`／`WindowDeactivate`（先到視窗事件處理器，再到可見的非視窗子 widget），並新增 `Widget::is_active_window`。**仍缺**：`ActivationChange` 事件、`SH_Widget_ShareActivation`（Tool 視窗共享啟用）、popup 視窗的 `isActiveWindow`、`QWidget::activateWindow`；tooltip 的「是否 active」仍讀 `PlatformWindow::is_active`，未改接到 toolkit 層。
- **Test**：既有 `window.rs::test_window_flags_to_win32_styles`（建立時樣式）、`test_native_window_lifecycle_and_methods`（只查 `flags` 欄位）。必要：`set_stays_on_top(false)` 後 `GetWindowLongPtrW(GWL_EXSTYLE) & WS_EX_TOPMOST == 0`。
- **HUD usage**：Python `hud_window.py:123-128,804-812`；Rust `hud_window.rs:538-545`。

### C11.2 顯示、隱藏、關閉
- **Qt behavior** `[QT-DOC]`：`isVisible()` 反映真實狀態（含 OS 隱藏）；`showEvent/hideEvent/closeEvent` 被遞送；`close()`／Alt+F4 送 `closeEvent` 後隱藏；`quitOnLastWindowClosed(False)` 時 app 繼續跑。Python HUD：`showEvent` → `QTimer.singleShot(0, _apply_theme)`（`hud_window.py:611-613`）；`closeEvent` 儲存幾何（`:607-609`）；`hideEvent` 排程 `trim_memory`（`:615-617`）。
- **qtrs required**：`Window` MUST 回報真實可見性；關閉請求 MUST 到達 app，Alt+F4 MUST 隱藏 HUD 而不結束程式；Show／Hide 通知 MUST 到達 widget 樹。
- **Current implementation**：`PARTIAL`。`show/hide` 呼叫 `ShowWindow`；`WM_CLOSE` post `CloseRequest`/`Close` 並回 0——**從不隱藏或銷毀**；`WM_SHOWWINDOW`/`WM_PAINT` 只對 `get_window_event_binding` post `Show/Hide/Expose`，HUD 從不綁定（grep `bind_event_loop` 只有定義）；widgets crate 從不處理這些事件。
- **Known gap**
  - **G11.2.a [P1, READ；已修復：RC-06]** 無 `Window::is_visible()`；Rust `HUDWindow.is_visible` 是自行追蹤的 bool（`hud_window.rs:131,518-523`）。
  - **G11.2.b [P0, READ；已修復：RC-06]** `CloseRequest` 在 `WindowEventHandler` 被 `_ => {}` 吞掉（grep `CloseRequest` 於 `rust/src`、`qtrs-widgets/src` 為空）。**Alt+F4 什麼也不做**；Python 會隱藏 HUD 並儲存幾何。
  - **G11.2.c [P0, READ；已修復：RC-06]** 無 `showEvent/hideEvent/closeEvent` hook：Python 的「show 時重新套用主題」「hide 時 trim_memory」沒有 Rust 對應（Rust 在 `hide()` 裡直接做，`hud_window.rs:479-485`）。
  - **G11.2.d [P2]** 無 `Expose` 重繪（分層 presenter 保留內容，非分層 `Win32DcPresenter` 視窗被遮蓋後不會重繪）。
  - **G11.2.e [P1, READ]** `Application::unregister_window` 在 drop 時、`quit_on_last_window_closed` 為 true 就呼叫 `quit`；Qt 在**關閉**時發 `lastWindowClosed`，不是銷毀時。
  - **G11.2.f [P2]** `GuiApplication::set_application_state`、`last_window_closed`、`focus_window_changed` 從不發射。
  - **G11.2.g [P1]** `main.rs` 從不 `set quit_on_last_window_closed(false)`（Python：`main.py:48`）。
  - **G11.2.h [P1, READ]** RC-06 之後 `Show`／`Hide` 仍只來自 `Window::show`／`hide`／`close`；原生發起的可見性改變（`WM_SHOWWINDOW` 被 post 給 `Window::event`，該處忽略；最小化／還原；他人呼叫 `ShowWindow`）不會送 `Show`／`Hide`，`Window::is_visible` 也不會跟著變。`close` 不發 `lastWindowClosed`、不處理 `WA_DeleteOnClose`（G11.2.e、G11.2.f 仍開放）。`Move` 只來自原生 `GeometryChange` 且 `old` 位置取自 qtrs 自己記錄的值。
  - **G11.2.i [P1, RAN；已修復：RC-17b]** Windows 原生 move／size loop 吞掉結束拖曳的 `WM_LBUTTONUP`，qtrs 沒有 `QWindowsContext::handleExitSizeMove` 的按鍵同步，所以以 `startSystemMove` 拖曳的視窗永遠收不到 `MouseButtonRelease`（HUD 的 release 即時存檔從未觸發，只靠 250 ms Move debounce）。`WM_ENTERSIZEMOVE`／`WM_EXITSIZEMOVE` 與 `InteractiveResizeStart`／`End` 本來就存在並到達 `qtrs_widgets::Window::handle_window_event`，但只用於 render 狀態；沒有第二套 EXITSIZEMOVE 機制。`[QT-SRC qwindowscontext.cpp:1261-1290]`：`WM_EXITSIZEMOVE` 時比較 `QGuiApplication::mouseButtons()` 與實體按鍵（`queryMouseButtons`，含 `SM_SWAPBUTTON`），對「app 認為按下、實體已放開」的每個按鍵合成 release：游標在視窗內為 `MouseButtonRelease`，在外為 `NonClientAreaMouseButtonRelease`（widgets 看不到）。qtrs 修復（`qtrs-platform/src/window.rs`）：thread-local `APP_MOUSE_BUTTONS`（DOWN／DBLCLK 設、UP 清）、`query_mouse_buttons`、`sync_mouse_buttons_after_move_loop`（內部走既有 `WM_*BUTTONUP` 路徑，不新增事件型別）。測試 `qtrs-widgets/tests/test_system_move_release.rs` 4 項（真實 HWND、真實 `WM_LBUTTONDOWN`／`WM_ENTERSIZEMOVE`／`WM_EXITSIZEMOVE`、真實游標位置）；`move_loop_end_releases_a_button_the_loop_swallowed_the_release_of` 修復前 FAIL（left 0 right 1），修復後 PASS，其餘 3 項為防過度遞送的護欄（修復前即 PASS，非 before-FAIL）。真實 `ClaudeHUD.exe` 拖曳 smoke：放開後 11 ms 內存檔（修復前約 1 s，靠 Move debounce）。未涵蓋：游標在視窗外的 `NonClientAreaMouseButtonRelease` 不遞送（與 Qt 對 widgets 的可見行為一致）；Wayland／X11／Cocoa 無 native move loop，未動。
- **Test**：既有 `test_application_layers.rs::{test_widget_application_window_registry_and_focus, test_core_application_exec_quit_and_about_to_quit}`（無 `closeEvent` 涵蓋）。必要：合成 `WM_CLOSE`，斷言視窗隱藏、`quit_on_last_window_closed(false)` 時 app 不結束、close hook 被呼叫；`Window::is_visible()` 跟隨 `show/hide`。
- **HUD usage**：Python `main.py:60`、`hud_window.py:861-866`、`main.py:48`；Rust `main.rs:335`、`hud_window.rs:518-523`。

### C11.3 幾何：move、resize、邏輯 vs 原生
- **Qt behavior** `[QT-DOC/INFERENCE]`：視窗幾何為裝置無關像素；原生幾何為 `QHighDpi::toNativePixels`，位置與大小**分別**取整；每次大小改變送一次 `Resize`，每次位置改變送一次 `Move`；`setMinimumSize` 由 OS 遵守。
- **qtrs required**：同上。
- **Current implementation**：主要 resize 路徑 `IMPLEMENTED`。`WM_SIZE`/`WM_MOVE`/`WM_GETMINMAXINFO`（`qtrs-platform/src/window.rs`）；`high_dpi.rs` 位置與大小獨立取整。
- **Known gap**
  - **G11.3.a [P0, READ；已修復：RC-08 implementation complete / real heterogeneous-DPI verification pending]** DPI 混用：WM handler 用 `GetDpiForWindow`，`Window::set_geometry` 用主螢幕 DPR（見 C10.7）。
  - **G11.3.b [P2]** 位置是 `i32` 邏輯值；滑鼠位置以取整後到達 widget，Qt 給 `QPointF`。
  - **G11.3.c [P1, READ]** `NativeWindow::geometry()` 回實體 `GetWindowRect`，`Window::geometry()` 為邏輯；原生視窗內快取的 `self.geometry` 混合實體寬高與邏輯 x／y。
- **Test**：既有 `test_resize_deferred_render.rs`（全部）、`test_single_resize_pipeline.rs::{interactive_one_wm_size_one_resize_one_present, normal_resize_stays_deferred_with_a_single_pipeline}`、`test_layered_geometry_sync.rs::hud_style_window_content_rect_equals_hwnd_rect_every_iteration`、`qtrs-platform/tests/{test_layered_interactive_resize.rs, test_dcomp_interactive_resize.rs}`。必要：邏輯→原生→邏輯在 125／150／175% 對奇數尺寸往返，對照已知 Qt 值。
- **HUD usage**：Python `hud_window.py:296-354,401,415-422,855-858`；Rust `hud_window.rs:592-635,805-831`。

### C11.4 螢幕與保持視窗在螢幕內
- **Qt behavior** `[QT-DOC]`：`screens()`、`primaryScreen()`、`widget.screen()`、`availableGeometry()`；`screenChanged` per window。Python HUD 用**視窗中心所在螢幕**；還原時要求至少 50×30 可見於任一螢幕，否則停靠到主螢幕 `avail.right - w - 40, avail.top + 50`（`hud_window.py:376-424`）。
- **qtrs required**：同上，且 MUST 以相同規則還原位置。
- **Current implementation**：`PARTIAL`（HUD 端 RC-13 已完成）。`Win32Screen`（`geometry`、`available_geometry`＝`rcWork`、`dpr`、`all_screens`）；`ensure_within_screens`／`clamp_window_rect_to_screens` 是 qtrs 自己的 helper（Qt 沒有對應 API），規則與 Python 不同（最大交集面積選螢幕、32×32 門檻、完全離開時置中），HUD **不使用**。HUD 的放置規則在 `rust/src/ui/placement.rs`（`restore_or_default_position`、`ensure_within_screen`、`reset_position`，純函式，以 `QRect` 的 inclusive `right()/bottom()/center()` 語意實作）。
- **Known gap**
  - **G11.4.a [P0, READ；已修復：RC-13]** Rust HUD 啟動時用 `primary_screen().geometry()` 與 `ensure_within_screen`（`hud_window.rs:178-193,640-662`），且與 `:823-824` 的 `available_geometry()` 不一致；**從不使用 `clamp_window_rect_to_screens`**；沒有以中心選螢幕、沒有「脫離所有螢幕就停靠右上」、沒有 50×30 門檻。修復：三個放置點（啟動還原、`apply_ui_mode_internal`、`reset_geometry`）改用 `placement.rs`。額外發現並一併修正（同一組規則）：(1) 啟動時 Rust 把位置夾進螢幕，Python 只在「至少 50×30 可見」時原樣 `move(x,y)`，不夾；(2) 只有一個座標存在時 Python 視為沒有儲存位置；(3) 模式切換時 Rust 以**舊尺寸**的視窗中心選螢幕，Python 在 `resize` 之後（新尺寸）；(4) `reset_geometry` Python 不夾，Rust 夾；(5) Rust 用 `geometry()`（含工作列），Python 用 `availableGeometry()`。證據：`rust/tools/gen_hud_placement_oracle.py` 以**未修改的 Python** `_ensure_within_screen`／`_restore_or_default_position`／`_reset_geometry` 對假螢幕（只假造螢幕清單）產生 3751 筆 oracle（`rust/src/ui/placement_oracle.txt`；6 種螢幕配置含負座標與上下堆疊、含「視窗中心在螢幕邊緣 ±1 px」），Rust 三個函式逐筆相同。舊算法（重新實作的算術，不是出貨程式碼）對同一 oracle 的結果：E 案例 1623／2714 筆不同、R 案例 959／992 筆不同（略過無主螢幕的案例；舊邏輯以主螢幕工作區近似，舊程式碼實際用含工作列的 `geometry()`），因此 oracle 能區分新舊；**沒有 before-FAIL**（舊邏輯內嵌在需要真實視窗的方法裡，無法呼叫）。變異檢查（不是 before-FAIL）：`right()` 改成 `x+width`、`center()` 改成 `x+width/2`、50×30 門檻改成 32×32，各使 oracle 測試 FAIL。Win32 煙霧測試（單螢幕 1536×864 邏輯，工作區 816 高）：儲存位置 (-3000,300) 與只露出 36 px 的 (1500,100) 都落到預設位置 (1216,50)；(1400,100)（露出 136 px）原樣保留，舊程式碼會夾到 x=846。多螢幕只以假螢幕 oracle 驗證，本機只有一個螢幕。  - **G11.4.b [P1, READ]** `Window` 無 `screen()`；`screen_changed` 只在 `WM_DISPLAYCHANGE` 發射，無訂閱者。RC-13 的 `Screens::window_screen` 以「完整幾何與視窗交集面積最大的螢幕」近似 `QWidget.screen()`；無交集時 Qt 用 `MonitorFromWindow(…NEAREST)`，近似版回到主螢幕。此 gap 仍開著。
  - **G11.4.c [P1, INFERENCE]** `Win32Screen::geometry` 以 dpr 除原點（`high_dpi::from_native_rect`），在 DPI 不同的副螢幕上不是 Qt 的虛擬桌面映射。
  - **G11.4.d [P2]** `Win32Screen::primary()` 寫死 `MonitorFromPoint(0,0)`；DPR 夾到最小 1.0。
  - **G11.4.e [P1, READ]** `qtrs_gui::Rect::right()`／`bottom()` 回傳 `x + width`／`y + height`，`QRect::right()`／`bottom()` 是 `x + width - 1`（`qrect.h:199-203`）；`center()` 是 `x + width / 2`，`QRect::center()` 是 `(x1 + x2) / 2`（`qrect.h:253-257`）。`contains`／`intersects` 的半開語意與 `QRect` 一致，只有這幾個存取器不同，名稱相同而數值差 1，移植 Python 程式碼時會靜默偏 1 px。qtrs 非測試程式碼約 83 處使用 `.right()`／`.bottom()`，**未修**（改語意影響全部呼叫者，是獨立 RC）；RC-13 的 `placement.rs` 不使用它們。
  - **G11.4.f [P2, INFERENCE]** 設定載入（`config.rs` sanitize）把 `window_x/y` 超出 −5000..10000 的值改為 `None`；Python 的 `hud_window.py` 沒有這個範圍檢查（未檢查 Python 設定模組）。
  - **G11.4.g [P2, INFERENCE]** 混合 DPI 多螢幕的工作區座標經 `high_dpi::from_native_rect`（G11.4.c）；RC-13 的 oracle 使用合成的單一座標系，本機只有一個螢幕，真實多螢幕、混合 DPI 未驗證。
- **Test**：既有 `test_power_events_and_screen_clamping`、`test_platform_screen_primary_and_multi_screens`（只練 helper，沒呼叫 HUD 邏輯）。必要：以「儲存位置在所有螢幕之外」與「在第二螢幕」兩種情況驅動 HUD 還原，斷言 Python 的停靠規則。
- **HUD usage**：Python `hud_window.py:376-424,856-858`；Rust `rust/src/ui/placement.rs`（被 `hud_window.rs` 的三個放置點呼叫）。

### C11.5 不透明度與呈現
- **Qt behavior** `[QT-DOC/INFERENCE]`：`setWindowOpacity(v)` 使整個視窗 `v` 透明；分層視窗以 `UpdateLayeredWindowIndirect` 的 `SourceConstantAlpha` 呈現。
- **qtrs required**：`set_opacity(v)` MUST 在 HUD 的分層視窗上改變可見 alpha，**不論選用哪個 surface**。
- **Current implementation**：`PARTIAL`。`NativeWindow::set_opacity` 存值，僅對**非** `LAYERED` 視窗呼叫 `SetLayeredWindowAttributes`；`Win32LayeredPresenter` 用它當 `SourceConstantAlpha`；`get_or_create_presenter` 對 `LAYERED` 視窗**先試 `DCompSurface::new`**，失敗才退回 GDI layered presenter。
- **Known gap**
  - **G11.5.a [P0（條件式：僅 DComp 可用的機器）, READ；本機 RAN：選到 Layered，未重現；已修復：RC-09 implementation complete / DirectComposition hardware verification pending]** **DComp 路徑丟棄 opacity**：`present_dirty_ref(&mut self, pixmap, _opacity, dirty)`（`surface/dcomp.rs`）從不使用它；`WindowsPresenter::set_opacity` 對 `DirectComposition` 為 no-op；以寫死的 `1.0` 呈現。若 DComp 被選用，HUD 的不透明度設定**無效**（`hud_window.py:132,820` vs `hud_window.rs:223,782-788`）。
  - **G11.5.c [test gap, RAN；部分處理：RC-09 新增明確標為 ignore 的 DComp 測試，既有 test_dcomp_* 仍靜默 skip]** `test_dcomp_*`（5 項）在本機因 `CreateDXGIFactory1 failed for IDXGIFactory2` 全部 skip，卻顯示為 passed；`window.rs:1490-1492` 對所有 `LAYERED` 視窗**無閘門地先試 DComp**，另有 `test_layered_interactive_resize::window_pipeline_*` 在本機選到 Layered。DComp 路徑在本機完全沒被測試。
  - **G11.5.d [P1, RAN；已修復：RC-09]** 非 `LAYERED` 視窗的 `setWindowOpacity` 無效：`dyn PlatformWindow::set_opacity`（widgets `Window::set_opacity` 走的路徑）只寫入欄位並通知 presenter；`Win32DcPresenter` 不處理 opacity；另一個 inherent `NativeWindow::set_opacity` 呼叫 `SetLayeredWindowAttributes` 卻沒設 `WS_EX_LAYERED`（對沒有該樣式的視窗會失敗），且 trait 路徑根本不會呼叫它。Qt：`setWindowLayered` 在 `opacity < 1` 時加 `WS_EX_LAYERED`，再 `SetLayeredWindowAttributes(qRound(255*level))`（`qwindowswindow.cpp:494-530`）。
  - **G11.5.e [test gap]** `Win32LayeredPresenter` 把 opacity 傳為 `UpdateLayeredWindowIndirect` 的 `SourceConstantAlpha`（與 Qt 的 `qRound(255*opacity)` 等價，讀碼確認）；沒有讀回合成結果的測試——桌面合成無法在不依賴桌面內容的前提下讀回。
  - **G11.5.f [P2, 未量測]** DComp 的 opacity 以 CPU 在 staging DIB 複製時逐像素縮放實作（`opacity < 1` 時每次呈現多一次乘法；opacity 變更時整面重傳），而非 GPU 端的 `IDCompositionEffectGroup::SetOpacity`。後者的 vtable slot 在本機無法驗證（本機沒有 DXGI factory），因此未採用。
  - **G11.5.g [P1, 未決策，不在 RC-09]** production 是否允許 DComp；`get_or_create_presenter` 對所有 `LAYERED` 視窗無閘門地先試 DComp，RC-09 未改選擇邏輯。
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
- **Known gap**：**G11.8.a [P1]** 選單位置換算用主螢幕 DPR（`tray_icon.rs:281`）；**G11.8.b [P2]** `show_message` 只收 title／text／4 值圖示 enum／時間，不收自訂 `QIcon`（Python 傳 `tray.icon()`，`main.py:75-82`）；**G11.8.c [P0, READ；= G11.9.a 的重複登錄；已修復：RC-12]** Python 的 `hotkey_failed` 訊息 Rust 沒有（→ G11.9.a）；**G11.8.d [P2]** 圖示：Python 依平台選 `.ico/.icns/.png`（`tray_icon.py:17-28`），Rust 內嵌 PNG；**G11.8.e [P2, INFERENCE]** 雙擊在 Windows 先 Trigger 兩次再 DoubleClick（如 Qt）；**G11.8.f [P2]** DBus／macOS 後端存在但未驗證。
- **Test**：既有 `tray_icon.rs` 內聯測試（`test_menu_item_constructors`、`test_menu_builder_and_hmenu_lifecycle`、`test_create_hicon_from_pixmap`、`test_tray_icon_lifecycle`、`test_tray_signals_and_window_proc_dispatch`）；應用層 `test_context_menu_parity_cards_and_table_modes`、`test_tray_and_hud_menu_unified_parity`（只比選單內容）。必要：對 tray 視窗 post `WM_LBUTTONUP` 與 `WM_LBUTTONDBLCLK`，斷言發射序列。
- **HUD usage**：Python `tray_icon.py:43-151`、`main.py:63-65,75-82`；Rust `rust/src/ui/tray_icon.rs:547-560`、`main.rs:343-357,431-449`。

### C11.9 全域熱鍵
- **Python behavior**：不用 `QShortcut`；以 ctypes `RegisterHotKey` 執行緒（`hotkey.py:34-110`）發射 `hotkey_triggered`/`clickthrough_triggered`/`hotkey_failed`/`unavailable`，經 queued 連線到 GUI 執行緒；`_safe_init_click_through` 在 click-through 熱鍵註冊失敗時停用啟動 click-through（`hud_window.py:426-437`）。
- **qtrs required**：Alt+C 與 Alt+Shift+C MUST 切換可見性與 click-through；註冊失敗 MUST 被回報，且鎖定防護（lockout guard）MUST 使用**真實註冊結果**。
- **Current implementation**：Windows 執行緒 `IMPLEMENTED-UNTESTED`（`rust/src/hotkey.rs`，id 9527/9528、`MOD_NOREPEAT`、drop 時 `WM_QUIT`）；`parse_hotkey`／`compute_ct_mods` 有單元測試。`qtrs-platform::Win32HotkeyManager` 存在但 **app 不用**。cocoa／unix／generic 的 `PlatformHotkeyManager` 只記錄 id 並回 `Ok`。
- **Known gap**
  - **G11.9.a [P0, READ；已修復：RC-12]** `HotkeyManager::start` 在 Windows 即使 `RegisterHotKey` 失敗也回 `Ok`，失敗只在執行緒內 `warn!`（`hotkey.rs:235-261`）；`main.rs:496` 檢查 `hotkey.is_none()`，真實衝突時永不成立 → **鎖定防護與托盤警告不會觸發**（Python 檢查 `clickthrough_registered` 並顯示警告：`main.py:78-89`、`hud_window.py:426-437`）。修復：工作執行緒在兩次 `RegisterHotKey` 之後才回報（`start` 等該回報，與 Python 的 `_ready_event` 相同），`HotkeyManager::registration()` 回傳每個熱鍵的 `HotkeyStatus::{Registered, Failed(GetLastError), Unsupported}`；`click_through_startup_allowed` 只在穿透熱鍵已註冊時回 true（`_safe_init_click_through`）；`main.rs` 依 Python 順序為每個失敗送托盤訊息（`hotkey_failed` 再 `unavailable`）。**與 D.2 原閘門的差異**：`start` 在熱鍵被占用時**不**回 `Err`，因為兩個熱鍵獨立，只有穿透熱鍵失敗時 Python 仍保留可用的顯示／隱藏熱鍵；若 `start` 回 `Err`，`HotkeyManager` 會被丟棄而同時失去它。`Err` 只用於執行緒無法啟動或在回報前結束。測試 `rust/src/hotkey.rs` 的 `mod tests`（6 項新增，用 `RegisterHotKey` 在本程序先占用同一組合以得到真實的 1409）。**沒有 before-FAIL**：測試用的 `registration()` 在舊程式碼不存在；以變異檢查代替（不是 before-FAIL）：把 toggle 的失敗判斷改成永不失敗，`start_reports_a_taken_toggle_hotkey_and_keeps_the_other` FAIL。`main.rs` 的接線沒有自動測試；以真實 HUD 行程做煙霧測試（先占用 Alt+Shift+C、`click_through=true`）：log 出現 `Failed(1409)`、兩則 `Hotkey registration issue`、`Click-through mode disabled on startup`，設定檔隨後還原。托盤氣泡的實際顯示**未**目視驗證。
  - **G11.9.b [P1]** macOS 熱鍵明確未實作（`hotkey.rs:7-8,146-151`）；Python 用 pynput。
  - **G11.9.c [D]** `GenericHotkeyManager`／`CocoaHotkeyManager`／`UnixHotkeyManager` 為 stub，回報成功卻未註冊。理由：未驗證平台；**但 stub 回 `Ok` 違反規則 3（誤用須可見失敗）**——必須改回錯誤。
  - **G11.9.d [P2]** app 以原子旗標 + `run_on_main_thread`（`main.rs:466-483`）取代 queued 信號。
  - **G11.9.e [P2, READ]** 穿透鎖定訊息文字不同：Rust「全域快捷鍵未註冊成功，…」（`main.rs`），Python「全域快捷鍵註冊失敗，…」（`hud_window.py:432`）。
  - **G11.9.f [P2, READ]** Rust 的主熱鍵取自設定 `hotkey`，穿透熱鍵由 `compute_ct_mods` 推導；Python 固定 `Alt+C`／`Alt+Shift+C`（`main.py:89`、`hotkey.py:70,79`）。預設設定下相同；訊息文字使用實際註冊的組合，而非固定的 `Alt+`。
  - **G11.9.g [P2, INFERENCE]** `HotkeyManager::start` 等工作執行緒回報，沒有逾時（Python 最多等 1.5 s）；`RegisterHotKey` 本身不會長時間阻塞，但此處未驗證執行緒卡住的情況。
  - **G11.9.h [P2]** 熱鍵失敗時的托盤氣泡（`main.rs`）只以 log 煙霧測試驗證，沒有自動測試，也沒有目視確認氣泡內容與連續兩則訊息的顯示。
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
| `QTimer.setSingleShot` debounce 250 ms | `hud_window.py:73-76,626,631` | 以 `ResizeDebouncer` 實作（RC-17：move／resize 皆經由它） | G12.5.d |
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
| `setToolTip` | `provider_card.py:125`、`usage_table.py:318,328,339,373-375`、`hud_window.py:155,160` | **已接線**（7 處；外觀 MANUAL） | G8.8.a |
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
| `QColor.darker(110)` | `usage_table.py:88` | **已修復：RC-25**（`qtrs_gui::color::darker`，`usage_table.rs` 的 pie 圖例） | G12.5.p |
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
- **G12.3.e [P2]** `font-family` 清單以一個原始字串存、查找時才拆；generic（`sans-serif`/`monospace`）被跳過而非映射到系統字型，`'Consolas', monospace` 在沒有 Consolas 時沒有等寬退路。**Qt 依據**：Linux 的 `QFontconfigDatabase::resolveFontFamilyAlias` 把家族字串交給 `FcConfigSubstitute`（`qfontconfigdatabase.cpp:970-995`），`fallbacksForFamily` 另把 style hint 轉成 fontconfig 的 `sans-serif`／`monospace` 等（`getFcFamilyForStyleHint`，`:347-367`）；macOS 無此家族時依 style hint 取 Menlo 等（`qcoretextfontdatabase.mm:625-640`）；Windows 取 Courier New 等（`qwindowsfontdatabasebase.cpp:939-961`）。與 G12.6.d 同一根因（字型來源與缺字型規則），一起處理，不單獨修。CSS 的 generic 名稱在 Qt 內經哪一條路徑變成 style hint，尚未追到 `qcssparser.cpp`／`qstylesheetstyle.cpp`，`[INFERENCE]`。
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
| G12.5.a [P0, READ；= G12.3.b] | Badge `max-height: 15px` 只在 Rust | `styles.rs:192,291` vs `styles.py:116-124` | 修復＋樣式表比對測試（幾何稽核：案例 A，見 C12.8） |
| G12.5.b [P0, READ；= G9.4.b] | 卡片根 layout spacing 2（Rust）vs 5（Python），無註解說明 | `provider_card.rs:159` vs `provider_card.py:27`（已讀確認） | 修復或寫理由（幾何稽核：案例 A，須與 G12.8.a 同做，見 C12.8） |
| G12.5.c [P1] | 面板底色不依 Acrylic 是否成功而改變 | `hud_window.rs:88-92,229` vs `hud_window.py:240-250` | 修復 |
| G12.5.d [P0, READ；已修復：RC-17] | 幾何持久化：Python 250 ms 單發於 move／resize 重啟＋mouse release 儲存；Rust 原本只有 resize 的 `ResizeDebouncer` + 3 s 輪詢抓移動，且無 release handler。已修復：3000 ms geometry poll removed because RC-06 window event lifecycle now supplies Move/Release/Close/Hide hooks；HUD 以 `Window::set_window_event_handler` 處理 Move（更新 config x/y＋`request_save`）、MouseButtonRelease 與 Close（立即 `persist_rect`，取消待存）；resize 寫入改為 clamp 至 `MIN_*`（以前寫未 clamp 的尺寸）；250 ms 仍是 HUD 的 `ResizeDebouncer`，不是 qtrs `Timer` 需求；測試 `ui/geometry_persist_tests.rs` 6 項（修前 6/6 失敗；真實 Win32 訊息）；殘餘：Hide 事件本身不另存（`HUDWindow::hide` 已存；Python 也只在 closeEvent 存）、高層 burst 重啟語意只由 `config.rs` 的 debouncer 測試涵蓋（debug build 每次 resize 渲染 180–300 ms，超過 250 ms）| `hud_window.py:595-609,624-649` vs `config.rs:396`、`main.rs:575-591` | 修復 |
| G12.5.e [P0, READ；已修復：RC-18] | 喚醒偵測（倒數 tick 間隔 >15 s 就刷新）在 Rust 原本不存在。已修復：以 application 層 timestamp-gap detection 實作（`tick_gap_exceeded`＋`HUDWindow::on_clock_tick`，`SystemTime`，嚴格 `>15 s`，第一次 tick 與倒退時鐘皆不刷新，長間隔剛好一次 `RefreshController::refresh()`）。`Power::Resume` 刻意不屬於 RC-18（見 G12.5.x）。測試 `ui::hud_window::tests` 4 項；before-FAIL 不可用（新函式／新 API，未偽造）；真實 suspend/resume：MANUAL WINDOWS VERIFICATION REQUIRED。 診斷（僅供驗證，不影響決策）：`HUDWindow::wake_refresh_seq`（長間隔 refresh 序號）；debug build 於長間隔分支 `eprintln!("[rc18] wake refresh #N previous_ms=… now_ms=… gap_ms=…")`；測試 `wake_refresh_sequence_advances_once_per_long_gap_only`（正常 tick 序號不變、17 s 間隔 +1、其後 1 s tick 不變、剛好 15 s 不變、16 s +1；TI-01 stub，無 real-time sleep）。真實 suspend/resume、lock/unlock、debugger pause 仍為 MANUAL WINDOWS VERIFICATION REQUIRED（可用 stderr `[rc18]` 行判讀）。 | `hud_window.py:461-467`；grep `gap|WM_POWERBROADCAST|resume` 於 `rust/src` 為空 | 修復 |
| G12.5.x [P2] | `Power::Resume`／`Suspend`（`WM_POWERBROADCAST`）目前沒有 toolkit consumer：`qtrs-widgets` `Window::handle_window_event` 的 `_ => {}` 丟棄 `WindowSystemEvent::Power`，HUD 也未連接 `TrayIcon::on_power_event`。Qt 本身沒有 suspend/resume 事件（`qwindowscontext.cpp` 只處理 `PBT_POWERSETTINGCHANGE`，顯示器喚醒時 `InvalidateRect`），所以這是 qtrs 擴充，與 RC-18 無關 | `window.rs:1126`、`window.rs`(widgets):1508；`qwindowscontext.cpp:271-298` | 待決策 |
| G12.5.f [P0, READ；已修復：RC-06] | `Window` 沒有 mouse-release／double-click／move／close 的 handler；雙擊在 Rust 會重新開始視窗移動，Python 是刷新 | `window.rs:1025`（platform 有發 release）；`window.rs:771-815` | 修復 |
| G12.5.g [P0, READ；= G11.2.b；已修復：RC-06] | Alt+F4 / `CloseRequest` 被吞 | G11.2.b | 修復 |
| G12.5.h [P1] | 單發時序：Python 300 ms（啟動 click-through）、150 ms（hide 後 trim）、1000 ms（busy→idle 後 trim）、2500 ms（啟動後 trim）；Rust 在 `hide()` 立即 trim，且只有 2500 ms | `hud_window.py:108,114,215,459,613,617` vs `hud_window.rs:484`、`main.rs:594` | 修復或核准 |
| G12.5.i [P0, READ；= G8.8.a；已修復：HUD tooltip 接線] | 所有 widget tooltip 缺失（錯誤與過期資料以 tooltip 顯示） | G8.8.a | 修復 |
| G12.5.j [P0, READ；= G11.9.a；已修復：RC-12] | 熱鍵註冊失敗不被回報；鎖定防護失效 | G11.9.a | 修復 |
| G12.5.k [P1] | `--smoke-test` 不檢查設定持久化 | `smoke_check.py:26-29` vs `main.rs:89-114` | 修復 |
| G12.5.l [P0, READ；= G11.4.a；已修復：RC-13] | 螢幕選擇／脫離螢幕還原規則 | G11.4.a | 修復 |
| G12.5.m [P1] | 托盤選單：Python 的托盤選單沒有鎖定／不透明度／間隔／重設／隱藏等項目；Rust 托盤選單是完整的 context menu | `tray_icon.py:54-122` vs `rust/src/ui/tray_icon.rs:73-240` | 需決定 |
| G12.5.n [P1] | 托盤通知：Rust 只有「ghost paused」；缺 hotkey 失敗、ghost 啟用、autostart 失敗 | `main.rs:503`、`main.rs:486-488`、`hud_window.rs:526-532`、`tray_icon.rs:686-690` | 修復 |
| G12.5.o [P1] | QMenu 外觀為寫死數值，非 QSS | G8.5.f | 修復或核准 |
| G12.5.p [P1] | `QColor.darker(110)` 缺失：Rust 用原色。**已修復：RC-25** | `usage_table.rs` vs `usage_table.py:88` | 已修復（`qtrs_gui::color::darker`，PySide6 逐位元比對；圖例像素與 PySide6 相同） |
| G12.5.q [P1] | `UsageDial` 最小尺寸 0 vs 84。**已修復：RC-29** | G8.3.e | 已修復（`minimum_size()` 84×84；PySide6 oracle 比對擠壓後的 dial 尺寸） |
| G12.5.r [P1] | 版面切換：`StackedWidget` vs 重建 | C9.6 | 決定 |
| G12.5.s [P0, READ；= G8.5.d；已修復：RC-10] | `Window::set_style_sheet` 為 app 全域 | G8.5.d | 修復 |
| G12.5.t [P0, READ；= G11.5.a；已修復：RC-09 implementation complete / DirectComposition hardware verification pending] | DComp 路徑 opacity 無效（待實測） | G11.5.a | 實測後修復 |
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
- **CI 失敗紀錄（commit `9dc62db` 之後，最後一次全綠；修復見後）**：`17a6e31` 起 Linux／macOS 的 `cargo check` 失敗，原因是 `qtrs-platform/src/window.rs` 有三處沒有 `#[cfg(windows)]`（`KeyboardAndMouse` import、`sync_interactive_resize`、`native_window_proc_inner`）；`1705cad` 起 Windows 的 `cargo test` 失敗：(1) `test_tray_and_hud_menu_unified_parity` 預設 `appearance=auto`，在淺色主題的 runner 上得到淺色選單；(2)(3) `test_labels_use_the_app_default_family`、`test_hud_layout_proportions` 受另兩個測試把**行程全域**的 application DPR 設成 1.25 的影響（CI 以多執行緒跑；本機一向 `--test-threads=1`，所以看不到；本機以預設執行緒數重現 4／4 失敗）。修復：補三處 `#[cfg(windows)]`；tray 測試固定 `appearance`；`rust/.cargo/config.toml` 設 `RUST_TEST_THREADS=1`（根因是全域 DPR，序列化是對症的隔離，不是消除全域狀態）。在 WSL（Ubuntu，與 CI 的 `cargo check --all-features` 及 `cargo test --all-targets` 同命令）驗證：check 通過；測試只剩本機已安裝版本不同的 `agy` CLI 造成的 `test_fetch_usage_live_benchmark`（CI 上 `cli_not_found` 會略過）。**macOS 沒有驗證**：本機無法交叉編譯 `ring`，只能依 CI 的錯誤清單（與 Linux 相同的 `windows_sys` 未閘控）判斷。
- **Known gap**：**G12.6.a [P1, RAN]** 6 項以 Windows 字型（Microsoft JhengHei UI／Segoe UI／Consolas）的 PySide6 實測值為期望的 HUD 版面測試（`usage_table.rs` 4 項、`provider_card.rs` 2 項）現在只在 `#[cfg(windows)]` 執行；Linux／macOS 上字型不存在、也沒有參考數值，所以這些平台的 CI 不驗證 HUD 版面。**G12.6.b [P2, RAN]** qtrs workspace 的測試在非 Windows 無法編譯（`GenericWindow` 沒有 `expect`、`LayeredSurface` 找不到），且不在 CI。**G12.6.d [P1, RAN]** 字型來源與缺字型時的替換規則和 Qt 不同。**Qt**：不掃目錄，向平臺資料庫要——Linux 為 fontconfig（`qtbase/src/gui/text/unix/qfontconfigdatabase.cpp`：`FcFontList`、`fallbacksForFamily` 的 `FcConfigSubstitute`＋`FcFontSort`，所以 `Consolas` 會被換成 fontconfig 認為最接近的等寬字型）、macOS 為 CoreText（`coretext/qcoretextfontdatabase.mm`：`fallbacksForFamily` 取 CoreText cascade list，無則依 style hint 取 Helvetica／Times New Roman／Menlo，預設字型是系統 UI 字型）、Windows 為 GDI／DirectWrite（`windows/qwindowsfontdatabasebase.cpp:925-963`：style hint → Arial／Courier New／Tahoma，再加 Segoe UI Emoji／Symbol）。找不到要求的家族時，`QFontDatabasePrivate::load`（`qfontdatabase.cpp:2884-2900`）依序試：要求的家族 → `QGuiApplication::font()` 的第一個家族 → 空家族（第一個支援該文字的字型）。**qtrs**：只掃固定目錄（`font_database.rs:200-212`）：Windows `%WINDIR%\Fonts`；其他系統 `/usr/share/fonts`、`/System/Library/Fonts`。缺家族時固定換成 Segoe UI，再換 Arial，兩者都沒有就回 `None`——這條鏈是 qtrs 自創，不是 Qt 的規則，「應用程式預設字型」寫死為 `Microsoft JhengHei UI`（`APP_DEFAULT_FAMILY`），在 Linux／macOS 上不存在，不是向平臺要系統 UI 字型。**CI 實測（run 37563543066，`4c2f13e`，全綠）**：Windows runner 有 Microsoft JhengHei UI、Segoe UI、Consolas（14px 標籤高度 18／19／17），Windows 版面測試用的是正確字型；macOS runner 沒有 Segoe UI（無替代，`load_font` 回 `None`），Microsoft JhengHei UI 與 Consolas 被換成另一個字型（高度 16），PingFang TC／SC 找不到（`/System/Library/Fonts/Supplemental` 的 290 個檔有被遞迴掃到，`/Library/Fonts` 只有 1 個檔，`~/Library/Fonts` 0 個）；Ubuntu runner 只有 10 個家族、沒有 Arial，14px 高度退為 15.4；HUD 執行檔在 Linux 沒有缺少的共享函式庫。**診斷**：CI 的 `diagnostics` 工作（不影響結果）在三個 OS 上執行 `qtrs-gui` 的 `font_probe` 範例（每個要求的家族是 INSTALLED／SUBSTITUTED／MISSING、各目錄字型檔數、版面測試依賴的數值），Linux／macOS 另跑 `rust/tools/ci_platform_fonts.py`：fontconfig 的 `fc-list`／`fc-match`、CoreText 的 `system_profiler SPFontsDataType`，比較平臺資料庫有、qtrs 沒掃到的字型；Linux／macOS 另列 HUD 執行檔的共享函式庫。**未修**：需要的修法是讓 qtrs 的字型來源與缺字型規則對齊平臺資料庫，範圍大於診斷，等 `ci_platform_fonts.py` 在 runner 上的輸出再定。**G12.6.c [P2]** 測試序列化依賴 `.cargo/config.toml` 的環境變數；全域 application DPR 本身仍是行程全域（Qt 的 `devicePixelRatio` 也是 application 層級，但 Qt 的測試是每個 binary 一個行程）。 **驗證狀態（`rust/tools/ci_platform_fonts.py`，commit `65a2f32`）**：CI 診斷步驟中 **macOS `system_profiler` 的 JSON 解析未驗證**（沒有 macOS 環境執行過）；若在 macOS 上驗證出問題，另開獨立修復，不改寫歷史。

### C12.7 驗收閘門：什麼時候可以說「一致」

| 宣稱 | 最低證據 |
|---|---|
| 「扁平 box／grid layout 與 Qt 一致」 | C9.7 的 harness，記錄 commit／cases／seed／差異數 |
| 「HUD layout 與 Qt 一致」 | C9.7(b) 的擴充全部完成，且 provider card 幾何（G12.5.b）與 Python 逐項比對 |
| 「文字渲染一致」 | `DIFF`：`qt_advance_compare.py`、`qt_glyph_compare.py`、`test_lcd_text_parity.rs` |
| 「像素一致」 | 在 100／125／150／200% 下對 Python 與 Rust HUD 截圖逐像素差分，**並列出差異像素數與平均絕對差**；之前一次臨時量測（125%：246,594 像素中 6,685 個不同，平均絕對差 2.38）**沒有存進 repo，無法重現，不得引用為證據** |
| 「行為一致」 | C12.5 每一項關閉，或在 §1.3 登記為刻意差異 |
| 任何涉及像素的回報 | 結尾 **MANUAL WINDOWS VERIFICATION REQUIRED**，除非逐像素差分實際為零 |

### C12.8 幾何差分稽核（RC-14／15／16；只量測，未改 production code）

完整報告：`rust/qtrs/GEOMETRY_DIFF_RC14_16.md`；工具與原始資料：`rust/tools/geometry_audit/`（`py_geom.py` 以真實 Python `HUDWindow` 為 oracle，`compare.py`，`results/*.json`）；Rust 端 `rust/src/ui/geometry_audit.rs`（`#[ignore]`，且會把 home 指到空資料夾以避免 provider 送出網路請求）。

- **方法**：逐 widget 比 rect／sizeHint／min／max／spacing／margins／字型度量；以 8 個「what-if」變體（只用公開 widget API 從外部改 Rust 的值）判斷 Rust 的某個值是否在補 qtrs 差異。**必須先設 `set_application_device_pixel_ratio(1.25)`**，否則 qtrs 走 GDI 路徑而得到錯誤結論（第一輪就量錯過）。
- **結果**：三個 RC 全為**案例 A（應用層）**；沒有發現 RC-14／15／16 的 Rust 值在補 qtrs 差異。橫向 y/h 不吻合 18 → 0（字級 14＋spacing 5）；直向 49 → 1（再加 Python 容器結構與 Preferred policy），剩下的 1 個與橫向殘餘 x/w 全部來自下列 qtrs 項（g、j）。
- **限制**：只量 DPR 1.25、Windows 字型、預設佔位文字；無即時資料、無 CJK badge、**無像素**；V5／V6 內各設定的貢獻未逐項隔離。

| ID | 嚴重度／驗證 | 層級 | 內容 |
|---|---|---|---|
| G12.8.a | P1, RAN；已修復：RC-14 | 應用 | 指標值字級：Python widget-local `font-size: 14px`；Rust 只有 `set_font(14)`，被 app sheet `QLabel#MetricValue { font-size: 16px }` 蓋過（符合 Qt：樣式表字級勝過 `setFont`），有效字級 16，`sizeHint` 高度 19 vs 17。RC-14 的第一個分歧 |
| G12.8.b | P1, RAN；已修復：RC-14 | 應用 | 橫向 body spacing：Python 8（`hud_window.py:337`），Rust 預設 6；卡片寬 213 vs 211 |
| G12.8.c | P1, RAN；已修復：RC-16 | 應用 | 直向容器 policy／stretch：Python 不設；Rust `stack`／`cards_container` `Expanding`、根 stretch 1、卡片 stretch。RC-16 的延伸 |
| G12.8.d | P2, RAN；已修復：RC-16 | 應用 | `title` size policy：Python `Minimum/Preferred`（`provider_card.py:34`），Rust 預設 |
| G12.8.e | P2, RAN | 應用 | badge 字重：Python 400；Rust Bold（`set_font(...Bold)`，QSS 沒有字重） |
| G12.8.f | P1, RAN；已修復：RC-23 | 應用 | 視窗大小常數：橫向最小／預設高 Python 125／145，Rust 130／152；直向最小 Python 320、預設 410，Rust 463（由 layout 推導）／490。**已修復：RC-23**。Python 來源 `hud_window.py:31-35`（`MIN_HORIZ 540×125`、`DEF_HORIZ 690×145`、`MIN_VERT 250×320`、`DEF_VERT 280×410`）與 `config_manager.py:15-17`；`_apply_cards_layout`（`:325-354`）每次 `setMinimumSize(MIN)`，存檔值低於 MIN 才退回 DEF（不是夾到 MIN）。PySide6 實測（新 config、DPR 1.25）：橫向 `size` 690×145、`minimumSize` 540×125；直向 280×410、250×320；layout 自身最小值 435×151／265×395，**被明確的 `setMinimumSize` 蓋過**（視窗可小於內容需求）。根因＝app 常數（`config.rs`）：Rust 把直向最小高由 layout 內容推導（463，`vertical_layout_min_height()`），預設 152／490 為自訂值；`sanitize` 與 `reset_geometry` 用同一組常數，所以 Python 保留的存檔尺寸（橫向 125–129、直向 320–462）被 Rust 重設。修復：`config.rs` 常數改為 Python 值（125／145、320／410），刪除推導用的 `vertical_layout_min_height()` 與其 6 個專用常數。未改 qtrs。測試（修改前 FAIL）：`config::tests::test_config_defaults`（490 vs 410）、`test_sanitize_keeps_stored_sizes_pyside6_keeps`（152,490 vs 125,320）、`hud_window::tests::test_window_default_and_minimum_sizes_match_pyside6`（690×152 vs 690×145）。已存的 geometry：≥ 新最小的值全部保留；只有「舊 Rust 預設 152／490 以外且 < 新最小」才會被重設，而新最小比舊的低，所以沒有原本有效的存檔值變成無效。 |
| G12.8.g | P1, RAN；已修復：RC-19 | **qtrs** | QSS `min/max-width/height` 盒模型：Qt 作用於 content＋padding＋border（`qstylesheetstyle.cpp:2603-2611`）；qtrs 當總尺寸。`layout_toggle_btn` 最大高 Python 22 vs Rust 18、最小寬 28 vs 18 |
| G12.8.h | P1, READ；已修復：RC-19 | **qtrs** | `Label::size_hint` 以 `max-height`（否則 `min-height`）當高度 hint（`label.rs`），Qt 沒有此規則 |
| G12.8.i | P1, RAN | **qtrs** | **已修復：RC-20**（預設字型的數值差屬 Application font，未處理）。`QProgressBar`：Python `sizeHint` 91×5、`minimumSizeHint` 91×17；qtrs 160×5、0×5 |
| G12.8.j | P1, RAN；已修復：RC-21 | **qtrs** | 文字寬度 1 px：`WEEKLY 7D` Python 59，qtrs 59.589 → `ceil` 60；`AI AGENT HUD (3-IN-1)` 144 vs 144.107 → 145。**已修復：RC-21**。根因**不是取整規則**：`QLabel` 的 `font-family: 'Segoe UI', 'SF Pro Display', 'Microsoft JhengHei', sans-serif` 是字型家族清單，qtrs 的 QSS 解析把整串當成單一家族名，找不到而退回別的字型（量到 59.589／144.107）。`[QT-SRC qcssparser.cpp:1252-1272]` `setFontFamilyFromValues` 以逗號切開並呼叫 `QFont::setFamilies`，字型庫依序取第一個已安裝的家族（Segoe UI）。整數轉換本身 Qt 與 qtrs 一致：`QLabelPrivate::sizeForWidth` 走 `fm.boundingRect(...)`→`rb.toAlignedRect()`（`qlabel.cpp:609`、`qfontmetrics.cpp:735-749`），qtrs 對寬度取 `ceil`。`ceil` 未改。修復：`QCssValue::FontFamilies`（解析器）、`ResolvedStyle::font_families`／`font_family()`（取第一個已安裝家族，否則第一個）。測試 `qtrs-widgets/tests/test_label_text_metric_rounding.rs`（HUD 實際樣式表，PySide6 oracle）：DPR 1.25 為 59／144／88、DPR 1.0 為 58／142／80；修復前 FAIL（`[60,145,88]` 對 `[59,144,88]`；`[59,143,83]` 對 `[58,142,80]`），修復後 PASS。HUD 幾何稽核 `geometry_audit` 重跑：`claude.m2_label` sizeHint 59×14 與 Python 相同（修復前 60×14）。 |
| G12.8.k | P2, RAN | **qtrs** | Button 的原生路徑判準與原生邊框：QSS 只有 `min-width`（無 padding／border）時 Python `QPushButton` 走原生 hint（81×24 @1.25、98×28 @1.0）、`minimumSize` 含原生邊框（22 vs 18）；qtrs 判準 `padding/border/min_width/max_height` 皆無才走原生，且原生路徑的 hint 是 18×15。HUD 沒有這種按鈕（`LayoutToggleBtn` 有 padding 與 border） |
| G12.8.l | P2, RAN | **qtrs** | `Frame` 無內容時 `size_hint`：Python `QFrame.sizeHint()` 為 (-1,-1)，qtrs 為 100×30（`frame.rs`，`None => Size::new(100, 30)`），`minimumSizeHint` 0×0。HUD 的分隔線 `min == max` 所以不受影響 |
| G12.8.m | P2, RAN | **qtrs** | `Button::minimum_size_hint`：Qt `QPushButton::minimumSizeHint() = sizeHint()`（B1 28×18）；qtrs 回傳 `minimum_size()`（28×0 或 18×0），layout 的最小值判斷（`qSmartMinSize`）會少 18 px |
| G12.8.n | P2, RAN | `Button::size_hint` 以**原始** `max-*`／`min-*` 夾 hint（`button.rs`，`h.min(max_h)`）。Qt **也**夾 hint，但在**內容盒**：`QStyleSheetStyle::sizeFromContents` 開頭 `rule.adjustSize(csz)`（`qstylesheetstyle.cpp:560-571`，先夾 max 再 expand 到 min），之後才 `boxSize`。qtrs 夾的是含 padding／border 的總尺寸，目前剛好遮住文字高度 1 px 的差（B1：qtrs 文字高 15、Python 14，不夾會是 19 vs 18；與 G12.8.j 同族）。（初稿寫成「Qt 的 hint 不夾」是錯的，已更正。） |
| G12.8.o | P2, RAN；已修復：RC-19 follow-up | **qtrs** | `ProgressBar::minimum_size`／`maximum_size` 在垂直方向把 min／max 轉置（`progress_bar.rs`，`Orientation::Vertical => Size::new(h, w)`）；QSS 的 `min-width` 是實體寬度，Qt 不轉置。PySide6 垂直 bar（`min-width: 5px; max-width: 5px`）→ min [5,0]、max [5,∞]；qtrs [0,5]、[∞,5]。RC-19 的測試沒有涵蓋垂直方向，這是它的缺口 |
| G12.8.p | P2, RAN；已修復：RC-22 | **qtrs** | QSS `width`／`height`（內容尺寸，`contentsSize`）被解析成 `min_* = max_* = 值`（`style/stylesheet.rs`），Qt 兩者是不同東西：`width`／`height` 只影響 `sizeFromContents`（`rule.size()`），不設 `minimumSize`／`maximumSize`。PySide6 `QProgressBar { width:120px; height:9px }` → min [0,0]、max [∞,∞]、hint 120×9；qtrs min／max 都是 120×9。**已修復：RC-22**。`[QT-SRC qstylesheetstyle.cpp:2595-2612]` `setGeometry` 只對有 `min-*`／`max-*` 宣告的軸設 `minimumSize`／`maximumSize`，值為 `boxSize(max(width, min-width))`／`boxSize(min(width, max-width))`；`[QT-SRC :561-574]` `adjustSize`、`[:5487-5489]` `CT_ProgressBar` 有 contents size 時為 `rule.size(sz)`。修復：`style/stylesheet.rs` 不再把 `width`／`height` 寫成 min＝max；`min_box_size`／`max_box_size` 依上式；新增 `ResolvedStyle::adjust_size`；`ProgressBar::size_hint` 改走它並在有 `width`／`height` 時取該值；`Button::size_hint` 以 `width`／`height` 取代文字尺寸。測試 `test_progress_bar_size_hint.rs` 新增 2 項（P8／P9 及 `width` 與 `min-width`／`max-width` 並用，PySide6 oracle，DPR 1.25／1.0 相同），修復前皆 FAIL（例：P8 min／max 為 [120,9] 對 [0,0]／[∞,∞]），修復後 PASS。未涵蓋：`Label`／`Frame` 的 `width`／`height` 無 PySide6 oracle（`QLabel::sizeHint` 不經 `sizeFromContents`，本來不受 `width` 影響）；`Button` 的 `width`／`height` 只做 `adjustSize` 的內容尺寸替換，沒有新的 PySide6 oracle。 |
| G12.8.q | P2, RAN | 應用 | 啟動時沒有套用模式的最小尺寸：Python `_apply_ui_mode`／`_apply_cards_layout` 在建構時就 `setMinimumSize`；Rust 只在 `apply_ui_mode_internal`（模式／版面切換）呼叫 `window.set_minimum_size`，`with_providers` 建構後 `minimum_size()` 為 (0,0)，所以剛啟動的視窗可被拖到小於最小值（直到第一次切換）。RC-23 修常數時發現，**未修**（屬套用時機，另算） |

- **修復歸屬**：a–f 屬 HUD（`rust/src`）；g–j 是 qtrs 層，各自是獨立 root cause，不併入 RC-14／15／16，也不得用 HUD 端的數值補償（Contract 規則 7）。
- **本節不代表任何項目已修復。**

- **Triage（只讀原始碼；沒有改任何程式、沒有新增測試）**：

| 項目 | 分類 | 理由／證據 |
|---|---|---|
| a、b、c、d、e | **純 `rust/src` 遷移**，不需要 qtrs RC | Python 的值在 qtrs 上已重現 Python 幾何（V6：橫向 45/45、直向 44/45 widget 吻合）；差異全在 HUD 設的值 |
| RC-15（badge `max-height`） | **純 `rust/src` 遷移**，但有未驗證前提 | 移除後 badge hint 與 Python 相同；DPR 1.0 未量。若保留該屬性，才會碰到 G12.8.g／h |
| f（視窗大小常數） | `rust/src`，**需使用者決定** | 130／152、490 可能是刻意值，沒有找到理由紀錄；改動會影響已存設定的視窗尺寸 |
| g + h → **新 qtrs RC-19** | qtrs | 同一根因：`min/max-width/height` 被 `Label`／`Button`／`Frame`／`ProgressBar` 的 `minimum_size`／`maximum_size`／`size_hint` 當原始長度使用（`label.rs:229-230,236-237,248-249`、`button.rs:332-340`、`frame.rs:565-573`、`progress_bar.rs:399,416-417,430-431`），而 Qt 對 box 模型的規則是 `rule.boxSize()`。`Button::size_hint` 已經用 content box 處理 `min-width`（`test_stylesheet_style.rs:227-232`），所以只有 `size_hint` 一處是對的，其餘不一致 |
| i → **新 qtrs RC-20** | qtrs | `QProgressBar::sizeHint` 是字型度量演算法（`qprogressbar.cpp:396-407`：`max(9,chunk)*7 + advance('0')*4`、`fm.height()+8`，再經 `sizeFromContents(CT_ProgressBar)`）；qtrs 用固定 160 與 QSS 高度。與 RC-19 不同根因（演算法 vs box 模型），但在 `CT_ProgressBar` 分支（`qstylesheetstyle.cpp:5485-5490`）與 RC-19 相鄰；不併入 |
| j → **qtrs RC-21（已修復）** | qtrs | 根因為 `font-family` 清單未解析（見 G12.8.j），不是取整規則；`ceil` 未動 |

- **對 RC-14／15／16 的影響**：g、j 只造成 V7 的殘差（直向 y/h 1 個、x/w 5 個），不阻擋 RC-14／15／16 的 HUD 遷移；HUD 遷移後這些殘差仍會在，且不得用 HUD 端數值補償。

---

## 附錄 A：Gap 總表

共 341 項：D 12、P0 34、P1 148、P2 142、test gap 5（計數含已修復項；標籤含「已修復」者共 62 項：G2.1.a、G3.2.b、G5.1.f、G6.1.a、G6.1.b、G6.2.c、G6.4.a、G6.4.d、G7.2.a、G8.1.a、G8.3.b、G8.3.e、G8.4.a、G8.4.g、G8.5.c、G8.5.d、G8.8.a、G9.1.a、G9.1.b、G9.1.c、G9.2.a、G9.3.c、G9.4.b、G9.5.a、G9.6.a、G10.7.a、G11.1.d、G11.2.a、G11.2.b、G11.2.c、G11.2.i、G11.3.a、G11.4.a、G11.5.a、G11.5.d、G11.8.c、G11.9.a、G12.3.b、G12.5.a、G12.5.b、G12.5.d、G12.5.e、G12.5.f、G12.5.g、G12.5.i、G12.5.j、G12.5.l、G12.5.p、G12.5.q、G12.5.s、G12.5.t、G12.8.a、G12.8.b、G12.8.c、G12.8.d、G12.8.f、G12.8.g、G12.8.h、G12.8.i、G12.8.j、G12.8.o、G12.8.p）。依章節排序。嚴重度與驗證等級見 §0。`D` 項必須附理由，且誤用時可見失敗。P0 項的修復單位見附錄 D（root cause）。

| ID | 嚴重度／驗證 | 摘要 |
|---|---|---|
| G2.1.a | P0, RAN；已修復：RC-01 | `set_parent(owned_child, None)` **銷毀 child** |
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
| G3.2.b | P0, READ；已修復：RC-04 | 目標執行緒沒有已註冊 loop 時 `post_event_to_thread` 回 `false`，呼叫端 |
| G3.2.c | P2, READ | 巢狀 pump |
| G3.2.d | P2 | 無 null receiver 警告 |
| G3.2.e | D | 無 `sendPostedEvents(receiver, type)` 篩選式 flush、`removePostedEvents`、`hasPendingEvents` |
| G3.2.f | P2, READ | RC-04 之後，投遞給「永遠不會建立 loop 的執行緒」的事件會一直留在 pending 佇列直到行程結束 |
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
| G5.1.f | P1, RAN；已修復：RC-11a | 重啟已啟動的 `Timer` 後 `timeout` 不再觸發（重新註冊物件時 liveness 被自己殺死） |
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
| G6.1.a | P0, RAN；已修復：RC-03 | **發射期間被 disconnect 的 slot 仍會執行** |
| G6.1.b | P0, RAN；已修復：RC-02 | **兩個 Signal 的 `ConnectionId` 在全域表 `GLOBAL_CONNECTIONS` 碰撞** |
| G6.1.c | P2, READ | `disconnect_receiver`／`disconnect_all` 不清 `GLOBAL_CONNECTIONS` |
| G6.1.d | P1, READ | 無 `UniqueConnection`、`SingleShotConnection`、signal-to-signal 連線 |
| G6.1.e | D | slot 需 `Fn(&T) + Send + Sync + 'static` |
| G6.1.f | P1, READ | `connect_with_type(Queued, …)` 只儲存 direct dispatcher，`emit` 時被當 direct 呼叫——「Queued」連線若不是用知 |
| G6.2.a | P1, READ | receiver 執行緒在**連線時**擷取，且優先於即時查詢 |
| G6.2.b | P2, READ | 同執行緒 BlockingQueued 直接呼叫 |
| G6.2.c | P0, READ；已修復：RC-04 | 目標執行緒無 loop 時 queued 閉包忽略 `post_event_to_thread` 回傳值 → **queued slot 靜默遺失** |
| G6.2.d | P2, READ | BlockingQueued 無逾時 |
| G6.2.e | P2 | queued 需 `T: Clone + Send + 'static` |
| G6.3.a | P1, READ | 無 emitter id 的 Signal |
| G6.3.b | P2, READ | 無存活檢查 |
| G6.3.c | P2 | queued 與巢狀路徑沒有測試 |
| G6.4.a | P1, RAN；已修復：隨 RC-02 | 受 G6.1.b 影響 |
| G6.4.b | P1, READ | 已排入的 `MetaCall` 不被清除 |
| G6.4.c | P2 | 無 receiver 的閉包連線 |
| G6.4.d | test gap, 已修復：RC-02 | `test_signal_sender_tracking_and_auto_disconnection` |
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
| G7.2.a | P0, READ；已修復：RC-04 | 目標執行緒無已註冊 loop 時回 false／靜默丟 |
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
| G8.1.a | P1, READ | **show／hide 不自動重排**。**已修復：RC-26** |
| G8.1.b | P2, READ | 隱藏 item 的 geometry 被設為 (0,0,0,0) |
| G8.1.c | P1, READ | 無 Show/Hide 事件 |
| G8.2.a | P1, READ | 傳遞、重繪、`EnabledChange`、焦點清除、`:disabled` 全缺 |
| G8.3.a | P1, READ | 無通用 min/max/fixed API |
| G8.3.b | P0, READ；已修復：RC-05 | **`Label.set_size_policy` 被丟棄** |
| G8.3.c | P2, READ | `WidgetBase::set_geometry` 不夾 min/max |
| G8.3.d | P2 | 預設 size_hint 100×30 會讓忘了覆寫的自訂 widget 得到假值 |
| G8.3.e | P1, READ；已修復：RC-29 | `UsageDial`：Python `setMinimumSize(84,84)` |
| G8.4.a | P0, READ；已修復：RC-06 | 無 parent 傳遞 |
| G8.4.b | P1 | 無 tracking 語意 |
| G8.4.c | P1 | 無隱式 grab |
| G8.4.d | P1 | Enter/Leave 非祖先鏈 |
| G8.4.e | P2 | 無 `WA_TransparentForMouseEvents`／`WA_NoMousePropagation` |
| G8.4.f | P1 | 右鍵 `context_menu_cb` 在 release 時觸發，與 widget 是否 accept 無關 |
| G8.4.g | P1；已修復：RC-06 | `Window` 沒有 release／double-click／move handler |
| G8.4.h | P1, READ | RC-06 之後仍存在的滑鼠傳遞限制：(1) `MouseMove` 不沿 parent 傳遞——Qt 的傳遞迴圈對 move 依賴 buttons 狀態與 `hasMouseTr |
| G8.4.i | P2 | `Window::set_mouse_press_handler`／`set_mouse_move_handler`／`set_context_menu_handler` 與新的  |
| G8.5.a | P1, READ | 無繼承比對 |
| G8.5.b | P1, READ | `attributes` 只有 Label |
| G8.5.c | P0, READ；已修復：RC-05 | **樣式變更不重排**：`WidgetBase::set_style_sheet` 只標 dirty |
| G8.5.d | P0, READ；已修復：RC-10 | `Window::set_style_sheet` 是**整個 Application 的** |
| G8.5.e | P1, READ | `:disabled`/`:focus` 不支援 |
| G8.5.f | P1, READ | **QMenu 規則被解析但從不被消費**：`type_name: "QMenu"` 在原始碼中不存在 |
| G8.5.g | P2, READ | `margin-*` 長手寫被解析後在 `apply_declaration` 丟棄 |
| G8.5.h | P2, READ | 父 widget 的 `font` 繼承未實作 |
| G8.5.i | P1, READ | RC-05 之後仍存在的失效傳播限制：(1) `updateGeometry` 只要求**直接 parent** 的 layout 重排 |
| G8.5.j | P2, READ；實測 | `Application::set_font` 只寫 `GLOBAL_FONT`：沒有任何 widget 讀 `Application::font()`，也沒有 `FontChan |
| G8.5.k | P2, READ | HUD 的選單沒有 parent：Rust `Menu::new("")` |
| G8.5.l | P2, READ | `StyleChange` 通知以 `try_borrow` 走訪：`style_changed_below` 跳過目前被 mutably borrow 的 widget |
| G8.6.a | P2 | 無 per-widget `WA_*` 屬性 |
| G8.6.b | P1, READ | `set_stays_on_top` 執行期路徑沒有測試 |
| G8.6.c | P1 | 無 layout 導出的頂層最小尺寸 |
| G8.7.a | P1, READ | 無 child 裁剪 |
| G8.7.b | P2 | 髒區只有整個 widget |
| G8.8.a | P0, READ；已修復：RC-11c + HUD 接線 | Python 在 `provider_card.py:125`、`usage_table.py:318,328,339,373-375`、`hud_window.py:155,16 |
| G8.8.b | P2, READ | 無 `showText` 的 `rect` 參數、`QToolTip::font/palette` |
| G8.8.c | P2, READ | 游標大小固定 16×16 邏輯像素；`QWindowsCursor::size()` 與 `fromNativePixels`（DPR）未建模 |
| G8.8.d | P1, READ | tip 無自動換行、無 rich text |
| G8.8.e | P2, READ | 不送 `ToolTipChange`；`StatusTip`／`WhatsThis`／`Action::tool_tip` 未接 |
| G8.8.f | P2, READ | tip 視窗重用、無淡入淡出、不因啟用狀態改變而隱藏（G11.1.d） |
| G8.8.g | P2, `[INFERENCE]` | 混合 DPI 的 tip 視窗未實測 |
| G9.1.a | P1, READ；已修復：RC-30 | `add_stretch(0)` 被強制成 1 |
| G9.1.b | P1；已修復：RC-31 | 無 `add_spacing`／`add_spacer_item`／`insert_stretch`／`set_stretch_factor` |
| G9.1.c | P1；= G9.2.a；已修復：RC-07 | 無 item 對齊 |
| G9.1.d | P1 | 無 `heightForWidth` |
| G9.1.e | P1 | 無 `retainSizeWhenHidden`、無 RTL |
| G9.2.a | P0, READ；已修復：RC-07 | **無 per-item 對齊** |
| G9.2.b | P1, READ | `Layout::add_widget_with_stretch` 對 grid **靜默忽略 stretch** 並新增一列 |
| G9.2.c | P1 | 無 `setRowStretch`／`setColumnStretch`／`setColumnMinimumWidth` 讀回 |
| G9.2.d | P1, READ | RC-07 之後仍存在的對齊限制：(1) 沒有 `heightForWidth` |
| G9.3.a | P1, INFERENCE | wrapper 是 QWidget item：其 `maximum_size` 為 16777215，而巢狀 `QLayout` 回報其子項最大值之和 |
| G9.3.b | P2 | wrapper 多一個 child widget 進入 hit-test／paint 樹 |
| G9.3.c | P0, READ；已修復：RC-16 | HUD 的 `header_widget` 額外被設為 `Expanding/Fixed` |
| G9.4.a | P1, READ | 依賴 Qt 預設的 layout |
| G9.4.b | P0, 已讀兩側原始碼確認；已修復：RC-14 | **卡片根 layout spacing 不同**：Python `layout.setSpacing(5)` |
| G9.5.a | P1, READ | 無向上傳遞：葉節點的 hint 變更不會爬到祖先 layout。**已修復：RC-28** |
| G9.5.b | P1, READ | `Button::set_text/set_font`、`Label::set_font/set_alignment`、`set_style_sheet`、`set_propert |
| G9.5.c | P2 | setter 立即重排與 Qt 壓縮不同 |
| G9.5.d | P1 | 頂層最小尺寸不從 layout 導出 |
| G9.6.a | P1, READ；決議：不在 P0 階段修，不標 D；已修復：RC-24 | `[QT-SRC qstackedlayout.cpp:417-448]`：Qt 的 `sizeHint` 取**所有頁面**的最大值 **已修復：RC-24**（見 C9.6） |
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
| G10.7.a | P0, READ；已修復：RC-08 implementation complete / real heterogeneous-DPI verification pending | `DpiChanged` 處理把 store 調成 `dpi_x/96`，然後呼叫 `do_render_and_present`，後者又以 `platform().primary |
| G10.7.b | P1, READ | `application_device_pixel_ratio` |
| G10.7.c | P1, READ | `HighDpiScaleFactorRoundingPolicy` 存了但從不讀 |
| G10.7.d | P1, READ | RC-08 之後視窗**內**的換算都用視窗自己的 DPR，但**視窗之間沒有共同的邏輯座標系**：`Window::new` 以主螢幕 DPR 決定原生位置 |
| G10.7.e | P1, READ | 彈出選單的螢幕夾限仍取 `primary_screen().available_geometry()` |
| G10.7.f | P2, READ | `hit_test.rs:321` 的 `DpiChanged` 分支以固定的 `old_dpr = 1.0` 呼叫 `propagate_dpi_change_recursive |
| G10.7.g | P1, READ | 視窗 DPR 只在建立時與 `DpiChanged` |
| G10.7.h | P2, READ | `PlatformWindow::device_pixel_ratio` 只有 Windows |
| G10.7.i | test gap | 沒有異 DPI 實機測試：RC-08 的測試以 fake platform |
| G11.1.a | P2 | 無 `set_window_flags` |
| G11.1.b | P1 | 測試只檢查 `flags` 欄位，不檢查 `WS_EX_TOPMOST`／`WS_EX_TRANSPARENT` |
| G11.1.c | P2, READ | X11／Wayland／Cocoa 後端是模擬，`is_active`／`TOOLTIP` 未對真實系統驗證 |
| G11.1.d | P1, READ | `Application::active_window()` 從不被設定，無 `ActivationChange`／`isActiveWindow`（`is_active` 尚未接到 toolkit 層）。**已修復：RC-27**（`ActivationChange` 等見 C11.1） |
| G11.2.a | P1, READ；已修復：RC-06 | 無 `Window::is_visible()` |
| G11.2.b | P0, READ；已修復：RC-06 | `CloseRequest` 在 `WindowEventHandler` 被 `_ => {}` 吞掉 |
| G11.2.c | P0, READ；已修復：RC-06 | 無 `showEvent/hideEvent/closeEvent` hook：Python 的「show 時重新套用主題」「hide 時 trim_memory」沒有 Rust  |
| G11.2.d | P2 | 無 `Expose` 重繪 |
| G11.2.e | P1, READ | `Application::unregister_window` 在 drop 時、`quit_on_last_window_closed` 為 true 就呼叫 `quit` |
| G11.2.f | P2 | `GuiApplication::set_application_state`、`last_window_closed`、`focus_window_changed` 從不發射 |
| G11.2.g | P1 | `main.rs` 從不 `set quit_on_last_window_closed(false)` |
| G11.2.h | P1, READ | RC-06 之後 `Show`／`Hide` 仍只來自 `Window::show`／`hide`／`close` |
| G11.2.i | P1, RAN；已修復：RC-17b | Windows 原生 move loop 吞掉 release，缺 `handleExitSizeMove` 按鍵同步 |
| G11.3.a | P0, READ；已修復：RC-08 implementation complete / real heterogeneous-DPI verification pending | DPI 混用：WM handler 用 `GetDpiForWindow`，`Window::set_geometry` 用主螢幕 DPR |
| G11.3.b | P2 | 位置是 `i32` 邏輯值 |
| G11.3.c | P1, READ | `NativeWindow::geometry()` 回實體 `GetWindowRect`，`Window::geometry()` 為邏輯 |
| G11.4.a | P0, READ；已修復：RC-13 | Rust HUD 啟動時用 `primary_screen().geometry()` 與 `ensure_within_screen` |
| G11.4.b | P1, READ | `Window` 無 `screen()` |
| G11.4.c | P1, INFERENCE | `Win32Screen::geometry` 以 dpr 除原點 |
| G11.4.d | P2 | `Win32Screen::primary()` 寫死 `MonitorFromPoint(0,0)` |
| G11.4.e | P1, READ | qtrs `Rect::right/bottom/center` 與 `QRect` 差 1（`x+w` vs `x+w-1`） |
| G11.4.f | P2, INFERENCE | 設定載入把 window_x/y 超出 ±範圍者改為 None，Python 沒有 |
| G11.4.g | P2, INFERENCE | 混合 DPI 多螢幕未驗證 |
| G11.5.a | P0（條件式：僅 DComp 可用的機器）, READ；本機 RAN：選到 Layered，未重現；已修復：RC-09 implementation complete / DirectComposition hardware verification pending | **DComp 路徑丟棄 opacity**：`present_dirty_ref(&mut self, pixmap, _opacity, dirty)` |
| G11.5.b | P2 | 非分層視窗 `SetLayeredWindowAttributes` 失敗時靜默 |
| G11.5.c | test gap, RAN；部分處理：RC-09 新增明確標為 ignore 的 DComp 測試，既有 test_dcomp_* 仍靜默 skip | `test_dcomp_*` |
| G11.5.d | P1, RAN；已修復：RC-09 | 非 `LAYERED` 視窗的 `setWindowOpacity` 無效：`dyn PlatformWindow::set_opacity` |
| G11.5.e | test gap | `Win32LayeredPresenter` 把 opacity 傳為 `UpdateLayeredWindowIndirect` 的 `SourceConstantAlpha` |
| G11.5.f | P2, 未量測 | DComp 的 opacity 以 CPU 在 staging DIB 複製時逐像素縮放實作 |
| G11.5.g | P1, 未決策，不在 RC-09 | production 是否允許 DComp |
| G11.6.a | P2 | 非 `LAYERED` 視窗的 `set_click_through(true)` 只得 `WS_EX_TRANSPARENT`，沒有 `WS_EX_LAYERED` 時不穿透 |
| G11.6.b | P1 | 沒有測試斷言樣式位元 |
| G11.7.a | P1, READ | HUD 在視窗可見之前呼叫 `set_backdrop` |
| G11.7.b | P2 | `None` 比 Python 的 `clear` 多做 DWM 呼叫 |
| G11.7.c | P2 | macOS 路徑只對 `MockObjcRuntime` 測過 |
| G11.8.a | P1 | 選單位置換算用主螢幕 DPR |
| G11.8.b | P2 | `show_message` 只收 title／text／4 值圖示 enum／時間，不收自訂 `QIcon` |
| G11.8.c | P0, READ；= G11.9.a 的重複登錄；已修復：RC-12 | Python 的 `hotkey_failed` 訊息 Rust 沒有 |
| G11.8.d | P2 | 圖示：Python 依平台選 `.ico/.icns/.png` |
| G11.8.e | P2, INFERENCE | 雙擊在 Windows 先 Trigger 兩次再 DoubleClick |
| G11.8.f | P2 | DBus／macOS 後端存在但未驗證 |
| G11.9.a | P0, READ；已修復：RC-12 | `HotkeyManager::start` 在 Windows 即使 `RegisterHotKey` 失敗也回 `Ok`，失敗只在執行緒內 `warn!` |
| G11.9.b | P1 | macOS 熱鍵明確未實作 |
| G11.9.c | D | `GenericHotkeyManager`／`CocoaHotkeyManager`／`UnixHotkeyManager` 為 stub，回報成功卻未註冊 |
| G11.9.d | P2 | app 以原子旗標 + `run_on_main_thread` |
| G11.9.e | P2, READ | 穿透鎖定訊息文字與 Python 不同 |
| G11.9.f | P2, READ | 主熱鍵取自設定、穿透熱鍵推導；Python 固定 |
| G11.9.g | P2, INFERENCE | `start` 等回報無逾時 |
| G11.9.h | P2 | 熱鍵失敗氣泡只以 log 煙霧測試驗證 |
| G11.10.a | P2 | 高對比與 `ShouldAppsUseDarkMode` 未驗證 |
| G11.10.b | P2 | `GuiApplication::new` 預設 `Palette::dark()`，不跟隨系統配置 |
| G11.10.c | P2 | `theme.rs` 無單元測試 |
| G11.11.a | P2 | 第二次啟動「喚醒」第一個 HUD 後的可觀察結果 |
| G11.11.b | P2 | 剪貼簿：兩個 HUD 都不用 |
| G11.12.a | P2 | 無視窗圖示 API |
| G12.3.a | P1 | QMenu 規則被解析但不消費 |
| G12.3.b | P0, READ；已修復：RC-15 | Rust 卡片 `QLabel#Badge` 加了 `max-height: 15px` |
| G12.3.c | P1, READ | 表格模式面板：Python 的 `get_hud_stylesheet(theme, vibrant)` 依 `vibrant` 選半透明 `panel` 或 `panel_sol |
| G12.3.d | P1 | 型別比對無繼承 |
| G12.3.e | P2 | `font-family` 清單以一個原始字串存、查找時才拆 |
| G12.3.f | P2, INFERENCE | `font-weight: 800` 映射到 `FontWeight::Black` |
| G12.3.g | P2 | `font-size` px 取整 |
| G12.3.h | P2, INFERENCE | 色彩：8 位數十六進位被當 `#RRGGBBAA` |
| G12.5.a | P0, READ；= G12.3.b；已修復：RC-15 | Badge `max-height: 15px` 只在 Rust |
| G12.5.b | P0, READ；= G9.4.b；已修復：RC-14 | 卡片根 layout spacing 2（Rust）vs 5（Python），無註解說明 |
| G12.5.c | P1 | 面板底色不依 Acrylic 是否成功而改變 |
| G12.5.d | P0, READ；已修復：RC-17（3000 ms geometry poll removed because RC-06 window event lifecycle now supplies Move/Release/Close/Hide hooks） | 幾何持久化：Python 250 ms 單發於 move／resize 重啟＋mouse release 儲存；Rust 原本只有 resize 的 `ResizeDebouncer` |
| G12.5.e | P0, READ；已修復：RC-18（application 層 timestamp-gap detection） | 喚醒偵測（倒數 tick 間隔 >15 s 就刷新）在 Rust 原本不存在 |
| G12.5.x | P2 | `Power::Resume`／`Suspend` 目前沒有 toolkit consumer（Qt 本身亦無此事件；與 RC-18 獨立） |
| G12.5.f | P0, READ；已修復：RC-06 | `Window` 沒有 mouse-release／double-click／move／close 的 handler；雙擊在 Rust 會重新開始視窗移動，Python 是刷新 |
| G12.5.g | P0, READ；= G11.2.b；已修復：RC-06 | Alt+F4 / `CloseRequest` 被吞 |
| G12.5.h | P1 | 單發時序：Python 300 ms（啟動 click-through）、150 ms（hide 後 trim）、1000 ms（busy→idle 後 trim）、2500 ms |
| G12.5.i | P0, READ；= G8.8.a；已修復：HUD tooltip 接線 | 所有 widget tooltip 缺失（錯誤與過期資料以 tooltip 顯示） |
| G12.5.j | P0, READ；= G11.9.a；已修復：RC-12 | 熱鍵註冊失敗不被回報；鎖定防護失效 |
| G12.5.k | P1 | `--smoke-test` 不檢查設定持久化 |
| G12.5.l | P0, READ；= G11.4.a；已修復：RC-13 | 螢幕選擇／脫離螢幕還原規則 |
| G12.5.m | P1 | 托盤選單：Python 的托盤選單沒有鎖定／不透明度／間隔／重設／隱藏等項目；Rust 托盤選單是完整的 context menu |
| G12.5.n | P1 | 托盤通知：Rust 只有「ghost paused」；缺 hotkey 失敗、ghost 啟用、autostart 失敗 |
| G12.5.o | P1 | QMenu 外觀為寫死數值，非 QSS |
| G12.5.p | P1 | `QColor.darker(110)` 缺失：Rust 用原色。**已修復：RC-25** |
| G12.5.q | P1；已修復：RC-29 | `UsageDial` 最小尺寸 0 vs 84 |
| G12.5.r | P1 | 版面切換：`StackedWidget` vs 重建 |
| G12.5.s | P0, READ；= G8.5.d；已修復：RC-10 | `Window::set_style_sheet` 為 app 全域 |
| G12.5.t | P0, READ；= G11.5.a；已修復：RC-09 implementation complete / DirectComposition hardware verification pending | DComp 路徑 opacity 無效（待實測） |
| G12.6.a | P1, RAN | 6 項 Windows 字型版面測試只在 Windows 執行 |
| G12.6.b | P2, RAN | qtrs workspace 測試在非 Windows 無法編譯，不在 CI |
| G12.6.c | P2 | 測試序列化靠環境變數；全域 DPR 仍是行程全域 |
| G12.6.d | P1, RAN | 字型來源與缺字型替換規則與 Qt（fontconfig／CoreText／GDI）不同 |
| G12.5.u | P2 | 色彩／字型解析細節 |
| G12.5.v | P1 | 發佈 profile `panic = "abort"` vs Python excepthook |
| G12.5.w | P2 | `rust/README.md` 仍描述 egui/eframe/reqwest |
| G12.8.a | P1, RAN；已修復：RC-14 | 指標值字級：Python widget-local `font-size: 14px`；Rust 只有 `set_font(14)`，被 app sheet `QLabel#MetricValue { font-size: 16px }` 蓋過（符合 Qt：樣式表字級勝過 `setFont`），有效字級 16，`sizeHint` 高度 19 vs 17。RC-14 的第一個分歧 |
| G12.8.b | P1, RAN；已修復：RC-14 | 橫向 body spacing：Python 8（`hud_window.py:337`），Rust 預設 6；卡片寬 213 vs 211 |
| G12.8.c | P1, RAN；已修復：RC-16 | 直向容器 policy／stretch：Python 不設；Rust `stack`／`cards_container` `Expanding`、根 stretch 1、卡片 stretch。RC-16 的延伸 |
| G12.8.d | P2, RAN；已修復：RC-16 | `title` size policy：Python `Minimum/Preferred`（`provider_card.py:34`），Rust 預設 |
| G12.8.e | P2, RAN | badge 字重：Python 400；Rust Bold（`set_font(...Bold)`，QSS 沒有字重） |
| G12.8.f | P1, RAN；已修復：RC-23 | 視窗大小常數：橫向最小／預設高 Python 125／145，Rust 130／152；直向最小 Python 320、預設 410，Rust 463（由 layout 推導）／490。**已修復：RC-23**。Python 來源 `hud_window.py:31-35`（`MIN_HORIZ 540×125`、`DEF_HORIZ 690×145`、`MIN_VERT 250×320`、`DEF_VERT 280×410`）與 `config_manager.py:15-17`；`_apply_cards_layout`（`:325-354`）每次 `setMinimumSize(MIN)`，存檔值低於 MIN 才退回 DEF（不是夾到 MIN）。PySide6 實測（新 config、DPR 1.25）：橫向 `size` 690×145、`minimumSize` 540×125；直向 280×410、250×320；layout 自身最小值 435×151／265×395，**被明確的 `setMinimumSize` 蓋過**（視窗可小於內容需求）。根因＝app 常數（`config.rs`）：Rust 把直向最小高由 layout 內容推導（463，`vertical_layout_min_height()`），預設 152／490 為自訂值；`sanitize` 與 `reset_geometry` 用同一組常數，所以 Python 保留的存檔尺寸（橫向 125–129、直向 320–462）被 Rust 重設。修復：`config.rs` 常數改為 Python 值（125／145、320／410），刪除推導用的 `vertical_layout_min_height()` 與其 6 個專用常數。未改 qtrs。測試（修改前 FAIL）：`config::tests::test_config_defaults`（490 vs 410）、`test_sanitize_keeps_stored_sizes_pyside6_keeps`（152,490 vs 125,320）、`hud_window::tests::test_window_default_and_minimum_sizes_match_pyside6`（690×152 vs 690×145）。已存的 geometry：≥ 新最小的值全部保留；只有「舊 Rust 預設 152／490 以外且 < 新最小」才會被重設，而新最小比舊的低，所以沒有原本有效的存檔值變成無效。 |
| G12.8.g | P1, RAN；已修復：RC-19 | QSS `min/max-width/height` 盒模型：Qt 作用於 content＋padding＋border（`qstylesheetstyle.cpp:2603-2611`）；qtrs 當總尺寸。`layout_toggle_btn` 最大高 Python 22 vs Rust 18、最小寬 28 vs 18 |
| G12.8.h | P1, READ；已修復：RC-19 | `Label::size_hint` 以 `max-height`（否則 `min-height`）當高度 hint（`label.rs`），Qt 沒有此規則 |
| G12.8.i | P1, RAN | **已修復：RC-20**（預設字型的數值差屬 Application font，未處理）。`QProgressBar`：Python `sizeHint` 91×5、`minimumSizeHint` 91×17；qtrs 160×5、0×5 |
| G12.8.j | P1, RAN；已修復：RC-21 | 文字寬度 1 px：`WEEKLY 7D` Python 59，qtrs 59.589 → `ceil` 60；`AI AGENT HUD (3-IN-1)` 144 vs 144.107 → 145。**已修復：RC-21**。根因**不是取整規則**：`QLabel` 的 `font-family: 'Segoe UI', 'SF Pro Display', 'Microsoft JhengHei', sans-serif` 是字型家族清單，qtrs 的 QSS 解析把整串當成單一家族名，找不到而退回別的字型（量到 59.589／144.107）。`[QT-SRC qcssparser.cpp:1252-1272]` `setFontFamilyFromValues` 以逗號切開並呼叫 `QFont::setFamilies`，字型庫依序取第一個已安裝的家族（Segoe UI）。整數轉換本身 Qt 與 qtrs 一致：`QLabelPrivate::sizeForWidth` 走 `fm.boundingRect(...)`→`rb.toAlignedRect()`（`qlabel.cpp:609`、`qfontmetrics.cpp:735-749`），qtrs 對寬度取 `ceil`。`ceil` 未改。修復：`QCssValue::FontFamilies`（解析器）、`ResolvedStyle::font_families`／`font_family()`（取第一個已安裝家族，否則第一個）。測試 `qtrs-widgets/tests/test_label_text_metric_rounding.rs`（HUD 實際樣式表，PySide6 oracle）：DPR 1.25 為 59／144／88、DPR 1.0 為 58／142／80；修復前 FAIL（`[60,145,88]` 對 `[59,144,88]`；`[59,143,83]` 對 `[58,142,80]`），修復後 PASS。HUD 幾何稽核 `geometry_audit` 重跑：`claude.m2_label` sizeHint 59×14 與 Python 相同（修復前 60×14）。 |
| G12.8.k | P2, RAN | Button 的原生路徑判準與原生邊框：QSS 只有 `min-width`（無 padding／border）時 Python `QPushButton` 走原生 hint（81×24 @1.25、98×28 @1.0）、`minimumSize` 含原生邊框（22 vs 18）；qtrs 判準 `padding/border/min_width/max_height` 皆無才走原生，且原生路徑的 hint 是 18×15。HUD 沒有這種按鈕（`LayoutToggleBtn` 有 padding 與 border） |
| G12.8.l | P2, RAN | `Frame` 無內容時 `size_hint`：Python `QFrame.sizeHint()` 為 (-1,-1)，qtrs 為 100×30（`frame.rs`，`None => Size::new(100, 30)`），`minimumSizeHint` 0×0。HUD 的分隔線 `min == max` 所以不受影響 |
| G12.8.m | P2, RAN | `Button::minimum_size_hint`：Qt `QPushButton::minimumSizeHint() = sizeHint()`（B1 28×18）；qtrs 回傳 `minimum_size()`（28×0 或 18×0），layout 的最小值判斷（`qSmartMinSize`）會少 18 px |
| G12.8.n | P2, RAN | `Button::size_hint` 以**原始** `max-*`／`min-*` 夾 hint（`button.rs`，`h.min(max_h)`）：Qt 的 hint 不夾（夾的是 `QWidgetItem`，用 box 後的 min/max）。這個夾制目前遮住文字高度 1 px 的差（B1：qtrs 文字高 15、Python 14，hint 19 vs 18；與 G12.8.j 同族）；移除夾制會讓 HUD 標頭多 1 px，所以未動 |
| G12.8.o | P2, RAN；已修復：RC-19 follow-up | `ProgressBar::minimum_size`／`maximum_size` 在垂直方向把 min／max 轉置（`progress_bar.rs`，`Orientation::Vertical => Size::new(h, w)`）；QSS 的 `min-width` 是實體寬度，Qt 不轉置。PySide6 垂直 bar（`min-width: 5px; max-width: 5px`）→ min [5,0]、max [5,∞]；qtrs [0,5]、[∞,5]。RC-19 的測試沒有涵蓋垂直方向，這是它的缺口 |
| G12.8.p | P2, RAN；已修復：RC-22 | QSS `width`／`height`（內容尺寸，`contentsSize`）被解析成 `min_* = max_* = 值`（`style/stylesheet.rs`），Qt 兩者是不同東西：`width`／`height` 只影響 `sizeFromContents`（`rule.size()`），不設 `minimumSize`／`maximumSize`。PySide6 `QProgressBar { width:120px; height:9px }` → min [0,0]、max [∞,∞]、hint 120×9；qtrs min／max 都是 120×9。**已修復：RC-22**。`[QT-SRC qstylesheetstyle.cpp:2595-2612]` `setGeometry` 只對有 `min-*`／`max-*` 宣告的軸設 `minimumSize`／`maximumSize`，值為 `boxSize(max(width, min-width))`／`boxSize(min(width, max-width))`；`[QT-SRC :561-574]` `adjustSize`、`[:5487-5489]` `CT_ProgressBar` 有 contents size 時為 `rule.size(sz)`。修復：`style/stylesheet.rs` 不再把 `width`／`height` 寫成 min＝max；`min_box_size`／`max_box_size` 依上式；新增 `ResolvedStyle::adjust_size`；`ProgressBar::size_hint` 改走它並在有 `width`／`height` 時取該值；`Button::size_hint` 以 `width`／`height` 取代文字尺寸。測試 `test_progress_bar_size_hint.rs` 新增 2 項（P8／P9 及 `width` 與 `min-width`／`max-width` 並用，PySide6 oracle，DPR 1.25／1.0 相同），修復前皆 FAIL（例：P8 min／max 為 [120,9] 對 [0,0]／[∞,∞]），修復後 PASS。未涵蓋：`Label`／`Frame` 的 `width`／`height` 無 PySide6 oracle（`QLabel::sizeHint` 不經 `sizeFromContents`，本來不受 `width` 影響）；`Button` 的 `width`／`height` 只做 `adjustSize` 的內容尺寸替換，沒有新的 PySide6 oracle。 |
| G12.8.q | P2, RAN | 啟動時沒有套用模式的最小尺寸：Python `_apply_ui_mode`／`_apply_cards_layout` 在建構時就 `setMinimumSize`；Rust 只在 `apply_ui_mode_internal`（模式／版面切換）呼叫 `window.set_minimum_size`，`with_providers` 建構後 `minimum_size()` 為 (0,0)，所以剛啟動的視窗可被拖到小於最小值（直到第一次切換）。RC-23 修常數時發現，**未修**（屬套用時機，另算） |

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
- `ConnectionId` 由每個 Signal 自己的 `next_id` 編號、全域表以 id 為 key（`signal.rs`）。（RC-02 已修復：現為全域唯一 id。）
- `set_parent` 把舊 parent 的 owned `Box` 搬進區域變數（`qobject.rs`；RC-01 之前）。
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
- **Required test**（`qtrs-core/tests/test_owned_child_ownership.rs`，7 項）：
  - 修改前 FAIL（以舊程式跑出：`set_parent(None destroyed an owned child)`）、修改後 PASS：`set_parent_none_on_owned_child_keeps_child_alive`。
  - 新 API 的行為測試（舊程式沒有 `reparent_owned`，無法在舊程式上跑，不屬於 before-FAIL 證據）：`reparent_owned_to_none_returns_the_box_and_child_survives`、`reparent_owned_moves_ownership_to_a_new_parent`、`reparent_owned_notifies_both_parents_through_event_filters`、`reparent_owned_rejects_self_and_descendant_parents_without_changes`、`reparent_owned_rejects_children_that_are_not_owned`、`reparent_owned_fails_visibly_when_a_party_is_borrowed_and_changes_nothing`。
- **Downstream**：任何把 child 從容器移出再重新掛接的 widget／action 操作。HUD 目前沒有呼叫（`READ`），因此屬靜默資料遺失型 P0，不是 HUD 可見型。
- **Impact analysis（Phase 1，未決定 API，未改程式）**：
  - 所有權模型 `[READ]`：core 的擁有者是 `ObjectData.owned_children: Vec<Box<dyn QObject>>`（parent 持有 `Box`）。`GLOBAL_OBJECT_REGISTRY` 的 `Arc<ObjectRecord>` 只存 metadata（parent／children id、liveness、borrow flag），`QOBJECT_REGISTRY` 存 `*mut dyn QObject` 裸指標，兩者都**不擁有**物件。`Box` 移動不改堆位址，因此搬移 `Box` 時 registry 指標仍有效。
  - widget 層是另一套模型：`WidgetRef = Rc<RefCell<Box<dyn Widget>>>`、parent 為 `Weak`、`WidgetBase.children: Vec<WidgetRef>`；widgets／gui／platform(Windows)／`rust/src` 都不寫 core 的 `ObjectData.parent/children/owned_children`，也不呼叫 `set_parent`／`add_owned_child`／`remove_owned_child`。
  - 呼叫端（全工作區 grep）：`set_parent` 只有 `ObjectData::set_parent` 包裝（`qobject.rs:402`）與 5 處測試——`qobject.rs:1240/1245/1250`、`test_qobject_lifecycle_and_hierarchy.rs:186/191`、`test_qobject_safety_and_qt6_features.rs:97`。其中只有 `:97` 的 child 在 parent 的 `owned_children` 中；其餘 child 不是 owned，不受 G2.1.a 影響。生產程式零呼叫端。`remove_owned_child` 零呼叫端，且只改 `ObjectData.children`，不更新 `ObjectRecord`、不送 `ChildRemoved`。
  - 結構性問題：`set_parent(child: &mut ObjectData, …)` 的 `child` 參數本身就是從 parent 的 `owned_children` 內部取得的借用（`:97` 即如此）。函式內再經 registry 指標取得 parent 的 `&mut`，與呼叫者手上的借用別名重疊；若把 `Box` 丟棄，呼叫者的 `&mut` 立即懸空。回傳 `Box` 也不能讓這個借用變合法。
  - API 候選 (1)(2)(3) 已評估，**選定 (2)**（使用者決定）。
- **Status**：**已修復**（RC-01）。
  - `ObjectData` 新增 `owned_by_parent`（`is_owned_by_parent()`）：`add_owned_child` 設為 true；`reparent_owned` 依目標更新。因為 `set_parent(&mut ObjectData)` 的借用就在擁有它的 `Box` 裡，必須由 child 自己帶著「被 parent 擁有」的事實，不能靠再借用 parent 去查。
  - `set_parent(child, new_parent) -> Result<(), ReparentError>`：對 owned child 在任何變更前回 `Err(OwnedByParent)`；非 owned child 行為不變（移除了舊的 `transferred` 分支）。
  - `reparent_owned(child_id, new_parent) -> Result<Option<Box<dyn QObject>>, ReparentError>`：在改動前取得 child／舊 parent／新 parent 的借用旗標並驗證，因此 `Err` 不留下半完成狀態。`Err`：`UnknownObject`、`NotOwned`、`InvalidParent`（自己、後代、未註冊）、`Busy`（任一方正在 callback 中——回報而非靜默略過）。child 的借用旗標只當旗標用、不解參考，避免 `&mut` 與正在搬移的 `Box` 別名。成功時 `ChildRemoved`／`ChildAdded` 走 `notify_helper`（object／application filter 看得到）。
  - 移除 `remove_owned_child`（零呼叫端，且不更新 `ObjectRecord`、不送事件）；5 處測試呼叫端已遷移（`test_reparent_transfers_ownership_without_split_brain` 改用 `reparent_owned`）。
  - 驗證：`qtrs` workspace exit 0、65 個 test binary ok、無 warning；主 crate 67 通過、1 失敗（僅 `test_fetch_usage_live_benchmark`，網路／時間敏感，已知）。
  - **未涵蓋／仍開放**：G2.1.d（`set_parent` 非 owned 路徑與 `ObjectData::drop` 在 parent 被借用時仍靜默略過）、G2.1.c（`set_parent` 非 owned 路徑仍用 `dispatch_to_object` 而非 `notify_helper`）、G2.1.e 只在 `reparent_owned` 內檢查環，`set_parent` 仍無環檢查。`reparent_owned` 的「舊 parent 被借用」分支沒有專屬測試。
- **Can remove app workaround**：n/a。
- **Phase**：1。

#### RC-02 Connection 身分：`ConnectionId` 在全域表碰撞
- **Contract gaps**：G6.1.b（P0, RAN）；後果 G6.4.a（P1, RAN）、G6.1.c（stale 記錄，P2）。
- **Qt behavior** `[QT-SRC qobject.cpp:1046-1180]`：連線屬於 sender 的連線串列；`~QObject` 對**所有**以該物件為 receiver 的連線斷線（`senders` 鏈）。Qt 沒有全域的 id → 連線表。
- **qtrs root**：`qtrs-core/src/signal/signal.rs`：每個 `Signal` 自己的 `next_id` 從 1 編號，卻共用以 id 為 key 的 `GLOBAL_CONNECTIONS`。
- **Evidence**：`RAN`。兩個 `Signal<i32>` 各以 `connect_to` 接一個 receiver，`id_a=1 id_b=1`；銷毀 receiver 1 後 `a.emit` 仍呼叫 slot（對照組：單一 Signal 正確）。
- **Required observable**：receiver 銷毀後，**任何** Signal 都不得再呼叫它的 slot；`disconnect(id)` 只影響該 Signal 的該連線。
- **Required test**（`qtrs-core/tests/test_signal_connection_identity.rs`，修改前 3 項全部 FAIL、修改後 PASS）：`receiver_destroyed_disconnects_from_every_signal`；`disconnect_with_another_signals_id_leaves_that_connection_intact`；`disconnect_removes_only_its_own_connection`。另已**改寫** `test_signal_sender_tracking_and_auto_disconnection`，drop 後斷言 `recv_count` 維持 1（§0 規則 4）；該測試只用單一 Signal，修改前就會通過，**不算** RC-02 的先失敗證據。
- **Downstream**：G6.4.a；G6.1.c 一併檢查。HUD 的 signal 連線都是閉包、沒有 receiver 物件（`READ`），因此是 framework P0 而非 HUD 可見型。
- **Can remove app workaround**：n/a。
- **Phase**：1。
- **Status**：**已修復**（RC-02）。修改：所有 Signal 以 `ConnectionId::next()`（全域原子計數）取 id，移除各 Signal 的 `next_id`；`Signal::disconnect` 只有在確實移除自己的連線時才 `unregister_connection`；刪除 `emit` 內永遠不成立的 `sub.id.0 >= highest_id` 檢查（快照與 `next_id` 在同一把鎖內取得）。驗證：`cargo test -j 1 --workspace -- --test-threads=1`（`qtrs`，62 個 test binary 全部 ok）；主 crate 67 通過、1 失敗（`providers::agy::tests::test_fetch_usage_live_benchmark`，需網路，預期失敗）。**未修**：G6.1.c（洩漏，P2）；G6.1.a 已由 RC-03 修復。

#### RC-03 發射快照的有效性：發射中被 disconnect 的 slot 仍會執行
- **Contract gaps**：G6.1.a（P0, RAN）。
- **Qt behavior** `[QT-SRC qobject.cpp:4269 doActivate; :4330 每次迭代檢查 receiver]`：發射沿連線串列走，**每個連線在呼叫前重新檢查 `receiver`**；發射中被斷開的連線（receiver 已被清為 null）不會被呼叫。
- **qtrs root**：`signal.rs` emit：先複製 slot 快照，呼叫前不再確認連線仍有效。
- **Evidence**：`RAN`（slot A 在發射中 disconnect slot B → B 仍被呼叫 1 次）。
- **Required observable**：發射開始後被 `disconnect` 的連線，在輪到它時不得執行；發射期間**新增**的連線不執行本次發射（Qt 同）。
- **Required test**（`qtrs-core/tests/test_signal_emit_while_mutating.rs`，7 項；修改前 5 項 FAIL、2 項 PASS）：
  - 修改前 FAIL、修改後 PASS：`slot_disconnected_during_emit_is_not_called`、`disconnect_all_during_emit_stops_the_remaining_slots`、`disconnect_receiver_during_emit_stops_that_receivers_slots`、`scoped_connection_dropped_during_emit_stops_its_slot`、`receiver_destroyed_during_emit_is_not_called`。
  - 守護測試（修改前就 PASS，只釘住既有行為）：`slot_connected_during_emit_does_not_run_in_that_emit`、`slot_disconnecting_itself_still_finishes_and_later_slots_run`。
- **Downstream**：RC-02（同一檔案，需同時確認連線有效性的判斷方式）。
- **Phase**：1（RC-02 之後，同一檔案）。
- **Status**：**已修復**（RC-03）。修改：每個 `Subscriber` 帶 `connected: Arc<AtomicBool>`；所有移除路徑（`disconnect`、`disconnect_receiver`、`disconnect_all`、`ScopedConnection`、`disconnect_all_for_object` 的 disconnect_fn，共 7 處）改走 `SignalInner::remove_where`，移除時清除旗標；`emit` 在每次呼叫前檢查旗標（對應 `doActivate` 每次迭代重新檢查 `receiver`）。無新鎖、無額外查表；每個連線多一個 `Arc<AtomicBool>` 配置。驗證：`qtrs` workspace `cargo test -j 1 --workspace -- --test-threads=1` exit 0，63 個 test binary 全部 ok；主 crate 68 通過、0 失敗（`test_fetch_usage_live_benchmark` 本次通過，屬網路／時間敏感測試）。**未涵蓋**：跨執行緒同時 disconnect 與 emit 只有旗標的 Acquire/Release 保證，無專門測試。

#### RC-04 事件投遞保證：目標執行緒尚無 loop 時事件被丟棄
- **Contract gaps**：G3.2.b、G6.2.c、G7.2.a（皆 P0, READ，**同一根因**）。相關 P1：G5.5.b、G6.2.d。**降級項**：G7.9.a（P2，見 D.4）。
- **Qt behavior** `[QT-SRC qcoreapplication.cpp:1658-1704, 1694]`：`postEvent` 把事件加入**接收者所屬執行緒**的 `postEventList`；該執行緒是否已有 event dispatcher 無關，事件先排隊，之後被處理。
- **qtrs root**：`qtrs-core/src/event_loop/loop.rs:621-632` `post_event_to_thread` 在 `THREAD_EVENT_HANDLES` 沒有該執行緒時回 `false`；呼叫端忽略回傳值：`signal.rs:522-524,629-631`、`widget.rs:296`、`timer.rs:402,610`（其中 `window.rs:153-163` 會檢查並退回同步渲染）。
- **Evidence**：`READ`。HUD 目前啟動順序在 worker 產生前已註冊 loop（`main.rs:319` → `application/mod.rs:120`），因此**不是 HUD 可見型**。
- **Required observable**：在執行緒的 loop 註冊之前 post 的事件，不遺失，於 loop 開始處理後送達，且保持 FIFO／優先序。
- **Required test**（`qtrs-core/tests/test_post_before_loop.rs`，6 項；修改前 6 項全 FAIL，修改後全 PASS）：`post_before_loop_exists_is_delivered_when_loop_starts`；`queued_signal_emitted_before_target_loop_exists_is_delivered`；`single_shot_zero_before_loop_runs_after_earlier_posted_events`；`events_posted_before_loop_keep_priority_and_fifo_order`；`events_posted_after_loop_exists_follow_the_buffered_ones`；`core_application_post_event_before_receiver_thread_has_loop_is_delivered`。修改前的 FAIL 是以「保留舊的丟棄語意、只加上新函式名 `post_event_to_thread_with_priority`」的版本跑出，測試檔與最終版相同。
- **Downstream**：`Window::queue_render` 的「無 loop 就同步渲染」fallback 是否仍需要；G5.5.b、G6.2.d。
- **Can remove app workaround**：`window.rs:141-144` 的同步渲染 fallback — **未確定**，須在 RC-04 完成後檢查渲染是否仍能在無 loop 時（`--snapshot`、`--smoke-test` 路徑）運作。
- **Status**：**已修復**（RC-04）。`loop.rs`：註冊表改為單一 `Mutex<ThreadEventRegistry { handles, pending }>`；`post_event_to_thread[_with_priority]` 在沒有該執行緒的 loop 時把事件放進 `pending`（一個 `EventQueue`，沿用優先序與壓縮規則）；`register_thread_event_loop` 在同一把鎖內依序倒入新 loop 的佇列，所以併發的 poster 不會插隊。`post_event_to_thread` 不再回傳 `bool`（回傳值原本被所有呼叫端忽略，只有 `window.rs` 使用）。`CoreApplication::post_event_with_priority` 改走同一路徑。
- **Cutover 影響**：`window.rs::queue_render` 原本靠 `post_event_to_thread` 的 `false` 判斷「沒有 loop → 同步渲染」；改為先以 `get_thread_event_sender(ThreadId::current()).is_some()` 明確詢問，行為不變（無 loop 時仍同步渲染，不會留下過期的 deferred render）。此 fallback **仍需要**，因為 `--snapshot`／`--smoke-test` 沒有 loop。
- **驗證**：`qtrs` workspace exit 0、64 個 test binary ok；主 crate 68 通過、0 失敗。
- **新增已知缺口**：G3.2.f（從不建 loop 的執行緒的 pending 事件不會釋放）；G6.2.d 行為改變（見該條）。
- **Phase**：1（只動 `qtrs-core`，與 RC-01～03 彼此獨立）。

#### RC-05 Widget 失效協定：樣式／字型／尺寸策略變更不更新也不重排
- **Contract gaps**：G8.5.c、G8.3.b（P0, READ）。同類 P1/P2：`Widget` trait 預設 `set_size_policy`、`set_style_sheet`、`set_property` 為靜默 no-op（`widget.rs:74,205,211`；`Label`、`ScrollBar`、`ScrollArea` 未轉發 `set_size_policy`）。
- **Qt behavior** `[QT-SRC qwidget.cpp:9502-9510]`：`FontChange`／`StyleChange` 的處理做 `update(); updateGeometry(); layout->invalidate();`。`updateGeometry` `[QT-SRC qwidget.cpp:10571-10587]`：頂層視窗不做事，否則使 parent layout 失效，或對可見的 parent post `LayoutRequest`。`setStyleSheet` 經 `repolish` `[QT-SRC qwidget.cpp:2594-2632; qstylesheetstyle.cpp:2978-2998]` 觸發 `StyleChange`。
- **qtrs root**：`qtrs-widgets/src/widget.rs:366-375` `WidgetBase::set_style_sheet` 只寫 `dirty`，不 post `UpdateRequest`、不要求 layout；`label.rs` 未覆寫 `set_size_policy`。
- **Evidence**：`READ`。
- **Required observable**：不呼叫任何手動 `update_layout()`／`render_and_present()`，在樣式字級改變、size policy 改變後，經過一次事件 pump，layout 與繪製的結果與 Qt 一致。
- **Required test**（`qtrs-widgets/tests/test_widget_invalidation.rs`，9 項；修改前 9 項 FAIL，修改後 9 項 PASS）：
  - `style_sheet_font_size_change_relayouts_parent_after_one_pump`（Button，隔離 style sheet 路徑）、`label_set_size_policy_changes_layout_result`、`label_set_font_relayouts_parent_after_one_pump`、`button_set_text_relayouts_parent_after_one_pump`：真實 `Window`＋`EventLoop`，每次改動後只 pump，不呼叫 `update_layout`／`render_and_present`。
  - `set_size_policy_on_every_widget_type_is_not_silently_dropped`、`set_style_sheet_on_every_widget_type_is_not_silently_dropped`、`set_property_on_every_widget_type_can_be_read_back`：涵蓋 10 個 widget 型別。
  - `changing_the_size_policy_requests_a_parent_layout_and_an_unchanged_one_does_not`。
  - `repolish_after_set_property_restyles_and_relayouts_after_one_pump`：新 API `repolish()`，舊程式沒有，無法在舊程式上跑（舊版 `set_property` 測試版本在修改前 FAIL）。
  - 期望值對照 PySide6（`QHBoxLayout`，400 寬，spacing 6，兩個 `QLabel`）：`Fixed` 的 label 寬度＝size hint，另一個＝`400 - hint - 6`；`Expanding`／`Fixed` 互換後相反；`font-size: 28px` 的 `Fixed` label 寬 77＝hint，旁邊 x＝83。Qt 在一次 `processEvents` 後即達成。
  - Contract 先前寫「`QObject::setProperty` 會觸發 repolish」**是錯的**：Qt 的 `QWidget`／`QApplication` 忽略 `DynamicPropertyChange`（`qwidget.cpp:9449`、`qapplication.cpp:2594`），Python HUD 因此在 `setProperty` 後手動 `style().unpolish/polish`（`usage_table.py:258-260`）。qtrs 照 Qt：`set_property` 只儲存，另有 `repolish()`。
- **Downstream**：HUD 的 6 處 `update_layout()`（`hud_window.rs:636,737`、`provider_card.rs:435,707`、`usage_table.rs:1498,1559`）、1 處手動 `LayoutScheduler`（`usage_table.rs:1594-1595`）、約 8 處 `render_and_present()`。
- **Can remove app workaround**：**是**，但只能在 RC-05 的測試通過**之後**逐一刪除，每刪一處重跑 HUD 快照與 layout harness；不得先刪。
- **Status**：**已修復**（RC-05）。
  - **根因**：失效協定散落在各 setter，且 `Widget` trait 對 `set_size_policy`／`set_style_sheet`／`set_property` 提供靜默 no-op 預設，各型別各自轉發（或忘了轉發；`ProgressBar` 的巨集甚至直接 `.set()` 繞過 base）。
  - **修改**：`Widget` 新增必要方法 `widget_base() -> &WidgetBase`；`size_policy`／`set_size_policy`／`style_sheet`／`set_style_sheet`／`set_property`／`property` 改為經它的提供方法，並**刪除**逐型別轉發（`Button`、`Frame`、`KeySequenceEdit`、`Label`、`Menu`、`StackedWidget`、`EmptyWidget`、`ProgressBar` 所用巨集，以及 `layout_probe` 與 HUD 3 個 widget 的 `size_policy`），另新增 `update_geometry()`、`repolish()`。實作者不可能再靜默丟棄這些設定（缺 `widget_base` 即編譯失敗）。
  - `WidgetBase::set_size_policy`：未變更則什麼都不做（對應 `QWidget::setSizePolicy`），否則 `update_geometry()`。`update_geometry()`：要求 parent layout 重排，並配一次 `update()`——qtrs 沒有 `LayoutRequest` 事件，佇列中的 layout 請求要靠下一次 render 的 `flush_layouts` 送達，沒有 `update()` 就沒有 render（這是測試第一次跑時 `label_set_size_policy…` 仍失敗所發現的）。`style_changed()`＝`update_geometry()`＋自身 layout 標 dirty（對應 `FontChange`／`StyleChange`）；`set_style_sheet` 呼叫它。
  - 影響 size hint 的 setter 補上 `update_geometry()`：`Label::set_text/set_font/set_alignment`、`Button::set_text/set_font/set_action`、`ProgressBar::set_font`。
  - 副帶修正：trait 的 `property()` 原本恆回 `None`（預設實作，無人覆寫），現在回傳 `WidgetBase` 儲存的值，簽名改為 `Option<String>`（零呼叫端）；`layout_probe` 範例的 policy 改存在 base。
  - **驗證**：`qtrs` workspace exit 0、66 個 test binary ok；主 crate 68 通過、0 失敗（含 `test_fetch_usage_live_benchmark`，本次通過）。HUD `--snapshot` 四張圖修改前後比對：差異像素全部落在「同一版本連跑兩次本來就不同」的區域（時間／倒數文字，遮罩外差異 0 像素），`hud_context_menu.png` 完全相同。遮罩區域內的差異無法以此方法排除，**未做逐像素零差異驗證**。
  - **未做**：沒有刪除任何 HUD workaround（`update_layout()`×6、手動 `LayoutScheduler`、`render_and_present()`）。這些多半與文字／可見性／幾何有關，不是 style／font／size policy；依本條規定須逐一刪除並重跑 HUD 快照與 layout harness，另行處理。HUD 的 `set_state`（`usage_table.rs:879`）仍只呼叫 `set_property`，Python 對應處有 `unpolish/polish`，應改呼叫 `repolish()`。
  - **仍開放**：G8.5.i（失效傳播限制；root 與 app 範圍的部分已由 RC-10 處理）；G8.5.d 已由 RC-10 修復；`Menu::set_font`、`ScrollBar`／`ScrollArea` 的 style 尚未驗證 size hint 相依。
- **Phase**：2。

#### RC-06 事件翻譯與傳遞：accept／ignore／冒泡，及 Close／Show／Hide／Move／DblClick
- **Contract gaps**：G8.4.a、G11.2.b（= G12.5.g）、G11.2.c、G12.5.f（P0, READ）。是 RC-17、RC-18 的前提。
- **Qt behavior** `[QT-SRC qapplication.cpp:2689-2762]`：滑鼠事件沿 parent 鏈送，直到某個 widget accept、碰到頂層視窗，或碰到 `WA_NoMousePropagation`。`Close` 由 `QWidgetWindow::closeEvent`（`qwidgetwindow.cpp:883`）轉為 widget 的 `closeEvent`，可 `ignore()` 取消。`close_helper` 的隱藏細節本次**未讀**，實作前須讀。
- **qtrs root**：`qtrs-widgets/src/hit_test.rs` 無 accept／冒泡；`qtrs-widgets/src/window.rs:990-1250` 的 `WindowSystemEvent` 處理沒有 `CloseRequest`、`Power`、雙擊、`Move` 的 arm；`EventKind` 已有 `Close`、`Show`、`Hide`、`Move`、`MouseButtonDblClick`（`event/mod.rs:68,85,236,242`）且 `Event` 有 `accepted`（`:754`），平台層也已產生 `CloseRequest`／`Power`（`qtrs-platform window.rs:525,1069`）。缺的是**翻譯**與**冒泡規則**。
- **Evidence**：`READ`。
- **Required observable**：子 widget 不 accept 的滑鼠事件到達 parent；視窗 Close 事件被 handler `ignore()` 後視窗保持可見，未被 ignore 時隱藏／關閉；Show／Hide 有 widget hook 且順序同 Qt。
- **Required test**（`qtrs-widgets/tests/test_event_propagation.rs` 18 項、`qtrs-platform/tests/test_native_double_click.rs` 2 項；修改前：widgets 以舊程式執行 11／12 項 FAIL（`accepted_press_stops_at_child` 本來就過，作回歸護欄）、4 項需要新 API 而無法編譯、2 項內建 widget 測試以局部還原原始碼後 FAIL；platform 1 項（`CS_DBLCLKS`）FAIL、1 項無法編譯；修改後全 PASS）：
  - 傳遞：`unaccepted_press_reaches_parent_widget`（每個祖先以**自己的座標**看到 press）、`accepted_press_stops_at_child`、`press_nobody_accepts_is_reported_unconsumed_after_visiting_every_ancestor`、`release_and_wheel_propagate_like_press`。
  - disabled：`disabled_widget_passes_mouse_press_to_its_parent`、`press_on_child_of_disabled_parent_is_not_delivered_to_the_child`（`QWidget::event` 對 disabled 的滑鼠事件回 false，`qwidget.cpp:8978-8998`）。
  - 雙擊：`double_click_a_widget_does_not_handle_is_delivered_to_it_as_a_press`（`QWidget::mouseDoubleClickEvent` 預設呼叫 `mousePressEvent`，`qwidget.cpp:9636`）、`unhandled_double_click_falls_back_to_the_press_handler`、`double_click_and_release_reach_the_window_handler_when_no_widget_takes_them`、平台層 `native_window_class_asks_the_os_for_double_clicks`（`CS_DBLCLKS`）與 `double_click_message_is_a_double_click_event_not_a_second_press`。
  - 內建 widget：`right_press_on_a_button_reaches_the_parent`（`QAbstractButton` 忽略非左鍵）、`wheel_over_a_label_inside_a_scroll_area_scrolls_it`。
  - 視窗：`close_request_without_handler_hides_window`、`close_event_ignored_keeps_window_visible`、`window_close_handler_can_refuse_the_close`、`accepted_close_hides_children_after_the_window`、`show_hide_events_delivered_in_order`（Show：子 → 自己；Hide：自己 → 子；重複 show／hide 不重發）、`explicitly_hidden_child_gets_no_show_event`、`window_move_reaches_the_window_widget_and_handler`。關閉測試用真正的 `WM_CLOSE`（`SendMessageW`）驅動。
- **Downstream**：RC-11（tooltip）、RC-17（幾何持久化）、RC-18（喚醒偵測，`Power::Resume` 已存在）。
- **Can remove app workaround**：`hud_window.rs:352` 的 `set_mouse_press_handler` 拖曳／縮放模擬；`hide()` 內的 trim（`hud_window.rs:479-485`）— **只有在 HUD 以真正的 `mousePressEvent` 冒泡重寫後**。
- **Status**：**已修復**（RC-06）。
  - **根因**：事件翻譯散在各處——`EventTreeDispatcher` 對每種滑鼠事件只送給最內層 widget 並回傳其 `bool`，沒有 accept／ignore 與 parent 鏈；`WindowEventHandler` 對 `CloseRequest`、雙擊、`Move` 沒有 arm（`_ => {}`），Win32 的 `WM_*DBLCLK` 被當成第二次 `MousePress`；`Show`／`Hide`／`Close` 從未送到任何 widget。
  - **修改**：
    - `hit_test.rs`：`hit_path`（root → 最內層，每層附自己座標與「有效 enabled」）；`deliver_with_propagation`（`QApplication::notify` 的迴圈：被 handle 且 accept 才停；disabled 跳過；到 root 為止）供 press／release／wheel／context menu 使用；`deliver_double_click`（widget 不處理 `DblClick` 就給它 press）。
    - `window.rs`：`WindowCore`（`show`／`hide`／`close` 與原生 `CloseRequest` 共用 `handleClose`、`show_helper`、`hide_helper` 的順序）；`Window::close`、`Window::is_visible`、`Window::set_window_event_handler`（收 `Close`／`Show`／`Hide`／`Move`，以及沒有 widget 處理的 `MouseButtonRelease`／`MouseButtonDblClick`；忽略雙擊則退回 press handler）。`Move` 送給 window widget（root）。順帶修正：`WindowEventHandler` 原本持有建構時的 root 副本，`set_root_widget` 後事件仍送舊 root；現在一律取 `render_state.root`。
    - 平台層：`WindowSystemEvent::MouseDoubleClick`；`WM_*DBLCLK` 改送它；視窗類別加 `CS_DBLCLKS`（Qt 同）。Qt 的 widget 層不會收到雙擊的第二次 press（`qwidgetwindow.cpp:570,680`），序列是 Press、Release、DblClick、Release，與此一致。
    - 內建 widget：`Button`／`ScrollBar` 忽略非左鍵 press／release；`EmptyWidget` 與 `ProgressBar` 的預設 wheel 不再吞掉（回 false），滾輪才能到達 `ScrollArea`。
  - **行為改變（需注意）**：Alt+F4／關閉要求現在**隱藏視窗**（以前什麼都不做）。`Window::hide` 現在會讓 `is_visible()` 為 false，HUD 的 `toggle_visibility` 改用它取代自己追蹤的 bool（以前 `main.rs:405,441` 直接呼叫 `window.hide()` 時該 bool 會失真）。關閉路徑**不會**呼叫 HUD 的 `persist_geometry`／`trim_memory`——Python 在 `closeEvent`／`hideEvent` 做這兩件事，需要 HUD 以 `set_window_event_handler` 註冊（RC-17）。
  - **驗證**：`qtrs` workspace exit 0、68 個 test binary ok；主 crate 見提交說明。**未做** HUD 手動操作（Alt+F4、雙擊、拖曳、右鍵、滾輪）——只有上述測試與 `SendMessageW` 模擬的原生訊息；測試通過不等於真滑鼠行為已驗證。
  - **未涵蓋**：G8.4.h（`MouseMove` 傳遞、非 Win32 的雙擊偵測）、G8.4.i、G11.2.h；press grab、enter／leave 祖先鏈、tooltip（RC-11）、右鍵選單在 release 觸發（G8.4.f）。
  - **HUD workaround 未移除**：`hud_window.rs:352` 的 `set_mouse_press_handler` 拖曳／縮放仍在，且仍是唯一的拖曳來源；依本條規定須以 `mousePressEvent` 冒泡重寫後逐一移除，並重跑 HUD 快照與 layout harness。
- **Phase**：2。

#### RC-07 Layout item 對齊
- **Contract gaps**：G9.2.a（P0, READ）。
- **Qt behavior** `[QT-SRC qlayoutitem.cpp:597-600]`：對齊影響 `expandingDirections` 與最大尺寸；`addWidget(w, row, col, alignment)`。
- **qtrs root**：`qtrs-widgets/src/layout.rs`：`add_widget(widget,row,col)`／`add_widget_with_span` 無對齊參數；`item_expanding` 無對齊邏輯。
- **Evidence**：`READ`；`qt_layout_compare.py` 目前**不涵蓋對齊**（`RAN` 的 7500×2 組 0 差異不能推論此項）。
- **Required observable**：對齊的 item 在儲存格內依對齊放置，不撐滿；`expandingDirections` 與最大尺寸隨之改變。
- **Required test**（修改前以「讓對齊旗標不生效」的方式重現舊行為：7 項中 5 項 FAIL、2 項為回歸護欄；harness 以舊 probe 跑 600 組 460 組不同）：
  - `qtrs-widgets/tests/test_layout_alignment.rs`：`grid_item_alignment_does_not_fill_cell`（HUD 三種對齊旗標，數值取自 PySide6：label `0,1,60,18`、legend `0,57,80,70`、pill `156,0,71,20`）、`grid_items_without_alignment_fill_their_cells`、`box_item_alignment_places_item_inside_the_cell`、`alignment_removes_the_aligned_axis_from_expanding_directions`、`aligned_expanding_item_is_placed_not_stretched`、`set_alignment_applies_to_an_existing_item_and_reports_unknown_widgets`、`stacked_layout_ignores_item_alignment_like_qt`（`qstackedlayout.cpp:453-467`）。
  - `tools/second_layer_harness/qt_layout_compare.py` 加入每個 item 的對齊（60% 的 item 有 0–2 個旗標；box 與 grid），probe 讀第 16 欄（Qt 的數值旗標）。修改後 3000＋7500×2 組 0 差異。
- **Downstream**：HUD workaround 的移除（見下）。
- **Status**：**已修復**（RC-07）。
  - **根因**：layout item 沒有對齊狀態。`QWidgetItem` 的 `align` 同時影響三處——`setGeometry`（對齊軸縮成 size hint 並定位）、`maximumSize`（`qSmartMaxSize`：對齊軸無上限，兩軸皆對齊則兩軸皆無上限）、`expandingDirections`（去掉對齊軸）——qtrs 的 `item_set_geometry` 只寫死「無對齊」特例，`smart_max_size`／`item_expanding` 不認對齊。
  - **修改**：新增 `ItemAlignment`（Qt 的 `Qt::Alignment` 位元值；`NONE/LEFT/RIGHT/H_CENTER/JUSTIFY/ABSOLUTE/TOP/BOTTOM/V_CENTER/BASELINE/CENTER`、`|`、`contains`、`horizontal()`、`vertical()`）；`LayoutItem`／`GridItem` 加 `alignment`；`smart_max_size`、`item_maximum_size`、`item_expanding`、`item_set_geometry` 加對齊參數，box 的 `setup_geom`／`activate` 與 grid 的 `setup_layout_data`／`activate` 傳入；`item_set_geometry` 完整移植 `QWidgetItem::setGeometry`（含 `Ignored` 政策取 widget 的 size hint）。
  - **API**：`Layout::set_alignment(&WidgetRef, ItemAlignment) -> bool`（`QLayout::setAlignment(QWidget*, …)`；**必要 trait 方法，沒有預設實作**，三個 layout 都實作；找不到 widget 回 false；只找直接 item）、`BoxLayout::add_widget_aligned(widget, stretch, alignment)`、`GridLayout::add_widget_aligned(widget, row, col, row_span, col_span, alignment)`。`StackedLayout::set_alignment` 找到頁面回 true，但不改變位置——與 Qt 相同（`QStackedLayout::setGeometry` 直接 `widget->setGeometry(rect)`），測試釘住這點。
  - **驗證**：qtrs workspace 與主 crate 結果見提交說明。**沒有做像素驗證**；HUD 尚未使用對齊，不預期有變化。
  - **未涵蓋**：G9.2.d。
  - **HUD workaround 未移除**：`usage_table.rs:1173-1209` 的 legend wrapper＋前後 `add_stretch(1)` 仍在。Python 的對齊有 8 處（4 個 `sub_label` 與 legend 為 `AlignVCenter|AlignLeft`、3 個 `m2_val` 為 `AlignHCenter`），Rust 目前一處都沒有。補齊並移除 wrapper 會改變 widget 矩形（hit-test、日後 tooltip 區域、`sub_label` 的 `padding-left: 18px`），須逐一做並重跑 HUD 快照與 layout harness。
- **Phase**：3。

#### RC-08 每視窗 DPR／螢幕
- **Contract gaps**：G10.7.a、G11.3.a（P0, READ，未實測）；相關 P1：G11.8.a。
- **Qt behavior** `[QT-DOC]`：視窗的 `devicePixelRatio` 取自**所在螢幕**。須在實作前於 `qtbase/` 確認。
- **qtrs root**：`primary_screen().device_pixel_ratio()` 出現在 `qtrs-widgets/src/window.rs:261,354,468,512,645,668,914`、`menu.rs:551,597,729`、`qtrs-platform/src/window.rs:1633,1891`、`tray_icon.rs:281`；`PlatformWindow` 沒有每視窗 DPR 查詢；WM handler 用 `GetDpiForWindow`（`qtrs-platform window.rs:404`）。
- **Evidence**：`READ`；**只在異 DPI 多螢幕下可見**，本機未實測。
- **Required observable**：視窗在非主螢幕時，backing store DPR 與該視窗所在螢幕一致，且在 `DpiChanged` 之後不退回主螢幕 DPR。
- **Required test**（`qtrs-widgets/tests/test_window_device_pixel_ratio.rs`；**以 fake platform 為之**——主螢幕 1.0、視窗所在螢幕 2.0，**沒有第二台螢幕**；修改前以「讓視窗每次讀主螢幕 DPR」模擬舊行為：4 項全 FAIL）：
  - `a_window_takes_its_ratio_from_its_own_screen_not_the_primary_screen`
  - `set_geometry_converts_with_the_window_ratio`
  - `a_dpi_change_is_not_undone_by_the_next_render`（原 P0：`DpiChanged` → `render_and_present` → store 退回主螢幕 DPR）
  - `custom_presentation_uses_the_window_ratio`（`present_custom`、`present_custom_at`）
- **Phase**：3。
- **Status**：**implementation complete / real heterogeneous-DPI verification pending**（RC-08）。
  - 驗證等級：implementation fixed；fake-platform regression verified；**≠ real heterogeneous-DPI verified**。fake platform PASS 不等於實機驗證完成；G10.7.a／G11.3.a 在實機驗證（G10.7.i）完成前不得視為驗證完畢。
  - **根因**：「視窗的 DPR」沒有單一來源；每個用到的地方各自讀 `primary_screen().device_pixel_ratio()`，只有 `DpiChanged` 路徑寫了視窗值，下一次 render 就覆蓋它。Qt 的 `QWindow::devicePixelRatio` 是每視窗快取（`qwindow.cpp:1425`），僅由 `updateDevicePixelRatio` 更新（建立、螢幕變更、DPI 變更）。
  - **修改**：`PlatformWindow::device_pixel_ratio()`（**必要 trait 方法**，對應 `QPlatformWindow::devicePixelRatio`；Windows = `GetDpiForWindow`）；`RenderState.device_pixel_ratio` 為單一快取；`Window::new` 建立後讀回視窗 DPR，與放置用的主螢幕 DPR 不同時，保持原生原點並以視窗 DPR 重設原生尺寸；`do_render_and_present` 改收 `dpr` 參數；`Window::set_geometry`、`set_geometry_silent`、`present_custom`、`present_custom_at`、兩條 `DpiChanged` 路徑與 trace 全用該快取；公開 `Window::device_pixel_ratio()`；`NativeWindow::present_region` 的 `target_pos` 與 `calc_frameless_edge` 改用 `get_window_dpr(hwnd)`；`menu.rs` 的游標換算與 `present_popup` 改用彈出視窗的 DPR。
  - **驗證**：新測試 4 項 PASS；qtrs workspace 與主 crate 結果見提交說明。**沒有做像素驗證**，也**沒有在異 DPI 實機驗證**（需人工：把視窗拖到不同縮放的螢幕，確認 backing store 尺寸與內容不被縮放兩次）。
  - **未涵蓋**：G10.7.b、G10.7.d–i、G11.8.a。

#### RC-09 Presenter 尊重 opacity
- **Contract gaps**：G11.5.a（= G12.5.t）；新增 G11.5.c（test gap）。
- **Qt behavior** `[QT-DOC]`：`setWindowOpacity` 對所有後端生效。
- **qtrs root**：`presenter.rs:361-363` DComp 以寫死 `1.0` 呈現；`set_opacity` 對 DComp 為 no-op（`:342-348`）；`window.rs:1490-1492` **對所有 `LAYERED` 視窗無閘門地先試 DComp**。
- **Evidence**：`READ` + `RAN`：本機 `FRAMELESS|LAYERED` 視窗選到 **Layered**；`test_dcomp_*` 5 項因 `CreateDXGIFactory1 failed for IDXGIFactory2` **全部 skip**（顯示為「通過」）。因此**本機未重現**，且 DComp 路徑在本機**完全沒被測試**。
- **Required observable**：不論選到哪個後端，`set_opacity(0.5)` 後合成 alpha 為 0.5。
- **Required test**（`qtrs-platform/tests/test_window_opacity.rs`；修改前以「DComp 縮放忽略 opacity」「trait 路徑回到舊的 set_opacity」兩處模擬舊行為：2 項 FAIL）：
  - `dcomp_staging_pixels_are_scaled_by_the_window_opacity`（純函式 `convert_rgba_to_staging_bgra`：1.0 只換通道序；0.5 → alpha 128、顏色減半、premultiplied 不變式；0.0 全透明）。
  - `a_standard_window_becomes_translucent_through_the_trait`（非 `LAYERED` 視窗經 `dyn PlatformWindow` 設 0.5 → `WS_EX_LAYERED` 且 `GetLayeredWindowAttributes` alpha=128；設 1.0 → 樣式移除）。
  - `dcomp_window_presents_with_the_window_opacity`：**標為 `ignore`（reason: requires DirectComposition）**，含 opacity 回到 1.0 且 dirty rect 不含探測像素時整面重傳。**本機以 `--ignored` 執行會失敗（「this machine selected a presenter other than DirectComposition」）——本機沒有 DComp，這項沒有通過過。**
- **Phase**：3。
- **Status**：**implementation complete / DirectComposition hardware verification pending**（RC-09）。
  - 驗證等級：implementation fixed；pure-function 與 standard-window regression verified；**≠ DComp end-to-end verified**（ignore 的測試從未在有 DComp 的機器上跑過）；Layered 路徑未做合成讀回（G11.5.e）。
  - **根因**：「opacity」在每個後端各自實作，DComp（`present_dirty_ref` 的 `_opacity` 被丟棄、`WindowsPresenter` 以寫死的 `1.0` 呈現）與標準視窗（兩個 `set_opacity`，trait 版不碰視窗樣式）都沒有實作；只有 Layered 有。
  - **修改**：`surface::dcomp::convert_rgba_to_staging_bgra`（premultiplied RGBA → BGRA，每個通道含 alpha 乘以 `qRound(255*opacity)/255`，等價於 `SourceConstantAlpha`）；`DCompSurface` 記住 `opacity`／`staged_opacity`，opacity 變更時下一次呈現整面重傳；`WindowsPresenter` 對 DComp 傳 `p.opacity()` 並轉發 `set_opacity`；`NativeWindow::set_opacity` 合併為一個（trait 版轉呼叫 inherent 版），非 `LAYERED` 視窗依 Qt 的 `setWindowLayered`／`setWindowOpacity` 加／移除 `WS_EX_LAYERED` 並設 `SetLayeredWindowAttributes`；`NativeWindow::present_region` 原本就每次呈現前呼叫 `p.set_opacity`。
  - **未動**：presenter 選擇（G11.5.g，production DComp 政策未決）。
  - **未涵蓋**：G11.5.c（既有 `test_dcomp_*` 仍靜默 skip）、G11.5.e、G11.5.f、G11.5.g。
  - **沒有做像素驗證。**

#### RC-10 樣式範圍與通知
- **Contract gaps**：G8.5.d（= G12.5.s）。相關新增：G8.5.j、G8.5.k、G8.5.l；G8.5.i 的 (2)(3) 部分。
- **Qt behavior**：
  - `QWidget::setStyleSheet` 只作用於該 widget 與其子樹，並對該 widget 及**所有後代**送 `StyleChange`（`QStyleSheetStyle::repolish(w)` → `updateObjects`）`[QT-SRC qwidget.cpp:2594-2631, qstylesheetstyle.cpp:2780-2800, 2978-2987]`。頂層視窗就是一個 `QWidget`，`HUDWindow.setStyleSheet` 只影響該視窗（`hud_window.py:246,250`）。
  - `QApplication::setStyleSheet` 才是應用範圍：`repolish(qApp)` 對所有已 polish 的 widget 送 `StyleChange`（`qapplication.cpp:885-900`、`qstylesheetstyle.cpp:2989-2998`）。
  - `StyleChange`／`FontChange` = `update(); updateGeometry(); layout->invalidate()`（`qwidget.cpp:9502-9510`）。
  - 串接：`weight = specificity + (origin+depth)*0x10000000000`，depth 壓過 specificity：app < 祖先 < 自己（`qcssparser.cpp:2215`）。qtrs 的 `resolve_chain` 排序 `(depth, score)`，一致。
  - `styleRules` 沿 `QObject::parent()` 收集每層的 `styleSheet`，所以 `QMenu(parent)` 取得 parent 的 sheet（`qstylesheetstyle.cpp:1496-1506, 1654-1675`；Python `hud_window.py:678`、`tray_icon.py:55`）。
- **Root cause**：樣式「來源」改變時沒有通知受影響的 widget。(a) `Application::set_style_sheet` 只寫全域，不通知任何 widget；(b) `WidgetBase::set_style_sheet` 只通知自己，不通知子樹；(c) 沒有 parent 的 root 的 layout 在 `style_changed` 後只被標 dirty，沒有人排程（G8.5.i(2)）；(d) `Window::set_style_sheet` 轉呼叫 `Application::set_style_sheet`，範圍錯為整個應用。
- **Contract 文字的更正**（舊版寫「真正前提是 popup 能掛 parent」，另寫「只改 `Window::set_style_sheet` 的範圍不會改變 HUD 行為」，兩者都不成立）：
  > Popup 的 parent-chain style resolution 已存在且可用；RC-10 的核心問題是 Window stylesheet scope 錯誤，以及 application / widget style change 的通知與失效傳播不完整。Menu 對 QSS 的實際消費另屬 G8.5.f。
  - 實測：掛在 A 下的 `QMenu` 取得 A 的 sheet，B 下的取不到（修改前就通過）。缺的是 `Menu` 不消費解析結果（G8.5.f）與 HUD 選單未掛 parent（G8.5.k），都不屬於 RC-10。
  - **HUD 行為會變**：舊的 `Window::set_style_sheet` 與 `Application::set_style_sheet` 是同一個槽，`hud_window.rs:574`（只呼叫後者）因此能蓋掉視窗的舊 sheet。範圍拆開後，視窗上殘留的另一模式的 sheet 會壓過 app sheet（table 模式的標題、時間等標籤顏色錯；是 HEAD 與修改後的 snapshot 對比發現的，既有單元測試沒有發現）。`hud_window.rs:574` 因此補上 `self.window.set_style_sheet(sheet)`（Python `_apply_theme` 的 `self.setStyleSheet`）。這是跟著語意調整，**不是移除 workaround**。
- **Evidence**：`RAN`。
- **Required test**（`qtrs-widgets/tests/test_style_scope.rs`，真實 `Window` 與 `EventLoop`，每次改動後只 pump，不呼叫 `update_layout`／`render_and_present`）。**修改前（原始碼）FAIL 的 5 項**：
  - `application_sheet_reaches_existing_widgets_after_one_pump`（size hint 31→71，geometry 停在 31）。
  - `application_sheet_reaches_every_window`（`QApplication::setStyleSheet` 是應用的：兩個視窗都要變）。
  - `window_set_style_sheet_reaches_its_existing_widgets_after_one_pump`。
  - `window_set_style_sheet_does_not_reach_another_window`（B 的 hint 31→71；且 `Application::style_sheet()` 不得變成非空）。
  - `a_sheet_on_a_root_widget_without_a_parent_relayouts_its_own_subtree`（G8.5.i(2)）。
  - 另 2 項在修改前就通過（保護既有行為）：`a_menu_takes_the_sheet_of_the_window_it_hangs_under_and_not_another`（只驗解析層級）；`a_sheet_on_a_nested_container_reaches_widgets_below_it`。後者在其餘修改就位、僅拿掉子樹遞迴時 **FAIL**（因此它是子樹遞迴的回歸測試，但不是原始碼的 before-FAIL）。
  - Before-FAIL 限制：Menu 的外觀「實際使用」解析結果無 API 可測（G8.5.f），沒有假造。
- **Status**：**已修復：RC-10**（真實 Win32 視窗，geometry／解析層級驗證）。HUD 只做了 snapshot 對比（見下），**沒有做互動式目視驗證**。
  - **修改**：
    - `Widget::set_style_sheet`（trait 預設）在 `WidgetBase::set_style_sheet` 之後對子樹呼叫 `style_changed_below`（Qt `updateObjects`）。`Widget::repolish()` 維持單一 widget（對應 Python `style().unpolish(w); polish(w)`）。
    - `Window::set_style_sheet` 改為 `root_widget().set_style_sheet(qss)`，不再碰 `Application`。
    - `Application::set_style_sheet` 寫入全域後，對每個存活視窗的 root 與其子樹送 `StyleChange`（經 `window::window_roots()`，取自既有的 `RENDER_STATES` 登錄；沒有新建第二套 registry。沒有走 `TOP_LEVEL_WINDOWS`＋`with_object`：它只在視窗呼叫過 `unsafe register()` 時才查得到，而 `RENDER_STATES` 在 `Window::new` 就登錄、`Drop` 時移除）。
    - `LayoutScheduler::activate_if_dirty`：`do_render_and_present` 在 `flush_layouts` 之後排程自己的 layout 為 dirty 的 root。
    - `Application::reset_for_test` 也重置 app sheet。
  - **HUD workaround 尚未移除**（HUD 仍在 `hud_window.rs:231,574,710` 呼叫 `Application::set_style_sheet`）。移除前提：`panel_look`（`hud_window.rs:94`）改讀 root 的 sheet；`:574` 改走 `window.set_style_sheet`；逐一移除並重跑 HUD snapshot 與 layout harness。注意：`Application::set_style_sheet` 現在會對所有視窗要求 repaint 與 relayout，HUD 每次換樣式都會多出這些請求，未做目視驗證。
  - **HUD 回歸測試**（`rust/src/ui/hud_window.rs`）：`test_switching_ui_mode_restyles_labels_from_the_new_sheet`（table→cards→table，`HeaderTitle` 的顏色必須等於該模式 sheet 單獨解析的結果）。拿掉 `hud_window.rs:574` 的 `window.set_style_sheet` 時 **FAIL**（`#94a3b8` 對 `#ebebf5` α0.62），加上後 PASS。
  - **HUD snapshot**（`--snapshot`，輸出含時鐘文字，非決定性）：HEAD 對 HEAD 兩次只差時鐘區（117 px）。調整前，table 模式與 HEAD 差 38489 px（標題、時間顏色錯）；調整後，與 HEAD 的差異只剩隨時間變動的文字與弧線（重設時間、`17:18`→`17:20`、小圓弧刻度）。cards 模式與 context menu 沒有看出結構差異（cards 差 257／441 px，與時鐘同區）。**沒有達到 per-pixel 差為零，所以：MANUAL WINDOWS VERIFICATION REQUIRED。**
  - **未涵蓋**：G8.5.j（`Application::set_font`，依使用者決定不進 RC-10）、G8.5.k、G8.5.l、G8.5.f（Menu 消費 QSS）、G8.5.i(1) 與「parent 沒有 layout」。
- **Depends on**：RC-05（已完成）、G8.5.i(2)（由本 RC 一併處理）。
- **Phase**：3。

#### RC-11 Tooltip
- **Contract gaps**：G8.8.a（= G12.5.i）。
- **Qt behavior** `[QT-SRC qapplication.cpp:2731; qwidget.cpp:9381-9386]`：`toolTipWakeUp.start(delay, this)`（需要 QObject 計時器）→ 送 `ToolTip` 事件 → widget 的 `event()` 顯示 `QToolTip::showText`；無 tooltip 則 `ignore()`。
- **qtrs root**：整個缺；`EventKind::ToolTip` 存在但無人處理。
- **Evidence**：`READ`；wake-up 計時器用 `Timer`（QTimer 類），**不依賴 G5.4.a**（`QObject::start_timer`，不在 RC-11 範圍）。
- **Depends on**：RC-06、RC-11a（`Timer` 重啟語意，G5.1.f）。
- **Phase**：3。
- **範圍（使用者核定）**：
  - **RC-11a**（已完成，commit 另列）：`Timer` 重啟語意，G5.1.f。只改 `register_object_metadata` 的 liveness 處理；未改其他計時器語意（G5.4.a/b、單發、`remaining_time` 皆未動）。實作完成，無需手動驗證。
  - **RC-11b**（implementation complete / real X11、Wayland、macOS 驗證 pending）：`PlatformWindow::is_active()`（必要方法，無 default；Win32 = `QWindowsWindow::isActive` 的 `GetForegroundWindow` 檢查，真實驗證；Cocoa `isKeyWindow`、X11 `FocusIn/Out`、Wayland `KeyboardEnter/Leave`、Generic `FocusIn/Out`，皆為模擬驗證）與 `WindowFlags::TOOLTIP`（Win32：`WS_POPUP`＋`WS_EX_NOACTIVATE|TOPMOST|TOOLWINDOW`＋`SW_SHOWNOACTIVATE`；Cocoa：`orderFront:` 不成為 key）。測試 `qtrs-platform/tests/test_window_activation.rs`（11 項，含「一般視窗 `show()` 會搶前景」的對照組；變異檢查：把 `SW_SHOWNOACTIVATE` 改回 `SW_SHOW` 時前景測試 FAIL）。新缺口 G11.1.c、G11.1.d。新 API 無法在舊 code 上執行，故無 before-FAIL，以變異檢查代替。
  - **RC-11c**（implementation complete；Win32 真實視窗驗證；外觀、X11／Wayland／Cocoa 真實驗證 pending）：`Widget::tool_tip` 等屬性；`EventKind::ToolTip` 改為 `{x,y,global_x,global_y}`（移除 `text`，並改寫 `test_advanced_event_system.rs` 原本釘住 `text` 的斷言）；`WindowSystemEvent::MouseMove` 新增 `buttons`（Win32 `MK_*`；其他後端記錄 press／release）；`EventTreeDispatcher::dispatch_mouse_move`（`QApplication::notify` 的喚醒）；wake-up 700 ms／20 ms、fall-asleep 2000 ms、hide 300 ms、存活 `10000+40*max(0,len-100)`；沿 parent 冒泡；`QToolTip::showText` 位置（翻轉＋夾入螢幕，純函式 `place_tip`）；以 `PlatformWindow::is_active`（RC-11b）判斷「視窗為 active 或 `WA_AlwaysShowToolTips`」。**慣例**：widget 的 `event()` 對 `ToolTip` 回傳 `false` 表示「交給 `QWidget::event` 的預設行為」（顯示自己的 `tool_tip`，空字串則 `ignore`）；回傳 `true` 則由該 widget 自行決定接受與否。**不修 G11.1.d**：RC-11c 直接查 `PlatformWindow::is_active`，沒有建 `Application::active_window`／`ActivationChange`。**證據分類**：新 API 在舊程式碼上無法執行，所以沒有 before-FAIL；以變異檢查取代（不是 before-FAIL）：喚醒延遲 700→70 ms 使 `hover_shows…` FAIL；移除 active 閘門使 `an_inactive_window…` FAIL；忽略按鈕狀態使 `a_move_with_a_button_down…` 與真實視窗的按鈕測試 FAIL。測試會把真實游標移到測試視窗上再還原（否則 Windows 對 `TrackMouseEvent` 立刻回 `WM_MOUSELEAVE`，那是真實的 `Leave`，會取消 tip）。新缺口 G8.8.b–G8.8.g。HUD 接線（Phase 4）未做，G8.8.a／G12.5.i 在 HUD 端仍開著。
  - **計時器規則**：700 ms／2 s 是 Qt tooltip 協定，明確豁免「不新增 timer／debounce」規則，不是 application debounce。
  - **外觀**（圓角、深色底、字型）留手動驗證；自動化契約只鎖：觸發、取消、冒泡、位置、存活、不搶焦點。

#### RC-19 QSS `min/max-width/height` 的 box 模型（候選，未核定）
- **Contract gaps**：G12.8.g、G12.8.h。
- **Qt behavior** `[QT-SRC qstylesheetstyle.cpp:2568-2611]`：`min/max-*` 作用於 `rule.boxSize(...)`（content＋padding＋border），`Label`／`Button` 等的 `minimumSize`／`maximumSize` 經此換算；`QLabel::sizeHint` 不使用 `max-height`。
- **qtrs root**：各 widget 各自讀 `style.min_*/max_*` 當原始長度；沒有共用的 box-size 換算。
- **Evidence**：`RAN`（`layout_toggle_btn` 最大高 Python 22 vs Rust 18、最小寬 28 vs 18）＋`READ`（`Label::size_hint` 用 `max-height` 當 hint）。
- **Required observable**：同一個樣式表下，各 widget 的 `minimumSize`／`maximumSize`／`sizeHint` 與 Python 實測一致（至少 `layout_toggle_btn`、badge 類 label、5 px 進度條）。
- **Can remove app workaround**：n/a（HUD 沒有為此補償；RC-15 的 `max-height` 與此項互動，見 C12.8）。
- **Status**：**已修復（RC-19，使用者核准進入實作）**。`ResolvedStyle::min_box_size`／`max_box_size`（`style/stylesheet.rs`）依 `QRenderRule::boxSize` 加上 border 與 padding，`Label`／`Button`／`Frame`／`ProgressBar` 的 `minimum_size`／`maximum_size` 改走這兩個函式；`Label::size_hint` 改為「文字＋box 再 expand 到 minimum（`qlabel.cpp:620`）」，不再用 `max-height`；`Label` 繪製背景／邊框改用整個 widget rect（原本以 `max-height` 當繪製高度）。稽核：`rust/tools/qss_box_audit/`（`py_box.py`＋`cases.json`＋`results/py_box*.json`＋`compare.py`，Rust 端為 `src/ui/qss_box_audit.rs`，`#[ignore]`）以 14 種 widget／QSS 組合在 DPR 1.25 與 1.0 比對；min／max 全部相符，唯一剩餘的 `min` 差異是 B2（G12.8.k）。測試：`qtrs-widgets/tests/test_qss_min_max_box.rs`（5 項，修改前全部 FAIL，例如 B1 `[18,0]/[∞,18]` vs `[28,0]/[∞,22]`）＋`test_vertical_extra_height_goes_to_header_like_pyside6` 新增 toggle 按鈕斷言（修改前 FAIL：y 34、高 18 vs y 32、高 22）。**未涵蓋**：`ProgressBar` 的 hint（RC-20，P1／P2 仍差）、文字度量殘差（RC-21）、G12.8.k–n（新登記，未修）。**僅 DPR 1.25／1.0、Windows 字型**。**Phase**：4（qtrs 側）。 **Follow-up（獨立 commit，G12.8.o）**：`ProgressBar::minimum_size`／`maximum_size` 不再於垂直方向轉置（QSS min／max 是 widget 座標）。`test_vertical_progress_bar_min_max_are_not_transposed` 在修前失敗（([0,5],[∞,5]) 對 ([5,0],[5,∞])），修後通過；oracle P11 OK（DPR 1.25／1.0）。

#### RC-20 `QProgressBar::sizeHint`／`minimumSizeHint` 演算法（已核准、已修復）
- **Contract gaps**：G12.8.i。
- **Qt behavior** `[QT-SRC qprogressbar.cpp:396-418; qstylesheetstyle.cpp:5485-5490]`：hint 由字型度量與 chunk 寬度算出，再經 `sizeFromContents(CT_ProgressBar)`；有 contents size 時用 `rule.size()`，否則 `rule.boxSize(base)`。
- **qtrs root**：`progress_bar.rs:398-436` 固定 160 或 QSS 高度，`minimumSizeHint` 只看 QSS。
- **Evidence**：`RAN`（Python 91×5／91×17；qtrs 160×5／0×5）。
- **Required observable**：預設與樣式表下的 hint 與 Python 相同。
- **Audit（已完成，未實作）**：`rust/tools/qss_box_audit/`（`cases.json` 的 P1–P13，PySide6 與 qtrs 在 DPR 1.25／1.0 比對）。**根因確認**，演算法為（`qprogressbar.cpp:396-418`、`qstylesheetstyle.cpp:5304-5320,5485-5490`）：`csz = (max(9, chunkWidth)*7 + advance('0')*4, fm.height()+8)`（垂直則轉置）→ `rule.adjustSize(csz)`（先夾 `max-*`、再 expand 到 `min-*`，皆為內容盒）→ `CT_ProgressBar`：有 `width`/`height` 內容尺寸則 `rule.size(sz)`，否則 `rule.boxSize(sz)`；`minimumSizeHint = (hint.width, fm.height()+2)`（垂直為 `(fm.height()+2, hint.height)`）。`chunkWidth` 來自 `::chunk { width }`（`PM_ProgressBarChunkWidth`，預設 9）。已用 PySide6 驗證各項（均 DPR 無關，因為都用明確 `font-size`）：P4 12px→91×23；P5 20px→111×33；P10 Segoe UI 14px→95×27；P6 border1+padding(2,3)→99×29；P12 `::chunk{width:20px}`→168×23；P13 `::chunk{width:5px}`→91×23；P1／P2 min=max=5→91×5／97×11；垂直 P11→5×91。qtrs 現況：水平固定 160 寬，高度為 QSS 的 `max/min-height`（原始值）或 `max(ceil(height)+6, 18)`；`minimum_size_hint` = `minimum_size()`。13 項中 hint 不符 13／13（水平全部、垂直 1），minHint 不符 13／13。**不用 `width`/`height` 的案例（P1–P7、P10–P13）屬 RC-20；P8／P9 另屬 G12.8.p（RC-22，已修復）；P11 的 min/max 另屬 G12.8.o。** 注意：qtrs 的 `ProgressBar` 沒有 QSS 字型（只有 `self.font`），實作需要先取得 QSS `font-size`／`font-family`（`Label::styled_font` 已有同類邏輯），且預設字型與 Qt 應用程式預設字型的對應屬「Application font」項，不在此處決定。HUD 影響：卡片 `m*_bar` 的 hint 寬 Python 91（DPR 1.25）／99（DPR 1.0，預設字型）對 qtrs 160，影響卡片 `sizeHint` 寬。
- **Status**：**已修復**。P4–P7、P10、P12–P15 與 PySide6 逐項相符（hint 與 minimumSizeHint：垂直先轉置、`adjustSize` 夾 content-box min/max、再加 box、`::chunk{width}`、QSS 字型）。before-FAIL：`tests/test_progress_bar_size_hint.rs` 4／4 在未修的 `src` 上失敗（160×22／0×0 對 87×24／87×18 等），修後通過。**未修**：P1–P3（預設字型是 Qt 應用程式字型 Microsoft JhengHei UI，屬 Application font）、P8／P9（G12.8.p）、P11 的 min／max（G12.8.o）。新增 `ResolvedStyle::width`／`height`（內容尺寸；RC-22 前曾同時寫入 min／max，現已分開）。oracle 的 P4–P13 改用明確 `font-family: 'Segoe UI'`，另加 P14／P15。只量幾何，未做像素驗證。

#### RC-21 文字寬度 1 px 差（`font-family` 清單）
- **Contract gaps**：G12.8.j。
- **Qt behavior** `[QT-SRC qcssparser.cpp:1252-1272]`：`font-family` 是以逗號分隔的清單，`setFontFamilyFromValues` 呼叫 `QFont::setFamilies`；字型庫取第一個已安裝的家族。`QLabel` 寬度 `[QT-SRC qlabel.cpp:594-609; qfontmetrics.cpp:735-749]`：`fm.boundingRect(0,0,w,2000,flags,text)` → `rb.toAlignedRect()`，即寬度向上取整；與 qtrs 的 `ceil` 一致。
- **qtrs root**：`qtrs-gui` 的 `parse_value` 把整串 `'Segoe UI', 'SF Pro Display', …` 當作一個名稱，`ResolvedStyle::font_family` 因此是不存在的家族，退回別的字型；寬度 59.589（`WEEKLY 7D`）而非 58.88。
- **Evidence**：`RAN`（修復前 `[60,145,88]`／`[59,143,83]`，PySide6 為 `[59,144,88]`／`[58,142,80]`）。`[DIFF]` 修復後相等。
- **Status**：**已修復**。`QCssValue::FontFamilies`、`ResolvedStyle::font_families`＋`font_family()`（第一個已安裝，否則第一個）；`Label`／`Button`／`ProgressBar` 的 `styled_font` 改用 `font_family()`。測試 `test_label_text_metric_rounding.rs`（2 項，修復前皆 FAIL）。**未涵蓋**：`sans-serif`／`monospace` 等泛用名稱不做替代（不是已安裝家族，會被略過）；家族比對為大小寫不敏感的完整名稱；字型之後才註冊時，解析在每次取 font 時重做，所以會反映。

#### RC-22 QSS `width`／`height` 內容尺寸被當作 min＝max
- **Contract gaps**：G12.8.p。
- **Qt behavior** `[QT-SRC qstylesheetstyle.cpp:2595-2612, 551-574, 5487-5489]`：`width`／`height` 是 `contentsSize`；`setGeometry` 只有在該軸有 `min-*`／`max-*` 宣告時才設 `minimumSize`／`maximumSize`（`boxSize(max(width, min))`、`boxSize(min(width, max))`）；`adjustSize` 以 `width`／`height` 取代內容尺寸再夾 `max-*`、擴到 `min-*`；`CT_ProgressBar` 有 contents size 時回傳 `rule.size(sz)`。
- **qtrs root**：`style/stylesheet.rs` 把 `width`／`height` 同時寫成 `min_* = max_*`。
- **Evidence**：`RAN`（P8、P9：PySide6 min [0,0]、max [∞,∞]；修復前 qtrs min／max 為 120×9／0×15）。`[DIFF]` 修復後相等。
- **Status**：**已修復**（使用者核准進入實作）。細節與測試見 G12.8.p。HUD 的樣式表只有 `QMenu::separator { height: 1px }` 使用 `height`，不經上述路徑。**Phase**：4（qtrs 側）。

#### RC-24 `StackedLayout` 的 `sizeHint`／`minimumSize` 只看目前頁
- **Contract gaps**：G9.6.a。
- **Qt behavior** `[QT-SRC qstackedlayout.cpp:417-436, 438-448]`：見 C9.6 與 G9.6.a。
- **qtrs root**：`stacked.rs` 的 `StackedLayout::size_hint`／`minimum_size` 只取 `current_widget()`。
- **Evidence**：`RAN`（PySide6 六個案例，見 G9.6.a）。`[DIFF]` 修復後相等；before-FAIL 3／5。
- **Status**：**已修復**。只改 `stacked.rs` 兩個方法；`expanding_directions`、`activate`、margins 的差異未動（見 G9.6.a）。**僅 Windows 驗證；未做像素驗證。**

#### RC-25 缺 `QColor::darker`／`lighter`
- **Contract gaps**：G12.5.p。
- **Qt behavior** `[QT-SRC qcolor.cpp:2941-2997, 2363-2402, 2215-2290; qdrawhelper_p.h:886-887]`：`darker(f)` 轉 HSV（16 位元元件），`value = value * 100 / f`（整數除法），轉回 RGB，再以 `qt_div_257` 取 8 位元。`lighter(f)` 對 value 乘 `f/100`，溢位時由 saturation 扣掉溢出量。`f <= 0` 原樣回傳；`f < 100` 時兩者互相委派（`10000 / f`）。
- **qtrs root**：qtrs 沒有 `QColor::darker`／`lighter`，HUD 的 pie 圖例（`usage_table.py:88`：`QColor(*theme["disc"]).darker(110)`）因此直接用原色。
- **Evidence**：`RAN`（PySide6 6.11.2：255 列 `darker`／`lighter` 輸入輸出，含原色、灰階、disc 色、factor 50／110／150／200／300、40 個種子固定的隨機色；圖例 12×12 像素）。`[DIFF]` 修復後逐位元相等；before-FAIL：HUD 圖例像素 (14,14,14,14) vs PySide6 (13,13,13,14)。第一版以 `>> 8` 取 8 位元，有 1 級差，改為 `qt_div_257` 後 255 列全部相等。
- **Status**：**已修復**。新增 `qtrs-gui/src/color/qcolor_ops.rs`（`darker`、`lighter`，回傳 8 位元色）與 `tests/test_qcolor_ops.rs`；HUD 加 `test_legend_pie_disc_is_darker_than_the_theme_disc`。HUD 唯一用到 `darker` 的地方（`usage_table.py:88`）已改。淺色主題 disc 的 alpha 只有 14，變暗後預乘像素與原色相同 (2,2,3,14)，因此淺色主題畫面不變、只有深色主題差 1 級。**僅 Windows 驗證；`lighter` 沒有 HUD 呼叫端，只由 PySide6 表驗證。**

#### RC-26 show／hide 不重排 parent
- **Contract gaps**：G8.1.a。
- **Qt behavior** `[QT-SRC qwidget.cpp:8465-8468; qlayoutitem.cpp:691-693]`：見 C8.1。`setVisible` 於 child 使 parent layout 失效；隱藏的 `QWidgetItem` 為空。
- **qtrs root**：`WidgetBase::set_visible`、`Widget for EmptyWidget::set_visible` 與 `input_common` 巨集只 `update()`，沒有 `update_geometry()`；排版器本來就把隱藏項當空，只是沒人叫它再跑一次。
- **Evidence**：`RAN`（qtrs 真實 `Window` + event loop，一次 pump）。before-FAIL：隱藏第一個 label 後第二個 label 仍在 x=203，期望 0；`Button` 隱藏不送出 layout request。
- **Status**：**已修復**。三處 `set_visible` 改呼叫 `update_geometry`；新增 2 項測試。HUD 的手動 `update_layout()` 未移除（行為相同，另案）。G8.1.b／G8.1.c 未動。**僅 Windows 驗證。**

#### RC-27 視窗啟用沒有接到 toolkit 層
- **Contract gaps**：G11.1.d。
- **Qt behavior** `[QT-SRC qapplication.cpp:1816-1880; qwidget.cpp:9317-9327, 6967-6990]`：`setActiveWindow` 設定 `active_window`，對該視窗送 `WindowActivate` 與 `ActivationChange`（失去時送 `WindowDeactivate`）；`QWidget::event` 再轉給可見的非視窗子 widget；`isActiveWindow` 比對 `window()` 與 `activeWindow()`。
- **qtrs root**：平台層 `PlatformWindow::is_active`（RC-11b）與 `FocusIn`／`FocusOut` 已存在，但 toolkit 的 `Window` 處理器只把它們轉成 `FocusIn`／`FocusOut` 事件，沒有人呼叫 `Application::set_active_window`，也沒有 `WindowActivate`／`WindowDeactivate` 與 `isActiveWindow`。
- **Evidence**：`RAN`（Windows，真實 `Window`，以 `dispatch_window_system_event` 送 `FocusIn`／`FocusOut`）。突變檢查：移除 `FocusIn` 的接線，3 項中 2 項 FAIL。修復前 `is_active_window` 不存在，無法編譯。
- **Status**：**已修復**（範圍見 G11.1.d 的「仍缺」）。新增 `qtrs-widgets/tests/test_window_activation_events.rs`（3 項）。HUD 沒有使用 `isActiveWindow`／`changeEvent`，無 HUD 行為變化。**僅 Windows 驗證；X11／Wayland／Cocoa 後端未驗證。**

#### RC-28 layout 失效不往祖先傳遞
- **Contract gaps**：G9.5.a。
- **Qt behavior** `[QT-SRC qwidget.cpp:10571-10587; qlayout.cpp:471-476, 956-969, 980-1131; qcoreapplication.cpp:1816-1858]`：`updateGeometry` 使 parent layout 失效並 post `LayoutRequest`；`QLayout::activate` 結尾無條件呼叫 `mw->updateGeometry()`，於是下一輪再往上一層，直到 `isWindow()`。停止條件：隱藏、視窗、min==max 兩軸皆固定；`QSizePolicy::Fixed` 不阻止。深 N 層需約 N 次 posted-event drain。
- **qtrs root**：`WidgetBase::request_layout` 只 post 直接 parent；`LayoutScheduler::activate_pending` 跑完 layout 只 `update()`，缺 `mw->updateGeometry()` 那一步。`BoxLayout::size_hint` 無快取（每次重算），不是快取過期問題。
- **Evidence**：`RAN`（Windows，真實 `Window` + event loop）。before-FAIL：Fixed wrapper 內 label 變長後 wrapper 寬度仍為 31，期望 222（新 size hint）。修復後通過，`pump_idle` 未觸發 16 輪上限。分析由 4 個只讀 agent（Claude Haiku 5.5）平行完成，再由主 agent 比對整合。
- **Status**：**已修復**。只改 `layout_scheduler.rs`（layout 真的重跑才往上傳）；新增 `test_widget_invalidation.rs` 1 項。未做：min==max 固定尺寸的停止條件（qtrs 無 `extra->minw/maxw` 對應判斷，仍會往上傳；結果相同、只多一次重排）、HUD 手動 `update_layout()` 的移除。**僅 Windows 驗證。**

#### RC-30 `addStretch(0)` 被強制成 1
- **Contract gaps**：G9.1.a。
- **Qt behavior** `[QT-SRC qboxlayout.cpp:18-19, 36-42, 301, 316, 872-879, 981-983; qlayoutengine.cpp:60, 72-73, 229-235]`：`addStretch(s)` → `insertStretch(-1, s)`，以 `QBoxLayoutItem(spacer, s)` 原值保存；`setupGeom` 的 `stretch = box->stretch ? box->stretch : hStretch()`，spacer 沒有 widget，不回退到 policy，仍為 0；spacer 是 Expanding，故 `expansive`。`qGeomCalc`：`sumStretch > 0` 時按 stretch 分配（stretch 0 得 0）；否則由 expansive 項平分。
- **qtrs root**：`layout.rs` 的 `Layout::add_stretch` 預設實作與 `BoxLayout::add_stretch` 都寫 `stretch.max(1)`（`93ec2c0` 引入、`eec7ead` 沿用，無說明）。排版引擎本身（`setup_geom`、`q_geom_calc`）已與 Qt 相同。
- **Evidence**：`RAN`（Windows）。PySide6 6.11.2 oracle：300×20 `QHBoxLayout`、margins 0、spacing 6、item `sizeHint` 50×20／`minimumSizeHint` 0×0。before-FAIL 3 項，結果恰為 stretch 1 的 Qt 結果：`[stretch0, w(stretch1)]` 得 150/150（Qt 0/300）；`[stretch0, w, stretch2]` 得 83/50/167（Qt 0/50/250）；`[Expanding w, stretch0]` 得 50/250（Qt 150/150）。對照組 `[stretch0, w, stretch0]`（125/50/125）修改前後皆通過。分析由 2 個只讀 agent（Claude Haiku 5.5）平行完成。
- **Status**：**已修復**。只改 `layout.rs` 兩行，新增 `test_box_layout_add_stretch_zero.rs`（4 項）。HUD 不受影響：所有呼叫端都明確傳 `add_stretch(1)`（`hud_window.rs`、`provider_card.rs`、`usage_table.rs`），Python 對應的是 `addStretch()`（= 0）；改成 0 屬 HUD 搬運清理，未做（在無 stretch>0 兄弟項時結果相同，見上）。**僅 Windows 驗證。**

#### RC-31 Box layout 缺 spacing／spacer／stretch API
- **Contract gaps**：G9.1.b。
- **Qt behavior** `[QT-SRC qboxlayout.cpp:412-420, 844-902, 970-998, 1069-1138; qlayoutitem.cpp:570-572, 607-653, 681-683]`：`insertSpacing` 建 `QSpacerItem(size, 0, Fixed, Minimum)`（垂直時轉置），`insertStretch` 建 `(0, 0, Expanding, Minimum)` 並保留 stretch，`insertSpacerItem` 照用呼叫端的 spacer、stretch 0；負的或超出範圍的 index 都附加到尾端（`validateIndex`）。`QSpacerItem`：hint 為給定大小；policy 可縮時 min 為 0，否則為給定大小；可長時 max 無上限，否則為給定大小；`isEmpty` 恆真，故不佔 layout spacing。`setStretchFactor(QWidget*)` 只找直接項，找不到回 false；`setStretch` 超出範圍忽略；`stretch(i)` 回存放值，超出範圍回 -1。
- **qtrs root**：`BoxLayout` 只有 `add_stretch`；spacer 在 `setup_geom` 寫死為 `(0, 0, Expanding, Minimum)`，無法表示固定大小的 spacing。
- **Evidence**：`RAN`（Windows）。PySide6 6.11.2 oracle（margins 0、spacing 6、item hint 50×20、min hint 0×0）：`[w, spacing 20, w]` 在 300 寬為 137/20/137、hint 126×20、min 26×0；擠到 60 寬為 17/20/17；垂直相同；`SpacerItem(40,10,Minimum,Minimum)` 為 127/40/127、min 46×10；`insert_stretch(1,1)`＋`insert_spacing(0,10)` 為 10/50/184/50；`set_stretch_factor` 回 true/false、`stretch` 2/None、`set_stretch(0,1)` 後 98/196。修改前測試無法編譯（API 不存在，12 個 error），修改後 8 項通過；RC-30 的 4 項仍通過。分析由 2 個只讀 agent（Claude Haiku 5.5）平行完成。
- **Status**：**已修復**。`add_stretch` 改為走 `insert_stretch`，結果不變。HUD 不受影響：Python 版未使用這些 API（只有 `addStretch()` 與 `addWidget(w, 1)`）。`stretch(i)` 以 `Option<u32>` 表示 Qt 的 -1。**僅 Windows 驗證。**

### D.2 HUD 應用層 root cause（`rust/src`，不由 qtrs 修）

| RC | 對應 gap | 位置 | 閘門（動手前必須先做） |
|---|---|---|---|
| RC-12 熱鍵註冊失敗不回報 | G11.9.a、G11.8.c、G12.5.j | `hotkey.rs:235-261`、`main.rs:496` | 無。`start` 必須回報 `RegisterHotKey` 失敗；測試：衝突的熱鍵使 `start` 回 `Err`，且 `click_through` 啟動時被關閉。**已完成（RC-12，Win32 真實 `RegisterHotKey` 衝突驗證）**：失敗改由 `HotkeyManager::registration()` 回報，而非 `start` 的 `Err`（理由見 G11.9.a）；新缺口 G11.9.e–G11.9.h |
| RC-13 螢幕選擇／還原 | G11.4.a、G12.5.l | `hud_window.rs:178-193,640-662,823-824` | 對照 Python 規則；使用已存在的 `clamp_window_rect_to_screens`（`qtrs-platform/src/screen.rs:532`）。**已完成（RC-13，單螢幕 Win32 煙霧測試＋Python oracle）**：**未**使用 `clamp_window_rect_to_screens`，因為它的規則與 Python 不同（見 C11.4），改以 Python 規則寫成 `rust/src/ui/placement.rs`；新缺口 G11.4.e–G11.4.g |
| RC-14 卡片根 spacing 2 vs 5 | G9.4.b、G12.5.b、G12.8.a | `provider_card.rs:159` vs `provider_card.py:27` | 閘門已完成（幾何 diff，`GEOMETRY_DIFF_RC14_16.md`）：**案例 A（應用層搬運錯誤）**，2 沒有在補 qtrs 差異。**修復必須與 G12.8.a（指標值字級 16 vs 14）同做**，不得單改 spacing。DPR 1.25／Windows 字型量測。**已完成（RC-14，含 G12.8.a／b）**：`provider_card.rs` 根 spacing 5、`m*_val` 改為 widget-local `font-size`（14px，錯誤時 13px，與 Python 相同）、`hud_window.rs` 橫向 body spacing 8。測試：`test_card_size_hint_matches_pyside6`（label 17、card 109）、`test_horizontal_cards_body_matches_pyside6_widths`（211／210／211），期望值取自 PySide6 oracle；修改前 FAIL（19 vs 17；213／214／213）、修改後 PASS。**僅 DPR 1.25、Windows 字型**。**未驗證**：8 個會建立 `HUDWindow` 的既有測試（`hud_window.rs` 4、`provider_card.rs` 4）因網路隔離（TI-01）未完成而**未執行**，其中版面斷言可能受 spacing／字級影響 |
| RC-15 Badge `max-height: 15px` | G12.3.b、G12.5.a、G12.8.e | `styles.rs:192,291` | 閘門已完成：**案例 A（多餘屬性）**。**已完成（RC-15）**：移除 `styles.rs` 兩處 `max-height: 15px`。DPR 1.0 的 PySide6 oracle 已量（`py_horizontal_dpr1.json`／`py_vertical_dpr1.json`，`QT_ENABLE_HIGHDPI_SCALING=0`、停用 fetch）：badge `sizeHint` = 25×**14**（DPR 1.25 為 25×15），所以 `max-height: 15px` 在 100 % 縮放下是**錯的**而非多餘（Rust 恆為 15）。測試：`test_badge_size_hint_matches_pyside6_at_both_ratios`（DPR 1.0→14、1.25→15）；修改前 FAIL（DPR 1.0：15 vs 14），修改後 PASS。**只涵蓋預設佔位文字 `--`、Windows 字型**；badge 寬度未納入斷言；字重（G12.8.e）另計、未修 |
| RC-16 header 多餘的 `Expanding/Fixed` | G9.3.c、G12.8.c | `hud_window.rs:272-275` | 閘門已完成：**案例 A，範圍比原描述大**：直向還需改 `stack`／`cards_container` policy、根 stretch、直向卡片 stretch（G12.8.c）。**已完成（RC-16，含 G12.8.c／d；使用者已核准照搬 Python 的直向行為）**：header_widget 不再設 policy；`cards_container` 不設 policy；根 layout 不對 stack 設 stretch；直向卡片不設 stretch（橫向維持 stretch 1）；卡片 `title` 設 `Minimum/Preferred`；`stack` 的 policy 隨頁面切換（卡片頁 Preferred、表格頁 Expanding，因 PySide6 表格頁實測由表格吃掉全部多餘高度 462／500，單純把 stack 改 Preferred 會讓 header 變 213 高，已由測試抓到）。測試：`test_vertical_extra_height_goes_to_header_like_pyside6`（header／title 86、卡片 109、y 0/122/244；修改前 FAIL：header 18 vs 86）、`test_table_mode_table_takes_extra_height_like_pyside6`、`test_ui_mode_switch_keeps_pyside6_extra_height_owner`（卡片↔表格切換）。既有 `test_hud_layout_proportions` 兩項斷言（stack.y ≤ 35、卡片高 ≥ 120）編碼的是舊行為（與 PySide6 相反），已改為 PySide6 行為。**僅 DPR 1.25、Windows 字型、佔位文字；DPR 1.0 的 vertical oracle 已量（`py_vertical_dpr1.json`）但未對 Rust 逐項比對；`layout_toggle_btn` 的 y 殘差屬 G12.8.g（RC-19），未修**。 |
| RC-17 幾何持久化 | G12.5.d | `hud_window.rs`（`set_window_event_handler`、`apply_size`、`persist_rect`）、`config.rs`（`ResizeDebouncer`） | 依賴 RC-06。**已完成（RC-17，Win32 真實訊息測試＋真實 `ClaudeHUD.exe` 拖曳 smoke）**：3000 ms geometry poll removed because RC-06 window event lifecycle now supplies Move/Release/Close/Hide hooks；沒有新增 timer，沒有改 qtrs `Timer`；250 ms 是 HUD 的 persistence policy（既有 `ResizeDebouncer`），不是 qtrs 需求。RC-18 未動。**後續 RC-17b（G11.2.i）**：真實拖曳中 release 原本到不了 HUD（原生 move loop 吞掉）；框架層補上 `handleExitSizeMove` 按鍵同步後，HUD 既有的 `MouseButtonRelease` → `persist_rect` 路徑不改即生效；`ClaudeHUD.exe` 真實拖曳放開後 11 ms 存檔。RC-17 fixed 狀態不變 |
| RC-18 喚醒偵測 | G12.5.e | `hud_window.rs`（`tick_gap_exceeded`、`on_clock_tick`）、`main.rs` 的 `clock_timer` | **已完成（RC-18）**：Implemented as application-level timestamp-gap detection. Power::Resume is intentionally not part of RC-18（Power::Resume currently has no toolkit consumer；獨立於 RC-18，見 G12.5.x）。audit 結論 C（不等價）。未改 qtrs Timer／Power dispatch／RefreshController。測試：4 項（T1–T6 純函式＋HUD 長間隔恰一次 refresh，TI-01 stub）；before-FAIL unavailable（新函式／app-private path，未建 16 秒 real-time 測試）；真實 suspend/resume 為 MANUAL WINDOWS VERIFICATION REQUIRED。 診斷（僅供驗證，不影響決策）：`HUDWindow::wake_refresh_seq`（長間隔 refresh 序號）；debug build 於長間隔分支 `eprintln!("[rc18] wake refresh #N previous_ms=… now_ms=… gap_ms=…")`；測試 `wake_refresh_sequence_advances_once_per_long_gap_only`（正常 tick 序號不變、17 s 間隔 +1、其後 1 s tick 不變、剛好 15 s 不變、16 s +1；TI-01 stub，無 real-time sleep）。真實 suspend/resume、lock/unlock、debugger pause 仍為 MANUAL WINDOWS VERIFICATION REQUIRED（可用 stderr `[rc18]` 行判讀）。 |
| RC-23 視窗大小常數 | G12.8.f | `config.rs`（常數、`sanitize`）、`hud_window.rs`（`reset_geometry`、`apply_ui_mode_internal`） | 閘門已完成（Python `hud_window.py:31-35,325-354` 與 PySide6 實測，見 G12.8.f）。**已完成（RC-23）**：只改 app 常數，未改 qtrs、`ResizeDebouncer`、RC-17 的 move／resize 持久化。測試 3 項（見 G12.8.f），修改前 FAIL、修改後 PASS。新缺口 G12.8.q（啟動時未套用最小尺寸）未修。**僅 Windows、DPR 1.25；未做像素驗證** |
| RC-29 `UsageDial` 最小尺寸 | G8.3.e、G12.5.q | `usage_table.rs`（`impl Widget for UsageDial::minimum_size`） | 閘門已完成：`[QT-SRC qwidget.cpp:3966-3971, 4005; qlayoutitem.cpp:616-623; qlayoutengine.cpp:309-336, 354-375]`：`setMinimumSize` 存入 `extra->minw/minh`，`QWidgetItem::minimumSize` 經 `qSmartMinSize`，明確最小值覆蓋 policy 推得的值；`qSmartMaxSize` 只在沒有 GrowFlag 時把最大值夾到 hint，所以 Expanding 仍可長大。`QWidget::minimumSize()` 與 item 的 `minimumSize()` 不同（後者含 layout-item margins、隱藏時為 0），dial 無 margins 故相同。PySide6 6.11.2 oracle（Windows）：`minimumSize` 84×84、policy Expanding/Expanding、`sizeHint`／`minimumSizeHint` 為 −1×−1；`QGridLayout::setGeometry` 擠壓時 dial 不小於 84（426×200→高 84、300×312→寬 84、200×150→84×84，grid 溢出）。**已完成（RC-29）**：app 層搬運漏掉 `setMinimumSize`，未改 qtrs。測試：`test_dial_minimum_size_matches_python`、`test_squeezed_table_keeps_the_dial_at_its_minimum`；修改前 FAIL（0×0；426×200 時 dial 高 26），修改後 PASS；`test_grid_matches_qt_geometry`（450×350）不變。未改：`size_hint` 84×84 與 Python 的 −1×−1 不同（未列入本次）；視窗最小尺寸（RC-23／G12.8.q）。**僅 Windows**。分析由 2 個只讀 agent（Claude Haiku 5.5）平行完成 |

### D.3 執行階段

| Phase | 內容 | 前提 |
|---|---|---|
| 0 | Contract 清理（本次已完成）；實測 G11.5.a（已測：本機為 Layered）、G10.7.a、RC-14/15/16 的幾何 diff（已完成，見 C12.8） | 無 |
| 1 | RC-01、RC-02、RC-03、RC-04；**每個 RC 一個提交**，各自附「修改前 FAIL、修改後 PASS」的測試 | RC-03 在 RC-02 之後（同一檔案）；RC-01、RC-04 與其他獨立 |
| 2 | RC-05、RC-06 | Phase 1 完成 |
| 3 | RC-07、RC-08、RC-09、RC-10、RC-11 | RC-08、RC-09 先實測；RC-10 依賴 RC-05；RC-11 依賴 RC-06 與 RC-11a（G5.1.f；不依賴 G5.4.a） |
| 4 | RC-12 ～ RC-18；移除 RC-05/RC-06 已取代的 workaround | RC-14/15/16 的幾何 diff 已完成（皆案例 A）；RC-17/18 依賴 RC-06 |

### D.4 未列入執行佇列的項目

| Gap | 處置 | 理由 |
|---|---|---|
| G7.9.a | 降為 **P2，待驗證**（非 `D`） | 讀碼推翻 P0 主張：`Application::new`（`main.rs:319`）在 `application/mod.rs:120` 註冊 loop，早於第一個 worker／熱鍵執行緒（`hud_window.rs:408`、`main.rs:466`）；單一實例 IPC 執行緒（`main.rs:269`）在註冊前啟動，但只寫 atomic。**不等於所有 interleaving 皆安全**；它是 RC-04 的一個假設性表現，RC-04 修復後自然消除 |
| G9.6.a | 降為 **P1**（非 `D`） | Python HUD 不使用 `QStackedLayout`（重建 layout）；Qt 的 `sizeHint`／`minimumSize` 取**所有頁面**的最大值 `[QT-SRC qstackedlayout.cpp:417-448]`，qtrs 只看當前頁（`stacked.rs:123-147`）。與 Qt 不同是事實，但目前 HUD 不依賴；**P0 階段不修**，待 qtrs 的 API 範圍擴大再處理。**已修復：RC-24** |
| G12.5.d 的 timer 部分（已結案：沿用既有 `ResizeDebouncer`，未新增 timer） | 見 RC-17 | 不得為了 parity 而新增 timer／debounce，除非先證明它不是在補 qtrs 缺陷 |
