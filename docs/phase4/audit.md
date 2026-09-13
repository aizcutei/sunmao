# Phase 4 最终审计

2026-09-13 定稿，基准提交见下文各行。本文件逐项记录**核对结果与核对方式**；
未核对的要求不视为完成，核对方式写不出来的"证据"同样不算。

三平台 hosted 全绿这一条自 run 34446209752 起长期满足，但**它从来不是本文件的判据**：
本 phase 已经踩过两次"job 全绿而断言 0 次执行"（run #82 的 `GUI key verified`、
run #86 的跨线程 viz 测试当时是 flaky 的）。因此下面每一行的证据都要求指向
**实际执行过的断言或实际读过的代码**，而不是 job 结论。

## 逐项核对

| 原始要求 | 当前证据 | 判断 |
|---|---|---|
| macOS/Windows/X11 各一条真实国际输入路径 | `baseview/tests/{macos_keyboard,windows_keyboard,x11_keyboard}.rs`，各自 blocking CI 步骤。三条各有独立的完整三平台 run 与产物校验：X11 `90bf6de` / run 34255696871（`X11 KEYBOARD VERIFIED`，German/Shift/AltGr/实时布局切换/compose/focus reset）、macOS `e14e5b3` / run 34361057036（原始日志第 3107 行 `MACOS KEYBOARD VERIFIED`，Option 字符/dead-key compose/焦点重置/按键释放）、Windows `d533e54` / run 34446209752（原始日志第 2878 行 `WINDOWS KEYBOARD VERIFIED`，德语 z/ü、Shift Ü、系统 dead-key é） | **完成，边界如实标注**：覆盖国际布局与 compose，**不包含完整 CJK IME**；Windows harness 在真实 Win32 窗口线程提交物理扫描码、由系统 `TranslateMessage` 生成字符，**不是硬件 `SendInput`** |
| floating CLAP editor 的 `set_transient` | 上游 `clap/ext/gui.h` 第 19–31/183–198 行核对（show 前应先设 transient/title）→ `clap_rs` `gui_set_transient_for_api` 保留 owner 自身的 api/union 字段 → core `FloatingViewOptions` + `ViewHandle::set_transient` → baseview 三平台原生接线（Win32 owner、macOS child window、X11 `WM_TRANSIENT_FOR`，Wayland 无标准 cross-client parent handle 故如实拒绝）。测试 `floating_owner_and_title_survive_show_but_not_destroy` 覆盖 show 前不建窗、不同 create/owner API、重复 show、打开后更新/拒绝、destroy/recreate 与 embedded 拒绝 | **完成**：`324b266` / run 34590881685 三平台 job success |
| floating CLAP editor 的 `suggest_title` | 此前 backend 只保存字符串、`gui_show`/`open_floating` 不消费它，既有测试只断言字段保存——**该行在 2026-09-10 审计中被重新打开**。现已传入原生窗口标题并在打开后支持更新；标题含 NUL 时不进原生回调（属性测试守卫） | **完成**：与 transient 同提交 `324b266` / run 34590881685 验收 |
| `SpectrumAnalyzer`/meter 消费 Phase 3 metering | `sunmao/gui/src/widgets/spectrum.rs` 的 `SpectrumSource`/`StaticSpectrum`/`SpectrumAnalyzer`；`MeterSource` 消费输出第一声道的 `MeterHandle`，显示 peak/RMS（-60..0 dBFS）。prelude 导出 + doc-test、数值属性、真实 `process` 到显示的零分配/reset 断言，三平台原始日志逐条核实（Linux 4490/4491/4517/4540/4545 行等） | **完成**：`89b0d18` / run 34451013385 三平台完整 success，三份产物 SHA-256/ZIP CRC 通过。**边界**：测量输出第一声道，peak/RMS 独立原子读取，**不承诺一致快照** |
| GUI semver/state 策略 | 逐条把 `docs/phase3/compatibility.md` §2bis 对回实现：§2bis.1 列的每个受保护名字都在 `sunmao::prelude` 里实际存在（`Widget`/`ParameterWidget`/6 控件/`Column`/`Row`/`Stack`/`Theme`/`accessibility_tree`/`AccessibleNode`/`AccessibleRole`/`viz_channel`/`VizPublisher`/`VizConsumer`/`MeterSource`）；§2bis.2 引用的机械守卫逐个存在（`both_themes_keep_text_readable_against_their_surfaces`、4 个 `host_sync_never_echoes_back_to_the_host`、`a_dropdown_of_numeric_options_is_still_a_dropdown`）；`ViewHandle` 的 `resizable`/`scalable`/`keyboard` 开关签名一致 | **完成，且本轮修掉一处文档与编译器互相矛盾**：§2bis.3 承诺"新增 role 变体不算破坏性"，而 `AccessibleRole` **当时并没有 `#[non_exhaustive]`**——下游任何穷尽 `match` 都会被新变体打断，即承诺不成立。已补上该属性并用一对 doc-test 机械守卫（带 `_` 分支须编译通过、穷尽 `match` 须 `compile_fail`），crate 内的 role→AccessKit 映射保持穷尽。**守卫本身已做反向验证**：本地临时摘掉 `#[non_exhaustive]` 后，那条 `compile_fail` doc-test 立即变红（`sunmao/gui/src/accessibility.rs - accessibility::AccessibleRole (line 46) - compile fail ... FAILED`），随即还原并重新转绿——因此它不是一条永远为真的装饰 |
| 文档与矩阵一致 | 本轮逐处核对并修正：`status.md` fixture 段仍写 skeleton/未进打包（实际该 crate 内**已无 skeleton 实现**，只在模块文档里记着那段历史；打包也已在 `tools/package_examples.sh` 矩阵内，第 108 行）；`status.md` M4 小节标题仍写"平台适配器接线未做"（正文同页已描述三个适配器）；`status.md` Wayland 段仍写"给 baseview 写 Wayland 后端……才谈得上验收"（`baseview/src/wayland/` 已是那份清单）；`semantics.md` accessibility 行仍写"**未做**：把 `TreeUpdate` 交给 `accesskit_*`"；`compatibility.md` §2bis.4 仍写"平台 accessibility 桥接当前三平台都不存在"；`accessibility.rs` 的**模块 rustdoc** 仍写 "per-platform bridging is not implemented yet"（这条对外可见，不只是内部文档） | **完成**：六处全部更正为实现现状，并在原处保留"曾经判断错在哪"而非抹掉 |
| 其余 M0–M5 要求及跨层不变量 | API/prelude/doc-test 见上行；布局与主题 M2 run #77；文本/剪贴板/焦点 M3 run #84；ownership/DPI M1 run #75（`GUI scale negotiated` 三平台各 18 次，拒绝方式按格式 8/8 分裂）；Wayland 各专项见 `status.md` 完成规则一节；proptest 覆盖 `sunmao/core`、`sunmao/dsp`、`sunmao/gui`（`layout_property` / `accessibility_property` / `accesskit_property`）与 `sunmao/tests/voice_property`；零分配守卫在 `core::viz`、`gui::layout`、`gui::text_ttf`、`gui::widgets::spectrum` 与 fixture 端到端断言 | **完成** |

## 仍然成立的降级（不是遗漏，是如实上报）

这些已在 `docs/phase2/semantics.md` 与 `docs/phase3/compatibility.md` §2bis.4 记录：

- **accessibility 只读**：三平台适配器都接了，但 `ActionHandler` 不执行任何 action——
  屏幕阅读器能读、不能改。AccessKit 的 trait 要求不支持的 action 必须无动作。
- **accessibility 证据分层**：Windows 有真实 UIA 往返断言（`UIA VERIFIED`，run #100）；
  **macOS/Linux 只有编译级证据**（AXUIElement 在 CI 上受 TCC 限制，AT-SPI 需要 runner
  未运行的总线）。
- **`gui_hide` 回 `false`**：baseview 没有"隐藏但保活"，关掉再开会让宿主拿到全新编辑器、
  丢失界面状态，故如实回不支持、让宿主退回 destroy/create。
- **VST3 无 Wayland**：规范里就没有 Wayland 平台类型，VST3 在 Wayland 桌面上一律经
  XWayland。Wayland 原生路径只有 CLAP 受益。
- **Wayland 无 transient**：没有标准的 cross-client parent handle，故 `set_transient`
  在 Wayland 上如实拒绝而不是假装成功。

## 未纳入本 phase 的已知遗留

- **Tab 会停在非交互控件上**（`Stack::focus_next` 只做索引边界检查）。由 accessibility
  proptest 抓出；本 phase 的修法是让树如实上报焦点，**没有改 `Stack` 的焦点语义**，
  因为那会连带调整 M2/M3 已三平台验收过的 `focus_next`/`focus_prev` 测试与四个控件的
  键盘处理。应单独立项并单独取三平台绿。
- **`vst3_rs` 控制器包装冗余**：`ControllerWrapper` 与 `GuiControllerWrapper` 有 25 个函数
  / 260 行在归一化类型名后 ≥0.995 相同。该处用**字段偏移算术**还原 `this`，宏化必须保持
  字段顺序与 `repr(C)` 布局不变并补布局断言测试，故须单独 commit 并单独取三平台绿。
- **Windows WGPU 收尾段错误**（exit 139，run #37 一次）自 run #66 起未再复现。
  **不改判为"已修复"**——连续绿不构成对间歇性失败的证明。
- **`main` 落后于已验收的 Phase 3/4 工作**（见 `status.md`"分支基点"）。合并需仓库所有者决定。
