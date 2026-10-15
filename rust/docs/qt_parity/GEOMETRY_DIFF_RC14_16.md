# RC-14 / RC-15 / RC-16 幾何差異稽核（geometry differential audit）

範圍：Python（PySide6）HUD 與 Rust HUD 的 cards 模式，逐 widget 比較 rect、sizeHint、
minimumSize／minimumSizeHint、maximumSize、layout spacing／margins、size policy、字型度量、文字寬度。
**本稿沒有修改任何 production code。** 新增的只有稽核用的測試與腳本（見「重現」）。
結論已併入 `PYTHON_QT_SEMANTIC_CONTRACT.md` 的 D.2 與 C12.8。

## 方法

- **Python oracle**：`rust/tools/geometry_audit/py_geom.py` 啟動真正的 `HUDWindow`（`ui_mode=cards`、dark、
  125 % DPR），`show()` 後處理事件，輸出每個 widget 的 rect（視窗座標）、`sizeHint`、`minimumSizeHint`、
  `minimumSize`、`maximumSize`、`sizePolicy`、字型（family、px、weight、`height/ascent/descent`）、文字、
  `alignment`、`textWidth`，以及各層 layout 的 `spacing`／`margins`／`sizeHint`。
- **Rust 端**：`rust/src/ui/geometry_audit.rs`（`#[cfg(test)]`、`#[ignore]`）建立真正的 `HUDWindow`，`show()` 後
  輸出同樣的欄位。測試先呼叫 `set_application_device_pixel_ratio(1.25)`，與 `Application::new` 在這台機器
  上做的事一致；不設的話 qtrs 會用 GDI 文字引擎而不是 DirectWrite，`QLabel` 高度會少 1 px（見「陷阱」）。
- **反事實變體（what-if）**：只用公開 widget API 從外部改設定，不動 production code，量「改成 Python 的值之後
  差異是否消失」。這是判斷「Rust 的值是不是在補 qtrs 差異」的依據：若 qtrs 有底層差異，改成 Python 的值後
  仍然對不上。

| 變體 | 內容 |
|---|---|
| V0 | 原樣 |
| V1 | V0 ＋ 指標值 label 補 Python 的 widget-local `font-size: 14px` |
| V2 | V1 ＋ 卡片根 spacing 5 |
| V3／V4 | V2／V0 ＋ 移除 Badge 的 `max-height: 15px` |
| V5 | V2 ＋ Python 的容器結構（header policy、body 不設 stretch、橫向 spacing 8、直向卡片不設 stretch） |
| V6 | V5 ＋ cards container 與 stack 的 size policy 改為 Preferred（Python 預設） |
| V7 | V6 ＋ 移除 Badge 的 `max-height` |

- **比較視窗大小**：橫向 690×145。直向 280×463：Rust 的 `MIN_VERTICAL_HEIGHT` 是 463（`config.rs`），不能小於
  它，所以 Python 也用 463（Python 預設 410）。
- **狀態**：預設佔位文字（`--`、`重設於: --`），沒有載入即時資料。時鐘文字 `--:--:--` 未變動，不需遮罩。
- **不是 snapshot**：比較的是 widget 幾何，沒有用像素。

## 結果摘要

橫向（690×145），三張卡片＋HUD 標頭共 45＋5 個有 rect 的 widget；「y/h」是垂直位置與高度不吻合的 widget 數：

| 變體 | y/h 不吻合 | x/w 不吻合 |
|---|---|---|
| V0 原樣 | 18 | 43 |
| V1 ＋字級 14px | 18 | 43 |
| V2 ＋spacing 5 | **0** | 41 |
| V3／V4 無 badge max-height | 0 | 41 |
| V5 ＋Python 容器結構 | 0 | 5 |
| V6／V7 | 0 | 5 |

直向（280×463）：

| 變體 | y/h 不吻合 | x/w 不吻合 |
|---|---|---|
| V0 原樣 | 49 | 11 |
| V2 | 41 | 5 |
| V5 | 41 | 5 |
| V6 ＋Preferred 容器 | **1**（`layout_toggle_btn`，見 G12.8.g） | 5 |
| V7 無 badge max-height | 1 | 5 |

逐 widget 的完整表由 `python rust/tools/geometry_audit/compare.py` 輸出。

## RC-14：卡片根 spacing 2 vs 5（G9.4.b、G12.5.b）

- **Python**：`layout.setSpacing(5)`（`provider_card.py:27`）；header `setSpacing(4)`、`m*_box.setSpacing(1)`。
- **Rust**：`set_spacing(2)`（`provider_card.rs:159`）；header 4、`m*_box` 1。**子層 spacing 與 Python 相同**。
- **first divergence（由下往上）**：
  1. 最低層的度量一致：`dot`／`title`／`m*_label`／`m*_sub`／`badge` 的 `sizeHint` 高度、字型 `ascent/descent/height` 在
     Python 與 Rust 相同（Segoe UI 10px 14、Consolas 9px 15（含 padding）等）。
  2. 第一個分歧：`m1_val`／`m2_val` 的有效字級。Python 在 widget 上 `setStyleSheet("font-size: 14px;")`
     （`provider_card.py:66,93,162,170`），Rust 只有 `set_font(Consolas 14)`（`provider_card.rs:212`）；
     套用 app sheet 的 `QLabel#MetricValue { font-size: 16px }` 後，**有效字級是 16**（這符合 Qt：樣式表字級
     勝過 `setFont`）。結果 `sizeHint` 高度 19 對 17。（G12.8.a）
  3. 第二個分歧：根 spacing 2 對 5。
  4. 在 145 px 高的視窗裡，兩邊的內容都超過可用高度（Python 109 > 103），都由 `qGeomCalc` 的「低於 hint」分支
     擠壓；Rust 107 > 103。
- **反事實**：V1（字級改 14，spacing 仍 2）不夠；V2（字級 14＋spacing 5）後，**橫向 45/45 個 widget 的 y/h 全部與
  Python 相同**（包含被擠壓後的 `m1_box` 35、`m1_val` 14）。也就是 qtrs 的 label 高度、`QBoxLayout`／
  `qGeomCalc` 在 Python 的輸入下重現了 Python 的幾何，**沒有 qtrs 高度差需要 spacing 2 來補**。
- **判定：案例 A（應用層搬運錯誤）。** 修復 owner：HUD（`provider_card.rs`）。修復必須同時補 G12.8.a 的字級，
  否則 spacing 5 會讓內容更超出（`[INFERENCE]` 2 可能是為了縮小 16px 字級造成的溢出而選的；沒有找到作者意圖的紀錄，
  兩者都在 `93ec2c0` 一次提交進來）。
- 不能單獨改 spacing：V0 → 只改 spacing 之前，高度 hint 為 107（比 103 大），改成 5 會是 113，擠壓更多。

## RC-15：Badge `max-height: 15px`（G12.3.b、G12.5.a）

- **Python**：`QLabel#Badge` 無 `max-height`（`styles.py:116-124`）。實測 `sizeHint` = 25×15，`maximumSize` = 無限。
- **Rust**：`styles.rs:192,291` 有 `max-height: 15px`。`Label::size_hint` 以 `max_height.or(min_height)` 當作高度 hint
  （`label.rs`，G12.8.h），所以有 max-height 時 hint 就是 15。
- **first divergence**：沒有分歧。移除 `max-height`（V4、V3、V7）後：Rust badge `sizeHint` 仍是 25×15，與 Python 相同；
  橫向 y/h 不吻合 0、直向與 V6 相同。文字度量：Consolas 9px 的 `layout_height` = 10.515625（DirectWrite，
  `FontMetrics::layout_height`），`ceil` 為 11，加上 border 2＋padding 2 = 15；Python 的 `QFontMetrics.boundingRect`
  高度也是 11，同樣得 15。
- **判定：案例 A（多餘的屬性）。** 不是在補 qtrs 差異。owner：HUD（`styles.rs`）。
- **限制**：只在 DPR 1.25（DirectWrite 路徑）量過。DPR 1.0 時 qtrs 走 GDI 路徑，同一個 label 是 14；Python 在
  DPR 1.0 下的值**沒有量**，所以「100 % 縮放下可安全移除」未驗證。移除前應在 100 % 縮放的機器上跑同一個
  oracle（`py_geom.py`＋`geometry_audit`）。
- **另一個由 Qt 原始碼確認的語意差異**：Qt 的 `max-height` 設的是 `rule.boxSize(...)`，即**內容高度加 padding 與
  border**（`qstylesheetstyle.cpp:2603-2611`）。Python 若有 `max-height: 15px`，`maximumSize().height()` 會是 19
  （實測 19）；qtrs 把它當作總高度（G12.8.g）。所以即使保留這個屬性，兩邊的語意也不同；移除它沒有這個問題。

## RC-16：header 的 `Expanding/Fixed`（G9.3.c）

- **Python**：header 是 `QHBoxLayout`，沒有 size policy；`expandingDirections()` = 水平（實測 1）。
- **Rust**：`header_widget` 是一個 `EmptyWidget`，policy `Expanding/Fixed`（`hud_window.rs:272-275`）；外層 `stack`
  `Expanding/Expanding`，`cards_container` `Expanding/Expanding`，根 layout 對 `stack` 設 stretch 1。
- **橫向 145 px**：沒有多餘高度，header policy 不造成差異（V0 的橫向 y/h 差異來自 RC-14，不是 header）。
- **直向 463 px**：Python 的多餘高度（68 px）全部給 header（title_label 18 → 86），卡片維持 hint（109）；
  Rust 給卡片（每張 132）。**first divergence**：根 layout 的 stretch／policy，而不是 `qGeomCalc`：
  - V5（header 改 Preferred、stack 不設 stretch、橫向 spacing 8、直向卡片不設 stretch）：直向 y/h 仍有 41 個不吻合，
    因為 `stack` 與 `cards_container` 的 policy 仍是 `Expanding`，`qGeomCalc` 會把所有多餘空間給 expansive 的項目。
  - V6（再把這兩個 policy 改成 Preferred）：直向 y/h 只剩 1 個（`layout_toggle_btn`，G12.8.g），**卡片、header、
    子 widget 全部與 Python 相同**。
  - 這說明 `QBoxLayout` 在「沒有 stretch、沒有 expansive」時的分配（標頭取走剩餘、容器取 hint）qtrs 已經完全
    重現；差異全在 HUD 設的 policy／stretch。
- **判定：案例 A，但範圍比 D.2 寫的大。** 只拿掉 header 的 `Expanding/Fixed` 不會讓直向與 Python 一致，必須一起處理
  `cards_container` 與 `stack` 的 policy、根 stretch、直向卡片 stretch（G12.8.c）。owner：HUD（`hud_window.rs`）。
  V5／V6 是同時套用的，沒有逐項隔離各自的貢獻。
- **副作用要先決定**：照 Python 做，直向模式下標頭列會在視窗變高時跟著變高（68 px），這是 Python 的實際行為，
  但看起來不一定是預期的設計。是否照搬需要使用者決定（它不是 qtrs 的問題）。

## 其餘發現（不屬於 RC-14/15/16，已登記為 G12.8.*）

| 項目 | Python | Rust | 層級 |
|---|---|---|---|
| a. 指標值字級 | `font-size: 14px`（widget-local） | 16px（app sheet 蓋過 `set_font(14)`） | 應用 |
| b. 橫向 body spacing | 8（`hud_window.py:337`） | 預設 6 → 卡片寬 213 對 211 | 應用 |
| c. 直向容器 policy／stretch | 不設 | `Expanding`、stretch 1 | 應用（RC-16 的延伸） |
| d. `title` size policy | `Minimum/Preferred`（`provider_card.py:34`） | 預設 | 應用 |
| e. badge 字重 | 400 | Bold（`set_font(...Bold)`，QSS 沒有字重） | 應用 |
| f. 視窗大小常數 | 橫向最小高 125、預設 145；直向最小 320、預設 410 | 130、152；463（由 layout 推導）、490 | 應用 |
| g. QSS `min/max-width/height` 的盒模型 | 內容＋padding＋border（`toggle` 最大高 22、最小寬 28） | 總尺寸（18、18） | **qtrs** |
| h. `Label::size_hint` 用 `max-height` 當 hint | 無此規則 | `label.rs` | **qtrs**（READ） |
| i. `QProgressBar` hint | `sizeHint` 91×5、`minimumSizeHint` 91×17 | 160×5、0×5 | **qtrs** |
| j. 文字寬度 1 px | `WEEKLY 7D` 59、`AI AGENT HUD (3-IN-1)` 144 | 59.589 → 60、144.107 → 145（`ceil`） | **qtrs** |

g 與 j 是剩下的 x/w、y/h 殘差的第一分歧（V7：y/h 1 個、x/w 5 個，全部來自這兩項）。

## 陷阱（避免重複）

- Rust 稽核測試若沒呼叫 `set_application_device_pixel_ratio(1.25)`，`uses_directwrite_engine()` 為假，
  `layout_height` 走 GDI 的取整路徑（Consolas 9px = 10 而不是 10.515625），badge 會是 14 而不是 15，
  得出「`max-height` 在補 qtrs 差異」的**錯誤結論**。第一輪我就是這樣量錯的；加上 DPR 後結論相反。
- 稽核測試的 `children()` 取得的是 widget 樹；`EmptyWidget::layout()` 回 `None`，要用 `layout_ref_mut()`。

## 沒有驗證的事

- 只量了 DPR 1.25、Windows 字型（Segoe UI、Consolas、JhengHei UI）、預設佔位文字。DPR 1.0、其他字型、即時資料
  （含 CJK 的 badge 文字、錯誤訊息）沒有量。
- 沒有逐項隔離 V5／V6 內各設定的貢獻。
- 沒有做像素比較；這份報告不證明像素一致。
- `agy`／`codex` 卡片的結果與 `claude` 一致（統計包含三張），但逐 widget 表只列 `claude`。

## 重現

```
# Python oracle（需要 PySide6；結果已提交於 rust/tools/geometry_audit/results）
cd python
python ../rust/tools/geometry_audit/py_geom.py horizontal ../rust/tools/geometry_audit/results/py_horizontal.json
VH=463 python ../rust/tools/geometry_audit/py_geom.py vertical ../rust/tools/geometry_audit/results/py_vertical.json
# Rust（8 個變體 × 2 模式；每個變體 ~15 s）
cd ../rust
GEOM_OUT=<dir> cargo test -j 1 geometry_audit -- --ignored --nocapture --test-threads=1
python tools/geometry_audit/compare.py <dir>
```
