# Phase 4 状态

更新时间：2026-09-05

## 目标与边界

在 Phase 1（run #25 / commit `c8401e6`）、Phase 2 核心（run #38 / commit `77f788c`）、
Phase 3（run #69 / commit `b45efea`）之上，完成 GUI 组件库与平台完善。目标能力：

- renderer 资源与线程归属（GL / WGPU / WebView 三后端的设备、表面、上下文与销毁顺序），
  scale/DPI 协商（VST3 `IPlugViewContentScaleSupport` ↔ CLAP `gui.set_scale`）
- 声明式布局与主题：`Column`/`Row`/gap/padding，`Label`/`Knob`/`Slider`/`Toggle`/`Dropdown`，
  参数双向绑定（`Knob::param`）零手写回调
- text rendering 与输入：字体栅格化与文本度量、clipboard、IME/国际键盘、cursor/focus
- 可视化与 accessibility：`VizChannel` 无锁 audio→GUI 通道、`SpectrumAnalyzer`/meter 组件、
  accessibility 树、floating CLAP editor
- X11 生命周期稳定后加入 Wayland

明确不做：AU GUI 契约扩展（AU 不进默认 feature/gate，留待 Phase 7）、完整 DAW 宿主
（Phase 5）、发布签名与安装器（Phase 6）。

## 硬门槛（每个 milestone 与最终验收通用）

- 同一 commit 三平台 hosted native jobs（macOS ARM64、Windows x86_64、Ubuntu x86_64）
  全绿并上传 artifacts；本地结果只作开发证据。
- Phase 1 + Phase 2 + Phase 3 全部既有 CI 步骤保持 blocking 且绿色。
- 每个 host-facing 能力：VST3 与 CLAP 同时落地；语义差异与降级写入
  `docs/phase2/semantics.md`（附测试名）；公共 API 进 `sunmao/gui` 或新 crate
  并入 prelude（带 doc-test）。
- audio callback 成功路径零 alloc/realloc/dealloc；GUI 线程与 audio 线程只经无锁通道
  通信；新增不变量补 proptest。

## 分支基点（与 loop prompt 的偏差，需知悉）

loop prompt 写的是"不存在则从 main 切出"。**实际从 `phase3/framework-dsp-library`
的尖端 `e844215` 切出**，原因是 `main` 仍停在 Phase 2 的 `2df01ce`，Phase 3 的 33 个
commit 从未合并进 main。已用 `git merge-base --is-ancestor main HEAD` 确认新分支是
main 的严格超集，因此"从 main 切出"在祖先关系上仍然成立；若真按字面从 main 切，会丢掉
`sunmao/dsp`（M4 明确要消费它的 metering）、两个模板与已 CI 验证的清理。
**Phase 3 → main 的合并需要仓库所有者决定，本轮未自行推送 main。**

## Acceptance fixtures

| fixture | crate | 验收能力 | M0 骨架状态 |
|---|---|---|---|
| Widgets GUI | `examples/sunmao_fx_widgets_gui_gl` | M2 控件与布局、M4 可视化与无锁发布 | M0 骨架：旋钮用框架现有 `Knob`；**下拉、开关、频谱是 crate 内 skeleton**（`DropdownSkeleton`/`ToggleSkeleton`/`SpectrumSkeleton` + `SpectrumPublisher`），9 单元测试含零分配断言（macOS 本地通过） |

验收方式与 Phase 3 相同：M2 把 skeleton 换成框架 `Toggle`/`Dropdown` 与声明式布局、
M4 把 `SpectrumPublisher` 换成 `VizChannel` + `SpectrumAnalyzer`，**而这些测试语义不变**。

**该 fixture 暂未进打包矩阵，这是有意推迟而非遗漏。** M0 阶段它的下拉/开关/频谱都还是
skeleton，宿主侧可见行为不超出既有的 `sunmao_fx_gain_gui_gl`，进矩阵只增加 CI 时长与
Linux GUI 面的风险。计划在 **M2 真控件落地时接入**——Phase 3 的教训（instrument 模板
一进打包矩阵，runner 立刻在真实宿主抓出两个单测看不到的缺陷）说明这一步不能省，只是
应该在有真实宿主可见行为时做。

## Milestone 矩阵

| Milestone | 范围 | 当前判断 | 权威证据 | 下一步 |
|---|---|---|---|---|
| M0 脚手架与清理收口 | 文档、GUI fixture、workspace/CI 骨架；ABI 去重取得三平台绿 | **完成**（三平台 hosted 全绿）：清理收口由 run #71/#72 完成；GUI fixture 与新 blocking 步骤 "Test Phase 4 acceptance fixtures" 由 run #73 验收，每 job **26 步零非成功**。已下载 Linux job 日志核实该步骤**非空转**：`running 9 tests ... 9 passed; 0 failed`，其中零分配断言在 glibc 分配器下同样通过 | 脚手架：[run #73](https://github.com/aizcutei/sunmao/actions/runs/33959635350)（commit `d37a46f`）三 job success，artifacts `phase1-macOS-ARM64`（49.9MB）、`phase1-Windows-X64`（74.5MB）、`phase1-Linux-X64`（901.4MB）可下载。清理：[run #71](https://github.com/aizcutei/sunmao/actions/runs/33956858763)（commit `7dabb3d`，25 步零非成功）与 run #72（commit `e844215`，同样零非成功） | — （M0 完成；进入 M1） |
| M1 renderer 资源与线程归属 | 三后端归属与销毁顺序文档、scale/DPI 协商两格式落地 | **完成**（三平台 hosted 全绿）：`docs/phase4/ownership.md` 落地；scale/DPI 协商自 `_sys` 补起（`vst3_sys` 此前**完全没有** `IPlugViewContentScaleSupport` 绑定，IID/typedef 自上游头文件转录）→ `vst3_rs` 第二 vtable + 布局断言 → core `ViewHandle::scalable`（并删除自 Phase 1 起就无调用方、且签名 `&self` 无法改动编辑器的死钩子 `SunmaoView::set_scale_factor`）→ 两 backend 路由到**活着的** view_handle（`backend_clap` 此前从未 override `gui_set_scale`，一直告诉 CLAP 宿主“不支持缩放”）→ baseview 按 `创建尺寸 × factor` 响应 → runner `gui-test` 两格式断言 | [run #75](https://github.com/aizcutei/sunmao/actions/runs/33963105660)（commit `0b51dcb`）三 job success，每 job **26 步零非成功**，artifacts 3 份可下载。已下载三平台 job 日志核实断言**真实执行**：每平台 `GUI scale negotiated: host applied 2.0` 各 **16 次**（8 个 GUI 插件 × 2 格式），零因子拒绝亦 16 次，且**拒绝方式按格式精确分裂 8/8**（CLAP 回 `false`、VST3 回 `kInvalidArgument`），三平台完全一致 | — （M1 完成；进入 M2） |
| M2 布局与主题 | `Column`/`Row`/gap/padding、五个控件、参数双向绑定、主题 token | **完成**（三平台 hosted 全绿）：`Theme` 角色化 token（暗/亮，对比度由测试机械断言）、新增 `Toggle`/`Dropdown`（连同既有 `Knob`/`Slider`/`Label`/`Button` 共 6 控件）、声明式 `Column`/`Row`（含 6 个布局 proptest）、`ParamBinder`+`ParamHost` 两向绑定（facade 以 `ViewContextHost` 适配 `ViewContext`），fixture 换真控件后**已无逐控件回调代码**并接入打包矩阵 | [run #77](https://github.com/aizcutei/sunmao/actions/runs/33965946234)（commit `aec872f`）三 job success，每 job **26 步零非成功**，artifacts 3 份。已下载三平台日志核实新 fixture 真被宿主执行：每平台 `SunMao Widgets GL` 出现 10 次、两格式各自 `Testing:` 一次，全 run **零失败套件**。本地打包 30→**32 套件**、600→**640 断言** | — （M2 完成；进入 M3） |
| M3 text rendering 与输入 | 字体栅格化/度量、clipboard、IME/国际键盘、cursor/focus | **完成**（三平台 hosted 全绿）：`GlyphSource`/`Font`（字形缓存）+ fontdue `TtfFont`；`Clipboard`/`SystemClipboard` 与焦点控件 Ctrl/Cmd+C·V；`TextInput` 由 `Key::Character` 产生（跳过 `is_composing` 预编辑）＝国际键盘/IME 路径，三平台经 baseview 同一条；`Stack` 焦点模型 + 四个控件的键盘处理；VST3 `IPlugView::onKeyDown`/`onKeyUp` 由 stub 改为真实转发；GL `draw_text` 由空实现改为按覆盖率绘制（同覆盖率合并成行）| [run #84](https://github.com/aizcutei/sunmao/actions/runs/33976552655)（commit `8f959ba`）三 job success、每 job 26 步零非成功、artifacts 3 份。**关键证据不是 job 成功而是断言真的跑了**：三平台各 `GUI key verified` 1 次（VST3，`Gain moved 0 -> 1`）＋格式跳过 1 次（CLAP）。run #82 曾三平台全绿但该断言 0 次执行（fixture 不在 GUI 矩阵里），补入矩阵后才成立 | — （M3 完成；进入 M4） |
| M4 可视化与 accessibility | `VizChannel`、`SpectrumAnalyzer`/meter、accessibility 树、floating CLAP editor | **部分完成，两项受阻**（详见下方“M4 受阻项”）。已落地：`sunmao_core::viz` 三缓冲 `VizChannel`（audio 侧 publish 零 alloc，含跨真实线程的撕裂读检测）、`SpectrumAnalyzer`（峰值即起、落差衰减、NaN/越界收敛）、`accessibility_tree` + `AccessibleNode`/`AccessibleRole`（角色由 `ParameterWidget::accessible_role` **声明**而非从显示文本推断）、CLAP `suggest_title` 由静默 stub 改为真实转发。fixture 已从 crate 内 `SpectrumPublisher` 换成 `VizChannel`+`SpectrumAnalyzer` | [run #86](https://github.com/aizcutei/sunmao/actions/runs/33980911401)（commit `1ddc210`）三 job success，每 job **26 步零非成功**，artifacts 3 份可下载（macOS 53.0MB / Windows 78.3MB / Linux 971.2MB）。已下载三平台 job 日志核实新断言**真的执行且通过**：`the_editor_describes_itself_to_assistive_technology ... ok`、accessibility proptest 套件、`VizChannel` 跨线程撕裂读测试各 1 次/平台，`GUI scale negotiated` 由 16 增至 **18** 次（widgets fixture 入 GUI 矩阵后 9 插件 × 2 格式），三平台**零 FAILED 套件** | 已完成两项，两项受阻（见下） |
| M5 Wayland 与总验收 | Wayland、GUI 侧兼容策略、proptest/文档收尾 | **部分完成；Wayland 原生受阻（规范级，非工程取舍）**。已落地：`docs/phase3/compatibility.md` 新增 §2bis GUI 兼容（受保护面 / 视觉不承诺但语义承诺 / accessibility role 规则 / 只能记录不能承诺的宿主侧行为）；accessibility proptest 4 条；补齐 `Knob`/`Slider` 缺失的 `host_sync_never_echoes_back_to_the_host`（此前只有 `Toggle`/`Dropdown` 有，而这正是防 automation 回声环的那条不变量）；`clap_sys` 补回上游遗漏的 `uikit` 成员与 `CLAP_WINDOW_API_UIKIT` 并加 union 布局断言 | 待本轮 CI | Wayland 见下方「M5 Wayland 受阻链」 |

## 完成规则

Phase 4 完成的唯一判定：同一 commit 三平台 hosted native jobs 全绿 + artifacts 可下载
+ 本文件 Milestone 矩阵 M0–M5 全部标记完成。本地结果任何情况下都不构成完成证据。

## M4 受阻项（需要决策，不是遗漏）

这两项都写在 M4 范围里，本轮**没有交付**，原因是它们各自需要一块 Phase 4 未预算的
跨三平台底层工作。选择如实标注而不是交付一个看起来能用的版本。

### 1. floating CLAP editor —— 受阻于 baseview

`clap_plugin_gui` 的 `is_floating` / `set_transient` / `suggest_title` 已在
`clap_rs` 与 `backend_clap` 落地（`suggest_title` 本轮由静默 stub 改为真实转发，
非 UTF-8 直接丢弃而不做有损转换）。但 `is_floating` 目前**如实回 `false`**：
真正的浮动窗口要求宿主之外自己开一个顶层窗口，而 vendored 的 baseview 只提供
`open_parented`（嵌入宿主窗口）和 `open_blocking`（**阻塞调用线程直到窗口关闭**）。
在 CLAP 的 `gui_show` 里调用 `open_blocking` 会卡死宿主主线程。

要交付需给 baseview 增加"非阻塞顶层窗口"模式，并在 Win32 / AppKit / X11 三条
后端各实现一遍事件泵归属。这是独立的一块工作量，应单独立项、单独取三平台绿。
语义差异已记入 `docs/phase2/semantics.md`。

### 2. accessibility 平台桥接 —— 框架侧已完成，OS 侧未做

`accessibility_tree()` 把控件树转成 `AccessibleNode`（role / label / 可朗读值 /
归一化值 / bounds / focus / disabled），这是 UIA、NSAccessibility、AT-SPI 三家
**共同**需要的那一层，也是唯一值得测试的那一层：6 个单测 + 1 个 doc-test + fixture
上的 `the_editor_describes_itself_to_assistive_technology`（断言真实编辑器被朗读为
Slider/ComboBox/CheckBox 而非一个不透明矩形）。

未做的是把这棵树发布给操作系统：Windows 要实现 `IRawElementProviderSimple` /
`IRawElementProviderFragment` 并挂到 HWND，macOS 要在 NSView 上实现
`NSAccessibilityProtocol`，Linux 要接 AT-SPI D-Bus。三份互不复用的原生实现。

**runner 无法验收这一项**：runner 通过 VST3/CLAP 插件 API 驱动插件，而这两个格式
都没有 accessibility 通道——accessibility 走的是 OS API，必须先有上述桥接才存在可被
hosted job 断言的宿主可见行为。（runner 里既有的 UIA helper 是**测试侧**用 UIA 拖动
宿主滑块的工具，不是插件侧的可访问性实现，无法复用为验收手段。）

## M5 Wayland 受阻链（每一环都有一手依据，不是推测）

Wayland 原生支持在 Phase 4 内**无法交付**。这不是"没来得及"，而是三个独立事实叠起来
的结果，其中两个是**规范层面**的，不由 SunMao 决定：

1. **VST3 根本没有 Wayland 平台类型。** `vst3_sys/src/gui/iplugview.rs:39-42` 转录自上游
   的平台类型只有 `HWND` / `NSView` / `UIView` / `X11EmbedWindowID`。VST3 插件在 Wayland
   桌面上一律经 **XWayland** 运行——这是 VST3 规范的现状，不是本项目的取舍。
2. **CLAP 声明了 `wayland`，但上游明确说不能嵌入。** `clap/ext/gui.h` 对
   `CLAP_WINDOW_API_WAYLAND` 的原文注释是 *"embed is currently not supported, use floating
   windows"*（已连同 `uikit` 一并补进 `clap_sys/src/ext/gui.rs` 的文档注释）。也就是说
   **CLAP 在 Wayland 上要求浮动窗口**。
3. **浮动窗口正是 M4 那条受阻项。** 于是 Wayland 与 floating editor 卡在**同一块**
   baseview 工作上；而且 baseview 的 Linux 后端本身就是 X11 独占——`baseview/src/lib.rs:6`
   只有 `mod x11`，全树零 Wayland 引用。

**结论：** 真正的 Wayland 原生支持 = 给 baseview 写一个 Wayland 后端（当前不存在）
＋ 非阻塞顶层窗口模式（M4 同一块）＋ 只有 CLAP 能受益（VST3 侧永远走 XWayland）。
这是独立立项的规模，应当单独取三平台绿，而不是塞进 Phase 4 收尾。

**现状不是"不能在 Wayland 上用"**：经 XWayland，X11 路径在 Wayland 桌面上照常工作，
这也是目前 Ubuntu hosted job 所验证的那条路径。

## 已知遗留

- **Tab 会停在非交互控件上**：`Stack::set_focus`/`focus_next` 只做索引边界检查，
  不区分子控件是否可交互，所以 Tab 可以把焦点停在 `SpectrumAnalyzer` 这种纯显示件上。
  这是**由 accessibility proptest 抓出来的**（`focus_is_reported_exactly_once_and_matches_the_stack`
  用一个只含 Display 的编辑器 shrink 出最小反例）：当时 `accessibility_tree` 对非参数
  子节点硬编码 `focused: false`，于是树说"无焦点"而 stack 说"焦点在 0"——桥接层会把
  屏幕阅读器的光标指向空处。**本轮的修法是让树如实上报**（树必须镜像 stack，不能替它
  隐瞒），而没有改 `Stack` 的焦点语义：跳过不可聚焦子节点是 M2/M3 已经三平台验收过的
  行为，此时改动要连带调整 `focus_next`/`focus_prev` 的既有测试与四个控件的键盘处理，
  应当单独立项并单独取三平台绿。

- **vst3_rs 控制器包装仍有冗余**：`ControllerWrapper` 与 `GuiControllerWrapper` 有 25 个
  函数 / 260 行在归一化类型名后 ≥0.995 相同。可仿 `clap_rs` 的 `audio_ports_config_ext!`
  用 `$bound`/`$type` 宏收敛，但该处用**字段偏移算术**还原 `this`，宏化必须保持字段顺序
  与 `repr(C)` 布局不变并补布局断言测试，故须单独 commit 并单独取得三平台绿。
- **Windows WGPU 收尾段错误**（exit 139，run #37 一次）自 run #66 起在 #66/#68/#69/#71/#72
  连续五次未复现。**仍不改判为"已修复"**——连续绿不构成对间歇性失败的证明。若在 M1 复现，
  按计划深入 WGPU/D3D 析构路径，不盲目重试。
- `main` 落后于已验收的 Phase 3 工作 33 个 commit（见上文"分支基点"）。
