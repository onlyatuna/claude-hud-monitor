# Qt 行為對照文件

HUD 的 Python（PySide6）版與 Rust 版之間的行為對照，以及 UI 函式庫 ferrokit 對 Qt 行為的規格。ferrokit 的程式碼在 https://github.com/onlyatuna/ferrokit ，不在本 repo。

| 檔案 | 內容 |
|---|---|
| `PYTHON_QT_SEMANTIC_CONTRACT.md` | 可觀察行為契約、gap 清單與各 RC 章節；開頭有路徑與命名對照（qtrs 現為 ferrokit） |
| `QTRS_SPEC.md` | 規格書（治理規則與已遷移的章節） |
| `QTRS_SPEC_MIGRATION.md` | 舊 Contract 到規格書的遷移對照表 |
| `GEOMETRY_DIFF_RC14_16.md` | cards 模式的幾何差異稽核 |

PySide6 探針在 `rust/tools/pyside_probes/`，量測與驅動腳本在 `rust/tools/second_layer_harness/`，HUD 專屬的 Qt 對照測試在 `rust/tests/`。
