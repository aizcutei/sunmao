# Phase 4 进展日志

按时间追加，格式固定：

```text
### YYYY-MM-DD — <milestone>
- Command/platform:
- Result:
- Evidence/artifact:
- Unresolved:
```

### 2026-09-05 — M0 脚手架：GUI fixture 并入 workspace 与 CI

- Command/platform: macOS ARM64，分支 `phase4/gui-component-library`。
  **基点是 `phase3/framework-dsp-library` 的尖端 `e844215`，不是 main**——main 仍停在
  Phase 2 的 `2df01ce`，Phase 3 的 33 个 commit 从未合并。已用
  `git merge-base --is-ancestor main HEAD` 确认新分支是 main 的严格超集，故"从 main 切出"
  在祖先关系上成立；若按字面从 main 切会丢掉 `sunmao/dsp`（M4 要消费其 metering）、
  两个模板与已 CI 验证的清理。Phase 3 → main 的合并留给仓库所有者。
- Change:
  - 新建 Phase 4 acceptance fixture `examples/sunmao_fx_widgets_gui_gl`，覆盖 Phase 4 要
    交付的四类控件：旋钮（连续，用框架现有 `Knob`）、下拉（离散，`IntParam`——`EnumParam`
    尚不存在）、开关（布尔）、频谱（audio→GUI 数据，非控件）。下拉/开关/频谱是 **crate 内
    skeleton**（`DropdownSkeleton`/`ToggleSkeleton`/`SpectrumSkeleton`），与 Phase 3 fixture
    先携 inline DSP 的做法一致；M2/M4 用框架组件替换它们时**测试语义必须不变**。
  - audio→GUI 走 `SpectrumPublisher`：每 band 一个 `AtomicU32` 位存 f32，audio 侧每块
    relaxed store 一次，GUI 侧绘制时读。无锁、无分配。8 个 band 用 `sunmao/dsp` 的 `Svf`
    带通分析，音色用 `OnePole`——即 Phase 3 组件在 Phase 4 的第一个消费方。
  - `.github/workflows/phase1.yml` 新增 blocking 步骤 "Test Phase 4 acceptance fixtures"
    （与 Phase 3 步骤同构：逐 crate `cargo test --locked -p` + 失败回显日志尾部 +
    `cargo build` 覆盖 cdylib 路径）。每 job 步骤数 25 → 26。
  - 新建 `docs/phase4/{status,progress}.md`。
- Result:
  - fixture 9 单元测试通过，其中 `process_and_spectrum_publish_do_not_allocate` 复用
    backends/`voice.rs` 的 `GlobalAlloc` 计数器把"audio 路径零分配"**机械钉住**。
    **该断言经反向验证有效**：临时在计数区间内插入一次 `vec![0; 64]`，测试如期失败并
    报告 "allocated 2 times"，随后还原——一个不会失败的零分配测试没有价值。
  - 完整 `RUSTFLAGS=-Awarnings cargo test --locked` **exit 0，126 套件 / 536 测试全绿**
    （Phase 3 基线 124 / 527，增量恰为本 fixture 的 lib + doc-test 两套件与 9 测试）。
  - `cargo check --locked --target x86_64-pc-windows-msvc -p sunmao_fx_widgets_gui_gl`
    exit 0。
  - `tools/package_examples.sh --debug --test` exit 0，**30 套件 / 600 断言，与 Phase 3
    基线逐位相同**——Phase 1/2/3 回归无损。
  - `cargo metadata --locked`、`cargo fmt --all -- --check`、`git diff --check`、
    `bash -n tools/package_examples.sh` 全过。
  - `nm -gU` 复查新 cdylib：导出 `_GetPluginFactory`、`_clap_entry`、`_bundleEntry`/
    `_bundleExit` 与两个 pixel probe 钩子，共 7 个符号，**AU 符号 0 个**。
- Evidence/artifact: macOS ARM64 本地日志（`/tmp/p4_test.log`、`/tmp/p4_pkg.log`、
  `/tmp/p4_win.log`）——本地证据等级，不构成验收。
- Unresolved:
  - 本 commit 需三平台 hosted 全绿（Phase 1+2+3 既有 gate 与新增 Phase 4 步骤同时绿）
    M0 才算完成。
  - fixture **暂未进打包矩阵**，是有意推迟到 M2 真控件落地时（理由见 status.md）；
    Phase 3 的教训是这一步不能省，只是应在有真实宿主可见行为时做。
  - M1（renderer 资源与线程归属、scale/DPI 协商）未开始。
  - M0 的"清理收口"一项在开 Phase 4 之前即已完成：[run #71](https://github.com/aizcutei/sunmao/actions/runs/33956858763)
    （commit `7dabb3d`，ABI 去重 −1567 行）与 run #72（commit `e844215`，CLAUDE.md 精简）
    各自三平台 25 步零非成功、artifacts 齐备。

### 2026-09-05 — M0 完成：hosted run #73 三平台全绿

- Command/platform: push `d37a46f` 触发 GitHub Actions #73：
  https://github.com/aizcutei/sunmao/actions/runs/33959635350
- Result: macOS ARM64、Windows x86_64、Ubuntu x86_64 三个 job 同一 commit 全部 success，
  每 job **26 步零非成功**（skip 仅为平台不适用项与未触发的失败诊断上传）。新增的 blocking
  步骤 "Test Phase 4 acceptance fixtures" 三平台均 success；Phase 1/2/3 既有 gate
  （GUI matrix、standalone、packager、runner、Phase 2/3 fixture、realtime 分配矩阵、
  proptest）保持绿色。Windows WGPU 收尾段错误未复现（自 run #66 起连续第 6 次）。
- Evidence/artifact: run #73 上传 `phase1-macOS-ARM64`（49.9MB）、`phase1-Windows-X64`
  （74.5MB）、`phase1-Linux-X64`（901.4MB），均可下载且未过期。
  **另下载 Linux job 原始日志核实新步骤非空转**：`running 9 tests` → `9 passed; 0 failed`，
  九项逐条列出，其中 `process_and_spectrum_publish_do_not_allocate` 在 glibc 分配器下
  同样通过——零分配结论不只在 macOS 成立。一个只会 success 而不真正跑测试的 gate，
  会让后面每个 milestone 的证据失效，所以这一步单独核实。
- Unresolved: M0 完成，进入 M1（renderer 资源与线程归属、scale/DPI 协商）。
  fixture 进打包矩阵推迟到 M2（理由见 status.md）。

### 2026-09-05 — M1：scale/DPI 协商两格式落地 + renderer 归属文档

- Command/platform: macOS ARM64，分支 `phase4/gui-component-library`。
- Change（自底向上，每层带测试）：
  - **`_sys`（缺口在此）**：`vst3_sys` **完全没有 `IPlugViewContentScaleSupport` 绑定**。
    新增 `IPlugViewContentScaleSupportVtbl`、`ScaleFactor` 与 IID
    `0x65ED9690,0x8AC44525,0x8AADEF7A,0x72EA703F`——三者均从上游
    `pluginterfaces/gui/iplugviewcontentscalesupport.h` **逐字转录**（本机未 vendored SDK 头，
    故直接取上游源文件，未凭记忆）。CLAP 侧 `clap_sys::set_scale` 早已存在。
  - **`_rs`**：`vst3_rs::PlugViewWrapper` 新增第二个 vtable，`queryInterface` 按 IID 交出该
    可选接口，其 IUnknown 三件套转发到视图自身的 refcount（宿主持有独立接口指针）。
    `GuiPlugin::gui_set_scale(f32) -> bool` 为新钩子。`vtbl_scale` 必须紧邻 `vtbl`——
    `from_scale` 靠减一个指针宽度还原 `this`——该布局由
    `plug_view_scale_vtbl_is_one_pointer_after_vtbl` 机械钉住，而不是只写在注释里。
  - **core**：`ViewHandle::scalable(value, resize, set_scale)` + `ViewHandle::set_scale`，
    与既有 `resizable` 同形。非有限/非正因子在此**统一挡掉一次**，两个 wrapper 不重复实现。
    顺带**删除死钩子** `SunmaoView::set_scale_factor`：它自 Phase 1 起就无调用方，且签名是
    `&self`，根本无法改动编辑器——真正能承载的是持有 `&mut` 的 `ViewHandle`。
  - **backend（两格式的真实缺口）**：`sunmao/backend_clap` **从未 override**
    `GuiHandler::gui_set_scale`，一直用默认实现回 `false`，即 CLAP 宿主被告知"不支持缩放"；
    VST3 侧连绑定都没有。两侧现均路由到**活着的** `view_handle`（而非 `plugin.view()` 新建的
    视图对象——那样因子到不了宿主正在显示的编辑器）。CLAP 侧把 `f64` 收窄为 `f32`，
    超出 `f32::MAX` 的值**拒绝**而不是让它变成 infinity。
  - **view_baseview**：三个后端的 `ViewHandle` 换成 `scalable`，因子实现为"窗口重设为
    **创建尺寸 × factor**"。基准尺寸存在 `ScalableWindow` 里而不是从当前窗口尺寸推导——
    否则连续两次 1.5 会复合成 2.25。
  - **runner**：`HostPlugin::set_gui_scale(f64) -> Result<bool, String>`，VST3 侧走真实
    `queryInterface` + `setContentScaleFactor`，CLAP 侧走 `clap_plugin_gui.set_scale`；
    `gui-test` 末尾断言两格式都能应用 2.0、拒绝 0.0、恢复 1.0。放在最后是为了让被放大的
    窗口不干扰前面的像素与手势检查。
  - **文档**：`docs/phase4/ownership.md`（三后端设备/表面/上下文归属与销毁顺序、Linux 专用
    GTK 线程与 100ms drain 的既有修复、Windows WGPU exit 139 的排查起点、对 M2–M4 的约束）；
    `docs/phase2/semantics.md` 新增 "GUI DPI scale 协商" 行。
- Result:
  - 完整 `RUSTFLAGS=-Awarnings cargo test --locked` **exit 0，126 套件 / 540 测试**
    （M0 为 536，增量 4 项即本轮新增测试）。
  - **本地实测两格式端到端**：`gui-test` 对同一 GL 插件的 `.vst3` 与 `.clap` 分别
    exit 0，日志为 `GUI scale negotiated: host applied 2.0`。**两格式的拒绝信号方式不同**且
    均被如实记录：VST3 回 `kInvalidArgument`（宿主侧呈现为错误），CLAP 回 `false`。
  - `tools/package_examples.sh --debug --test` exit 0，**30 套件 / 600 断言**，与基线逐位相同。
  - 触及平台代码的 9 个 crate 的 `--target x86_64-pc-windows-msvc` 检查 exit 0。
    （注：整 workspace 交叉编译会在 `au_sys` 失败，那是 macOS-only crate 的既有情况，
    与本轮无关——AU crates 本轮零改动；CI 的 Windows 覆盖走 native runner 而非交叉编译。）
  - `nm -gU` 复查：GUI 插件仍只导出 `GetPluginFactory` + `clap_entry`，**AU 符号 0 个**。
  - `cargo fmt --all -- --check`、`git diff --check`、`cargo metadata --locked` 全过。
- Evidence/artifact: macOS ARM64 本地日志（`/tmp/m1_test.log`、`/tmp/m1_pkg.log`、
  `/tmp/m1_win2.log`）——本地证据等级。
- Unresolved:
  - 本 commit 需三平台 hosted 全绿才算 M1 完成。
  - M1 第三项"Windows WGPU exit 139 若复现则定位"：本轮**未复现**，故未做定位；
    `ownership.md` 已写下复现时的排查起点（`WgpuHandler` drop 必须早于窗口销毁）。
  - 本轮只协商 scale，未做 M2 的布局/主题。

### 2026-09-05 — M1 完成：hosted run #75 三平台全绿

- Command/platform: push `0b51dcb` 触发 GitHub Actions #75：
  https://github.com/aizcutei/sunmao/actions/runs/33963105660
- Result: 三个 job 同一 commit 全部 success，每 job **26 步零非成功**。Phase 1/2/3 既有 gate
  与 Phase 4 fixture 步骤同时绿。Windows WGPU 收尾段错误未复现（自 run #66 起连续第 8 次）。
- Evidence/artifact: artifacts 3 份可下载。**另下载三平台 job 原始日志核实断言非空转**：
  每平台 `GUI scale negotiated: host applied 2.0` 各出现 **16 次**（8 个 GUI 插件 × VST3/CLAP
  两格式），零因子拒绝同样 16 次；且拒绝方式**按格式精确分裂 8/8**——CLAP 回 `false`
  （日志 "correctly refused a zero factor"）、VST3 回 `kInvalidArgument`（日志 "refused a zero
  factor with an error"），三平台完全一致。这同时反向确认了 semantics.md 里"两格式拒绝信号
  方式不同"那条记录是准确的，而不是只在 macOS 上观察到的巧合。
- Unresolved: M1 完成，进入 M2（布局与主题：`Column`/`Row`/gap/padding 与
  `Label`/`Knob`/`Slider`/`Toggle`/`Dropdown`、参数双向绑定、主题 token）。
  M2 同时要把 `sunmao_fx_widgets_gui_gl` 的三个 skeleton 换成框架控件并接入打包矩阵。

### 2026-09-05 — M2：布局与主题、五个控件、参数双向绑定

- Command/platform: macOS ARM64，分支 `phase4/gui-component-library`。
- Change:
  - **主题 token**（`sunmao/gui/src/theme.rs`）：按**角色**命名（`surface`/`accent`/`track`/`muted`）
    而非按外观，这正是同一份控件代码能在 `Theme::dark()` 与 `Theme::light()` 下都正确渲染的
    前提。`Color::luminance()` 为此新增，测试用它**机械断言两套主题的前景/背景对比度**都够读
    ——只在暗色下好看的控件是那种"用户切主题才发现"的回归。
  - **两个新控件**：`Toggle`（布尔）与 `Dropdown`（离散）。两者都实现 `ParameterWidget`，
    值一律走归一化 `f32`——这是 VST3 与 CLAP 表达布尔/步进参数的共同形式。`Dropdown`
    把选项 `i/(n-1)` 映射为归一化值并在读回时**四舍五入到最近步**，单选项时钉在 `0.0`
    而不是除以零。
  - **声明式布局**（`Column`/`Row`/`gap`/`padding`/`child`）：主轴保持子控件自身尺寸、
    交叉轴拉伸填满，规则小到可以记住；没有半吊子 flex。`content_extent()` 供编辑器按内容
    请求宿主改窗口大小。
  - **参数双向绑定**（`ParamBinder` + `ParamHost`）：编辑器过去每个控件都要手写三件事——
    绘制前拉取宿主值、编辑后推回、拖拽期间用 begin/end-edit 把一次手势包成一条自动化。
    现在 binder 对整棵树做完这三件事。为此在 `Widget` 上加了 `as_parameter()`，
    binder 借它在树里找控件而**不必 downcast 到具体类型**（否则 binder 要认识所有未来控件）。
    `sunmao_gui` 不依赖 `sunmao_core`，故 `ParamHost` 是 gui 侧自定义的最小面，
    由 facade 的 `ViewContextHost` 适配 `ViewContext`（本地类型，避免 orphan rule）。
  - **fixture 换真控件并进打包矩阵**：`ToggleSkeleton`/`DropdownSkeleton` 删除，编辑器改为
    `Column` + `Knob`/`Dropdown`/`Toggle` + `ParamBinder`，**本文件里已无任何逐控件回调代码**。
    `SpectrumSkeleton` 保留（M4 才换 `VizChannel`）。CI 的打包矩阵与 `package_examples.sh`
    同时加入该 fixture。
- Result:
  - 完整 `cargo test --locked` **exit 0，127 套件 / 578 测试**（M1 为 126 / 540）。
    新增含 `sunmao_gui` 的 **6 个布局 proptest**（相邻不重叠、尺寸永不为负、主轴尺寸保持而
    交叉轴填充、`content_extent` 与实际落位一致、**重复 layout 幂等**、逐边 padding）。
  - **一个真实缺陷由本轮新写的测试当场抓出**：`Dropdown::set_value` 收到 NaN 时会回退到
    index 0，即宿主发一个非有限值就会把控件**静默跳到第一个选项**。改为
    `index_for_value` 返回 `Option` 并让调用方保持原选择——与 `Toggle`/`Knob` 的既有约定一致。
  - `tools/package_examples.sh --debug --test` exit 0，**32 套件 / 640 断言**
    （M1 为 30 / 600，增量恰为新 fixture × 两格式）。**该 fixture 在两格式真实宿主下各 20/20**
    ——这正是 M0 推迟进矩阵时说的"有真实宿主可见行为时再做"。
  - `--target x86_64-pc-windows-msvc` 检查 exit 0；fmt/diff/metadata 全过。
- Evidence/artifact: macOS ARM64 本地日志（`/tmp/m2_test.log`、`/tmp/m2_pkg.log`）。
- Unresolved:
  - 本 commit 需三平台 hosted 全绿才算 M2 完成。
  - **两个 skeleton 单测随类型一起删除**（`toggle_only_reacts_inside_its_bounds`、
    `dropdown_cycles_and_clamps`）。不算覆盖倒退：它们测的是已删除的类型，等价行为现由
    `sunmao_gui` 的 `Toggle`（6 测试）与 `Dropdown`（11 测试）覆盖，另在 fixture 层新增
    `the_editor_binds_one_control_per_parameter` 断言控件树与绑定。**这一点如实记下，
    因为 M0 当时写的是"这些测试跨替换保持不变"，实际是行为测试不变、skeleton 测试被取代。**
  - `Label`/`Slider`/`Button` 已存在，M2 未重写；M3 的字体栅格化落地后再看 `Label` 的度量。

### 2026-09-05 — M2 完成：hosted run #77 三平台全绿

- Command/platform: push `aec872f` 触发 GitHub Actions #77：
  https://github.com/aizcutei/sunmao/actions/runs/33965946234
- Result: 三个 job 同一 commit 全部 success，每 job **26 步零非成功**。Windows WGPU 收尾
  段错误未复现（自 run #66 起连续第 10 次）。
- Evidence/artifact: artifacts 3 份可下载。**另下载三平台日志核实新进矩阵的 fixture 真被宿主
  执行**：每平台 `SunMao Widgets GL` 出现 10 次（打包两格式 + 两格式各测一次），
  `Testing: SunMao Widgets GL (VST3)` 与 `(CLAP)` 均在场，且全 run **零失败套件**
  （`Summary: N passed, ≥1 failed` 匹配数为 0）。这正是 M0 推迟进矩阵、M2 兑现的那一步。
- Unresolved: M2 完成，进入 M3（text rendering 与输入：字体栅格化与文本度量、clipboard、
  IME/国际键盘、cursor/focus 模型，runner 断言按键→参数变化可观测）。

### 2026-09-05 — M3 上半：字体栅格化与文本度量

- Command/platform: macOS ARM64，分支 `phase4/gui-component-library`。
- 出发点（先查现状再动手）：`GuiContext::measure_text` **返回 0.0**、GL 后端的 `draw_text`
  是空实现。也就是说，此前每个画标签的编辑器都在**对着一个谎言排版**——按 0 宽度居中，
  所有标签会叠在同一处。
- Change:
  - `sunmao/gui/src/text.rs`：`GlyphSource` trait（度量/栅格化/行度量/字形存在性）、
    `Font`（带**字形缓存**，按 char + 1/64 像素的尺寸键）、`TextMetrics`、
    `PositionedGlyph`、`Font::measure`/`Font::layout`（含换行与折行）。
    栅格化放在 trait 后面而不是硬绑一个字体库，**布局逻辑因此可以在没有字体文件的情况下
    被完整测试**，也给插件自带栅格化留了口子。
  - `sunmao/gui/src/text_ttf.rs`（`text` feature）：`TtfFont`，fontdue 支撑。
    **SunMao 不自带字体**——捆绑字体是许可与体积决策，属于发行插件的人。无字体时
    `Font::default()` 用 `MetricsOnlyFont` 仍能度量（等宽近似），只是不出墨。
  - `measure_text` 在 `NullContext` 与 GL 后端都接上真实度量。`GlContext` 拥有自己的
    `Font`——按 `ownership.md` 的 M3 约束，字形缓存**不能是进程级 static**，否则两个插件
    实例会共享并互相释放字形。
  - 测试用 `epaint_default_fonts`（**已在 workspace lockfile 里**，经 eframe 引入）取真实
    字体字节，因此**没有往仓库里塞二进制字体**，也没有新增许可文件。
- Result:
  - 完整 `cargo test --locked` **exit 0，127 套件 / 599 测试**（M2 为 578）。
  - **两个真实缺陷由本轮新写的测试当场抓出，且都是单测漏掉、proptest 抓到的**：
    (1) 折行时若某个词**从行首开始**仍放不下，原逻辑把它整体下移一行——落点完全相同，
    于是永远溢出；改为此时直接断词。(2) 更隐蔽：把词下移之后**没有重新判断是否仍然超限**，
    因此比整行还长的词在下移后照样画出边界（`"a xmcbim"` @22px/76px 限宽，末尾 'm'
    画到 77.71 > 76.49）。折行溢出在截图里看不出来，但会画到相邻控件上。
  - `tools/package_examples.sh --debug --test` exit 0，32 套件 / 640 断言，与 M2 一致。
  - `--target x86_64-pc-windows-msvc` 检查 exit 0；fmt/diff/metadata 全过。
- Evidence/artifact: 本地日志 `/tmp/m3_test.log`、`/tmp/m3_pkg.log`、`/tmp/m3_win.log`。
- Unresolved:
  - 本 commit 需三平台 hosted 全绿。
  - **M3 下半未做**：GL 后端把字形上传为纹理并真正 `draw_text`、clipboard、
    IME/国际键盘、cursor/focus 模型、runner 的"按键→参数变化"断言。
  - `TtfFont` 的 CJK：Ubuntu Light 无 CJK 覆盖，`has_glyph` 会如实报 false 供调用方替换；
    真要渲染 CJK 需插件自带字体。这一点在 M3 下半的 IME 落地时要一并说清。

### 2026-09-05 — M3 上半验收：hosted run #79 三平台全绿

- Command/platform: push `c405bd0` 触发 GitHub Actions #79：
  https://github.com/aizcutei/sunmao/actions/runs/33968821893
- Result: 三个 job 同一 commit 全部 success，每 job **26 步零非成功**，artifacts 3 份。
  Windows WGPU 收尾段错误未复现（自 run #66 起连续第 12 次）。
- Evidence/artifact: run #79 artifacts 可下载。
- Unresolved: **M3 尚未完成**——下半（GL 真正绘制字形、clipboard、IME/国际键盘、
  cursor/focus 模型、runner 的按键→参数断言）未开始。本条只是把已绿的上半钉住，
  避免下半失败时连度量层一起回滚。

### 2026-09-05 — M3 下半：焦点模型、键盘控制、国际输入、宿主键盘转发

- Command/platform: macOS ARM64，分支 `phase4/gui-component-library`。
- 出发点：`IPlugView::onKeyDown`/`onKeyUp` 是**返回 `kResultFalse` 的 stub**——宿主转发的
  按键从来没到过插件；`Event::TextInput` 从未被产生过——国际键盘与输入法的文本路径不存在；
  没有任何控件响应键盘。
- Change（自底向上）：
  - **`_sys`**：`vst3_sys::base::keycodes` 转录上游 `pluginterfaces/base/keycodes.h` 的
    `KeyCodes` 枚举。
  - **`_rs`**：`vst3_rs` 实现 `on_key_down`/`on_key_up`，经新钩子 `GuiPlugin::gui_key`
    下发；编辑器不要的键回 `kResultFalse`，宿主保留自己的快捷键。
  - **core**：`ViewKey` + **格式中立**的 `ViewKeyCode`；`ViewHandle::builder()` 取代不断
    加参数的 `scalable()`，新增 `send_key`。
  - **backend_vst3**：把 VST3 编号翻译成中立码——原始编号只有 backend 该认识。
  - **view_baseview**：`HostKeyQueue`（有界 64）+ `HostKeyedState` 适配器。宿主在**它自己的
    线程**调用，控件活在窗口线程，baseview 没有"向活着的 handler 注入事件"的接口，故排队并在
    下一帧 `draw` 开头排空。**`TextInput` 从 `Key::Character` 产生**（国际键盘/IME 路径），
    跳过 `is_composing` 的预编辑串。
  - **sunmao_gui**：`Stack` 焦点模型（Tab/Shift-Tab、按下鼠标转移焦点、键盘**只**投递给
    焦点控件）；`Knob`/`Slider` 方向键微调（Shift 精调、Home/End 到端点）、`Toggle`
    Space/Enter、`Dropdown` 方向键/Home/End/Escape；`ParamBinder` 把键盘编辑包成**独立手势**
    （没有 press/release 可以包夹，缺 begin/end 的自动化点会被部分 DAW 丢弃）。
  - **runner**：`HostPlugin::send_gui_key`，VST3 侧走真实 `IPlugView` vtable；`gui-test`
    断言 Tab 聚焦 → End 推满 → **经宿主 API 读回参数确认真的变了**。
- Result:
  - 完整 `cargo test --locked` **exit 0，127 套件 / 624 测试**（M3 上半为 599）。
  - **本地实测整条链打通**：`GUI key verified: Gain moved 0 -> 1 via host key forwarding`
    ——宿主 ABI → wrapper → backend → ViewHandle → 跨线程队列 → 窗口线程 → 焦点 → 控件 →
    binder → 宿主参数。CLAP 侧如实走 skip 路径。
  - `tools/package_examples.sh --debug --test` exit 0，32 套件 / 640 断言不变。
  - 8 个触及平台代码的 crate `--target x86_64-pc-windows-msvc` 检查 exit 0。
- Evidence/artifact: `/tmp/m3b_test.log`、`/tmp/m3b_pkg.log`、`/tmp/keytest_vst3.log`。
- Unresolved:
  - 本 commit 需三平台 hosted 全绿。
  - **本轮最该记的一条**：VST3 `KeyCodes` 编号最初凭记忆写，13 个里 12 个错
    （`KEY_RETURN` 实为 4 而非 6、`KEY_LEFT` 实为 11 而非 13……），而**单测照着错误实现写
    所以全绿**。是按硬性规则去读上游头文件才发现的。现在
    `vst3_key_codes_match_the_upstream_numbering` 直接钉住上游整数字面量，而不是引用常量
    ——引用常量的话常量本身写错就测不出来。
  - **M3 仍有未做项**：GL 后端把字形上传为纹理并真正绘制（`draw_text` 仍是空实现，
    度量已真实）、clipboard、wgpu/WebView 后端的宿主键盘队列排空。这三项如实列为遗留，
    不算 M3 完成。

### 2026-09-05 — M3 下半修复：run #81 三平台同步失败，非 flake

- Command/platform: run #81（commit `8588c39`）**三平台同时**在 "Package and exercise
  native GUI backends" 失败。三平台同步失败即排除 flake，是本轮代码问题。
- 根因（读 macOS job 日志得到确切一行）：`GUI key did not reach a parameter: Gain stayed at 0`,
  被测插件是 **`SunMao Gain GL`**。我把"该格式提供宿主键盘转发"当成了"这个编辑器会处理键盘"。
  Phase 1/2 的 8 个 GUI 示例都是手写 view、没有 `on_keyboard_event`，按键在那里**本来就该
  什么都不做**——断言却要求参数变化，于是它们因为行为正确而被判失败。
- Change: 键盘断言改为只对声明了键盘处理的编辑器生效（`info().name.contains("Widgets")`），
  与 runner 既有先例同形——`latency_alignment` 也只测 `OS Distortion`，因为只有它具备被测性质。
  其余插件打印"该编辑器未声明键盘处理，跳过"。
- Result: 本地把 CI 的实际矩阵**逐一复现**：`GainGL.vst3`/`GainGL.clap` 均 exit 0 且走跳过路径，
  `WidgetsGL.vst3` exit 0 且 `Gain moved 0 -> 1`，`WidgetsGL.clap` exit 0 且走格式跳过路径。
- Evidence/artifact: `/tmp/fix_GainGL.vst3.log` 等四份本地日志。
- Unresolved / 这轮的教训:
  **推送前的本地验证只覆盖了新 fixture，没覆盖 GUI 矩阵里另外 8 个插件。** 那 8 个只在
  hosted GUI 步骤里跑（需要显示器），本地不会自动触发，我也没有推理"这一步实际测哪些插件"。
  以后凡是改动 `gui-test` 共享路径，本地必须至少手动跑一个**新 fixture之外**的 GUI 插件。

### 2026-09-05 — run #82 绿但**断言空转**，补进 GUI 矩阵

- Command/platform: run #82（commit `3a0b1e6`）三平台 success、26 步零非成功、artifacts 齐备。
- **但核实日志发现断言从未执行**：三平台 `GUI key verified` 各 **0 次**，
  `declares no keyboard handling` 各 16 次——即 GUI 矩阵里 8 个插件 × 2 格式全部走了跳过路径。
  原因是 widgets fixture 只加进了**打包矩阵**（`runner test`），没加进 **GUI 矩阵**
  （`gui-test`），而它是唯一声明键盘处理的编辑器。
- **这正是本轮一直在防的失败模式，这次出在我自己的改动上**：一个只会 success、
  却从不真正执行的 gate，会让后面每个 milestone 的"证据"失效。若不是逐平台数
  `GUI key verified` 的出现次数，只看 conclusion 会把它当作 M3 已验收。
- Change: `package_and_test_gui_lifecycle WidgetsGL` 加入 GUI 矩阵，并在 workflow 里
  写明原因（不加这一行断言就是空的）。
- Result: 待三平台 hosted 复验；判定标准是三平台各出现 **1 次** `GUI key verified`
  （VST3）与 1 次格式跳过（CLAP），而不只是 job success。
- Unresolved: M3 在该验证通过前不算完成。

### 2026-09-06 — M3 收口：clipboard 与 GL 字形绘制

- Command/platform: macOS ARM64，分支 `phase4/gui-component-library`。
- Change:
  - **clipboard**：`Clipboard` trait + `MemoryClipboard`（测试与降级用）+ `SystemClipboard`
    （`clipboard` feature，arboard 支撑；**arboard 已在 workspace lockfile 里**，经 eframe 引入）。
    连接失败**不是致命错误**——无剪贴板会话（无头 CI）下插件仍要能开编辑器，故握手失败后
    每次操作如实回 false 而不是 panic。
  - 真实消费方：`ParamBinder` 处理焦点控件的 Ctrl/Cmd+C / +V。复制写的是
    `display_value()`（下拉是 "Warm" 这样的标签，不是裸浮点），粘贴经新增的
    `ParameterWidget::set_from_text`——默认解析归一化浮点，`Toggle` 认 On/Off、
    `Dropdown` 认选项名。**无剪贴板时快捷键不处理**，宿主因此保住自己的 Ctrl+C，
    而不是被静默吞掉。
  - **GL `draw_text` 不再是空实现**：按字形覆盖率绘制，同一扫描线上覆盖率相同的像素
    合并成一个矩形（实心竖干因此是 1 次绘制而非逐像素）。覆盖率直接当 alpha，
    抗锯齿得以保留（混合本来就已开启）。合并逻辑抽成 `GlyphBitmap::runs()`，
    因此**不需要 GL 上下文就能测**，并覆盖了"覆盖率数组短于 width×height"的畸形位图
    ——栅格化器的 bug 不该变成渲染器里的越界 panic。
- Result: `cargo test --locked` **exit 0，127 套件 / 631 测试**；打包 32 套件 / 640 断言不变；
  Windows target check exit 0；fmt/diff/metadata 全过。
- Evidence/artifact: `/tmp/m3c_test.log`、`/tmp/m3c_pkg.log`。
- Unresolved（**如实说明，不要读成"文字已经能显示"**）:
  - **默认字体仍然没有**，所以 `draw_text` 默认画不出字形：`Font::default()` 是
    `MetricsOnlyFont`，只量不画。栅格化、度量、布局、run 合并、GL 绘制路径都已实现并有测试，
    但要真正看到字，调用方需经 `GlContext::set_font` 提供字体。捆绑字体是许可与体积决策，
    留给发行插件的人——这一条从 M3 上半起就是这样，此处再次点明以免误读。
  - wgpu / WebView 后端仍不排空宿主键盘队列（如实回"未使用"）。

### 2026-09-06 — M3 完成：hosted run #84 三平台全绿且断言非空转

- Command/platform: push `8f959ba` 触发 GitHub Actions #84：
  https://github.com/aizcutei/sunmao/actions/runs/33976552655
- Result: 三个 job 同一 commit 全部 success，每 job **26 步零非成功**，artifacts 3 份。
  Windows WGPU 收尾段错误未复现（自 run #66 起连续第 14 次，其间 #81 的失败是本轮
  自身的断言错误，与该 flake 无关）。
- Evidence/artifact: **判定依据不是 job success，而是断言确实执行**：三平台各
  `GUI key verified` **1 次**（VST3 路径，`Gain moved 0 -> 1`）与格式跳过 **1 次**（CLAP）。
  对照 run #82：同样三平台全绿、26 步零非成功，但该断言 **0 次**执行。
- Unresolved: M3 完成，进入 M4（`VizChannel` 无锁 audio→GUI、`SpectrumAnalyzer`/meter、
  accessibility 树、floating CLAP editor）。M3 遗留两项已记在 status.md：
  (1) 无默认字体，`draw_text` 在调用方经 `GlContext::set_font` 提供字体前不出字形；
  (2) wgpu/WebView 后端不排空宿主键盘队列。

### 2026-09-06 — M4 本地完成：VizChannel、SpectrumAnalyzer、accessibility 树；两项如实标记受阻

- Command/platform: macOS ARM64 本地。`cargo fmt --all -- --check`、`git diff --check`、
  `cargo metadata --locked`、`RUSTFLAGS=-Awarnings cargo test --locked`（不走管道）、
  `cargo check --locked --workspace --target x86_64-pc-windows-msvc`、
  `tools/package_examples.sh --debug --test`。
- Change:
  - **`sunmao_core::viz` 三缓冲 `VizChannel`**：生产者恒有空槽可写、消费者恒有完整槽可读，
    `publish` 是一次 store + 一次 swap，无分配无锁无无界循环。`FRESH` 位与索引挤在同一个
    原子字里，因此发布与消费各是**单次原子操作**。选三缓冲而不是队列是因为显示要的是
    **最新帧**——GUI 以 60Hz 重绘而 audio 每块发布，丢掉中间帧是正确行为而非丢数据。
    5 个测试：零分配（1000 次 publish）、最新帧胜出、三索引恒互异的不变量、
    `latest()` 在无新帧时重复上一帧（而不是闪回 default）、**跨真实线程**的撕裂读检测。
  - **`SpectrumAnalyzer`**：峰值即起、按 falloff 衰减（看不见的峰值等于没有），
    NaN/越界收敛（削波显示为满刻度而不是空表——反过来正是最糟的失效方式），
    源长于显示时截断。显示件**从不消费事件**，点击穿透到它背后的控件。
  - **accessibility 树**：`accessibility_tree()` → `AccessibleNode`
    （role/label/可朗读值/归一化值/bounds/focus/disabled）。这是 UIA、NSAccessibility、
    AT-SPI 三家共同需要、也是唯一值得测试的那一层。role 由
    `ParameterWidget::accessible_role()` **声明**而非从显示文本推断——
    选项恰好是 `"1"`/`"2"` 的下拉仍是下拉。
  - **`clap_rs` 的 `suggest_title` 由静默 stub 改为真实转发**：宿主建议的标题此前被
    完全丢弃、插件永远无从知晓；现在解码后经 `GuiHandler::gui_suggest_title` 送达
    （非 UTF-8 **丢弃而不做有损转换**）。
  - fixture 从 crate 内 `SpectrumPublisher` 换成 `VizChannel` + `SpectrumAnalyzer`，
    **M0 起的测试语义未改**，零分配断言现在覆盖 `VizPublisher::publish`。
- Result: `cargo test --locked` **exit 0，128 套件 / 656 测试**（M3 为 127/631）；
  fmt / diff / metadata / Windows target check 全过。
- Evidence/artifact: `/tmp/full.log`、`/tmp/win.log`、`/tmp/pkg_m4.log`。
- **两个自身缺陷由测试抓出，如实记录**（都不是"顺手改了改"）:
  1. `impl SpectrumSource for Vec<f32>` **打断了一个无关示例的编译**：该 trait 在 prelude 里，
     于是每个插件的每个 `Vec<f32>` 都多出一个 `fill` 方法并**遮蔽 `slice::fill`**
     （`sunmao_fx_tempo_delay` 的 `line.fill(0.0)` 直接类型错误）。固有方法优先级救不了这种情况:
     trait impl 直接落在 `Vec<f32>` 上、无需 deref，因此赢得方法解析。改成 newtype
     `StaticSpectrum`。**教训：不要给 prelude 里的 trait 在 `Vec`/数组这类无处不在的外部类型上开 impl。**
  2. accessibility 树对非参数子节点**硬编码 `focused: false`**，而 `Stack::set_focus` 只做
     索引边界检查、显示件同样能拿到焦点 → 树说"无焦点"、stack 说"焦点在 0"。
     由 proptest `focus_is_reported_exactly_once_and_matches_the_stack` shrink 出
     "只含一个 Display 的编辑器" 这个最小反例抓到。**单元测试抓不到**（手写编辑器里显示件
     总在最后）。修法是让树如实镜像 stack，而不是改 `Stack` 已三平台验收过的焦点语义。
- Unresolved（**M4 有两项没有交付，这是决策项不是遗漏**，详见 status.md「M4 受阻项」）:
  - **floating CLAP editor 受阻于 baseview**：`is_floating` 如实回 `false`。vendored baseview
    只有 `open_parented` 与**阻塞式** `open_blocking`，在 `gui_show` 里调用后者会卡死宿主主线程。
    要交付得先给 baseview 加"非阻塞顶层窗口"模式并在 Win32/AppKit/X11 各实现一遍事件泵归属。
  - **accessibility 的 OS 桥接未做**：框架侧的树已完成并测试（6 单测 + 4 proptest +
    1 doc-test + fixture 断言），但把树发布给 UIA / NSAccessibility / AT-SPI 是三份互不复用的
    原生实现。**runner 无法验收这一项**：runner 经 VST3/CLAP 插件 API 驱动，而两个格式都
    没有 accessibility 通道——它走 OS API，必须先有桥接才存在可被 hosted job 断言的宿主可见行为。
    （runner 里既有的 UIA helper 是测试侧拖动宿主滑块的工具，不是插件侧实现，不能复用为验收手段。）

### 2026-09-06 — M4 验收（hosted run #86 三平台全绿且断言非空转）＋ M5 除 Wayland 外收尾

- Command/platform: push `1ddc210` 触发 GitHub Actions #86：
  https://github.com/aizcutei/sunmao/actions/runs/33980911401
- Result: 三 job 同一 commit 全部 success，每 job **26 步零非成功**，artifacts 3 份可下载
  （macOS 53.0MB / Windows 78.3MB / Linux 971.2MB）。Windows WGPU 收尾段错误未复现。
- Evidence/artifact: **判定依据是断言真的跑了并通过**，已下载三平台 job 日志逐条数：
  `the_editor_describes_itself_to_assistive_technology ... ok` 各 1 次、accessibility
  proptest 套件各 1 次、`VizChannel` 跨真实线程撕裂读测试各 1 次；`GUI scale negotiated`
  由 M1 的 16 次增至 **18** 次（widgets fixture 进 GUI 矩阵后 9 个 GUI 插件 × 2 格式），
  三平台 **零 FAILED 套件**，打包 `Summary: 20 passed, 0 failed`。
- Change（M5 本轮部分，Wayland 除外）:
  - **`docs/phase3/compatibility.md` 新增 §2bis GUI 兼容**：受 semver 保护的 GUI 面；
    **像素级外观不承诺、语义承诺**（主题角色与对比度下限、参数控件的归一化约定与
    "set_value 不得回调"、`display_value`/`set_from_text` 往返、`VizChannel` 的
    "最新帧、可跳不可退"投递语义）；accessibility 的 role 兼容规则（新增变体不算破坏、
    改已声明的 role 算破坏）；以及**只能记录不能承诺**的宿主侧行为清单。
  - **补齐 `Knob`/`Slider` 缺失的 `host_sync_never_echoes_back_to_the_host`**：
    写兼容文档时去核对引用的测试名，发现四个参数控件里只有 `Toggle`/`Dropdown` 有这条
    测试——而它正是防"宿主 automation 被回显成用户编辑"反馈环的那条不变量。两个控件的
    实现本来就是对的，缺的是守卫。**教训：文档里引用测试名要真的去 grep，指不到的
    承诺是文档债。**
  - **`clap_sys` 补回上游遗漏项**：`clap_window_handle_u` 少了 `uikit` 成员、
    少了 `CLAP_WINDOW_API_UIKIT` 常量、也没转录上游那些**载有语义**的注释
    （cocoa/uikit "uses logical size, **don't call set_scale()**"；wayland
    "embed is currently not supported, use floating windows"）。union 是 ABI 边界，
    因此补成员的同时加了布局断言（所有成员都是指针宽或 c_ulong，union 仍是一个字）。
- Result: `cargo test --locked` exit 0，**128 套件 / 660 测试**；fmt / diff / Windows
  target check 全过。
- Evidence/artifact: `/tmp/full5.log`、`/tmp/job_1013458386*.log`。
- Unresolved: **Wayland 原生受阻，且是规范层面而非工程取舍**（详见 status.md
  「M5 Wayland 受阻链」）。三个独立事实叠加：(1) VST3 **根本没有 Wayland 平台类型**
  （上游只有 HWND/NSView/UIView/X11EmbedWindowID），VST3 在 Wayland 上一律走 XWayland；
  (2) CLAP 虽声明 `wayland`，但上游 `gui.h` 原文说 *"embed is currently not supported,
  use floating windows"*——即 **Wayland 上必须用浮动窗口**；(3) 浮动窗口正是 M4 那条
  受阻项，且 baseview 的 Linux 后端本身 X11 独占（`baseview/src/lib.rs:6` 只有 `mod x11`，
  全树零 Wayland 引用）。**现状不是"不能在 Wayland 上用"**：经 XWayland，X11 路径
  在 Wayland 桌面上照常工作。

### 2026-09-06 — run #87 Linux 失败：我自己写的并发测试是 flaky 的

- Command/platform: push `fdc5869` 触发 GitHub Actions #87：
  https://github.com/aizcutei/sunmao/actions/runs/33982600495
- Result: **Linux job failure**，步骤 "Test format adapters and host"。
  `sunmao_core` 41 passed / **1 failed**：`viz::tests::frames_survive_crossing_a_real_thread_boundary`
  在 `viz.rs:281` panic，消息 `the consumer never saw a frame`。
- 根因（是测试的缺陷，不是 `VizChannel` 的）: 该测试 spawn 一个发布 5000 帧的生产者线程，
  消费者则**固定轮询 50000 次**后断言 `seen > 0`。消费者每次轮询只是一次原子 load，
  在负载高的 runner 上它可以在被 spawn 的线程**尚未被调度**之前就把 50000 次跑完，
  于是一帧也没看到。**断言依赖的是线程调度而不是通道行为。**
- 修法（不是重试，也不是加 sleep）: 改成轮询到生产者置位 `finished` 为止（不再是固定次数），
  join 之后再做**一次收尾 take**。这让 `seen > 0` 成为关于通道的事实而非关于调度的事实：
  要么消费者在过程中取到过帧，要么最后一次 publish 留下的 FRESH 位仍在、收尾 take 必然取到。
  同时新增 `last == 4999` 断言——最终看到的必须是最新帧。本地连跑 10 次全绿。
- **诚实说明**：同一个 flaky 测试在 **run #86 三平台都通过了**。这不改变 #86 对 M4 其余断言的
  验收效力（`the_editor_describes_itself_to_assistive_technology`、accessibility proptest、
  scale 协商 18 次等均为确定性断言），但**这一条并发断言在 #86 的通过是运气**，
  据此不能声称"跨线程行为已在三平台验证"——该结论要等修复后的版本取得绿才成立。
- Unresolved: 修复后需重新取三平台绿。

### 2026-09-06 — run #88 三平台全绿：flaky 修复生效，M4/M5 可交付部分验收

- Command/platform: push `c25dabc` 触发 GitHub Actions #88：
  https://github.com/aizcutei/sunmao/actions/runs/33984024056
- Result: 三 job 同一 commit 全部 success，每 job **26 步零非成功**，artifacts 3 份可下载
  （macOS 53.0MB / Windows 78.3MB / Linux 971.2MB）。
- Evidence/artifact: 已下载三平台 job 日志逐条核实，**每一条都是真跑并通过**：
  - `frames_survive_crossing_a_real_thread_boundary ... ok` —— #87 在 Linux 上失败的那条，
    修复后三平台均绿。**至此"跨线程行为已三平台验证"才成立。**
  - `the_editor_describes_itself_to_assistive_technology ... ok`（accessibility 树在真实
    编辑器上的断言）
  - `host_sync_never_echoes_back_to_the_host ... ok` **各 4 次/平台**（本轮从 2 个控件补到
    四个参数控件全覆盖）
  - `the_window_api_names_match_upstream ... ok`（`clap_sys` 补回 `uikit` 后的常量钉死）
  - `GUI scale negotiated` 各 18 次/平台；**三平台零 FAILED 套件**。
- Unresolved: **Phase 4 按其自身完成规则仍未完成**——M0–M3 全部完成并各自取绿，
  M4/M5 的可交付部分已由 #86/#88 取绿，但 M4 的 floating CLAP editor 与 accessibility
  OS 桥接、M5 的 Wayland 原生三项未交付，且都是范围问题而非工期问题（依据与文件行号见
  status.md 的「M4 受阻项」「M5 Wayland 受阻链」「完成规则 → 当前判定」）。
  三者共同的性质：工作量不在 SunMao 这一层，而在 vendored baseview 与三个 OS 的原生 API 上。

### 2026-09-06 — floating CLAP editor 交付：我之前把它判为受阻是错的

- Command/platform: macOS ARM64。全量 gate 见下。
- **先说错在哪。** 上一轮我写下"vendored baseview 只有 `open_parented` 与阻塞式
  `open_blocking`，加一个非阻塞顶层窗口模式比 M4 其余全部加起来还大"，并据此把
  floating editor 记为受阻。**这个判断是从函数名推出来的，不是从代码。** 把三个平台的
  `open_blocking` 读完之后：真正阻塞的**只有最后一步**——macOS 的 `app.run()`、
  Windows 的 `GetMessageW` 泵、X11 的 `join`。建窗与事件循环早已分离，Windows 那条
  甚至已经把 `WindowHandle` 返回出来了。**教训：判"受阻"之前必须把被指为障碍的代码读完。**
- Change（自底向上）:
  - **baseview 新增 `Window::open_floating`（三平台各一条实现，与 `open_blocking` 共用建窗路径）**
    - macOS：复用建窗代码但 **`ns_app: None`**。这一个字段是关键——它意为"本窗口拥有
      application"，非空时关窗会调 `stop_application_event_loop()`，**在插件里那会停掉宿主的
      run loop**。同理不碰 `setActivationPolicy`/`finishLaunching`/`activateIgnoringOtherApps`
      （那是 standalone 的 app 引导，插件无权对宿主做），并用 `orderFront_` 而非
      `makeKeyAndOrderFront_`：编辑器弹出不该把用户从手头的事上拽走。
    - Windows：`Self::open(false, null_mut(), ..)` 已返回 handle，直接用。`DispatchMessageW`
      按 HWND 派发到对应窗口过程，宿主的消息泵自然驱动我们的窗口。
    - X11：`open_parented` 的结构、parent 传 `None`；不传 `stop_requested`（那是 standalone
      停循环用的），窗口寿命由返回的 handle 掌握。
  - **core**：`SunmaoView::supports_floating()`（默认 false）与 `open_floating()`（默认 None）。
    两个方法分开是必要的：宿主在创建**之前**就要问（`is_api_supported`），而
    **"查询说支持、创建却失败"是格式契约明令禁止的**。
  - **view_baseview**：`BaseviewView` 的嵌入与浮动**共用同一段 `open_with`**——GL 配置、
    WGPU 回退、初始化校验、`ViewHandle` 接线只写一遍，差别只有传进去的建窗函数。
  - **backend_clap**：`is_api_supported(_, true)` 直接返回 `view.supports_floating()`，与
    `gui_create` 同源；`gui_create` 只记录模式（CLAP 的 create/show 分工：create 分配、
    show 上屏），窗口在 **`gui_show`** 才真正打开；浮动模式下 `gui_set_parent` 一律拒绝；
    `gui_destroy` 连模式一起清（否则之后一次嵌入式 create 会被当成浮动）。
  - `docs/phase2/semantics.md` 的 floating 行整条重写（此前记的是"一律拒绝"）。
- Result: `cargo test --locked` **exit 0，128 套件 / 662 测试**（上一轮 660，增量为三条
  floating 测试）；fmt / diff / metadata / Windows target check 全过。
- Evidence/artifact: `/tmp/fl.log`、`/tmp/pkg_fl.log`。
- **仍降级一项**：`gui_hide` 回 false。baseview 没有"隐藏但保活"的操作，关掉再开会让宿主
  拿到一个全新编辑器、丢失界面状态，故如实回不支持、让宿主退回 destroy/create。
- Unresolved: X11 那条实现**本地无法编译验证**（交叉编译缺 X11 sysroot），依赖 Linux
  hosted job。Wayland 的受阻链因此缩短为一条：**baseview 没有 Wayland 后端**，
  而"CLAP 在 Wayland 上要浮动窗口"这一环已经不再是障碍。

### 2026-09-06 — run #90 三平台全绿：floating editor 验收

- Command/platform: push `b111338` 触发 GitHub Actions #90：
  https://github.com/aizcutei/sunmao/actions/runs/33986603921
- Result: 三 job 同一 commit 全部 success，每 job **26 步零非成功**，artifacts 3 份可下载
  （macOS 53.1MB / Windows 78.3MB / Linux 971.6MB）。
- Evidence/artifact: 已下载三平台日志逐条核实：
  `a_floating_capable_view_opens_on_show_and_refuses_a_parent ... ok`、
  `a_floating_capable_view_still_embeds ... ok`、
  `a_suggested_title_reaches_the_plugin_and_a_non_floating_view_declines ... ok`，
  三平台**零 FAILED 套件**。**X11 的 `open_floating` 本地无法编译验证**（交叉编译缺
  X11 sysroot），Linux job 是它唯一的证据。
- Unresolved: Phase 4 仍剩两项，且我已按同样的方式核实过它们**不是**另一个"读函数名下的错判"：
  全树 grep `NSAccessibility|IRawElementProvider|atspi` 与 `wayland`，**平台侧零代码**
  （命中的三处全是本轮新写的框架侧 accessibility 树）。floating 之所以能快速交付，是因为
  baseview 里建窗结构本来就在、只需拆出来；这两项则是三个平台各自从零起：
  (1) accessibility OS 桥接（UIA / NSAccessibility / AT-SPI 三份互不复用，且 runner 目前
  无法验收——需要 Phase 5 的交互式 host；Windows 侧或可复用既有 UIA helper 反向查询）；
  (2) baseview 的 Wayland 后端（`wl_surface`/`xdg_shell`/EGL/`wl_seat`+xkbcommon/`wl_output`，
  且 CI 需装无头 compositor 才谈得上验收，Ubuntu job 现在跑的是 Xvfb ＝ X11）。

### 2026-09-06 — accessibility 第二层：到 AccessKit 的翻译（同样是纠正一次过重的判断）

- Command/platform: macOS ARM64。
- **又一次判断过重，如实记下。** 上一轮我把 accessibility 记为"需要 UIA /
  NSAccessibility / AT-SPI 三份互不复用的原生实现"。事实是
  [AccessKit](https://github.com/AccessKit/accesskit)（MIT/Apache-2.0，MSRV 1.85）
  已经在维护那三个适配器，egui 与 winit 都在用。**SunMao 需要写的只是数据映射。**
  与 floating 那次是同一个毛病：先断言规模，后核实。这次核实的方式是 `cargo search` +
  读 `accesskit-0.25.0/src/lib.rs` 的真实定义（`TreeUpdate` 需要 `tree_id`、
  `Tree` 已 deprecate 为 `TreeInfo`——**docs.rs 摘要漏了前者，是源码补上的**）。
- Change:
  - `sunmao_gui` 新增 `accesskit_update(&AccessibleNode) -> accesskit::TreeUpdate`，
    放在 **off-by-default 的 `accessibility` feature** 后（与 `text`/`clipboard` 同理，
    也合本项目"AU 不进默认 feature"的既有约定）。只新增 2 个 crate：`accesskit` + `uuid`。
  - 映射要点：role 保守对应（`Graphic → Role::Image`，AccessKit 无 meter，读作"存在、
    可描述、不可交互"）；归一化值**连同 min/max 一起写**，否则屏幕阅读器算不出百分比；
    **非有限值直接不写**，否则会被念成乱码；id 深度优先分配且父节点先占位再递归，
    因此子节点永远拿不到父节点的 id。
  - 4 条 proptest 钉住 AccessKit 的树形约束（它对此很严格）：唯一 id、每个非根节点恰好被
    一个父节点认领、无自环、focus 必指向存在的节点。
  - `sunmao` facade 同名 feature + prelude 导出 `accesskit_update`（带 doc-test）。
  - **CI 新增 blocking 步骤 "Test the accessibility feature"**（每 job 26→27 步）。
    这一步不能省：默认 `cargo test` 不编译该 feature，不单开就等于三平台上全是死代码——
    run #82 已经踩过一次"进了矩阵却没真跑"的坑。
- Result: 默认 `cargo test --locked` exit 0，**129 套件 / 662 测试**；
  feature-on `cargo test -p sunmao_gui --features accessibility` exit 0，
  **95 + 4 + 4 + 10 + 5 测试**（含 7 个 accesskit 单测、4 条 accesskit proptest、doc-test）；
  Windows target check（带 feature）exit 0；fmt / diff / metadata 全过。
- Evidence/artifact: `/tmp/ak.log`、`/tmp/ak_feat.log`。
- Unresolved: **还差最后一层**——把 `TreeUpdate` 交给 `accesskit_windows`/`_macos`/`_unix`
  并接进 baseview 三个后端的窗口生命周期。runner 侧的验收手段已经存在（既有 UIA helper
  用的正是 `IUIAutomation6` + `IUIAutomationRangeValuePattern`，可反向查询插件暴露的元素），
  因此 **Windows 是三平台里最先能被 hosted job 断言的一条**。

### 2026-09-06 — accessibility 第三层：Windows UIA 适配器接通

- Command/platform: macOS ARM64（Windows 侧只能交叉编译检查，运行时证据靠 hosted job）。
- Change（自底向上）:
  - **baseview 新增 `accessibility` feature**：`WindowHandler::accessibility_tree()`
    （默认 `None`——"这个窗口没有可描述的结构"是裸 framebuffer 窗口的诚实答案）。
  - **Windows 后端接 `accesskit_windows::Adapter` + `WM_GETOBJECT`**。没有用
    `SubclassingAdapter`：它要求窗口尚未可见，而 baseview 的嵌入窗口带 `WS_VISIBLE`
    创建，会直接 panic。
    - 适配器**懒创建**：`Adapter::new` 要初始化 UIA，而绝大多数情况没有辅助技术在跑；
      Windows 只在真有人问时才发 `WM_GETOBJECT`。
    - **`WM_GETOBJECT` 来自操作系统，在里面 panic 会带走宿主**：handler 的 `RefCell`
      一律 `try_borrow_mut`，借不到就回 `None` 让 `DefWindowProc` 去答。
    - 每帧后 `update_if_active`（只在真有客户端时回调工厂），且必须放在 `on_frame`
      **之后**——那次借用结束了，发布要再借一次。
    - `winapi` 与 `windows` crate 的 `HWND`/`WPARAM`/`LPARAM` 是不同类型，
      边界**显式转换**而非 transmute。
    - action 未接：屏幕阅读器能读、不能改；`ActionHandler` 如实无动作。
  - `sunmao_view_baseview`/`sunmao`/fixture 各自透传同名 feature；`sunmao_gui`
    重新导出 `accesskit`，这样插件不必自己加依赖、也不会用上版本不匹配的一份。
  - CI 的 "Test the accessibility feature" 步骤扩展为同时构建 facade 与 fixture 的
    feature 版本——**Windows 上这一步是唯一会编译 UIA 适配器与 `WM_GETOBJECT` 分支的地方**。
- Result: 默认 `cargo test --locked` exit 0，**129 套件 / 662 测试**；
  `cargo check --target x86_64-pc-windows-msvc` 对 `baseview` / `sunmao_view_baseview` /
  fixture 的 feature 版本均 exit 0；macOS 上 fixture feature 版 9 测试通过；
  fmt / diff / metadata 全过。
- Evidence/artifact: `/tmp/uia.log`。
- Unresolved:
  - **macOS/Linux 适配器未接**：树已产出，但 AppKit/X11 后端还没交给
    `accesskit_macos`/`accesskit_unix`，两平台上开 feature 只是多算一棵树、无人消费。
    **如实记为未接通。**
  - **宿主侧断言未做**：目前 hosted job 只证明这条链三平台编译并通过单测。真正用
    runner 的 UIA 机制反查插件暴露的元素，需要先把 fixture 以该 feature 打包进矩阵。

### 2026-09-06 — accessibility 三平台适配器接通（macOS 与 Linux）

- Command/platform: macOS ARM64。**Linux 侧本地无法编译验证**（交叉编译缺 X11 sysroot），
  其唯一证据是 hosted Linux job——这一点必须先说清楚。
- Change:
  - **macOS**：`accesskit_macos::SubclassingAdapter`。它靠 swizzle NSView 的 accessibility
    方法工作，所以与 Windows 相反**不能懒创建**——必须在 AppKit 提问之前装好。
    两个坑：(1) activation handler 持 **`Weak`**，适配器住在 `WindowState` 里，
    用 `Rc` 会成环并泄漏窗口；(2) macOS 的 `on_frame` 把 handler **取出** `RefCell`，
    所以"handler 不在"是常态，此时回 `None`，发布也必须放在 handler 放回之后。
  - **Linux**：`accesskit_unix::Adapter`（AT-SPI）。形状与另两个不同：activation handler
    必须 `Send` 且跑在适配器自己的线程上，够不到事件循环持有的 handler，
    因此树改为**每帧推**进共享槽、由 handler 读回。首帧前回 `None` 是 AccessKit 明确允许的
    （只要树在下一次刷新前送达，而那正是循环发布的时机）。懒创建：构造它会起 D-Bus 连接。
  - 三平台共同降级：**action 未接**，屏幕阅读器能读不能改；`ActionHandler` 如实无动作。
- Result: 默认 `cargo test --locked` exit 0，**129 套件 / 662 测试**；
  `cargo metadata --locked` 在新增 Linux 依赖后仍 exit 0；macOS 上 `sunmao_gui` 与 fixture
  的 feature 版全绿；fmt / diff 全过。
- Evidence/artifact: `/tmp/mac.log`。
- Unresolved:
  - **Linux 那条实现只有 CI 能验证**（同 floating 的 X11 分支）。
  - **仍无宿主侧断言**：hosted job 目前证明的是三平台编译并通过单测。用 runner 的 UIA
    机制反查插件暴露的元素需要先把 fixture 以该 feature 打包进矩阵。
  - macOS 的 AXUIElement 验收在 CI 上受 TCC 权限限制，Windows UIA 无此限制，
    因此**宿主侧断言优先做 Windows**。

### 2026-09-06 — accessibility 的宿主侧运行时断言（Windows UIA 往返）

- Command/platform: macOS ARM64（该测试只在 Windows 编译与运行，本地只能交叉编译检查）。
- Change: 新增 `examples/sunmao_fx_widgets_gui_gl/tests/windows_uia.rs`
  ——**树里其余所有 accessibility 测试查的都是数据结构，这一条查的是真东西**：
  开一个真实窗口，然后用**屏幕阅读器所用的同一套 API（UI Automation）**问里面有什么，
  断言旋钮/下拉/开关分别以 Slider / ComboBox / CheckBox 出现。
  - **只做 Windows，理由具体**：UIA 不需要额外权限；macOS 的 AXUIElement 在 CI 上受 TCC
    限制拿不到授权，AT-SPI 需要 runner 并未运行的 a11y 总线。这是唯一能**断言**而非假定
    这条往返的平台。
  - 这个测试之所以现在写得出来，是因为**本 phase 早先补的 `open_floating`**：
    嵌入式窗口需要一个宿主父窗口，而浮动窗口自己就是顶层窗口。
  - 必须自己泵消息：UIA 经 `WM_GETOBJECT` 应答，不泵的话查询会超时，看起来像"树不存在"
    而不是"卡住了"。并带 20s 重试——UIA 是异步挂载的，首查可能早于 provider 注册。
  - `ViewHandle` 有意隐藏平台句柄，为一个测试加访问器会把 Win32 漏进核心抽象；
    浮动窗口建在本线程上，所以改用 `EnumThreadWindows` 向系统要。
  - 窗口开不出来（无窗口站的会话）时**跳过而不是失败**：那会把窗口问题报成 accessibility 问题。
- Result: 默认 `cargo test --locked` exit 0，**130 套件 / 662 测试**（新增该测试目标）；
  `cargo check --tests --target x86_64-pc-windows-msvc --features accessibility` exit 0；
  fmt / diff / metadata 全过。
- Evidence/artifact: `/tmp/uia2.log`。
- Unresolved: **该断言的真实执行证据只能来自 Windows hosted job**——本地无法运行。
  下一轮核实日志里它是否真的跑了并通过（run #82 的教训：绿不等于断言执行过）。

### 2026-09-06 — run #95 三平台绿，但 UIA 断言的证据不成立，已修

- Command/platform: push `7d80d53` → GitHub Actions #95，三平台 success、27 步零非成功。
  Windows 日志里 `a_screen_reader_can_see_the_editor_controls ... ok`。
- **为什么这还不够。** 该测试在开不出窗口的会话里会**跳过并返回**，而 cargo test 在成功时
  **捕获 stdout/stderr**——于是"断言通过"和"静默跳过"在日志里长得**一模一样**，两者都只显示
  `... ok`。我确认不了它到底验证了什么。**这正是 run #82 那个坑的同一形状**：绿色，
  但断言可能一次都没跑。
- Change:
  - 两条路径各打一个**互斥的标记**：`UIA VERIFIED: ...` 与 `UIA SKIPPED: ...`。
  - CI 该步骤加 `-- --nocapture`，否则标记根本不会出现在日志里。
  - **Windows job 上新增硬检查**：日志里没有 `UIA VERIFIED` 就 `exit 1` 并回显实际走的
    那条路径。跳过不再能伪装成通过。
- Result: `cargo check --tests --target x86_64-pc-windows-msvc --features accessibility` exit 0；
  fmt / diff 全过；macOS 上该 feature 的 fixture 测试仍 9 通过（UIA 测试在非 Windows 上
  由 `#![cfg]` 整体排除）。
- Unresolved: 下一轮看 Windows 日志里是 `UIA VERIFIED` 还是 `UIA SKIPPED`。
  **若是 SKIPPED，就说明 hosted runner 开不出顶层窗口**，那时才谈得上换验收路径
  （例如经打包矩阵用 runner 的宿主进程去查），而不是现在就假设它能行。

### 2026-09-06 — run #96：断言如实报告"跳过"，验收路径改为嵌入式

- Command/platform: push `3ba516a` → run #96。**Windows job failure**，正是新加的硬检查
  拦下的：日志里是 `UIA SKIPPED: no floating window could be opened in this session`，
  而测试本身仍报 `... ok`。
- **这一步的价值就在这里**：run #95 同一个测试"三平台绿 + `... ok`"，看起来完全正常，
  实际上一次断言都没跑。加标记 + 加硬检查之后，第一时间就把它暴露了。
  **绿色从来不是证据，断言执行过才是。**
- 根因：hosted Windows runner 的测试进程**开不出顶层窗口**，因此 `open_floating` 回 `None`。
- Change: 验收路径从"浮动窗口"改为"**自建父窗口 + 嵌入式编辑器**"——
  `CreateWindowExW` 造一个宿主窗口（runner 自己的 GUI 测试就是这么做的，在该平台可用），
  再 `view.open(ParentWindow::Win32(..))` 把编辑器嵌进去，然后从父窗口查 UIA
  （屏幕阅读器在 DAW 里走的正是这条遍历）。**顺带更对**：嵌入才是插件在宿主里的真实路径。
  两条跳过路径各有独立标记，分别指向"根本建不了窗口"与"编辑器嵌不进去"。
- Result: `cargo check --tests --target x86_64-pc-windows-msvc --features accessibility` exit 0；
  fmt / diff 全过。
- Unresolved: 下一轮仍要看 Windows 日志是 `UIA VERIFIED` 还是某条 `UIA SKIPPED`。

### 2026-09-06 — UIA 往返抓到一个真缺陷：适配器不能在 `WM_GETOBJECT` 里创建

- Command/platform: run #97 的 Windows job 给出了决定性诊断（raw-view 遍历 + 元素名）。
- **诊断读数**：`type=50033 name="SunMao Widgets"`——50033 是 Pane，而 `"SunMao Widgets"`
  正是 **baseview 的窗口标题**（`BaseviewConfig.title`），不是我们的根节点标签
  （那会是 `WidgetsPlugin::NAME` = `"SunMao Widgets GL"`）。**即：UIA 看到的是默认的
  HWND pane，我们的 provider 根本没装上。** 其余元素（TitleBar/MenuBar/Minimize/
  Maximize/Close）全是宿主窗口自己的非客户区。
- **根因，而且上游早就写明了**：`accesskit_windows::Adapter::new` 的文档说它
  **不得在处理 `WM_GETOBJECT` 期间调用**，因为它必须在那条消息被处理**之前**初始化 UIA；
  否则会产生嵌套的 `WM_GETOBJECT`，且**辅助技术会认为该窗口不原生支持 UIA**——
  与观察到的现象逐字吻合。我为了省开销把适配器做成"首次 `WM_GETOBJECT` 时懒创建"，
  正好踩中这一条。**我读过那段注释，但没把它和自己的懒加载联系起来。**
- Change: 改为**随窗口创建**（handler 装好之后、任何 `WM_GETOBJECT` 之前）。
  仍然跳过"handler 不描述自己"的窗口，所以只有真正参与的编辑器才付这份开销——
  该判断移到了创建时而不是消息处理里。macOS 侧本来就是在 `finish` 里 eager 创建的，
  没有这个问题。
- **这正是这个测试存在的理由**：整条链在三平台都编译通过、所有单测和 proptest 全绿，
  而真实的 UIA 客户端看到的仍然是一个不透明矩形。只有拿屏幕阅读器用的同一套 API
  去问，才问得出来。
- Result: `cargo check --target x86_64-pc-windows-msvc --features accessibility` exit 0；
  fmt / diff 全过。
- Unresolved: 下一轮看 Windows 日志里是否出现 `UIA VERIFIED`。

### 2026-09-06 — 第二个真缺陷：Windows 走的是 WGPU 回退，而那条路径没转发 accessibility

- Command/platform: run #98 的 Windows job 仍报同一读数——`type=50033 name="SunMao Widgets"`
  （窗口标题＝默认 HWND pane）。eager 创建适配器**没有**改变 UIA 看到的东西，说明还有第二个原因。
- **根因**：`install_accessibility` 先问 handler "你描述自己吗"，`None` 就跳过。
  而 handler 的具体类型在 Windows 上**不是** `GlHandler`——`sunmao_fx_widgets_gui_gl` 在
  Windows runner 上 GL 初始化失败（该回退本来就是为此存在的：Windows 基础驱动可能没有
  sRGB framebuffer），于是跑的是 `BaseviewHandler::Wgpu(WgpuHandler<WgpuFallbackState<..>>)`。
  我只在 `GlHandler` 上实现了 `accessibility_tree`，**WGPU 那条路径拿的是默认的 `None`**，
  于是整扇窗口静默失去可访问性。
- Change: `WgpuViewState` trait 补上同名钩子（默认 `None`）、`WgpuFallbackState` 转发给内层
  `ViewState`、`WgpuHandler` 转发给它的 state。
- **两个缺陷的共同点**：都不是"编译不过"或"单测不过"，而是**只有拿屏幕阅读器用的同一套 API
  去真机问一次才暴露**。三平台编译绿、全部单测与 proptest 绿的情况下，真实 UIA 客户端
  看到的仍是一个不透明矩形。
- Result: Windows target check（含 tests）exit 0；macOS 上 fixture feature 版 9 测试通过；
  fmt / diff 全过。
- Unresolved: 下一轮再看 `UIA VERIFIED`。若仍失败，下一个怀疑点是 UIA 对无名 pane 的
  过滤，或适配器需要 `update_if_active` 先跑过一帧。

### 2026-09-06 — run #100：UIA 往返真通了，M4 accessibility 收口

- Command/platform: push `c1fc054` → [run #100](https://github.com/aizcutei/sunmao/actions/runs/33999399095)，
  三平台 success、每 job 27 步零非成功、artifacts 3 份可下载。
- Evidence/artifact: **Windows 日志里的真实往返**（不是"编译通过"）：
  ```
  UIA VERIFIED: slider + combo box + check box among 11 elements
  UIA element: type=50026 name="SunMao Widgets GL"   ← Group（我们的根）
  UIA element: type=50015 name="gain"                ← Slider
  UIA element: type=50003 name="mode"                ← ComboBox
  UIA element: type=50002 name="bypass"              ← CheckBox
  ```
  对照修好之前：同一查询只返回宿主窗口自己的 TitleBar/MenuBar/Minimize/Maximize/Close，
  以及一个名字取自窗口标题的默认 HWND pane。
- Result: 三平台绿；默认 `cargo test --locked` 130 套件 / 662 测试；
  feature-on 套件三平台各自通过。
- **本轮最该记住的**：这条断言从写下到通过一共暴露了 **2 个真缺陷**，
  而它们**在三平台编译全绿、全部单测与 proptest 全绿的前提下依然存在**。
  可访问性这种"输出给别的进程看"的能力，只有拿消费方的真实 API 去问才算验过。
- Unresolved: macOS/Linux 仍只有编译级证据（AXUIElement 受 TCC、AT-SPI 需要总线）；
  三平台共同的降级是 action 未接（能读不能改）。Phase 4 只剩 **Wayland 原生** 一项。

### 2026-09-06 — Wayland：把"我们对 Wayland 宿主怎么回答"钉死，并如实划定未做的部分

- Command/platform: macOS ARM64（该测试 Linux only，靠 CI 执行）。
- Change: 新增 `embedded_wayland_is_refused_while_x11_is_accepted`（Linux only）。
  这一条要精确，因为**宿主会照着答案行动**：
  - `is_api_supported(Wayland, is_floating=false)` → **false**，`gui_create` 同样拒绝。
    baseview 的 Linux 后端 X11 独占，声称支持而后交不出窗口是格式契约明令禁止的。
  - `is_api_supported(X11, false)` → true，**这正是 Wayland 桌面经 XWayland 实际走的那条**。
  - `is_api_supported(Wayland, is_floating=true)` → true，**有意如此**：CLAP 允许宿主为
    浮动模式传 null API，因为窗口由插件自己拥有、自选工具包；在 Wayland 桌面上那扇窗口
    是 XWayland 下的 X11 窗口。这一点连同其余差异一并写进 semantics.md。
- Result: macOS 上 `sunmao_backend_clap` 38 + 1 + 1 测试通过；新测试由 Linux job 执行。
- **Wayland 原生仍未做，且这次的判断是查过的**（吸取前两次"读函数名就下结论"的教训）：
  `baseview/src/lib.rs` 只有 `mod x11`，全树零 Wayland 引用；不存在可以拆出来复用的结构
  （不像 floating 那次——建窗代码本来就在，只是和事件循环缠在一起）。
  真正交付 = 一个完整的 baseview Wayland 后端（`wl_surface`/`xdg_shell`/EGL/
  `wl_seat`+xkbcommon/`wl_output` 缩放）＋ CI 装无头 compositor（Ubuntu job 现跑 Xvfb ＝ X11），
  **且只有 CLAP 受益**（VST3 无 Wayland 平台类型）。这是独立立项的规模。
- Unresolved: Phase 4 的 M0–M4 与 M5 除 Wayland 外全部完成并三平台验收；**只剩 Wayland 一项**。

### 2026-09-06 — Wayland 第一步：先建可验证的测试床，而不是先写盲代码

- Command/platform: macOS ARM64。**Wayland 代码在本地一行都编译不了**（Linux-only，
  且交叉编译缺 X11 sysroot），所以这一轮刻意先解决"在哪儿验"这个真瓶颈。
- **为什么先做测试床**：前两轮的教训是反过来的两个方向——floating/accessibility 是
  "不看代码就说做不了"，而 UIA 那轮是"写完才发现没法证明它真的работа"。Wayland 如果
  先写 1000+ 行盲代码再想验收，等于把 UIA 那个坑放大一遍：CI **根本没有 compositor**，
  绿了也什么都不能说明。
- Change:
  - `baseview` 新增 off-by-default 的 `wayland` feature（Linux-only）与
    `baseview::wayland::probe`：连接 `WAYLAND_DISPLAY`、枚举 registry、报告有哪些 global。
    `can_open_a_window()` 要求 `wl_compositor` **且** `xdg_wm_base`——只有前者是开不出
    带标题栏的窗口的，报告"能"会把调用方晾在半路。无 compositor 时回
    `Err(NoCompositor)` 而不是 panic：那是 X11 会话下的**正常**答案，意思是"走 X11 后端"。
  - CI 的 Ubuntu job 装 `weston`，以 `--backend=headless-backend.so` 起一个无头
    compositor，**轮询 socket 出现而不是固定 sleep**（冷 runner 的启动时间不可预测），
    然后带 `WAYLAND_DISPLAY` 跑探针并把 global 列表打进日志。
  - 该步骤目前**非 blocking**：它的作用是确认测试床存在。等真有后端依赖它时再转 blocking。
  - 模块文档写清了为什么 Wayland 必须是**独立后端**而非 X11 的变体：两者在 baseview 的
    立身之本——嵌入——上不一致。X11 有 XEmbed，Wayland 没有等价物（CLAP 上游原文
    *"embed is currently not supported, use floating windows"*），VST3 更是连 Wayland
    平台类型都没有。**因此 Wayland 后端只需实现顶层窗口那条路径，而 `open_floating`
    这个抽象本 phase 已经有了。**
- Result: 默认 `cargo test --locked` exit 0，**130 套件 / 662 测试**（Wayland 代码在
  macOS 上不参与编译）；`cargo metadata --locked` exit 0（lockfile 仅 +2 行，wayland
  相关 crate 早已因 wry/gtk 间接在册）；fmt / diff 全过。
- Unresolved: 下一轮看 Ubuntu 日志——weston 能否在 runner 上起来、探针报告哪些 global。
  **若 `xdg_wm_base` 在场，后端就有地方验；若 weston 起不来，则先解决那个**，
  在此之前不写后端代码。

### 2026-09-06 — 测试床确认可用（run #103），随即写真实的 Wayland 顶层窗口

- Command/platform: Ubuntu hosted job 给出测试床的确凿读数：
  ```
  weston is up on wayland-ci
  WAYLAND PROBE: connected, 19 globals, window=true input=false
  ```
  `wl_compositor` / `xdg_wm_base` / `wl_shm` / `wl_output` / `wp_viewporter` 均在场，
  故 `can_open_a_window() == true`。**`wl_seat` 不在场**——无头 weston 没有输入设备，
  于是 `input=false`。这是个具体且重要的事实：**这台 runner 能验窗口创建，验不了输入**。
  正因如此才先建测试床——这类结论只能从真机得到，猜不出来。
- Change: `baseview::wayland::toplevel` —— 真实的 `wl_surface` + `xdg_surface` +
  `xdg_toplevel` 握手。
  - **只有顶层这一条路径，且这不是偷工减料**：Wayland 没有 XEmbed 等价物，CLAP 上游
    原文让用浮动窗口，VST3 连 Wayland 平台类型都没有。嵌入式编辑器继续走 X11 后端
    （Wayland 桌面上经 XWayland），这里服务浮动那一路——而 `open_floating` 这个抽象
    本 phase 已经有了。
  - **必须应答 `xdg_wm_base::Ping`**，否则合成器会判定客户端无响应并可能杀掉它。
  - 握手不是"一次调用就有窗口"：先 commit 求 configure，收到 `xdg_surface::configure`
    后 **ack**，才能 attach。`ToplevelProgress` 逐段记录走到哪一步，**失败时能说出
    卡在哪**，而不是只报"没有窗口"。
  - 缓冲区走 `wl_shm` 而非 EGL：**无头合成器没有 GPU**，shm 让协议路径在这台 runner 上
    可验；EGL/GL 接入是下一步。
  - `is_mapped()` 要求三段全过——configure 了但没 attach 是一扇没有内容的窗口，
    合成器什么都不显示，报"已映射"是撒谎。
  - CI 断言 `WAYLAND TOPLEVEL VERIFIED`：**有合成器却没映射出窗口就 `exit 1`**。
    跳过不能伪装成通过（UIA 那轮的教训）。
- Result: 默认 `cargo test --locked` exit 0，**130 套件 / 662 测试**（Wayland 代码在
  macOS 上不编译）；`cargo metadata --locked` exit 0；fmt / diff 全过。
- Unresolved: 下一轮看 Ubuntu 日志里是 `WAYLAND TOPLEVEL VERIFIED` 还是卡在哪一段。
  之后是 EGL/GL 接入与 `open_floating` 的分派（无 `wl_seat` 的 runner 上输入无法验收）。

### 2026-09-06 — Wayland probe skips cleanly outside a Wayland session

- Command/platform: macOS ARM64, `cargo test --locked -p baseview --features wayland --lib`.
- Change: the real top-level integration test now checks `WAYLAND_DISPLAY` before attempting a socket connection. Desktop shells without a Wayland session report `WAYLAND TOPLEVEL SKIPPED` instead of probing an implicit socket that can block; Ubuntu CI still starts weston and exercises the full configure/ack/shm mapping path.
- Result: local baseview library test passes; no protocol behavior changed for Linux runners with weston.
- Unresolved: native Wayland renderer/event-loop dispatch into `Window::open_floating` remains the next M5 bottleneck.

### 2026-09-06 — run #106：既有测试三平台通过，完成判断更正

- Command/platform: GitHub Actions hosted native jobs，commit `cbb39b0`，macOS ARM64、Windows x86_64、Ubuntu x86_64。
- Change: 无代码变更；验证当前提交的完整 Phase 1 blocking workflow，并核对 Wayland 测试床与顶层窗口路径。
- Result: [run #106](https://github.com/aizcutei/sunmao/actions/runs/34015968358) 三 job 全部 success。三个 job 的 blocking 步骤均成功；Ubuntu 的 `Probe a headless Wayland compositor` 成功，Wayland 顶层窗口测试通过；三个 artifacts 均已上传且未过期：`phase1-macOS-ARM64`、`phase1-Windows-X64`、`phase1-Linux-X64`。
- Unresolved: 撤回本轮最初的总验收完成判断。Wayland 探针没有接入 `Window::open_floating`、编辑器 renderer/event-loop/input；M5 尚未完成。API 只核实了 artifacts 存在且未过期，下载及测试日志仍需核实。不能用当前 CI 全绿替代缺失实现的验收。

### 2026-09-06 — 修正 Wayland 探针透明缓冲区

- Command/platform: macOS 上源码检查；Docker daemon 未运行，尚无本地 Linux 执行结果。
- Change: `wl_shm` 文件原先全部为零，ARGB alpha 为零，现写入不透明实色像素；缓冲区 stride/size 使用 checked arithmetic 并限制到协议 i32 范围，拒绝零尺寸。
- Result: `cargo metadata --locked`、`cargo fmt --all -- --check`、`git diff --check` 和 `RUSTFLAGS=-Awarnings cargo test --locked` 均通过（完整测试 exit 0，日志 `/tmp/sunmao-phase4-local-tests.log`）。修正 Rust 2018 下 `TryFrom` 导入与数组迭代兼容性。新增尺寸边界测试为 Linux-only，尚未执行；Docker 应用启动后 daemon 仍不可连接。
- Unresolved: 本改动未提交；仍需 Linux 测试、完整本地 gates 和 hosted 验证。原生编辑器 renderer/event-loop/input 接线仍未完成。

### 2026-09-06 — run #107 通过；堵住 Wayland CI 环境缺失时误报成功的路径

- Command/platform: commit `4ac29c7` 的 GitHub Actions run #107；本次 workflow 修正在 macOS ARM64 执行本地 gates。
- Change: Weston 未创建 socket、探针未连接 compositor 两种情况改为 error 并 exit 1，避免缺失测试环境仍得到绿色结果。
- Result: [run #107](https://github.com/aizcutei/sunmao/actions/runs/34027056116) 三平台 job success，Linux Wayland 步骤 success；三个 artifacts 已上传且未过期（仅核实 API，尚未下载验证）。本次修正的 cargo metadata --locked、cargo fmt --all -- --check、git diff --check、RUSTFLAGS=-Awarnings cargo test --locked 均 exit 0，完整测试日志为 `/tmp/sunmao-phase4-ci-gate-tests.log`。
- Unresolved: 本次 workflow 修正待提交及 hosted 验证。原生 Wayland 编辑器的 EGL、窗口分派、事件循环与输入尚未实现；M5 不标记完成。

### 2026-09-06 — Wayland EGL 上下文与真实像素测试

- Command/platform: macOS ARM64 完整本地 gates；Windows MSVC target check；Linux target check 尝试。
- Change: 新增可选 EGL 上下文，持有 Wayland 连接并清理部分初始化资源；新增真实窗口红/绿像素读回、resize、swap、teardown 测试。Ubuntu Wayland 步骤启用 OpenGL、软件渲染、180 秒超时并强制 EGL 成功标记。
- Result: cargo metadata --locked、cargo fmt --all -- --check、git diff --check、RUSTFLAGS=-Awarnings cargo test --locked 均 exit 0；Windows target check exit 0。Linux 交叉检查因 X11 pkg-config 缺少 sysroot 失败，未编译到新增 EGL 代码。完整本地测试日志 `/tmp/sunmao-phase4-egl-tests.log`。此前 CI gate 修正已作为 `67596ef` 推送，run #108 的 Linux/macOS 已 success，Windows 尚在运行。
- Unresolved: EGL 新代码待 hosted 编译和实际渲染验证，不能据 macOS 测试判定通过。Window::open_floating 分派、编辑器接线、事件循环与输入仍未完成。

### 2026-09-06 — EGL hosted 验证通过；接入统一 GL 上下文

- Command/platform: run #109 / commit `7fcd65e` 三平台 hosted；统一 GL 分派改动在 macOS ARM64 跑完整本地测试并执行 Windows target check。
- Change: GlContext 增加 EGL 后端分派，保留原生上下文路径；Wayland 像素验收测试改为通过统一接口执行。
- Result: [run #109](https://github.com/aizcutei/sunmao/actions/runs/34031561111) 三平台 success，Ubuntu EGL 像素、resize、swap、teardown 步骤 success，三份 artifacts 已上传且未过期（尚未下载核验）。当前分派改动 metadata、fmt、diff 检查及完整 cargo test --locked 均通过，Windows target check 通过；日志 `/tmp/sunmao-phase4-gl-dispatch-tests.log`。
- Unresolved: 统一 GL 分派待提交及 hosted 验证；浮动窗口分派、事件循环、输入和实际编辑器接线仍需完成。

### 2026-09-06 — run #110：统一 GL 接口与产物核验

- Command/platform: GitHub Actions run #110，commit `6977d6b`，三平台 hosted jobs。
- Change: 下载 Linux job 日志与三平台 artifacts，核实渲染断言及产物完整性。
- Result: 三平台 success；日志确认 `egl_renders_and_resizes_a_wayland_surface ... ok` 和 `WAYLAND EGL VERIFIED: pixels, resize, swap and teardown`。Windows 78,342,930 bytes / 364 ZIP 条目，Linux 971,565,587 bytes / 96 条目，macOS 53,086,608 bytes / 152 条目；全部 SHA-256 匹配 GitHub digest，ZIP CRC 校验通过。产物位于 `/tmp/sunmao-run110-{windows,linux,macos}.zip`。
- Unresolved: 浮动窗口分派、事件循环与输入尚未实现；M5 保持未完成。

### 2026-09-06 — Wayland 握手增加超时

- Command/platform: macOS ARM64 完整本地 gates，Windows MSVC target check。
- Change: 新增基于 poll 的事件分派和带期限的同步握手；probe、顶层窗口和 EGL 验收使用五秒握手期限；新增 Unix socket 无响应对端测试。
- Result: metadata、fmt、diff、完整 cargo test --locked 和 Windows target check 均通过；日志 `/tmp/sunmao-dispatch-tests.log`。Linux 新路径仍待 hosted 验证。
- Unresolved: 本次超时改动待 hosted 验证；原生浮动窗口分派、编辑器事件循环与输入接线仍未完成。

### 2026-09-06 — run #111 通过；补齐遗漏的顶层握手期限

- Command/platform: GitHub Actions run #111 / `51f7103`；macOS ARM64 本地完整 gates 和 Windows MSVC target check。
- Change: 源码复核发现上一条记录不准确：probe 已使用有期限握手，但 toplevel 与 EGL 测试仍保留 7 处 queue.roundtrip。现全部改为五秒期限的 dispatch::roundtrip。
- Result: run #111 三平台 jobs success，三个 artifacts 已上传且未过期（本轮仅核实 API，未下载）；当前修正 metadata、fmt、diff、完整 RUSTFLAGS=-Awarnings cargo test --locked、Windows target check 均 exit 0。测试日志 /tmp/sunmao-wayland-timeout-tests.log。
- Unresolved: 当前修正待提交及 hosted 验证；原生浮动编辑器窗口分派、事件循环与输入仍未接入，M5 未完成。

### 2026-09-08 — 定位并修复 run #112 的 Linux 编译错误

- Command/platform: 下载 run #112 Linux job 101500655736 日志，核对 check-run annotations。
- Change: 上次替换把嵌套 tests 模块内四处调用写成 super::dispatch，错误地指向 toplevel::dispatch，触发 E0433。统一使用 crate::wayland::dispatch，避免模块深度影响路径解析。
- Result: 日志确认原失败发生在编译阶段，未运行 Wayland 测试。当前 metadata、fmt、diff、Windows MSVC target check 均通过；完整本地测试仍在运行（/tmp/sunmao-fix112-tests.log）。
- Unresolved: 修正待完整本地 gate、提交推送及 hosted 验证。M5 原生浮动编辑器接入仍未完成。

### 2026-09-08 — run #113 通过；Linux Wayland 编译修正验收

- Command/platform: GitHub Actions run #113 / `a6c9e8b`，macOS ARM64、Windows x86_64、Ubuntu x86_64。
- Change: 验证 `crate::wayland::dispatch` 路径修正后的完整 workflow。
- Result: 三平台 jobs 全部 success；Ubuntu 的 headless Wayland probe success。Artifacts `phase1-macOS-ARM64`、`phase1-Windows-X64`、`phase1-Linux-X64` 均已上传且未过期，API digest 已记录。
- Unresolved: M5 仍未完成：`Window::open_floating` 在 Linux 尚未分派到原生 Wayland，编辑器 renderer/event-loop/input 尚未接线。

### 2026-09-08 — 原生 Wayland 浮动编辑器接线与 run #114

- Command/platform: commit `ea89eab`，Ubuntu hosted Weston、macOS ARM64、Windows x86_64。
- Change: Linux 新增 X11/Wayland 分派层；`open_floating` 在 Wayland 会话创建持久化 `xdg_toplevel` worker，绑定 EGL，返回 Wayland raw handle，发送初始/变化 `Resized`，执行 `WindowHandler::on_frame`，处理 compositor close 与有序 EGL/协议销毁；embedded 仍走 X11。
- Result: [run #114](https://github.com/aizcutei/sunmao/actions/runs/34202574798) 三平台 jobs success，Ubuntu Wayland 步骤 success；三份 artifacts 已上传且未过期（macOS 53,086,626 bytes；Windows 78,340,787 bytes；Linux 971,756,295 bytes）。
- Unresolved: `wl_seat`/xkbcommon 输入、output scaling 和面向真实 editor 的 hosted acceptance 尚未完成；M5 继续进行。

### 2026-09-08 — 新增真实 Wayland 编辑器验收，纠正 probe 证据范围

- Command/platform: Linux target cargo check（x11/dox 仅跳过系统库链接探测，不作为运行证据），Windows target check，macOS 完整本地测试。
- Change: 新增 native_wayland_editor_renders_resizes_and_reopens，禁用 DISPLAY 后通过 BaseviewView::open_floating 启动真实 SunMao GL renderer，校验红绿 shader 像素、resize 新帧、两次开关及 state 析构。Ubuntu CI 强制成功标记，不允许跳过。
- Result: Linux 新测试 Rust 类型检查通过，Windows target check 通过；完整本地 RUSTFLAGS=-Awarnings cargo test --locked 已 exit 0（/tmp/sunmao-editor-tests.log）；metadata、fmt、diff 检查均通过。run #114 只有旧 probe，因此不证明该窗口路径实际运行。上一轮附件不可读的判断也更正：命令路径把 abd8 拼错成 ab8d，正确附件一直存在。
- Unresolved: 新增运行验收待 hosted 执行；输入、cursor/focus、output scaling、facade feature 传递仍需完成，M5 保持未完成。

### 2026-09-08 — run #116：真实 Wayland GL 编辑器运行验收通过

- Command/platform: GitHub Actions run 34210132607 / `fee7aa1`；Linux job 102008893143 日志下载到 /tmp/sunmao-run116-linux.log。
- Change: 核对新增编辑器验收实际执行，更新状态矩阵的证据范围。
- Result: 同一提交三平台 jobs success。Linux 日志 09:34:23Z 明确输出 WAYLAND EDITOR VERIFIED: shader rendering, resize, close and reopen without X11，测试通过。较早的普通包测试仅跳过 Wayland 环境，不能当作运行证据。
- Unresolved: M5 输入、cursor/focus、output scaling 和 facade feature 传递仍需完成；此验收没有证明原生输入，也未完成 artifacts 下载核验。

### 2026-09-08 — Wayland seat/pointer 接入

- Command/platform: 按 wayland.xml 核对 seat capabilities、pointer frame 和 button/axis；Linux target 类型检查通过（x11/dox 仅跳过系统库探测）。
- Change: 按 seat 绑定 pointer，热插拔与 capability 移除释放对象；保留 enter 坐标与 frame 内顺序，将 mouse events 转发给真实 WindowHandler。移除 pointer 时补发 release 防止拖拽卡住；新增顺序与移除测试。
- Result: Linux 类型检查通过，fmt/diff/metadata 通过；此前完整本地测试 exit 0（/tmp/sunmao-pointer-tests.log）；新增真实输入验收后的最终回归运行中（/tmp/sunmao-pointer-final-tests.log）。
- Unresolved: 代码尚未提交；Windows all-features target check 与 Linux 测试类型检查已通过，仍需最终完整本地 gate 与 hosted 运行。headless Weston 无 seat，新增单测不证明真实鼠标注入。键盘修饰键、focus/cursor、output scaling 尚未完成。

### 2026-09-08 — 原生 Wayland 鼠标真实输入验收接入 CI

- Command/platform: macOS ARM64 完整 `RUSTFLAGS=-Awarnings cargo test --locked`；Windows MSVC baseview all-features check；Linux baseview/view_baseview tests 类型检查。
- Change: 新增 seat/pointer 接线与移除时释放按键的 proptest。CI 在 Xvfb 内启动带 seat 的 Weston kiosk，由 xdotool 注入真实移动/点击；编辑器进程移除 DISPLAY，只连接 Wayland。`native_wayland_pointer_changes_rendered_pixels` 断言有坐标的左键按下、释放按顺序到达 ViewState，且 shader 像素由红变蓝；脚本强制成功标记并在失败时输出 compositor 日志。
- Result: 完整本地测试 exit 0（/tmp/sunmao-pointer-final-tests.log）；Linux 类型检查与 Windows all-features check exit 0；locked metadata、fmt、diff 和 bash 语法检查通过。本地 macOS 不能提供 Wayland 输入运行证据。
- Unresolved: 新增真实鼠标验收待 hosted CI；键盘/xkbcommon、修饰键、focus/cursor、output scaling、facade feature 传递仍未完成，M5 不标记完成。

### 2026-09-08 — 鼠标验收提交已推送；等待 hosted CI

- Command/platform: `00a5eac` 已通过 HTTPS 推送到 phase4/gui-component-library；GitHub Actions API 确认新 run 34213118156 运行中。
- Change: 等待新鼠标验收的三平台 hosted 结果；同时补核 run #116 的历史产物。
- Result: run #116 三份 ZIP 均已下载并通过 API digest SHA-256 与 ZIP CRC 校验：Windows 78,339,259 bytes / 364 条目，Linux 971,756,391 bytes / 96 条目，macOS 53,086,610 bytes / 152 条目。文件位于 /tmp/sunmao-run116-phase1-{Windows-X64,Linux-X64,macOS-ARM64}.zip。
- Unresolved: [鼠标 CI](https://github.com/aizcutei/sunmao/actions/runs/34213118156) 经再次 API 查询仍运行中：Linux job 102018511076、macOS job 102018511236、Windows job 102018511498 均处于 Test format adapters and host，Wayland pointer 步骤尚未开始。首次查询遇到 TLS 中断，重查成功；未重启 job。下一步核对 Linux WAYLAND POINTER VERIFIED 标记和三 job 结论。

### 2026-09-08 — 修复鼠标 CI 的 Weston 窗口查找

- Command/platform: run 34213118156 / `00a5eac`，Linux job 102018511076 日志与 check-run annotations；源码核对 Weston 13 的 backend-x11/x11.c 与 xdotool 的 xdo_search.c。
- Change: Weston 已创建 640×480 X11 输出（日志 window id 2097157），但只设置 _NET_WM_NAME；xdotool --name 使用 XGetWMName 查询 WM_NAME，因此测试床查找超时。改为按明确设置的 WM_CLASS 精确查找 Weston Compositor，并用 --limit 1 避免管道。
- Result: 原 Linux job 的 headless EGL/editor 验收及 pointer 单测/proptest 均通过；真实鼠标测试尚未启动，不能宣称输入失败或成功。当前 bash 语法、metadata、fmt、diff 检查通过；完整本地 RUSTFLAGS=-Awarnings cargo test --locked 已 exit 0（/tmp/sunmao-pointer-class-tests.log）。
- Unresolved: 修正已通过完整本地 gate，待提交推送与 hosted 实测；本轮继续聚焦真实 pointer 验收，M5 其余未完成项保持不变。

### 2026-09-08 — Weston WM_CLASS 修正已推送

- Command/platform: macOS 完整本地回归 exit 0；HTTPS 推送 `a511279` 到 phase4/gui-component-library。
- Change: 修正嵌套 Weston 的窗口查找，使真实 pointer 测试能够越过测试床准备阶段。
- Result: GitHub API 确认新 run 34214877140 的 head_sha 为 a5112791eb06f9bcd55a3341c3938ce71d6a7b74，状态 queued；原 run 34213118156 已 completed/failure。
- Unresolved: [修正后 CI](https://github.com/aizcutei/sunmao/actions/runs/34214877140) 已从 queued 进入 in_progress。两次间隔 API 查询核实 Linux job 102024176963、macOS job 102024176947、Windows job 102024176777 均已进入 Test format adapters and host；尚无失败，pointer 步骤仍未开始。需核对真实 WAYLAND POINTER VERIFIED 标记及三平台结果，M5 保持未完成。

### 2026-09-08 — 定位鼠标验收的事件与渲染断点

- Command/platform: run 34214877140 / a511279；Linux job 102024176963 日志下载至 /tmp/sunmao-run118-linux.log，已核对 check-run annotations。
- Change: WM_CLASS 修正确认有效，真实测试已执行并读到红色帧，但等待蓝色帧超时。renderer probe 位于 swap/commit 之前，不能据红帧判定表面已映射并获得输入。验收改为先移动并等待 ViewState 收到坐标事件，再点击并等待有序 press/release，最后校验蓝色像素；加入测试事件日志与 WAYLAND_DEBUG=client，区分协议、事件转发、渲染各阶段。
- Result: 原 headless renderer 验收通过；原 pointer 失败只证明点击后未观察到蓝帧，尚不能定位为平台实现缺陷或映射竞态。当前 Linux tests 类型检查、metadata、fmt、diff、bash 语法检查通过；完整本地 RUSTFLAGS=-Awarnings cargo test --locked 已 exit 0（/tmp/sunmao-pointer-readiness-tests.log）。
- Unresolved: 本地 gate 已通过，待提交及 hosted 运行，按事件和协议证据继续修复；M5 其余输入、焦点、光标、缩放与 feature 传递仍未完成。

### 2026-09-08 — 分阶段鼠标验收已推送，等待协议证据

- Command/platform: `7e78eb3` 已 HTTPS 推送到 phase4/gui-component-library；完整本地测试 exit 0。
- Change: 点击前确认移动事件已到达 ViewState，点击后分别确认 press/release 和像素更新，并启用 Wayland 客户端协议日志。
- Result: GitHub API 确认 run 34216552175 / 7e78eb3bfbae9b0cbbedd182267691e6747a58f3 状态 queued；前一 run 34214877140 已 completed/failure。
- Unresolved: [新 CI](https://github.com/aizcutei/sunmao/actions/runs/34216552175) 已进入 in_progress：两次间隔查询确认 Linux job 102029553653、Windows job 102029553912、macOS job 102029553981 均在执行格式适配器与宿主测试；尚无失败，pointer 步骤尚未开始，M5 保持未完成。

### 2026-09-08 — 分阶段鼠标 CI 步骤通过，等待完整 job

- Command/platform: GitHub Actions run 34216552175 / 7e78eb3；Linux job 102029553653。
- Change: 核对本次分阶段输入与协议诊断验收状态，未修改代码。
- Result: GitHub jobs API 确认 Probe a headless Wayland compositor 和 Verify native Wayland pointer input 均 completed/success；后续间隔查询确认三平台均已通过 realtime callback allocation matrix，正在 Package and exercise native GUI backends；尚无失败。
- Unresolved: 三平台 jobs 尚未结束，需下载 Linux 日志核对 WAYLAND POINTER EVENT 与 WAYLAND POINTER VERIFIED 的实际输出，不能据单个步骤宣称 M5 完成。

### 2026-09-08 — run #119：真实 Wayland 鼠标输入验收完成

- Command/platform: GitHub Actions run 34216552175 / 7e78eb3 三平台全部 success；Linux job 102029553653 完整日志已下载至 /tmp/sunmao-run119-linux.log。
- Change: 核对协议事件、编辑器事件与 shader 像素变化，更新状态矩阵和跨格式语义的证据范围。
- Result: 日志 10:45:10Z 明确显示 wl_pointer.enter(320,240)、button 272 的 press/release、ViewState 的 MouseMove/MouseDown/MouseUp，随后 WAYLAND POINTER VERIFIED 与测试 ok。早先普通包测试的 skip 不作为证据。Artifacts API 确认 Linux 971,756,359 bytes、macOS 53,086,641 bytes、Windows 78,345,325 bytes 均已上传且未过期；本轮随后完成三份下载，SHA-256 与 ZIP CRC 均通过：Linux 96 条目、macOS 152 条目、Windows 367 条目，文件 /tmp/sunmao-run34216552175-phase1-*.zip。
- Unresolved: M5 键盘/xkbcommon、修饰键、focus/cursor、output scaling、facade feature 传递及最终同提交产物下载验证仍未完成；下一瓶颈为原生键盘输入。

### 2026-09-08 — 原生 Wayland 键盘接入与真实输入验收

- Command/platform: 按 wl_keyboard 上游协议与 xkbcommon API 实现；Linux baseview/view_baseview tests 类型检查、Windows MSVC all-features check 均通过。
- Change: 按 seat 管理 keyboard/keymap 生命周期，使用 compositor 的 XKB 布局和 masks/group；支持 compose、修饰键、重复与释放，Enter/Leave 驱动被动焦点。失焦/移除清理按键与 compose 状态；复用 Linux 物理键码映射，不复用 X11 硬编码 US 逻辑字符。GL ViewState 接收 FocusIn/FocusOut。
- Result: 新增布局组/Shift/é 测试与焦点重置 proptest，CI 真实注入 Shift+A、dead_acute+e、长按 r 与释放，要求编辑器文字事件和像素更新。核对 Weston X11 backend 后发现其布局来自外层 X server，故测试床用 setxkbmap 与 Xvfb -noreset 明确设定 us(intl)，避免仅改 weston.ini 无效。完整本地 RUSTFLAGS=-Awarnings cargo test --locked 已 exit 0（/tmp/sunmao-keyboard-tests.log）；fmt、diff、locked metadata、bash 语法检查通过。
- Unresolved: 键盘已通过本地 gate，尚待 hosted 实际运行；compose 不等于 text-input-v3 IME。cursor、主动 focus、output scaling、facade feature 传递与最终同提交产物下载核验仍未完成，M5 保持进行中。

### 2026-09-08 — 键盘提交已推送，等待三平台 hosted 验收

- Command/platform: `aa9d25e1ad2fdbf3249af5c12d1466f8b6043c41` 已通过 HTTPS 推送至 phase4/gui-component-library，完整本地测试 exit 0。
- Change: 启动键盘真实输入验收；记录 run #119 历史产物已全部下载且 SHA-256/ZIP CRC 校验通过。
- Result: [新 CI](https://github.com/aizcutei/sunmao/actions/runs/34219433180) 已从 queued 进入 in_progress。Windows job 102038801629、macOS job 102038802074 正在 Test format adapters and host；Linux job 102038802033 正在安装 GUI 依赖。真实 pointer and keyboard 步骤尚未开始。随后两次间隔查询确认 Linux 依赖安装已完成；最新间隔查询：Linux 已进入 Test the accessibility feature，macOS 正在 Test standalone runtime, facade, and reference examples，Windows 已进入 Check facade renderer contracts independently。真实键盘步骤仍 pending，尚无失败；本轮为已验证的 CI 等待。
- Unresolved: 等待三 job 结论并下载 Linux 日志确认 WAYLAND KEYBOARD VERIFIED 实际输出；不能据编译或旧鼠标证据判断新键盘通过。M5 后续 cursor/主动 focus、缩放、feature 传递与最终产物验收保持未完成。

### 2026-09-08 — Linux 真实键盘步骤通过，等待完整 job 与日志

- Command/platform: GitHub Actions run 34219433180 / aa9d25e，Linux job 102038802033；两次间隔 jobs API 查询。
- Change: 持续核对同一提交的 hosted 运行状态，未修改代码。
- Result: Linux 的 Probe a headless Wayland compositor 与 Verify native Wayland pointer and keyboard input 均 completed/success，已进入 Check baseview feature combinations。Windows 正在 standalone runtime/facade/reference examples 测试；macOS 正在非 blocking 的 system-capture 后续检查。三 job 尚未结束。后续间隔查询确认 Linux/macOS 均已进入 Package and exercise native GUI backends，Windows 已通过实时回调分配矩阵，正在构建 standalone 应用；仍无失败，Linux job 尚未结束，完整日志待下载。
- Unresolved: 步骤通过尚不是最终完整验收；需下载 Linux job 日志确认 WAYLAND KEYBOARD VERIFIED、Shift+A/é/repeat/release 实际输出，核对三平台 job 结论与产物。M5 保持未完成。

### 2026-09-08 — Linux 键盘完整 job 与真实事件日志核验通过

- Command/platform: run 34219433180 / aa9d25e，Linux job 102038802033 completed/success；完整日志 /tmp/sunmao-run34219433180-linux.log。
- Change: 下载并核对 wl_keyboard 协议事件、ViewState 文本事件与强制验收标记，未修改代码。
- Result: 11:16:58Z 日志确认 keymap/enter/modifiers，Shift+A 对应 TextInput A，dead_acute+e 对应 TextInput é；11:16:59Z 两次客户端 repeat 后收到 R 的 release，随后 WAYLAND KEYBOARD VERIFIED 与测试 ok，包含像素变化断言。布局/compose 单测与焦点重置 proptest 均执行并通过。普通包测试中早先同名测试的 skip 不作为证据。
- Unresolved: Windows/macOS 尚在 Package and exercise native GUI backends，需继续核对同一提交的完整三平台结论与 artifacts。cursor、主动 focus、output scaling、facade feature 传递仍未完成；M5 不标记完成。

### 2026-09-08 — run #120 三平台全绿，键盘瓶颈验收通过

- Command/platform: GitHub Actions run 34219433180 / aa9d25e；Windows 102038801629、Linux 102038802033、macOS 102038802074 均 completed/success。
- Change: 更新 M5 状态矩阵和跨格式语义的键盘证据范围，开始下载本提交三平台 artifacts。
- Result: 同一提交三平台完整 jobs success；Linux 真实 Shift+A、组合 é、repeat/release 与像素变化日志已核实，单测/proptest 亦通过。三份产物已下载且 SHA-256/ZIP CRC 校验通过：Windows 78,351,134 bytes / 364 条目；macOS 53,085,640 bytes / 152 条目；Linux 972,286,500 bytes / 96 条目。文件 /tmp/sunmao-run34219433180-phase1-*.zip。
- Unresolved: 本提交产物下载校验已完成；M5 cursor、主动 focus、output scaling、facade feature 传递与最终兼容性/文档审计仍待完成。下一瓶颈为光标与主动焦点，不能据键盘验收宣称整个 Phase 4 完成。

### 2026-09-08 — Wayland 光标通用协议接入与像素验收

- Command/platform: 核对 wl_pointer.set_cursor/enter 上游协议与 wayland-cursor 主题库；Linux baseview tests 类型检查 exit 0。
- Change: set_mouse_cursor 从 no-op 改为窗口线程设置主题图像或隐藏，使用每次 enter 的最新 serial；支持动画，离开/移除 pointer 清理状态。使用通用 wl_pointer/wl_shm，不依赖可选 cursor-shape 扩展。缺少主题形状时记录诊断并回退默认箭头。
- Result: 加入序号失效 proptest；新增真实 compositor 像素验收，检查十字、手形、隐藏与重入后恢复。截图观察器通过 X11 读取 Weston 的窗口像素，编辑器进程仍禁用 DISPLAY。Linux 编译、fmt、diff、locked metadata 与脚本语法检查通过；Windows all-features check 已 exit 0（/tmp/sunmao-cursor-windows.log）；完整本地测试仍运行中，保持 session 14031（/tmp/sunmao-cursor-tests.log），不可因观察超时重启。后续两次轮询确认同一进程仍存活，编译已结束，baseview 测试通过，当前已通过 facade 模板行数测试，正在执行 voice proptest，尚无失败；本轮为已验证的本地 gate 等待。
- Unresolved: 光标实现尚待本地 gate 收尾、提交与三平台 hosted 实测。主动 focus、output scaling、facade feature 传递及最终审计仍未完成；M5 保持进行中。

### 2026-09-08 — 光标完整本地 gate 通过，准备 hosted 验收

- Command/platform: 保留原 session 14031 的 macOS ARM64 完整 `RUSTFLAGS=-Awarnings cargo test --locked`；通过 GitHub API 与 ls-remote 核对用户手动 push 后的分支。
- Change: 光标实现、主题依赖与真实 compositor 像素测试准备提交；未重启完整测试，未重复已通过的 Linux/Windows 类型检查。
- Result: 完整测试含 doc-tests 已 exit 0（/tmp/sunmao-cursor-tests.log）。远端仍为 aa9d25e，对应 run #120 三平台 success；光标改动在本地，尚未包含在该 run 中。Linux tests check、Windows all-features check、locked metadata、fmt/diff 和脚本语法检查均已通过。
- Unresolved: 新光标提交需触发 hosted CI，核对真实 WAYLAND CURSOR VERIFIED 与三平台同提交产物；主动 focus、output scaling、facade feature 传递和最终审计仍未完成，M5 不标记完成。

### 2026-09-08 — 光标提交已推送，GitHub CI 已启动

- Command/platform: HTTPS 推送 dcbebb7fe00b7946041a0f5ea701f2007fc503d6 至 phase4/gui-component-library；GitHub Actions API 查询。
- Change: 提交原生 Wayland 光标主题/隐藏/动画/重入处理，以及 Weston 输出像素验收。
- Result: 推送成功，API 确认新 run 34224204311 的 head_sha 为 dcbebb7fe00b7946041a0f5ea701f2007fc503d6，初始状态 queued：https://github.com/aizcutei/sunmao/actions/runs/34224204311 。完整本地 gate 已通过。
- Unresolved: 两次间隔 API 查询确认同一 run 已 in_progress：Linux job 102054224166 完成依赖安装，Windows job 102054224314 完成 Rust 安装，与 macOS job 102054224525 均进入 Test format adapters and host。尚无失败，真实 pointer/keyboard/cursor 步骤仍 pending。本轮为已验证的 CI 等待；需核对 Linux WAYLAND CURSOR VERIFIED 实际日志与三份 artifacts。后续两次间隔查询确认三平台均已通过格式适配器与宿主测试；最新 Linux/macOS 正在 Test standalone runtime, facade, and reference examples，Windows 正在 Check facade renderer contracts independently，真实光标步骤仍 pending，尚无失败。M5 保持未完成，CI 运行期间不推进其它实现。

### 2026-09-08 — 定位并修正光标重入测试的 compositor 假设

- Command/platform: run 34224204311 / dcbebb7，Linux job 102054224166 failure；下载完整日志至 /tmp/sunmao-run34224204311-linux.log，核对 commit check-runs 与 annotations，阅读 Weston 13 libweston/input.c、backend-x11/x11.c、kiosk-shell/kiosk-shell.c 上游源码。
- Change: 原测试移动到外层 X11 窗口之外等待 wl_pointer.leave；Weston 的 X11 LeaveNotify 调用 clear_pointer_focus，而该函数实际上为空（上游 FIXME），所以没有 leave 协议事件。改为映射第二个绿色原生 Wayland 窗口，验证原窗口 leave 和新窗口 enter，然后关闭第二窗口，验证原窗口的新 enter 与手形像素恢复。保留十字/手形/隐藏/重入全部断言，不重跑旧 job 掩盖失败。
- Result: 旧 run 的真实鼠标和键盘标记通过；光标已通过隐藏、十字、手形、再次隐藏的像素断言，失败明确为 pointer leave 超时。修正后的 Linux baseview tests 类型检查 exit 0（/tmp/sunmao-cursor-reentry-check.log）；locked metadata、fmt 和 diff 检查通过。完整本地回归已启动并确认持续运行，session 24351（/tmp/sunmao-cursor-reentry-tests.log）；不得因观察超时重启。本轮仅修改 Linux 专属验收测试，没有改动平台实现或打包/示例。
- Unresolved: 本地完整 gate 仍运行；本轮两次轮询确认 session 24351 持续存活，baseview 的三项 macOS 生命周期/handler 测试已通过，尚无失败。旧 run 的 Windows 正在 Build cross-platform examples and tools、macOS 正在 Package and exercise native GUI backends，暂无新增失败。后续两次轮询确认同一 session 24351 已推进过 CLAP/VST3 后端与 core，DSP 的 48 项单元测试全部通过，正在运行 DSP proptest；尚无失败。旧 run 的 Windows/macOS 均已进入原生 GUI 打包验收。本轮为已验证的等待，修正尚未提交。需修正提交的真实 hosted 光标重入证据及三平台同提交产物；M5 主动 focus、缩放、feature 传递与最终审计仍待完成。

### 2026-09-08 — 光标重入修正完整本地 gate 通过

- Command/platform: 原 session 24351 的 macOS ARM64 `RUSTFLAGS=-Awarnings cargo test --locked` 完整执行，含 doc-tests；GitHub API 核对旧 run 34224204311。
- Change: 用第二个原生 Wayland 表面替代外层 X11 窗口离开动作，保留真实 leave/enter 与光标像素恢复断言。
- Result: 完整本地回归 exit 0（/tmp/sunmao-cursor-reentry-tests.log）；Linux tests 类型检查、locked metadata、fmt、diff 检查已通过。旧 CI 已结束，Windows/macOS success，Linux 仅真实 pointer/keyboard/cursor 步骤 failure，已定位为 Weston 空 clear_pointer_focus 引起的 leave 超时。
- Unresolved: 修正待提交推送与 hosted 验收，尚无新重入运行证据；M5 其它输入平台收尾与最终审计保持未完成。

### 2026-09-08 — 光标重入修正已推送，等待 hosted 验收

- Command/platform: HTTPS 推送 8dee3e84089cf7588c3336c8f47a5217d1012504 至 phase4/gui-component-library；完整本地测试 exit 0。
- Change: 原生双窗口切换触发 compositor 的真实 leave/enter，避免 Weston X11 外层离开的空实现。
- Result: GitHub API 确认新 run 34226222440 / 8dee3e8 已 in_progress：https://github.com/aizcutei/sunmao/actions/runs/34226222440 。
- Unresolved: 两次间隔 API 查询确认同一 run 34226222440 / 8dee3e8 持续运行：Windows job 102060896711、Linux job 102060896912 均已完成环境安装，与 macOS job 102060897115 一起执行 Test format adapters and host。尚无失败，真实光标步骤仍 pending；本轮为已验证的 CI 等待。后续两次间隔查询确认 Linux/macOS 已推进到 Test the accessibility feature，Windows 已通过格式适配器与宿主测试，正在 Check facade renderer contracts independently；真实光标仍 pending，尚无失败。需新提交三平台 hosted jobs、Linux WAYLAND CURSOR VERIFIED 实际日志与同提交 artifacts；M5 保持未完成。CI 运行中仅监控记录，失败按日志修正。

### 2026-09-08 — 修正后的真实光标步骤通过，等待完整 hosted 结果

- Command/platform: GitHub Actions run 34226222440 / 8dee3e8；两次间隔 jobs API 查询。
- Change: 核对原生双窗口光标重入验收状态，未修改实现。
- Result: Linux job 102060896912 的 Probe a headless Wayland compositor 与 Verify native Wayland pointer, keyboard and cursor 均 completed/success，后续已通过 baseview feature combinations，正在构建示例和工具。macOS job 102060897115 正在原生 GUI 打包验收，Windows job 102060896711 正在 Phase 4 fixtures。尚无失败。
- Unresolved: 光标步骤通过不等于完整验收；本轮两次间隔查询确认 Linux/macOS 正在 Package and exercise native GUI backends，Windows 已通过 baseview feature combinations，正在 Build cross-platform examples and tools；尚无失败，Linux job 未结束，完整日志暂未下载。需 Linux 完整 job 日志的 WAYLAND CURSOR VERIFIED、真实 leave/enter 与像素断言，并核对三 job 结论及同提交 artifacts。M5 保持未完成，本轮为已验证的 CI 等待。

### 2026-09-08 — Linux 光标完整 job 与真实重入日志核验通过

- Command/platform: run 34226222440 / 8dee3e8，Linux job 102060896912 completed/success；完整日志 /tmp/sunmao-run34226222440-linux.log。
- Change: 下载并核对真实光标像素验收与双窗口切换协议，未修改实现。
- Result: 日志 12:33:44Z 确认十字、手形、隐藏请求；映射绿色第二窗口使原窗口收到 leave(38)，第二窗口 enter(39)；关闭第二窗口后原窗口 enter(44)，set_cursor 使用新 serial 44 恢复手形。随后 WAYLAND CURSOR VERIFIED 与 native_wayland_cursor_changes_compositor_pixels_and_survives_reentry ... ok，包含全部像素断言。序号失效 proptest 亦通过。普通包测试的早先 skip 不作为 runtime 证据。macOS job 102060897115 也已 success。
- Unresolved: Windows job 102060896711 仍在原生 GUI 打包验收；需等待同提交三平台完整结论并下载校验 artifacts。主动 focus、output scaling、facade feature 传递和最终审计仍未完成，M5 不标记完成。

### 2026-09-08 — 光标三平台验收与同提交产物下载完成

- Command/platform: GitHub Actions run 34226222440 / 8dee3e84089cf7588c3336c8f47a5217d1012504；Windows 102060896711、Linux 102060896912、macOS 102060897115 全部 completed/success。
- Change: 更新 M5 状态矩阵和跨格式语义中的原生光标证据；光标瓶颈已收口，仍保留其它未完成项。
- Result: Linux 真实十字/手形/隐藏与双窗口 leave/enter、新 serial 恢复手形的像素断言通过，完整日志已核实。下载 session 94398 exit 0；三份 ZIP 全部通过 API SHA-256 与 ZIP CRC：Windows 78,349,976 bytes / 364 条目，Linux 972,286,481 bytes / 96 条目，macOS 53,085,641 bytes / 152 条目。文件 /tmp/sunmao-run34226222440-phase1-*.zip。
- Unresolved: M5 主动 focus、output scaling、facade feature 传递和最终兼容性/文档审计仍未完成；下一瓶颈为主动 focus。compose 不等于 text-input-v3 IME，不能据光标成功宣称完整 Wayland 或 Phase 4 完成。

### 2026-09-08 — 原生 Wayland 主动焦点请求与 compositor 策略验收接入

- Command/platform: 阅读 xdg-activation-v1 上游 XML、Sway 1.9 激活与 launcher 策略；核对 Weston 13 源码与上一轮实际 registry 日志。
- Change: focus() 从 no-op 改为异步 xdg_activation_v1 token/activate 请求，保留最新输入序号与 seat，处理扩展/seat 移除、请求合并、超时和销毁；只由真实 keyboard Enter/Leave 更新焦点。新增序号 proptest、双窗口真实焦点与按键路由验收，以及 Sway headless blocking CI 步骤。Weston 13 无该扩展，原有 Weston 验收全部保留。
- Result: Linux baseview tests 类型检查和 Windows all-features 检查均 exit 0；locked metadata、fmt、diff 和 bash 语法检查通过。完整本地 RUSTFLAGS=-Awarnings cargo test --locked 已启动，session 6214，日志 /tmp/sunmao-focus-tests.log；当前编译结束、测试持续运行，不得因观察超时重启。后续两次间隔轮询确认同一 session 6214 持续推进，baseview 已通过，当前进入 clap_sys 示例，尚无失败；本轮为已验证的本地 gate 等待。
- Unresolved: 完整本地 gate 尚在运行；新实现未提交，Sway 实测尚无 hosted 证据。验收范围是激活协议请求与拒绝策略下真实键盘焦点/路由，不承诺强制抢焦点。M5 缩放、facade feature 传递和最终审计仍未完成。

### 2026-09-08 — 主动焦点完整回归持续推进

- Command/platform: 重新读取任务约束与状态；两次间隔轮询原始完整测试 session 6214（/tmp/sunmao-focus-tests.log）。
- Change: 核对 wtype 上游实现，确认常驻注入器先创建虚拟键盘并上传 keymap 再休眠，可保持 headless seat 的键盘能力；未修改平台实现。
- Result: session 6214 仍存活并持续推进；baseview 通过，CLAP 后端 38 项全部通过（view_creation_panic_is_contained_before_returning_through_clap_abi 用时约 65 秒后正常通过），当前执行 VST3 后端，尚无失败。上一轮属于实现进展及已验证等待，本轮属于已验证的本地 gate 等待。后续一轮重新确认 session 6214 存活，两次间隔轮询显示已推进过示例和 GUI 后端（WebView 2 项通过），vst3_rs 的 66 项全部通过，当前进入 vst3_rs 示例；完整进程尚未结束，无失败。再一轮两次间隔确认原 session 6214 已完成常规测试，推进到 sunmao 文档测试；仍无失败，等待整个进程的最终退出码。随后一轮两次间隔轮询确认 facade/后端文档测试已推进，core 的 15 项文档测试全部通过（50.05 秒），当前进入 DSP 文档测试，原 session 6214 仍存活。后续两次间隔轮询确认 DSP 文档测试已通过，当前推进到 widgets GUI fixture 的文档测试；原进程保持运行，无失败，本轮继续作为已验证等待。
- Unresolved: 完整回归尚未结束，新主动焦点改动尚未提交推送；必须等待原进程结束后继续提交与同提交三平台 hosted 验收。M5 保持未完成。

### 2026-09-08 — 主动焦点完整本地 gate 通过

- Command/platform: 原 session 6214 的 macOS ARM64 RUSTFLAGS=-Awarnings cargo test --locked 完整执行，日志 /tmp/sunmao-focus-tests.log。
- Change: 准备提交 xdg_activation_v1 主动焦点请求、输入序号生命周期与 Sway 真实键盘焦点策略验收；保留已完成的光标证据更新。
- Result: 全部常规测试与 doc-tests 完成，原进程 exit 0。Linux tests 类型检查、Windows all-features check、locked metadata、fmt、diff 与脚本语法检查均已通过。没有修改示例或打包逻辑。
- Unresolved: 主动焦点实现仍需新提交的三平台 hosted jobs 与 Sway 实测日志，以及同提交 artifacts 下载校验；M5 缩放、facade feature 传递和最终审计仍待完成。

### 2026-09-08 — 主动焦点提交已推送，GitHub CI 已排队

- Command/platform: HTTPS 推送 7ed7be099b0b7ab41584eb7d2b7116981ff12a16 至 phase4/gui-component-library；GitHub Actions API 查询。
- Change: 提交原生 Wayland 激活请求及 Sway 双窗口焦点/按键路由验收，原 Weston 步骤保持 blocking。
- Result: push exit 0；API 确认新 run 34231341466 的 head_sha 精确匹配 7ed7be099b0b7ab41584eb7d2b7116981ff12a16，当前 queued：https://github.com/aizcutei/sunmao/actions/runs/34231341466 。完整本地 gate 已通过。
- Unresolved: 两次间隔查询确认 run 34231341466 / 7ed7be0 的三 job 持续 in_progress：macOS 102077973525 已通过格式适配器与宿主测试，进入 standalone runtime/facade/reference examples；Linux 102077973855、Windows 102077973891 正在格式适配器与宿主测试，均无失败。Sway 主动焦点步骤仍 pending，本轮为已验证的 CI 等待。后续一轮两次间隔查询确认三平台格式适配器与宿主测试均通过；macOS 已通过 accessibility 并进入非 blocking 的 system-capture 后续检查，Linux/Windows 已通过 facade renderer contracts 并执行 standalone runtime/facade/reference examples；Linux 的 Sway 步骤仍 pending，无失败。需同提交三平台 hosted 完整 jobs、实际 WAYLAND FOCUS VERIFIED 与 xdg_activation 协议日志、三份 artifacts 下载校验。CI 运行期间只监控记录，失败按日志修正；M5 保持未完成。

### 2026-09-08 — Sway 主动焦点 hosted 步骤通过

- Command/platform: run 34231341466 / 7ed7be0；两次间隔 GitHub jobs API 查询。
- Change: 持续核对同一提交的真实 Wayland 验收步骤，未修改实现。
- Result: Linux job 102077973855 的 Probe a headless Wayland compositor、Verify native Wayland pointer, keyboard and cursor、Verify native Wayland activation and focus policy 均 completed/success，已进入 baseview feature combinations。macOS 正在原生 GUI 打包验收，Windows 正在 accessibility，尚无失败。
- Unresolved: 步骤通过尚不是完整验收；后续两次间隔查询确认三平台均已推进到 Package and exercise native GUI backends，暂无失败，Linux job 尚未结束、完整日志暂不可下载。后续一轮三次间隔查询确认 macOS 完整 job success；Linux 已通过原生 GUI 打包验收，正在 Upload packaged Phase 1 artifacts；Windows 已推进到 Exercise repository packaging helper。evidence 查询确认 Linux 仍非终态，本轮未取得完整日志，不将步骤成功替代日志证据。需 Linux 完整日志中的 WAYLAND FOCUS VERIFIED、真实 token/serial/activate 与 keyboard Enter/Leave/按键路由断言，并核对三平台最终 jobs 和同提交 artifacts 下载校验。M5 保持未完成，本轮为已验证的 CI 等待。

### 2026-09-08 — 主动焦点三平台全绿，真实协议日志核实

- Command/platform: run 34231341466 / 7ed7be099b0b7ab41584eb7d2b7116981ff12a16；macOS 102077973525、Linux 102077973855、Windows 102077973891 全部 completed/success。
- Change: 下载并核对 Linux 完整日志 /tmp/sunmao-run34231341466-linux.log；启动同提交三份 artifacts 下载校验（session 69649）。
- Result: 13:27:39Z 确认前台与后台两次 get_activation_token/set_serial(9)/set_surface/commit → Done → activate → destroy。13:27:40Z 确认后台请求后按键仍送给 peer；关闭 peer 后原窗口获得真实 keyboard.enter(33)。测试 native_wayland_focus_request_obeys_compositor_policy ... ok 与 WAYLAND FOCUS VERIFIED，序号移除 proptest 通过。普通包测试早先的无 compositor skip 不作为运行证据。验收证明 advisory 请求和 compositor 拒绝策略下的焦点/按键路由，不承诺绕过策略强抢焦点。
- Unresolved: 三份产物下载与 SHA-256/ZIP CRC 校验仍运行，保持 session 69649。两次间隔轮询确认该进程仍存活；Windows 78,350,801 bytes / 364 条目已通过 SHA-256 与 ZIP CRC，其余仍下载中。后续两次间隔轮询确认 session 69649 持续存活；Linux ZIP 实际写入量由 441,483,264 增至 714,457,088 bytes，下载有进展，尚未完成校验；未重复启动。M5 output scaling、facade feature 传递和最终兼容性/文档审计仍未完成，不能标记 Phase 4 完成。

### 2026-09-08 — 主动焦点同提交三份产物校验完成

- Command/platform: 原下载 session 69649 exit 0；run 34231341466 / 7ed7be099b0b7ab41584eb7d2b7116981ff12a16 三平台 completed/success。
- Change: 更新 M5 状态矩阵与跨格式语义中的主动焦点验收证据，保留 compositor 可拒绝请求的明确语义。
- Result: 三份产物全部下载且 API SHA-256 与 ZIP CRC 校验通过：Windows 78,350,801 bytes / 364 条目；Linux 972,286,508 bytes / 96 条目；macOS 53,085,652 bytes / 152 条目。文件 /tmp/sunmao-run34231341466-phase1-*.zip。Linux Sway 真实协议与焦点/按键路由日志已核实，主动焦点瓶颈收口。
- Unresolved: 下一瓶颈为 Wayland output scaling，其后 facade feature 传递与最终兼容性/文档审计；M5 和 Phase 4 保持未完成。compose 仍不等于 text-input-v3 IME。

### 2026-09-08 — Wayland 输出缩放接入与真实密度验收

- Command/platform: 阅读 wl_output scale/done、wl_surface enter/leave/preferred_buffer_scale/set_buffer_scale、xdg-shell configure/min/max、fractional-scale-v1 与 viewporter 上游协议。
- Change: 区分 surface-local 逻辑尺寸与 buffer 物理像素；接入整数输出集合与移除、preferred buffer scale、fractional preferred scale/viewport，configure 按 xdg_surface 提交批次应用且独立保留零维度；resize 限定逻辑大小，密度与新缓冲区原子提交。光标按输出比例加载更高分辨率主题，修正热点/损伤坐标与 viewport 生命周期。加入比例/输出移除 proptest、Sway 动态整数/分数/跨屏/移除/显式覆盖与 EGL 真实尺寸/边缘像素测试、Weston 整数 fallback、2x 光标真实 compositor 像素验收。
- Result: Linux baseview tests 类型检查 exit 0（/tmp/sunmao-scale-linux.log），locked metadata/fmt/diff/bash 语法检查通过。首次 Linux 检查抓到 Rust 2018 闭包捕获 options 的部分移动，已修正且后续检查通过。Windows all-features 检查 session 27833 仍在执行（/tmp/sunmao-scale-windows.log）；完整本地回归 session 4841 已启动，当前等待同一 Cargo build 锁（/tmp/sunmao-scale-tests.log），不得因等待重启。
- Unresolved: 后续间隔轮询确认 Windows check session 27833 已 exit 0（1m39s）；完整回归 session 4841 已获得构建锁并持续编译，尚无失败。后续两次间隔轮询确认同一进程已完成编译（3m45s），正在执行 baseview 测试，尚无失败；本轮为已验证的本地 gate 等待。改动未提交；Sway/Weston 新缩放与光标实测尚无 hosted 证据。M5 output scaling 待验收，facade feature 传递及最终审计仍未完成。

### 2026-09-08 — 用户手动 push 后核对 CI 与缩放回归

- Command/platform: GitHub Actions API 查询最新 phase4/gui-component-library runs；git status/log；继续轮询原完整回归 session 4841。
- Change: 核对已推送提交与待验收缩放改动的边界，未重启测试或旧 CI。
- Result: GitHub 最新 run 仍为 34231341466 / 7ed7be0，三平台 completed/success；本地 HEAD 同为 7ed7be0，缩放实现和新 CI 步骤仍未提交，所以现有绿色结果不覆盖缩放。原 session 4841 持续存活，已通过 baseview 并推进到 clap_sys 示例，当前尚无失败。
- Unresolved: 完整本地 gate 尚未结束；按既定流程等待原进程退出成功后提交和 HTTPS 推送缩放改动，再核对新提交三平台 hosted 结果、实际 Sway/Weston 缩放日志与产物。M5 保持未完成，本轮为已验证的等待。

### 2026-09-08 — 缩放完整回归持续推进

- Command/platform: 重新读取任务与阶段约束；两次间隔轮询原完整回归 session 4841（/tmp/sunmao-scale-tests.log）。
- Change: 未改动实现，未重启原进程；上一轮及本轮均为已验证的本地 gate 等待。
- Result: 原 session 4841 持续存活，已推进过 clap_sys 示例、facade 与模板预算测试，当前执行 CLAP backend；已输出 state 迁移、零分配处理、事件路由等测试通过，尚无失败。 后续一轮两次间隔轮询确认 VST3 backend 两项超过 60 秒的视图回调测试已正常通过；原进程继续推进过 core 与 DSP，DSP 14 项属性测试全部通过，当前进入示例测试，尚无失败。该轮仍为已验证等待。 再一轮两次间隔轮询确认原 session 4841 继续执行示例：meter 10 项、widgets GUI fixture 9 项全部通过（含音频侧零分配发布与 accessibility 描述树），尚无失败，完整进程仍存活。 后续两次间隔轮询确认同一进程已推进过 GUI 与合成器/模板示例，poly synth 6 项通过，当前进入 sunmao_unittest_runner；尚无失败，本轮为已验证等待。 后续一轮两次间隔轮询确认常规测试全部结束，原 session 4841 已进入文档测试并推进到 sunmao_core；尚无失败，仍待整个命令退出码，该轮为已验证等待。
- Unresolved: 完整回归尚未结束，缩放实现尚未提交；退出成功后提交推送，等待同提交三平台 hosted 缩放运行日志和产物验收。M5 保持未完成。

### 2026-09-08 — 缩放完整本地 gate 通过

- Command/platform: 原 session 4841 的 macOS ARM64 RUSTFLAGS=-Awarnings cargo test --locked 完整结束，日志 /tmp/sunmao-scale-tests.log。
- Change: 准备提交 Wayland 输出整数/分数缩放、逻辑几何与缓冲区密度分离、光标密度更新，以及 Sway/Weston 实际 EGL 尺寸/像素验收。
- Result: 原完整回归 exit 0，包含全部常规测试与 doc-tests。Linux tests 类型检查、Windows all-features 检查、locked metadata、fmt、diff 与脚本语法检查已通过。未改动示例或打包逻辑。
- Unresolved: 缩放实现仍需同提交三平台 hosted 完整 jobs、实际动态密度/跨输出/移除/显式覆盖/2x 光标验收日志和三份产物下载校验。facade feature 传递与最终审计仍待完成；M5 保持未完成。

### 2026-09-08 — 缩放提交已推送，hosted CI 已排队

- Command/platform: HTTPS 推送 708f391f7ac2aea308381facfcd24276f127eb2e 至 phase4/gui-component-library；GitHub Actions API 查询。
- Change: 提交输出缩放实现与新增 blocking Sway/Weston 缩放及 2x 光标步骤，保留全部旧验收。
- Result: push exit 0；API 确认 run 34237571022 的 head_sha 精确匹配 708f391f7ac2aea308381facfcd24276f127eb2e，状态 queued：https://github.com/aizcutei/sunmao/actions/runs/34237571022 。完整本地 gate 已通过。
- Unresolved: 两次间隔 API 查询确认 run 34237571022 / 708f391 三平台持续 in_progress：macOS job 102099154161 已通过格式适配器与宿主测试，进入 standalone runtime/facade/reference examples；Linux job 102099154608 已通过格式适配器与宿主测试，进入 facade renderer contracts；Windows job 102099154489 正在格式适配器与宿主测试。尚无失败，Linux 新缩放步骤仍 pending。本轮为已验证的 CI 等待。后续一轮两次间隔查询确认 Windows 已通过格式适配器与宿主测试，正在 facade renderer contracts；Linux 已推进到 accessibility；macOS 已通过 accessibility，进入 baseview feature combinations（Wayland 步骤按平台条件 skipped）。Linux 新缩放步骤仍 pending，三平台尚无失败。需同提交三平台完整 jobs、Linux 新缩放步骤实际运行日志和三份产物下载校验；CI 运行期间只监控记录，失败按日志修复。M5 保持未完成。

### 2026-09-08 — 修正输出移除验收的首选比例假设

- Command/platform: run 34237571022 / 708f391，Linux job 102099154608 failure；下载 /tmp/sunmao-run34237571022-linux.log，核对 check-runs/annotations；阅读 fractional-scale-v1 XML 与 Sway 1.9 surface.c、output.c、container.c。
- Change: Sway 验收不再假定禁用 2x 输出必然下发 1x preference。保留移除后真实绘制检查，清空旧帧后确认最后的 2x surface preference 仍生效；随后将剩余输出调到 3x，验证新通知、EGL 密度和逻辑 resize。脚本增加 global_remove、wl_output release 和 preferred_scale(360) 的日志检查。平台实现未改动，语义文档补充首选比例独立于输出集合。
- Result: 原 hosted 日志确认 1→2→1.5→1 与跨屏 2x 的 EGL 实际尺寸/边缘像素全部通过；global_remove(45) 与 enter(output10) 后未收到新 preferred_scale，客户端继续正常提交 320x240 缓冲区，旧测试等待 1x 超时。Sway surface_update_outputs 对 current_outputs 求最大比例，而 output_disable 直接 untrack 容器输出，不走 surface_leave_output，因此此场景保留 2x。Linux tests 类型检查 exit 0（/tmp/sunmao-scale-removal-check.log），metadata/fmt/diff/bash 语法检查通过。完整本地回归 session 70371 已启动，日志 /tmp/sunmao-scale-removal-tests.log。
- Unresolved: 本轮多次间隔轮询确认原 session 70371 持续推进，常规测试全部通过，文档测试已推进过 core/DSP 至效果器示例；暂无失败。旧 CI 的 macOS job 102099154161 已 completed/success，Windows job 102099154489 进入原生 GUI 打包验收。上一轮为修正进展，本轮为已验证等待。修正仍待完整本地 gate 与新提交 hosted 运行；Weston core fallback、2x 光标步骤在旧失败之后尚未执行，不能宣称缩放验收完成。原 CI 的 Windows/macOS 继续运行，不重跑旧 Linux job 掩盖失败；M5 保持未完成。

### 2026-09-08 — 输出移除验收修正完整 gate 通过

- Command/platform: 原 session 70371 的 macOS ARM64 RUSTFLAGS=-Awarnings cargo test --locked 完整结束，日志 /tmp/sunmao-scale-removal-tests.log。
- Change: 提交 Sway 输出移除后的 surface preference 保留与后续 3x 通知验收，以及协议日志检查和语义说明。
- Result: 完整回归 exit 0，包含全部 doc-tests；Linux tests 类型检查、metadata/fmt/diff/bash 语法检查已通过。只修改 Linux 专属测试、验收脚本与文档，无平台实现或示例/打包改动。
- Unresolved: 修正待新提交三平台 hosted 验收，旧 run 的 macOS 已 success、Linux 为已定位的测试假设失败；还需新日志与三份同提交 artifacts。M5 保持未完成。

### 2026-09-08 — 输出移除验收修正已推送

- Command/platform: HTTPS push 0b9e8431407b65ebb3dcf312a553713a3929b114 至 phase4/gui-component-library；GitHub Actions API 查询。
- Change: 保留真实输出移除、后续比例变化、EGL 尺寸/边缘像素、Weston fallback 与 2x 光标验收，按 surface preference 协议纠正 Sway 1.9 的预期。
- Result: push exit 0；新 run 34239337659 精确匹配修正提交，当前 pending：https://github.com/aizcutei/sunmao/actions/runs/34239337659 。完整本地 gate 已通过。
- Unresolved: 间隔 API 查询确认新 run 34239337659 / 0b9e843 持续 in_progress：Windows job 102105277820、macOS job 102105277823 已进入格式适配器与宿主测试，Linux job 102105277998 完成 Rust 安装并推进 GUI 依赖安装；尚无失败，新缩放步骤仍 pending。旧 run 34237571022 已 completed/cancelled（Linux 先前 failure、macOS 先前 success），不能作为三平台验收。本轮为已验证 CI 等待。后续一轮两次间隔查询确认三平台格式适配器与宿主测试均通过：Windows 进入 facade renderer contracts，Linux 进入 standalone runtime/facade/reference examples，macOS 进入 accessibility；尚无失败，Linux 缩放专项仍 pending。需新提交三平台 hosted 完整 jobs、Linux 实际缩放日志和同提交三份产物下载校验；M5 保持未完成。

### 2026-09-08 — 保持 3x 缩放验收窗口位于输出内

- Command/platform: run 34239337659 / 0b9e843，Linux job 102105277998 failure；下载 /tmp/sunmao-run34239337659-linux.log，核对 check-runs/annotations；阅读 Sway 1.9 desktop/output.c 的比例通知与相交表面遍历。
- Change: 在剩余输出改成 3x 后，以 Sway 命令聚焦验收窗口并移到逻辑坐标 (10,10)，保证缩小后的 426x320 输出仍包含窗口，再等待 3x surface preference。保留全部原密度、移除、resize、显式覆盖、Weston 与光标断言；平台实现未改动。
- Result: 本次 hosted 已通过输出移除后的 2x EGL 尺寸与边缘像素检查；随后日志只收到 output.scale(3)、surface.leave(output10)，没有 preferred_scale(360)，测试等待 3x 超时。Sway 仅对活跃工作区与输出相交的 surface 发更新，旧浮动位置在逻辑输出缩小后落到范围外。修正 Linux tests 类型检查 exit 0（/tmp/sunmao-scale-position-check.log），metadata/fmt/diff 通过。
- Unresolved: 完整本地回归 session 52399 已启动（/tmp/sunmao-scale-position-tests.log）；本轮两次间隔轮询确认原进程持续推进，常规测试已全部通过，当前执行 core 15 项文档测试，暂无失败。旧 run 的 macOS job 102105277823 已 completed/success，Windows job 102105277820 进入原生 GUI 打包验收。上一轮为修正进展，本轮为已验证等待。完整回归尚未完成；修正未提交，仍需新提交三平台 hosted 完整结果、Weston fallback 与 2x 光标实际日志以及产物。旧 Windows/macOS 仍运行；M5 保持未完成。

### 2026-09-08 — 3x 验收窗口位置修正完整 gate 通过

- Command/platform: 原完整回归 session 52399 exit 0，macOS ARM64；日志 /tmp/sunmao-scale-position-tests.log。
- Change: 在 Sway 3x 输出变更后聚焦并定位浮动测试窗口，确保实际与剩余输出相交后再验证 surface preference。
- Result: 完整常规测试与 doc-tests 全部通过；Linux tests 类型检查、metadata、fmt、diff 已通过。仅修改 Linux 验收测试和进展文档，无平台实现或示例/打包改动。
- Unresolved: 修正待新提交三平台 hosted jobs、真实缩放/Weston/2x 光标日志与同提交产物下载校验；M5 保持未完成。

### 2026-09-08 — 3x 验收位置修正已推送

- Command/platform: HTTPS push b0f435a7934ca32b0c43558fdc94e5c6602fea6f 至 phase4/gui-component-library；Actions API 查询。
- Change: 提交浮动验收窗口定位操作，保留原比例、EGL 尺寸、边缘像素与后续 Weston/光标全部断言。
- Result: push exit 0；run 34241125174 精确匹配 b0f435a，状态 queued：https://github.com/aizcutei/sunmao/actions/runs/34241125174 。完整本地 gate 已通过。
- Unresolved: 两次间隔 API 查询确认 run 34241125174 / b0f435a 三平台持续 in_progress：macOS job 102111266719、Linux job 102111267122、Windows job 102111267633 均已进入格式适配器与宿主测试，暂无失败，Linux 缩放专项仍 pending。本轮为已验证 CI 等待。后续一轮间隔查询确认三平台格式适配器与宿主测试均通过，macOS/Linux 已进入 standalone runtime/facade/reference examples，Windows 进入 facade renderer contracts；暂无失败，Linux 缩放专项仍 pending。需新提交三平台 hosted 完整 jobs、Linux 实际缩放/光标日志与同提交产物下载校验；旧 run 34239337659 已 cancelled，不能替代新提交验收。M5 保持未完成。

### 2026-09-08 — 修正光标主题环境变量覆盖输出密度

- Command/platform: run 34241125174 / b0f435a，Linux job 102111267122 failure；完整日志 /tmp/sunmao-run34241125174-linux.log；核对 wayland-cursor 的 load/load_or/load_from_name 实现。
- Change: 将 XCURSOR_SIZE 解释为逻辑尺寸（默认 24），乘以输出整数比例后通过 load_from_name 加载；保留 XCURSOR_THEME，不修改宿主环境。补充默认值、用户尺寸、非法值与溢出测试，保留 hosted 48px buffer / 24-unit viewport 与真实光标像素断言。
- Result: 旧 hosted 的动态/分数/跨屏/移除/3x/resize/显式覆盖 EGL 尺寸和像素、Weston core 2x 全部通过。光标形状/隐藏/重入像素通过，但协议日志显示 viewport(12,12)：load_or 将请求的 48px 覆盖为环境变量 24px，导致尺寸错误。Windows all-features target check、metadata/fmt/diff 通过；Linux tests 类型检查 exit 0（使用 PKG_CONFIG_ALLOW_CROSS=1 与 /opt/X11/lib/pkgconfig，仅类型检查，不作 Linux 原生运行证据）；完整本地回归 session 47691 exit 0，含全部文档测试，日志 /tmp/sunmao-cursor-density-tests.log。
- Unresolved: 修正待完整本地 gate、新提交三平台 hosted 和产物校验；M5 保持未完成。

### 2026-09-08 — 光标密度修复已推送并触发 hosted CI

- Command/platform: HTTPS push aa8694e20d93fb0e20f8b68f6f1c695ea952c88e 至 phase4/gui-component-library；Actions API 查询。
- Change: 提交逻辑光标尺寸读取、输出密度换算、环境变量覆盖修复与回归测试，保留原有严格 hosted 断言。
- Result: 完整本地 locked 回归、metadata/fmt/diff、Windows target 和 Linux tests 类型检查通过；push exit 0。新 run 34242729268 精确匹配 aa8694e，当前 pending：https://github.com/aizcutei/sunmao/actions/runs/34242729268 。
- Unresolved: 本轮间隔查询确认新 run 34242729268 / aa8694e 从 pending 转为 in_progress：Linux job 102116978531、Windows job 102116978681、macOS job 102116978792 均已进入 Test format adapters and host，尚无失败，缩放专项仍 pending。上一轮为实际修复/提交进展，本轮为已验证 CI 等待。旧 run 34241125174 已 completed/cancelled（macOS success、Linux 已定位 failure、Windows cancelled），不能替代新提交验收。后续一轮间隔轮询确认三个原 job 持续 in_progress：Linux 已通过格式适配器与宿主测试并进入 facade renderer contracts，macOS 推进到 Phase 4 acceptance fixtures，Windows 仍在格式适配器与宿主测试；暂无失败，缩放专项仍 pending。本轮为已验证等待。再一轮两次查询确认三个原 job 持续运行：macOS 进入 Build standalone reference applications，Linux 推进到 Phase 4 acceptance fixtures，Windows 已通过格式适配器与宿主测试并进入 facade renderer contracts。一次沙箱 DNS 查询失败经沙箱外重试恢复；没有重启任何 job，暂无 CI 失败，Linux 缩放仍 pending。本轮和上一轮均为已验证 CI 等待。等待新提交三平台 hosted 完整 jobs、48px/24-unit 光标日志与同提交三份产物下载校验；M5 保持未完成。

### 2026-09-08 — 光标密度修复的 hosted 缩放步骤通过

- Command/platform: 两次间隔查询 run 34242729268 / aa8694e 的同一组三平台 jobs。
- Change: 本轮只核对 CI 实际进展并记录等待，没有重跑或修改代码。
- Result: Linux job 102116978531 的 Wayland probe、pointer/keyboard/cursor、activation/focus、output scaling 四项均 completed/success，已进入 Check baseview feature combinations。macOS job 102116978792 正在原生 GUI 打包验收，Windows job 102116978681 正在 standalone/runtime/facade/reference examples；暂无失败。上一轮与本轮均为已验证 CI 等待。
- Unresolved: 后续一轮两次间隔查询确认同一组三平台 jobs 持续 in_progress；Linux 已通过 baseview feature combinations 并进入 Build standalone reference applications，Windows 进入 accessibility，macOS 仍在原生 GUI 打包验收，暂无失败。本轮为已验证等待。再一轮间隔查询确认 macOS job 102116978792 已 completed/success；Linux job 102116978531 正在原生 GUI 打包验收，Windows job 102116978681 已通过实时分配检查并进入 Build cross-platform examples and tools；暂无失败。本轮仍为已验证 CI 等待。本轮继续间隔核对同一组三平台 jobs：Linux 已通过原生 GUI 打包验收并进入 Exercise repository packaging helper，Windows 进入原生 GUI 打包验收，macOS 保持 success；暂无失败。本轮为已验证等待。缩放专项步骤 success 尚不等于同提交三平台完整验收；Linux job 未结束，待完整日志核实 48px buffer、24-unit viewport 与所有实际缩放/像素断言，并下载校验三份产物。M5 保持未完成。

### 2026-09-08 — Linux 完整成功并核实缩放日志

- Command/platform: run 34242729268 / aa8694e；Linux job 102116978531 completed/success；下载 /tmp/sunmao-run34242729268-linux.log。
- Change: 核实实际输出缩放、EGL 尺寸与光标协议/像素证据；没有改动代码或重跑 CI。
- Result: 日志 14644–15368 确认 1→2→1.5→1、跨屏/移除 2x、后续 3x、180x130 resize 的 EGL 540x390、显式 1.25 下 EGL 200x150（输出变更前后）和边缘像素全部通过，含 WAYLAND SCALE VERIFIED。15520–15561 确认 Weston core 2x 与 resize、WAYLAND CORE SCALE VERIFIED。15814/15854 的主题缓冲区实际为 48x48/stride192，15818/15856/16135 的 viewport 实际为 24x24；16172/16177 含真实光标像素及 WAYLAND CURSOR SCALE VERIFIED。新 cursor_size_setting_is_logical_and_validated 在 16330 明确 ok。macOS job 102116978792 亦 completed/success。
- Unresolved: Windows job 102116978681 仍在原生 GUI 打包验收，暂无失败。本轮获得了新的完整 Linux 实测证据，仍需 Windows 完整成功及同提交三份产物下载校验；缩放尚不标记正式验收，M5 保持未完成。

### 2026-09-08 — 缩放修复三平台全绿，下载同提交产物

- Command/platform: run 34242729268 / aa8694e20d93fb0e20f8b68f6f1c695ea952c88e completed/success；三个原 jobs 均 completed/success。产物下载校验 session 59899。
- Change: 核对 Windows 原生 GUI 与打包收尾成功，开始下载并校验三份同提交 artifacts；Linux 缩放完整日志已在上一轮核实。
- Result: Windows artifact 78,350,159 bytes / 364 entries，SHA-256 与 ZIP CRC 通过。Linux artifact 正在下载，已观察到约 242 MiB 文件，原 session 59899 经多次轮询仍活跃；后续一轮两次轮询仍在运行，Linux 文件从约 277 MiB 增至 423 MiB，未重启下载。再一轮两次轮询确认原 session 59899 仍活跃，Linux 文件从约 444 MiB 增至 562 MiB。本轮两次间隔轮询仍确认原 session 59899 活跃，Linux 文件从约 626 MiB 增至 729 MiB。本轮为已验证下载等待；macOS artifact 待顺序下载。
- Unresolved: 保持原下载 session，不重启下载；剩余 Linux/macOS 产物校验完成后才能将 output scaling 标记正式验收。M5 仍有 facade feature 传递与最终兼容/文档审计，不能标记 Phase 4 完成。

### 2026-09-08 — 输出缩放正式验收完成

- Command/platform: run 34242729268 / aa8694e20d93fb0e20f8b68f6f1c695ea952c88e 三平台 completed/success；原下载校验 session 59899 exit 0。
- Change: 更新 Phase 4 状态与跨格式语义，标记 output scaling 已正式验收，下一瓶颈为 facade Wayland feature 传递。
- Result: 三份同提交 artifacts 全部下载且 SHA-256/ZIP CRC 通过：Windows 78,350,159 bytes / 364 entries；Linux 972,286,517 bytes / 96 entries；macOS 53,085,612 bytes / 152 entries。文件 /tmp/sunmao-run34242729268-phase1-*.zip。Linux 完整日志 /tmp/sunmao-run34242729268-linux.log 已核实动态整数/分数/跨屏/移除/3x/resize/显式覆盖的 EGL 尺寸与边缘像素、Weston core 2x，以及 48x48 光标缓冲区、24x24 viewport 和真实 compositor 光标像素。严格验收脚本全部成功；此前 XCURSOR_SIZE 覆盖物理尺寸的真实缺陷已被修复并由 hosted 验证。
- Unresolved: M5 仍有 facade feature 传递及最终兼容/文档审计（包括过时的 M3 输入与 accessibility 描述）；Phase 4 未完成。本轮完成缩放瓶颈，验收文档留待下一实施提交一并带上。

### 2026-09-08 — 接通 facade Wayland feature 与公开 API 验收

- Command/platform: facade/适配器 manifests、公开 prelude、现有 Wayland acceptance 与 CI 自底向上核对；新增 sunmao/tests/wayland_facade.rs。
- Change: 新增默认关闭的 sunmao/gui-wayland，包含 gui-gl 并传递 sunmao_view_baseview/wayland → baseview/wayland。README 与 doc-test 说明 GL 浮动编辑器范围，保留 X11 嵌入；测试只依赖 facade，禁用 DISPLAY，通过公开 API 绘制红/绿像素、resize、关闭及重开，使用既有 C 诊断 ABI 读回实际 renderer 像素。CI 独立编译 gui-wayland、执行 doc-test，并在 Weston 下要求 WAYLAND FACADE VERIFIED。
- Result: macOS 独立 gui-wayland 编译 exit 0（/tmp/sunmao-facade-wayland-check.log），metadata/fmt/diff 通过。Linux tests 类型检查 session 64341 已 exit 0（4m31s，包含新的 facade integration test 类型检查）；Windows target check session 94098、doc-test session 68432、完整 locked 回归 session 16900 已启动。后续一轮两次轮询确认原 handles 持续运行：doc-test 已拿到锁并编译 WGPU/proptest，Windows 和完整回归仍等待构建锁，没有失败。再一轮两次间隔轮询确认 doc-test session 68432 exit 0（3 passed / 1 ignored，新 Native Wayland 示例明确 ok）；完整回归 session 16900 已获得锁并编译效果器/乐器示例，Windows session 94098 仍等待锁，暂无失败。本轮为已验证等待并取得 doc-test 成功证据。后续一轮两次轮询确认完整回归 session 16900 已完成编译并开始执行常规单元测试（当前 AU fixture 部分），Windows session 94098 已拿到锁并编译到 backend_vst3；暂无失败。本轮为已验证等待。本轮确认 Windows target check session 94098 exit 0；完整回归原 session 16900 经两次间隔轮询仍活跃，已推进至 CLAP backend 测试，已执行项均通过。本轮为已验证等待并取得 Windows 编译成功证据，现仅剩完整回归门槛。后续一轮两次间隔轮询确认原 session 16900 仍活跃：VST3 backend 与 core 已推进通过，DSP 48 单元测试全部通过，当前进入 DSP property tests，无失败。本轮为已验证完整回归等待。日志分别 /tmp/sunmao-facade-wayland-{linux,windows,doc,tests}.log。
- Unresolved: 上一轮为实际实现进展，本轮为已验证等待并取得 Linux tests 类型检查成功证据。完整本地 gate 尚未完成，变更未提交；不重启等待中的检查。通过后推送并验证同提交三平台 hosted、真实 facade 编辑器日志与三份产物。Phase 4 仍未完成。

### 2026-09-08 — 手动 push 后核对 CI 与剩余本地 gate

- Command/platform: 查询最新 GitHub Actions runs；继续轮询原完整回归 session 16900，日志 /tmp/sunmao-facade-wayland-tests.log；git status/log/diff --check。
- Change: 确认远端最新运行仍为 aa8694e 的 run 34242729268，三平台 completed/success；gui-wayland facade 八个文件仍为未提交改动，尚未进入该运行。本轮未重启测试或 CI。
- Result: 原完整回归连续轮询仍活跃，已从效果器示例推进通过 GUI 后端至合成器 GUI 示例，已执行测试均通过；diff --check exit 0。既有 scaling 同提交三份产物校验成功记录有效。
- Unresolved: 仅剩原完整本地回归退出结果；随后提交并推送 facade feature 与严格 hosted 验收。当前 GitHub success 不覆盖未提交改动，不能据此标记 facade 或 Phase 4 完成。本轮为已验证等待。 后续一轮两次间隔轮询确认原 session 16900 仍活跃，常规测试已通过并进入 doc-tests；facade 文档测试 3 passed / 1 ignored，当前推进至 backend_vst3 文档测试，无失败。HEAD 仍为 aa8694e；本轮继续为已验证等待。

### 2026-09-08 — facade Wayland 完整本地 gate 通过

- Command/platform: macOS ARM64 原完整 locked 回归 session 16900 exit 0；日志 /tmp/sunmao-facade-wayland-tests.log。
- Change: 提交默认关闭的 gui-wayland feature、公开 API doc-test、仅依赖 facade 的真实浮动编辑器验收与 CI 接线，并带上 output scaling 正式验收记录。
- Result: 完整常规测试与全部 doc-tests 通过；此前 metadata/fmt/diff、macOS 独立 feature 编译、Windows target check、Linux tests 类型检查与独立 gui-wayland doc-test 均通过。没有修改示例或打包实现。
- Unresolved: 待新提交三平台 hosted 全部成功、Linux WAYLAND FACADE VERIFIED 实际运行日志及同提交三份 artifacts 下载校验。Phase 4 与 M5 尚未完成。

### 2026-09-08 — facade Wayland 已推送并触发 GitHub CI

- Command/platform: HTTPS push 910f5225404c470f1dccfc2d957ee1bc50fbcf67 至 phase4/gui-component-library；GitHub Actions API 查询。
- Change: 提交 facade feature 传递、公开文档与真实像素/resize/关闭重开验收，既有三平台 blocking 验收全部保留。
- Result: push exit 0；新 run 34248163394 的 head_sha 精确匹配 910f522，当前 queued：https://github.com/aizcutei/sunmao/actions/runs/34248163394 。完整本地 gate 已通过。
- Unresolved: 两次间隔 API 查询确认 run 34248163394 / 910f522 三平台持续 in_progress：Windows job 102135429500、Linux job 102135429550、macOS job 102135429745 均已进入 Test format adapters and host，暂无失败，Linux facade 专项仍 pending。上一轮为实际提交/推送进展，本轮为已验证 CI 等待。等待同提交三平台完整 jobs、Linux facade 实际执行日志与三份产物下载校验。M5 仍未完成；CI 运行期间只监控记录，实际失败按日志修复。

### 2026-09-09 — facade Wayland hosted 持续运行

- Command/platform: 两次间隔查询 run 34248163394 / 910f522 的三个原 jobs；git status/log。
- Change: 本轮只核对运行状态并记录，没有改动代码或重跑 jobs。
- Result: macOS job 102135429745 已通过格式适配器与宿主测试，进入独立 facade renderer contracts；Windows job 102135429500 与 Linux job 102135429550 仍在格式适配器与宿主测试，三平台均 in_progress，暂无失败。HEAD 保持 910f522。
- Unresolved: 上一轮与本轮均为已验证 CI 等待。Linux facade 专项仍 pending；需同提交完整三平台成功、实际运行日志及三份产物校验，M5 保持未完成。 后续一轮两次间隔查询确认三个原 jobs 持续 in_progress：Windows/Linux 已通过格式适配器与宿主测试，进入独立 facade renderer contracts；macOS 已通过该检查，进入 standalone runtime/facade/reference examples。暂无失败，Linux Wayland 专项仍 pending。本轮为已验证等待。 再一轮两次间隔查询确认独立 facade renderer contracts 已三平台通过：Windows 进入 standalone/runtime/facade/reference examples，Linux 进入 Phase 4 acceptance fixtures；macOS 已推进至 system-capture linkage（非 blocking follow-up），Wayland 项按平台条件 skipped。三个原 jobs 仍 in_progress，暂无失败，Linux 真实 facade 编辑器专项尚未执行。本轮继续为已验证等待。

### 2026-09-09 — facade Wayland 所在 hosted 步骤通过

- Command/platform: 两次间隔查询 run 34248163394 / 910f522 的原三平台 jobs。
- Change: 核对新增 facade 验收所在的 Probe a headless Wayland compositor 步骤状态；本轮没有修改代码或重跑 jobs。
- Result: Linux job 102135429550 的 headless compositor probe 与 pointer/keyboard/cursor 均 completed/success，当前执行 activation/focus，output scaling 尚 pending；Windows job 102135429500 进入 Phase 4 acceptance fixtures；macOS job 102135429745 进入实时回调分配矩阵。三平台仍 in_progress，暂无失败。
- Unresolved: 上一轮与本轮均为已验证 CI 等待。步骤 success 尚不等于最终验收；Linux 完整 job 结束后下载日志核实 WAYLAND FACADE VERIFIED 与实际测试结果，再核对同提交三个完整 jobs 和三份产物。M5 保持未完成。 后续一轮两次间隔查询确认 Linux 的 activation/focus 和 output scaling 也 completed/success，四项 Wayland 步骤全部通过；Windows/Linux 进入 baseview feature combinations，macOS 进入 VST3/CLAP/standalone 原生打包验收。三个原 jobs 仍在运行，无失败。本轮为已验证等待。 再一轮两次间隔查询确认 Windows/Linux 已通过实时回调分配矩阵；Windows 进入 standalone reference applications 构建，Linux/macOS 正在 native GUI backends 打包验收。三平台持续 in_progress，暂无失败；本轮为已验证等待，尚待完整 Linux 日志与产物。 后续一轮两次间隔查询确认 Windows 也已进入 native GUI backends 打包验收，三平台同一批原 jobs 均在该步骤持续 in_progress，暂无失败；未重跑或提前获取未完成日志。本轮为已验证等待。

### 2026-09-09 — Linux 原生打包验收通过，开始上传产物

- Command/platform: 两次间隔查询 run 34248163394 / 910f522 原三平台 jobs。
- Change: 本轮仅监控与记录，没有重跑或修改实现。
- Result: Linux job 102135429550 已通过 native GUI backends 与 repository packaging helper，进入 Upload packaged Phase 1 artifacts；macOS job 102135429745 已通过 native GUI backends，进入 repository packaging helper；Windows job 102135429500 仍在 native GUI backends。三个 jobs 仍 in_progress，暂无失败。
- Unresolved: 本轮为已验证 CI 等待。Linux job 完成后获取完整日志，核实 facade 成功标记与测试结果；还需三平台完整成功及同提交三份产物下载校验，M5 保持未完成。

### 2026-09-09 — facade Wayland 三平台全绿并核实实际运行

- Command/platform: run 34248163394 / 910f5225404c470f1dccfc2d957ee1bc50fbcf67 completed/success，三个原 jobs 均 completed/success；下载完整 Linux 日志 /tmp/sunmao-run34248163394-linux.log（session 6667 exit 0）。
- Change: 核实新 facade 验收真实执行，启动三份同提交产物下载及 SHA-256/ZIP CRC 校验（原 session 68362）。
- Result: Linux 日志 5133–5141 明确编译 sunmao、运行 tests/wayland_facade.rs，输出 WAYLAND FACADE VERIFIED: shader rendering, resize, close and reopen without X11；facade_wayland_editor_renders_resizes_and_reopens ... ok，1 passed / 0 failed / 0 ignored。命令 4933–4935 明确禁用 DISPLAY，只启用 sunmao/gui-wayland。既有 output/core/cursor scale 成功标记及实际测试结果亦已核实。下载 session 68362 经轮询仍活跃。 后续一轮两次间隔轮询确认原 session 68362 持续运行：Windows 产物 78,351,539 bytes / 366 entries，SHA-256 与 ZIP CRC 已通过；Linux 产物已开始下载（约 12 MiB），macOS 等待顺序下载。本轮为已验证下载等待，并取得 Windows 产物成功校验证据。 后续一轮两次间隔轮询确认原 session 68362 仍活跃，Linux 文件由约 33 MiB 增至 160 MiB，没有下载失败或重启；macOS 仍等待顺序下载。本轮为已验证下载等待。 再一轮两次间隔轮询确认原 session 68362 活跃，Linux 文件由约 212 MiB 增至 345 MiB，未出现失败；本轮继续为已验证下载等待。 后续一轮两次间隔轮询确认原 session 68362 仍活跃，Linux 文件由约 374 MiB 增至 517 MiB；下载持续推进，macOS 尚待顺序下载。本轮为已验证等待。 再一轮两次间隔轮询确认原 session 68362 活跃，Linux 文件由约 554 MiB 增至 623 MiB，尚未开始 CRC 校验或 macOS 下载；本轮为已验证下载等待。 后续一轮两次间隔轮询确认原 session 68362 仍活跃，Linux 文件由约 661 MiB 增至 707 MiB，下载无失败；本轮继续为已验证等待。 再一轮两次间隔轮询确认原 session 68362 活跃，Linux 文件由约 724 MiB 增至 808 MiB；macOS 仍未开始下载，本轮为已验证等待。
- Unresolved: 三平台完整 CI 与 Linux 实测证据已齐；等待原 session 68362 完成三份 artifacts 下载校验后才正式标记 facade 验收。随后仍需最终兼容/文档审计与原目标中真实输入路径证据，M5 和 Phase 4 未完成。

### 2026-09-09 — facade Wayland 正式验收完成

- Command/platform: run 34248163394 / 910f5225404c470f1dccfc2d957ee1bc50fbcf67 三平台 completed/success；原下载 session 68362 exit 0。
- Change: 更新 Phase 4 状态与跨格式语义，标记 facade gui-wayland 公开入口正式验收。
- Result: 同提交三份 artifacts 全部下载且 SHA-256/ZIP CRC 通过：Windows 78,351,539 bytes / 366 entries；Linux 972,286,464 bytes / 96 entries；macOS 53,085,660 bytes / 152 entries。文件 /tmp/sunmao-run34248163394-phase1-*.zip。完整 Linux 日志 5133–5141 已确认仅依赖 facade 的真实 shader 像素、resize、关闭重开测试实际执行，WAYLAND FACADE VERIFIED，1 passed / 0 failed / 0 ignored。
- Unresolved: 本轮完成 facade 瓶颈。下一瓶颈为最终兼容/文档及原目标中 macOS/Windows/X11 真实输入路径审计；过时的 M3 IME 与 accessibility 描述尚待据实纠正，不能用逻辑字符映射单测替代真实输入证据。M5 与 Phase 4 未完成，验收记录留待下一实施提交携带。

### 2026-09-09 — 修复 X11 硬编码布局并接入真实输入验收

- Command/platform: 核对 GitHub run 34248163394 / 910f522 三平台 success；自底向上审计 baseview 的 X11/macOS/Windows 输入实现。
- Change: X11 删除 US 硬编码逻辑字符表，读取服务器 XKB keymap；共享既有 Wayland layout/compose 翻译器及 proptest，保留物理 Code。监听服务器 keymap 更新，以事件自身的有效修饰键/group 快照翻译，申请 per-client detectable autorepeat，失焦释放 held keys 并取消 compose。新增 XTest 真实窗口测试覆盖德语 Y→z、Shift、AltGr、运行中切换 us(intl)、dead acute+e→é、失焦取消组合；CI 在独立 Xvfb 下要求明确成功标记。README 记录 Linux runtime 库要求；M3 国际输入验收重新打开，纠正合成字符单测等于真实 IME 的过度声明。
- Result: metadata/fmt/diff 已通过；Linux default tests 类型检查 session 17576 与 wayland tests 类型检查 session 80676 exit 0，Windows baseview target check session 21154 exit 0。首次 Linux 交叉检查因 pkg-config 缺 cross 配置失败，使用 PKG_CONFIG_ALLOW_CROSS=1 后成功；这仅为类型检查，不能替代 Linux 运行。随后补 detectable autorepeat/保留命名多媒体键映射，最新 Linux 复查 session 69993 等待构建锁。完整 macOS locked 回归 session 46433 正在编译，日志 /tmp/sunmao-x11-tests.log；没有重启测试。
- Unresolved: 本轮实现尚未提交。保持原 sessions 46433/69993，等待完整本地 gate 后提交/push 并验证同提交三平台 hosted、X11 实际成功日志和 artifacts。真实输入新测试尚未运行，不标记 X11 验收；macOS/Windows 真实国际输入及最终文档仍待完成，M5/Phase 4 未完成。该实现为国际键盘/compose，未实现 XIM/CJK 预编辑协议。

### 2026-09-09 — X11 按键生命周期属性检查与 Linux 编译通过

- Command/platform: 继续轮询原完整回归 session 46433 与 Linux tests 类型检查 session 69993；未重启构建。
- Change: 新增 held_keys_release_their_original_logical_key_and_cancel_is_idempotent，以任意按键/修饰键序列检查 held 集合、重复按键标记、释放保留原始逻辑字符、失焦清理幂等且无残留。
- Result: 最新 Linux（含 wayland feature 与 tests）session 69993 exit 0，日志 /tmp/sunmao-x11-linux-wayland.log；类型检查完成时间晚于新增属性测试文件变更。fmt/diff 再次通过。完整 macOS 回归 session 46433 已完成编译并开始常规测试，当前推进至 AU fixture，已执行项通过。上一轮为实际实现进展，本轮为新增不变量检查及 Linux 编译证据进展。
- Unresolved: 保持原完整回归 session 46433，待退出成功后提交/push 并等待三平台 hosted 实测。X11 新测试与属性测试尚未在 Linux 上运行，不能仅凭交叉类型检查标记验收；M3 输入与 M5/Phase 4 均未完成。

### 2026-09-09 — X11 修复完整回归持续运行

- Command/platform: 两次间隔轮询原完整 locked 回归 session 46433，日志 /tmp/sunmao-x11-tests.log；核对 git status/log。
- Change: 本轮只监控并记录，没有重启回归或扩大实现范围。
- Result: 原 session 46433 持续活跃，已从 AU fixture 推进到 baseview 常规测试，已执行项均通过。HEAD 仍为 910f522，X11 修复保持未提交；此前 Linux tests 类型检查与 Windows target check 成功记录有效。上一轮为实现/编译证据进展，本轮为已验证等待。
- Unresolved: 等待原完整回归退出成功，然后提交/push 并核对新提交三平台 hosted、真实 X11 输入日志与 artifacts。M3 输入与 M5/Phase 4 均未完成。

后续一轮两次间隔轮询确认原 session 46433 持续运行：baseview 三个 macOS 测试全部通过（含 resize 重入与关闭测试），当前已推进到平台 integration tests；X11 测试在 macOS 按 cfg 为 0 tests，不构成 Linux 验收。暂无失败，未重启；上一轮与本轮均为已验证等待。

再一轮两次间隔轮询确认原 session 46433 活跃：clap_rs 56 个单元/属性测试全部通过，已推进至 clap_sys fixtures，暂无失败。HEAD 与实现保持不变；本轮为已验证完整回归等待，未重启或提前提交。

后续一轮两次间隔轮询确认原 session 46433 仍活跃：clap_sys 合成器零分配测试通过，sunmao facade 常规测试已推进完成，当前进入 backend_au；暂无失败。本轮继续为已验证等待，尚未提交 X11 实现，Linux hosted 真实输入验收仍待本地完整 gate 后执行。

再一轮两次间隔轮询确认原 session 46433 持续运行：已通过 CLAP backend 常规测试并推进其导出元数据 integration tests，已执行项通过，暂无失败。本轮为已验证等待，保留原完整回归；提交与 hosted 验收仍待完整退出结果。

后续一轮两次间隔轮询确认原 session 46433 活跃，已进入 VST3 backend；零分配、旧 state 迁移、transport 等已执行项通过。unified_view_context_notifies_vst3_handler_and_plug_frame 与 view_creation_panic_is_contained_before_returning_through_vst3_abi 报告运行超过 60 秒，但进程未退出且没有失败结果；继续保留原 handle，不能据观察超时重启。本轮为已验证等待。

再一轮两次间隔轮询确认原 session 46433 活跃：此前超过 60 秒的两个 VST3 GUI 测试均已 ok，VST3 backend 28 tests 全部通过（84.94s）；core 与 DSP 已推进通过，DSP 14 条属性测试通过，当前进入效果器 fixtures。没有重启或失败；本轮取得 VST3 耗时测试完成证据，继续等待完整回归退出。

后续一轮两次间隔轮询确认原 session 46433 持续运行，效果器增益/时序测试与 layout_gain 5 tests 均通过，当前进入 LPF GUI fixture，暂无失败。本轮为已验证等待；不重启原完整回归，X11 修复仍待完整本地 gate 后提交及 hosted 验收。

再一轮两次间隔轮询确认原 session 46433 活跃，效果器测试持续推进：SVF 六项测试全部通过，当前进入 tempo_delay，暂无失败。本轮为已验证等待，仍保留原完整回归，未提前提交 X11 变更。

后续一轮两次间隔轮询确认原 session 46433 仍活跃：tempo_delay 九项测试全部通过，已从 widgets fixture 推进到 GUI 后端，当前进入 sunmao_gui_webview；暂无失败。本轮为已验证等待；完整本地 gate 仍未结束，继续保留原进程。

再一轮两次间隔轮询确认原 session 46433 活跃，已从 GUI 后端/宏测试推进至合成器 fixtures；合成器八项参数、包络、滤波与平滑测试通过，当前进入 sunmao_syn_poly_expr，暂无失败。本轮为已验证等待，继续保留原完整回归，未提交 X11 实现。

后续一轮两次间隔轮询确认原 session 46433 活跃：poly_expr 六项表达控制测试通过，sine GUI 示例重置测试亦通过，当前进入模板 fixtures，暂无失败。本轮为已验证等待；完整回归尚未退出，不重跑或提前提交。

再一轮两次间隔轮询确认原 session 46433 活跃：模板 fixtures 已推进，runner 的 44 项测试全部通过，当前进入 sunmao_view_baseview；暂无失败。本轮为已验证等待，保留原完整回归，X11 修复仍待其退出成功后提交及三平台 hosted。

后续一轮两次间隔轮询确认原 session 46433 活跃，已从 view_baseview/CLI 推进到 vst3_sys fixtures；VST3 speaker mask 与 raw effect callback 零分配测试通过，当前进入 VST3 GUI fixture，暂无失败。本轮为已验证等待，原完整回归尚未退出，X11 实现尚未提交。

再一轮两次间隔轮询确认原 session 46433 活跃：全部常规测试已完成并进入 doc-tests，VST3 raw synth 六项测试通过，当前执行 sunmao 文档测试；暂无失败。本轮取得常规测试完成证据，仍等待原完整回归含全部 doc-tests 退出成功，未提前提交。

后续一轮两次间隔轮询确认原 session 46433 活跃：sunmao 文档测试 3 passed / 1 ignored，已推进至 core 的 15 项文档测试，已执行项通过，暂无失败。本轮为已验证等待，仍需全部 doc-tests 完成与原进程退出结果。

再一轮两次间隔轮询确认原 session 46433 活跃：core 的 15 项文档测试全部通过，已从 DSP 文档推进至效果器文档测试，暂无失败。本轮为已验证等待；完整回归尚未退出，保持原 handle。

后续一轮两次间隔轮询确认原 session 46433 仍活跃，效果器文档已推进完毕，当前执行 sunmao_gui 的四项文档测试，暂无失败。本轮为已验证等待，未重启原完整回归，仍待退出成功后提交 X11 修复。

再一轮两次间隔轮询确认原 session 46433 活跃：sunmao_gui 四项文档测试全部通过，已推进至 runtime/state migration 文档，暂无失败。本轮为已验证等待，保留原完整回归，尚未提交 X11 修复。

后续一轮两次间隔轮询确认原 session 46433 活跃，文档测试已从合成器推进至模板，暂无失败。本轮为已验证等待，原完整回归未退出；继续等待完整结果后提交/push。

### 2026-09-09 — X11 输入修复完整本地 gate 通过

- Command/platform: 原完整 locked 回归 session 46433 exit 0，日志 /tmp/sunmao-x11-tests.log；metadata/fmt/diff 最终检查通过。
- Change: 提交服务器 XKB 布局、共享 compose 翻译、布局更新/失焦清理、按键生命周期属性测试及 Xvfb/XTest hosted 验收，携带此前 facade Wayland 正式验收记录和 M3 输入证据纠正。
- Result: macOS 完整常规测试与全部 doc-tests 通过；Linux tests 类型检查（含 wayland）和 Windows target check 已通过。没有重启耗时回归，没有修改示例或打包实现。
- Unresolved: 待新提交三平台完整 hosted 全绿、X11 KEYBOARD VERIFIED 实际运行日志和同提交三份 artifacts 下载校验。Linux 原生输入与新增属性测试尚待实际运行；macOS/Windows 原生国际输入及最终审计仍未完成，M5/Phase 4 保持未完成。

### 2026-09-09 — X11 修复已推送并触发三平台 CI

- Command/platform: HTTPS push 90bf6de4f85519801d7d5f499e4c4596559a346f 至 phase4/gui-component-library；原 push session 30847 exit 0；Actions API 查询。
- Change: 推送 X11 原生布局/compose 修复与真实按键 blocking 验收，保留既有三平台 gate。
- Result: 新 run 34255696871 的 head_sha 精确匹配 90bf6de，当前 queued：https://github.com/aizcutei/sunmao/actions/runs/34255696871 。完整本地 gate 已通过。
- Unresolved: 等待同提交三平台完整 jobs 成功、X11 实际运行标记及三份 artifacts 下载校验。CI 运行期间只监控/记录，实际失败依日志处理；M3 输入与 M5/Phase 4 保持未完成。

### 2026-09-09 — X11 修复三平台 hosted 持续运行

- Command/platform: 两次间隔查询 run 34255696871 / 90bf6de 原 jobs；git status/log。
- Change: 本轮仅监控/记录；状态查询 helper 加入 X11 步骤展示，未修改实现或重跑 jobs。
- Result: Linux job 102160815689 安装 GUI 构建依赖，Windows job 102160815799 与 macOS job 102160815981 已进入 Test format adapters and host，三个 jobs 均 in_progress，暂无失败。新 X11 输入步骤 pending。上一轮为提交/push 进展，本轮为已验证 CI 等待。
- Unresolved: 等待同提交三个完整 jobs、X11 实际成功标记与三份产物校验。M3 输入与 M5/Phase 4 未完成，不能仅据已触发 CI 宣称验收。

### 2026-09-09 — 继续核对已推送提交的 hosted CI

- Command/platform: 两次间隔查询 Actions run 34255696871，核对 HEAD 90bf6de 与 phase4/gui-component-library。
- Change: 仅监控原三平台 jobs 并记录，没有重跑或修改实现。
- Result: 三个平台均 in_progress，暂无失败；Linux 已推进到 Check facade renderer contracts independently，Windows 执行 Test format adapters and host，macOS 执行 Test standalone runtime, facade, and reference examples。X11 原生输入步骤仍 pending。本轮为已验证 CI 等待。
- Unresolved: 等待同提交三个完整 jobs 全绿、X11 实际运行成功日志与三份 artifacts 下载校验；M3 输入与 M5/Phase 4 尚未完成。

### 2026-09-09 — 三平台原 jobs 继续推进

- Command/platform: 两次间隔查询 run 34255696871 / 90bf6de 的原三平台 jobs，核对分支与 HEAD。
- Change: 本轮仅监控并记录，没有重启 jobs 或修改实现。
- Result: 三个平台均 in_progress，暂无失败；Linux 推进至 Test standalone runtime, facade, and reference examples，Windows 执行 Check facade renderer contracts independently，macOS 推进至 system-capture linkage 后续检查。Linux X11 原生输入步骤仍 pending。上一轮与本轮均为已验证等待。
- Unresolved: 同提交三平台完整成功、X11 原生输入日志和三份 artifacts 校验仍待完成；M3 输入与 M5/Phase 4 保持未完成。

### 2026-09-09 — 三平台 CI 推进到 accessibility 与实时分配检查

- Command/platform: 两次间隔查询 run 34255696871 / 90bf6de 原 jobs；核对 git status/log 与既定文档。
- Change: 仅监控并记录，没有重启 CI 或修改实现。
- Result: 三个平台均 in_progress，暂无失败；Linux 已进入 Test the accessibility feature，Windows 执行 standalone/runtime/facade 测试，macOS 已进入 Test realtime callback allocation matrix。macOS 的 X11 步骤按平台跳过，Linux X11 步骤仍 pending，不构成原生输入验收。上一轮与本轮均为已验证等待。
- Unresolved: 等待同提交三个完整 jobs 成功、Linux X11 真实输入日志及三份 artifacts 下载校验；M3 输入与 M5/Phase 4 未完成。

### 2026-09-09 — Linux 进入原生 Wayland 回归

- Command/platform: 两次间隔查询 run 34255696871 / 90bf6de 原 jobs，核对分支、HEAD 与既定文档。
- Change: 仅监控并记录，没有重启 jobs 或修改实现。
- Result: 三平台均 in_progress，暂无失败；Linux compositor probe 成功，正在执行 native Wayland pointer/keyboard/cursor；Windows 执行 accessibility 测试；macOS 已推进至 Package and exercise native GUI backends。Linux X11 原生输入仍 pending。上一轮与本轮均为已验证等待。
- Unresolved: 等待三平台完整成功、Linux X11 实际输入成功日志及同提交三份 artifacts 下载校验；M3 输入与 M5/Phase 4 未完成。

### 2026-09-09 — X11 原生国际键盘 hosted 步骤成功

- Command/platform: 两次间隔查询 run 34255696871 / 90bf6de 原三平台 jobs，核对 HEAD、分支及既定文档。
- Change: 仅监控并记录，没有重启 CI 或修改实现。
- Result: Linux 的 Verify native X11 international keyboard input 已 completed success；Wayland compositor、pointer/keyboard/cursor、activation/focus、output scaling 步骤均成功。Linux 与 Windows 当前检查 baseview feature combinations，macOS 正在打包验证 GUI 后端；三 jobs 仍 in_progress，暂无失败。上一轮为已验证等待，本轮获得原生输入步骤成功证据。
- Unresolved: 尚需完整 Linux 原始日志核对实际 X11 测试/标记、同提交三平台完整 jobs 成功与三份 artifacts 下载校验，不能只据单步成功宣称完整验收。macOS/Windows 原生国际输入及 M5 总审计仍待处理，Phase 4 未完成。

### 2026-09-09 — 原三平台 CI 进入构建与打包验证

- Command/platform: 两次间隔查询 run 34255696871 / 90bf6de 原三平台 jobs；核对既定文档、git status/log。
- Change: 本轮仅监控并记录，没有重启 CI 或修改实现。
- Result: Linux 与 macOS 已进入 Package and exercise native GUI backends，Windows 已进入 Build standalone reference applications；三 jobs 均 in_progress，暂无失败。Linux X11 与 Wayland 输入相关步骤保持成功。上一轮获得 X11 单步成功证据，本轮为已验证等待。
- Unresolved: 完整三平台 jobs 成功、Linux 原始 X11 日志及三份 artifacts 下载校验仍待完成；M3 的 macOS/Windows 原生国际输入与 M5 总审计尚未收口，Phase 4 未完成。

### 2026-09-09 — macOS 推进到 job 收尾

- Command/platform: 两次间隔查询 run 34255696871 / 90bf6de 原三平台 jobs，核对既定文档与 git status/log。
- Change: 本轮仅监控并记录，没有重启 jobs 或修改实现。
- Result: Linux 和 Windows 正在 Package and exercise native GUI backends；macOS 已推进到 Complete job，但 API 仍报告 in_progress，尚未判定完整成功。三平台暂无失败，Linux X11 与 Wayland 步骤保持成功。上一轮与本轮均为已验证等待。
- Unresolved: 等待三平台完整 jobs 成功、Linux 原始输入日志及三份 artifacts 下载校验；M3 的 macOS/Windows 原生国际输入与 M5 总审计仍未完成。

### 2026-09-09 — macOS 完整 hosted job 成功

- Command/platform: 两次间隔查询 run 34255696871 / 90bf6de 原三平台 jobs；核对既定文档、git status/log。
- Change: 仅监控并记录，没有重启 CI 或修改实现。
- Result: macOS job 102160815981 已 completed success；Linux 推进到 Exercise repository packaging helper，Windows 正在 Package and exercise native GUI backends，二者均 in_progress，暂无失败。Linux X11 与 Wayland 输入步骤保持成功。上一轮为已验证等待，本轮取得 macOS 完整 job 成功证据。
- Unresolved: 等待 Linux/Windows 完整成功、Linux 原始输入日志和同提交三份 artifacts 下载校验；M3 的 macOS/Windows 原生国际输入及 M5 总审计尚未收口，Phase 4 未完成。

### 2026-09-09 — Linux 开始上传产物

- Command/platform: 两次间隔查询 run 34255696871 / 90bf6de 原三平台 jobs；核对既定文档与 git status/log。
- Change: 仅监控并记录，没有重启 CI 或修改实现。
- Result: macOS 保持 completed success；Linux 已进入 Upload packaged Phase 1 artifacts，Windows 已进入 Exercise repository packaging helper，两者仍 in_progress，暂无失败。Linux X11 与 Wayland 输入步骤均成功。上一轮取得 macOS 完整成功证据，本轮为已验证等待。
- Unresolved: 等待 Linux/Windows 完整 jobs 成功、Linux 原始输入日志及三份 artifacts 下载校验；M3 的 macOS/Windows 原生国际输入和 M5 总审计尚未完成。

### 2026-09-09 — X11 修复三平台完整 CI 全绿，核对真实输入日志

- Command/platform: run 34255696871 / 90bf6de 三平台 jobs API；Linux 日志下载 session 42565 exit 0，/tmp/sunmao-run34255696871-linux.log；同提交 artifacts 下载 session 1148。
- Change: 监控完成后下载验收证据，未重启 jobs 或修改实现。
- Result: Linux 102160815689、Windows 102160815799、macOS 102160815981 均 completed success。Linux 日志 16318–16325 显示隔离 Xvfb 实际测试 1 passed / 0 failed / 0 ignored，X11 KEYBOARD VERIFIED 明确覆盖物理键、German、Shift、AltGr、实时布局切换、compose、focus reset；5077–5081 和 16439–16443 证实共享布局与两项属性测试实际通过。常规测试中的 skip 不计原生证据。上一轮为已验证等待，本轮取得三平台成功与原始输入证据。
- Unresolved: 原 artifacts 下载 session 1148 仍在运行，继续轮询同 handle，不重复下载；三份产物校验完成后才能正式收口 X11。macOS/Windows 原生国际输入与 M5 总审计仍未完成，Phase 4 保持未完成。

### 2026-09-09 — Windows 产物下载校验成功

- Command/platform: 两次间隔轮询原 artifacts 下载 session 1148；核对既定文档、git status/log。
- Change: 保留原下载进程并记录验收证据，没有重复下载或修改实现。
- Result: phase1-Windows-X64 产物 78,349,539 bytes、364 entries，SHA-256 与 ZIP CRC 校验成功；session 1148 仍在运行。三平台完整 CI 与 Linux X11 真实输入成功证据保持有效。上一轮取得三平台与原始日志证据，本轮取得 Windows 产物校验证据。
- Unresolved: 等待同进程完成 Linux/macOS 产物下载校验；M3 macOS/Windows 原生国际输入及 M5 总审计仍待完成，Phase 4 未完成。

### 2026-09-09 — 继续等待原产物下载进程

- Command/platform: 两次间隔轮询原 artifacts 下载 session 1148，核对既定文档与 git status/log。
- Change: 仅监控记录，保留原下载进程，没有重复下载或修改实现。
- Result: session 1148 两次均确认仍在运行，尚无新的产物校验输出；此前 Windows SHA-256/CRC 成功、同提交三平台完整 CI 成功和 X11 真实输入日志证据有效。上一轮取得 Windows 产物证据，本轮为已验证等待。
- Unresolved: Linux/macOS 产物校验仍待完成；M3 macOS/Windows 原生国际输入及 M5 总审计仍未完成，Phase 4 保持未完成。

### 2026-09-09 — 原产物下载仍在运行

- Command/platform: 两次间隔轮询原下载 session 1148，核对既定文档、分支与 HEAD。
- Change: 仅监控记录，未重启下载或修改实现。
- Result: session 1148 仍在运行，尚无 Linux/macOS 校验完成输出；Windows 产物校验、三平台完整 CI 与 X11 实际输入证据保持有效。上一轮与本轮均为已验证等待。
- Unresolved: 等待剩余两平台产物下载校验；M3 macOS/Windows 原生国际输入与 M5 总审计尚未完成。

### 2026-09-09 — X11 原生国际输入正式验收

- Command/platform: 原下载 session 1148 exit 0；run 34255696871 / 90bf6de 三平台完整 jobs 与 Linux 原始日志。
- Change: 三份产物校验完成，更新 status 的 X11 输入证据与下一瓶颈，没有重启下载或修改实现。
- Result: Windows 78,349,539 bytes / 364 entries、Linux 996,747,245 bytes / 96 entries、macOS 53,085,619 bytes / 152 entries，三份 SHA-256 与 ZIP CRC 均通过。产物位于 /tmp/sunmao-run34255696871-phase1-{Windows-X64,Linux-X64,macOS-ARM64}.zip。同提交三 jobs success；Linux 日志 16318–16325 确认 X11 实际输入测试 1 passed、0 failed、0 ignored 与 VERIFIED 标记，布局/compose 和按键生命周期属性测试亦实际通过。上一轮为已验证等待，本轮完成 X11 验收。
- Unresolved: M3 仍需 macOS/Windows 各一条真实原生国际输入路径的验收；M5 最终兼容与文档审计尚未完成，Phase 4 不标记完成。

### 2026-09-09 — macOS 原生死键处理缺陷定位

- Command/platform: 检查 baseview/src/macos/{keyboard,view,window}.rs 与已有测试；Swift AppKit/CoreGraphics 探针 /tmp/sunmao-macos-keyboard-probe.swift，session 29299 exit 0。探针只由原始物理键码及 modifier 构造 CGEvent，再通过 NSEvent(cgEvent:) 让系统生成字符，不手工写入 Unicode，也不向其他应用发送按键。
- Change: 定位下一输入瓶颈；更新 semantics 的 X11 hosted 验收引用。未修改 macOS 实现，未重跑已完成 CI。
- Result: 系统返回 KeyA→a、Option+KeyA→å、Option+KeyE→空 characters 且 charactersIgnoringModifiers=e。现有 KeyboardState 在第三种情况下回退为 Character("e") 且 is_composing=false，会把尚未提交的死键当普通 e 交给文本适配器。view.rs 的 keyDown 只直接转发，未接 interpretKeyEvents/insertText/markedText。这个系统探针证明了底层输入语义及现有错误分支，但不是完整窗口/宿主验收。上一轮完成 X11 验收，本轮取得 macOS 实现缺陷证据。
- Unresolved: macOS 需要正确处理原生组合文本提交与死键，不可只增加 Character("å") 合成测试；补实际 NSView 输入路径、失焦清理及 hosted 验收后再处理 Windows。M3 与 M5/Phase 4 未完成。

### 2026-09-09 — macOS 死键分支修复开发中

- Command/platform: 核对 SDK UnicodeUtilities.h / TextInputSources.h；AppKit 组合探针 /tmp/sunmao-macos-compose-probe.swift；cargo test --locked -p baseview native_dead_key_does_not_emit_its_unmodified_letter -- --nocapture，原 session 32726。
- Change: macOS 空 characters 不再回退到 charactersIgnoringModifiers，而输出 Key::Dead / is_composing=true；新增 CoreGraphics 原始键码到 NSEvent 的回归测试草稿。cargo fmt 与 git diff --check 通过，尚未提交。
- Result: AppKit 探针明确显示 Option+E 为空、后续 E 的 NSEvent.characters 为 é，但测试 NSTextView 没收到 insertText，断言失败（不是框架测试成功证据）。Rust 回归已编译完成，session 32726 仍在运行，尚无退出结果；不能因观察超时重启。测试草稿依赖当前布局，非死键布局的分支目前仅诊断，正式验收前必须改成显式选择布局/强断言的主线程窗口 harness，不能空转计通过。上一轮定位缺陷，本轮修复已知错误分支并启动验证。
- Unresolved: 保留 session 32726 并核对其结果；继续主线程 NSView 原生国际输入 harness、组合/失焦语义及必要属性测试。完整本地 gate 与三平台 hosted 尚未执行，macOS/Windows 输入与 M5 总审计仍未完成。

### 2026-09-09 — macOS 原生 harness 暴露输入源约束

- Command/platform: 主线程 `baseview/tests/macos_keyboard.rs` harness 编译并以 `SUNMAO_MACOS_KEYBOARD_TEST=1` 运行；沙箱外 session 37427 exit 101，日志 /tmp/sunmao-macos-keyboard.log。
- Change: 新增 macOS 测试入口并接入 workflow 草稿；测试明确要求系统键盘布局为 US/ABC，避免把输入法事件误判为物理键盘国际输入。
- Result: 当前会话实际输入源为 `com.apple.keylayout.PinyinKeyboard`，测试在发送事件前明确失败退出；没有伪造字符、没有宣称验收。此前死键分支 Rust 回归通过；`git diff --check` 仍通过。
- Unresolved: hosted macOS 需显式切换到英文键盘布局后运行真实 NSView harness；当前修改尚未提交，CI workflow 也需在实现稳定后再纳入 blocking。Windows 原生国际输入和 M5 总审计未完成。

### 2026-09-09 — macOS 真正 NSView 组合输入回归失败已定位

- Command/platform: `SUNMAO_MACOS_KEYBOARD_TEST=1 RUSTFLAGS=-Awarnings cargo test --locked -p baseview --test macos_keyboard`，沙箱外 session 20429 exit 101，/tmp/sunmao-macos-keyboard.log；此前普通入口 session 21913 exit 0 仅显式 skip，不计原生验收。
- Change: harness 通过 TIS 枚举已启用 US/ABC 并临时选择，RAII 保存精确原输入源（含 IME），成功或断言 unwind 均恢复；移除需要人工切换输入源的前置条件。CoreGraphics 空句柄 Drop 已安全处理。
- Result: 真实 baseview NSView 的 a、Option+å、Dead/composing 断言均已通过；后续 E 实际返回 Character("e") 而非预期 Character("é")，强断言准确失败。临时输入源恢复未报告错误。现有 captured keyDown 没有让 AppKit 解释组合序列；此前 NSTextView 探针调用系统 keyDown 后能在后续 NSEvent 中观察到 é，而 baseview captured 路径不能。仅修空 characters 不足以完成 macOS 输入。
- Unresolved: 需接通 AppKit 组合输入上下文及文本提交，继续使用这个失败的窗口回归验证，不能降低断言或把 skip 计通过。失焦清理、属性测试、本地完整 gate 与三平台 hosted 尚未完成；修改未提交，Phase 4 未完成。

### 2026-09-09 — macOS 原生组合字符窗口回归通过

- Command/platform: 原生 NSView harness session 5467 exit 0，/tmp/sunmao-macos-keyboard.log 输出 MACOS KEYBOARD VERIFIED；SDK UCKeyTranslate/TIS API 已核对。
- Change: 单独 interpretKeyEvents 实验仍失败（session 54386 exit 101），已移除；改为每窗口保存系统 Unicode 布局数据和 UCKeyTranslate dead state，原生布局数据变化时清空旧组合状态；空 characters 不再误输出普通字母。添加 Unfocused 时清理组合状态与后续 plain e 的窗口断言。
- Result: 已完成版本的 NSView a、Option+å、Dead/composing、组合 é 强断言全部通过且输入源恢复成功。随后新增的失焦清理版本已启动独立原生测试，结果待核对。fmt 与 diff 检查通过，没有提交或推送。
- Unresolved: 核对 /tmp/sunmao-macos-keyboard-focus.log 对应原进程；继续复核 key-up/repeat、布局切换、错误路径、属性测试及完整本地 gate/Windows check，然后三平台 hosted 验收。此实现为原生国际键盘/compose，不是完整 CJK IME 预编辑。Windows 输入与 M5 总审计尚未完成。

### 2026-09-09 — macOS 失焦组合清理通过，补按键生命周期

- Command/platform: 原失焦测试 session 55804 exit 0，/tmp/sunmao-macos-keyboard-focus.log 包含 MACOS KEYBOARD VERIFIED；属性测试 session 39334 在运行，日志 /tmp/sunmao-macos-lifecycle.log。
- Change: 按物理键记录按下时的 KeyboardEvent，key-up 与 repeat 保持原逻辑字符；失焦清空组合与 modifier 状态并释放 held keys。新增逻辑字符保持和 cancel 幂等属性测试；原生 harness 改为每次真实 keyDown/keyUp 配对并断言释放字符一致。
- Result: 之前的真实 NSView 组合/失焦清理全部通过；当前新增生命周期版本属性测试已编译，仍待退出结果。fmt 与 diff 检查通过。原生生命周期版本已启动，日志 /tmp/sunmao-macos-keyboard-lifecycle.log，尚未取得成功结果。
- Unresolved: 继续原属性/原生测试进程，检查布局变化及异常路径；完整本地 gate、Windows target check 与三平台 hosted 尚未执行。macOS 修复未提交，Windows 原生国际输入和 M5 总审计尚未完成。

### 2026-09-09 — macOS 生命周期测试通过，完整本地 gate 启动

- Command/platform: 原属性测试 session 39334 exit 0（属性测试 1 passed，0.14s）；原生生命周期 session 25960 exit 0，/tmp/sunmao-macos-keyboard-lifecycle.log 当时输出 VERIFIED；metadata/fmt/diff 与 Windows baseview target check exit 0。
- Change: 补 named editing/navigation key 取消未完成 compose、非法 UTF-16 清理组合状态；原生 harness 增加 Escape 取消与 held key 失焦单次释放强断言。semantics 更新 macOS 国际键盘实现与 hosted 待验收状态。
- Result: 上述新增断言版本 session 27118 仍在运行，日志 /tmp/sunmao-macos-keyboard-lifecycle.log；完整 `RUSTFLAGS=-Awarnings cargo test --locked` session 51485 已启动，日志 /tmp/sunmao-macos-full-tests.log。没有重启原活跃进程，未提前提交/push；无打包/示例实现改动，不追加打包 gate。
- Unresolved: 轮询原 27118 与 51485，待两个测试完整成功后最后审查并提交/push，再做同提交三平台 hosted/原生日志/artifacts 验收。Windows 原生输入与 M5 总审计仍未完成。

### 2026-09-09 — macOS 原生边界断言通过，完整回归编译推进

- Command/platform: 原生边界测试 session 27118 exit 0，/tmp/sunmao-macos-keyboard-lifecycle.log 输出 VERIFIED；两次间隔轮询完整 locked 回归 session 51485。
- Change: 本轮仅核对与记录测试证据，没有重启测试或修改实现。
- Result: 最新原生 harness 包含 Escape 取消、按键释放保持原字符、失焦单次释放，整体成功退出。完整回归原 session 51485 仍在运行，编译已从 baseview 推进到 view_baseview 和 VST3/CLAP GUI fixtures，暂无失败。上一轮启动完整 gate，本轮取得原生边界测试成功证据并确认完整回归活跃。
- Unresolved: 等待原完整回归含 doc-tests 成功退出，随后审查提交/push 与三平台 hosted/实际输入日志/产物验收；Windows 原生输入和 M5 总审计仍未完成。

### 2026-09-09 — 完整 macOS 回归持续运行

- Command/platform: 两次间隔轮询原完整 locked 回归 session 51485，日志 /tmp/sunmao-macos-full-tests.log；核对分支与 HEAD。
- Change: 仅监控记录，没有重启回归或修改实现。
- Result: 原 session 51485 持续活跃，当前仍在 GUI fixtures 编译阶段，尚无失败输出。此前原生边界/属性测试与 Windows target check 成功记录有效。上一轮取得原生边界证据，本轮为已验证等待。
- Unresolved: 等待原完整回归含 doc-tests 成功退出，再提交/push 并完成同提交三平台 hosted/原生日志/产物验收；M3 Windows 原生输入与 M5 总审计仍未完成。

### 2026-09-09 — 完整回归完成编译并进入 baseview 测试

- Command/platform: 两次间隔轮询原完整 locked 回归 session 51485，/tmp/sunmao-macos-full-tests.log；核对分支与 HEAD。
- Change: 仅监控记录，没有重启回归或修改实现。
- Result: 原 session 51485 仍活跃，编译已完成并进入测试；baseview 的 webview 名称校验、窗口资源销毁及新增按键生命周期属性测试均已 ok，暂无失败。上一轮与本轮均为已验证等待。
- Unresolved: 等待原完整回归含全部 doc-tests 成功退出，再提交/push 与三平台 hosted 验收；M3 Windows 输入及 M5 总审计仍未完成。

### 2026-09-09 — 完整回归 baseview 四项测试通过

- Command/platform: 两次间隔轮询原完整 locked 回归 session 51485，/tmp/sunmao-macos-full-tests.log；核对分支与 HEAD。
- Change: 仅监控记录，没有重启回归或修改实现。
- Result: baseview 四项测试全部通过（23.08s），含窗口 resize 重入/关闭与新增按键属性测试；完整回归继续推进平台 integration tests，session 51485 仍活跃，暂无失败。macOS 上 Linux cfg 的 0 tests 不计对应原生验收。上一轮与本轮均为已验证等待。
- Unresolved: 等待原完整回归含 doc-tests 成功退出，再提交/push 并进行三平台 hosted/日志/产物验收；M3 Windows 输入及 M5 总审计未完成。

### 2026-09-09 — 完整回归 CLAP 包装层通过

- Command/platform: 两次间隔轮询原完整 locked 回归 session 51485，/tmp/sunmao-macos-full-tests.log；核对分支与 HEAD。
- Change: 仅监控记录，没有重启回归或修改实现。
- Result: CLAP 包装层 56 项测试全部通过，完整回归继续推进 fixtures；原 session 51485 仍在运行，暂无失败。上一轮与本轮均为已验证等待。
- Unresolved: 等待原完整回归含 doc-tests 成功退出，再提交/push 与三平台 hosted/原生日志/产物验收；M3 Windows 输入和 M5 总审计仍未完成。

### 2026-09-09 — 完整回归推进至 VST3 backend

- Command/platform: 两次间隔轮询原完整 locked 回归 session 51485，日志 /tmp/sunmao-macos-full-tests.log；核对分支与 HEAD。
- Change: 仅监控记录，没有重启回归或修改实现。
- Result: 原回归从 facade/AU backend 推进到 VST3 backend，音频成功路径零分配、旧 state 迁移、transport 等已执行项通过；session 51485 仍活跃，暂无失败。上一轮与本轮均为已验证等待。
- Unresolved: 等待原完整回归含 doc-tests 成功退出，再提交/push 与三平台 hosted/日志/产物验收；M3 Windows 输入和 M5 总审计仍未完成。

### 2026-09-09 — 完整回归 VST3 backend 全部通过

- Command/platform: 两次间隔轮询原完整 locked 回归 session 51485，/tmp/sunmao-macos-full-tests.log；核对分支与 HEAD。
- Change: 仅监控记录，没有重启回归或修改实现。
- Result: VST3 backend 28 tests 全部通过（27.35s），含两个 GUI/view 测试；回归已继续进入 core 后续测试，session 51485 仍活跃，暂无失败。上一轮与本轮均为已验证等待。
- Unresolved: 等待原完整回归含 doc-tests 成功退出，再提交/push 与三平台 hosted/实际日志/产物验收；M3 Windows 输入及 M5 总审计尚未完成。

### 2026-09-10 — macOS 输入提交三平台 hosted 成功，产物核验进行中

- Command/platform: 原完整 locked 回归 session 51485 exit 0；metadata、fmt、diff 检查通过；推送 e14e5b3，查询 run 34361057036。
- Change: 提交 macOS 原生布局 compose、按键生命周期、NSView harness 与 blocking CI 步骤，修正 IME 支持范围注释。
- Result: 同提交 macOS 102497837797、Linux 102497838165、Windows 102497838208 三个完整 jobs 均 success。产物下载 session 90326 仍活跃，Linux ZIP 约 19 MB，尚未完成 SHA-256/CRC 核验，不能据此正式验收。
- Unresolved: 继续轮询原下载 session 90326，核对 macOS 原始日志的实际成功标记并完成三份产物校验；随后处理 Windows 原生国际输入及 M5 总审计。

### 2026-09-10 — macOS hosted 原生输入日志核实

- Command/platform: 下载 run 34361057036 的 macOS job 102497837797 原始日志，/tmp/sunmao-run34361057036-macos.log；轮询原产物下载 session 90326。
- Change: semantics.md 更新为 hosted 实际输入已验证、产物校验待定，未提前关闭 M3。
- Result: 日志第 3107 行实际输出 MACOS KEYBOARD VERIFIED，涵盖原生布局、Option 字符、dead key、组合字符及 NSView focus reset；第 3159 行 logical_keys_survive_modifier_changes_and_cancel_releases_once 通过。下载 session 90326 仍活跃，Linux ZIP 增长至约 21 MB。上一轮为提交/CI 成功的进展，本轮取得原始运行证据。
- Unresolved: 等待原下载 session 90326 完成三份产物 SHA-256/CRC 校验；Windows 国际输入及 M5 总审计仍未完成。

### 2026-09-10 — 清理重复产物下载进程

- Command/platform: 轮询 session 90326；通过 ps 核对下载父子进程；TERM 81461/81495 与 82019/82049。
- Change: 停止前轮误启动的两组重复下载，保留最新原进程 82276/82308（session 90326），没有重启下载。
- Result: 发现此前丢弃 exec_command 返回的 session_id 导致错误重启，两组重复 curl 同时写同一 ZIP；本轮已清理，保留进程仍活跃。最终必须以 SHA-256/CRC 判断文件完整性，不以文件大小判断成功。上一轮取得日志证据，本轮修正下载操作错误。
- Unresolved: 等待 session 90326 下载及校验三份 artifacts；macOS 实际日志已核实但正式验收尚未关闭，Windows 国际输入和 M5 总审计待完成。

### 2026-09-10 — 原产物下载进程继续等待

- Command/platform: 间隔 40 秒轮询原 session 90326，核对 ZIP stat 与当前 e14e5b3 工作区。
- Change: 仅监控原下载，没有再次启动进程。
- Result: session 90326 两次均确认活跃，无退出结果；Linux ZIP 当前 22904832 bytes，观察间隔大小不变、修改时间从 13:28:19 更新到 13:29:17。此前重复写入使文件大小不能代表保留进程的下载偏移，因此不据此判断下载完成或失败。上一轮清理重复进程属进展，本轮为已验证等待。
- Unresolved: 等待原进程完成并给出三份产物 SHA-256/CRC 结果；M3 Windows 原生输入与 M5 总审计仍未完成。

### 2026-09-10 — 核对下载实际写入偏移

- Command/platform: 两次 lsof -a -p 82308 -o -d 8，并间隔 40 秒轮询原 session 90326。
- Change: 仅监控记录；没有重启下载或修改实现。
- Result: 原 session 90326 仍活跃，curl 的 artifact HTTPS 连接 ESTABLISHED；两次实际写入偏移均为 22904832，本观察窗口没有新增字节，尚无退出/失败结果。不能把连接存活解释为吞吐正常，也不能据一次观察停滞重启下载。上一轮与本轮为已验证等待。
- Unresolved: 等待原下载进程的终态及 SHA-256/CRC 结果；M3 Windows 国际输入和 M5 总审计未完成。

### 2026-09-10 — macOS 与 Windows 产物完整校验通过

- Command/platform: 独立目录 /tmp/sunmao-mac-evidence，macOS 下载 session 21958 exit 0、Windows session 3650 exit 0；保留原 Linux session 90326。
- Change: 在当前产物验收瓶颈内独立下载较小的两平台 ZIP，添加连接/低速超时，避免受 Linux 单条连接影响；未重启原 Linux 下载。
- Result: macOS 53678427 bytes，SHA-256 eb727cf2944312c9494e9e793dcdf06231bb84ab541c228e0a84eead4ea89721；Windows 78349905 bytes，SHA-256 9eb2a314105bc0cfdd160687faca978a5518ca079491aabda4656d1ac709cdb8；两份均与 GitHub digest 一致且 ZIP CRC 通过。原 Linux curl 仍在偏移 22904832，session 活跃。上一轮为已验证等待，本轮完成两份实际产物验收。
- Unresolved: Linux ZIP 尚未完整下载/校验，不能关闭 macOS 提交正式验收；Windows 原生国际输入及 M5 总审计仍需完成。

### 2026-09-10 — Linux 停滞连接断点续传恢复

- Command/platform: 原 session 90326 多轮实际偏移固定 22904832；两份独立产物已成功，确认传输问题局限于旧连接。TERM 82276/82308 后 session 90326 exit 143；curl -C - 开始 session 94806。
- Change: 保留已下载前缀，从文件末尾续传，增加 connect-timeout 30 与 speed-limit 1024 / speed-time 60；不是因一次观察超时从零重启。
- Result: session 94806 仍活跃，文件由 22904832 增长至 124780544 bytes，总大小 996747105；传输已恢复。macOS、Windows 两份既有 SHA-256/CRC 通过证据保留。上一轮完成两份产物核验，本轮恢复 Linux 实际传输。
- Unresolved: 继续轮询 session 94806，等待 Linux SHA-256/CRC 通过才能关闭本提交正式验收；随后 Windows 国际输入与 M5 总审计。

### 2026-09-10 — 三平台 CI 全步骤核对，Linux 续传推进

- Command/platform: run 34361057036 --audit-steps（session 11119 exit 0）；间隔轮询 Linux 续传 session 94806。
- Change: 补充全步骤状态核验，没有改动实现或重启下载。
- Result: 同提交三个 jobs 各 33 steps，所有步骤均 success 或 skipped，零 failure/cancelled；macOS 专项国际键盘步骤 success。Linux ZIP 从 188809216 增长至 377044992 bytes，总计 996747105，session 94806 仍活跃，尚未到 SHA-256/CRC 阶段。上一轮恢复续传属进展，本轮新增步骤证据并确认续传推进。
- Unresolved: 等待 session 94806 完成 Linux 产物校验；Windows 原生国际输入与 M5 总审计尚未完成。

### 2026-09-10 — macOS 国际键盘修复正式验收完成

- Command/platform: 持续轮询 Linux 断点续传 session 94806，最终 exit 0；对照 run 34361057036 / e14e5b3 的三平台 jobs、macOS 原始日志与三份产物校验。
- Change: status.md 与 semantics.md 将 macOS 国际键盘/compose 标为已验收，保留 Windows 原生输入与 M5 总审计缺口。
- Result: Linux ZIP 996747105 bytes，SHA-256 1607d20b4d6063e6e3df8cac44f4abdf713d7b915a20a2194fdad5843fe12150，与 GitHub digest 相符且 ZIP CRC 通过。连同此前 macOS 53678427 bytes / Windows 78349905 bytes，两份均已 SHA-256/CRC 通过，本提交三平台产物全部验收。三平台完整 jobs success，每平台 33 steps 无失败/取消；macOS 实际 NSView 输入成功标记与生命周期属性测试已核实。
- Unresolved: 下一瓶颈是 Windows 至少一条真实国际输入路径；随后 M5 兼容性、文档与全部原始要求的总审计，Phase 4 尚未完成。

### 2026-09-10 — Windows 消息钩子补系统字符翻译

- Command/platform: 核查 win/keyboard.rs、hook.rs 与 window.rs；cargo check --locked -p baseview --test windows_keyboard --target x86_64-pc-windows-msvc（session 70014 exit 0）；metadata/fmt/diff 通过；完整回归 session 83565 已启动，日志 /tmp/sunmao-windows-keyboard-full-tests.log。
- Change: WH_GETMESSAGE 钩子在吞掉按键前调用 TranslateMessage，确保 Windows 可产生布局/组合字符；释放注册表锁后再进入窗口回调。新增 windows_keyboard 主线程原生消息队列 harness 和 blocking Windows CI 步骤，仅输入扫描码/虚拟键，不合成 WM_CHAR 或 Unicode payload。
- Result: 发现普通键依靠缓存布局映射掩盖了系统 compose 消息从未生成的问题。harness 覆盖德语物理键 Y→z、ü、Shift Ü、dead-key é，检查单次按键只产生一个逻辑事件并恢复线程布局/修饰键。初次交叉检查抓出 HKL 导入位置错误，修正后 Windows target 编译通过；尚无 Windows 实际运行证据。上一轮正式验收 macOS，本轮实现 Windows 缺陷修复与原生验收入口。
- Unresolved: 等待原完整回归 session 83565 成功，再最终检查、commit/push 与三平台 hosted 原生输入/产物验收；M5 总审计尚未完成。

### 2026-09-10 — Windows 修复完整回归继续编译

- Command/platform: 间隔 40 秒轮询原完整回归 session 83565，/tmp/sunmao-windows-keyboard-full-tests.log；复核 windows_keyboard harness 与钩子 diff。
- Change: 仅复核与监控，没有重启回归或修改实现。
- Result: session 83565 仍活跃，编译从 baseview/view_baseview 推进到 VST3/CLAP baseview 示例，尚无错误；复核确认 harness 不提交 WM_CHAR/Unicode，dead-key 后的 é 断言可区分系统 compose 与旧缓存 e 映射。Windows 实际执行证据仍需 hosted 提供。上一轮实现修复属进展，本轮为已验证等待。
- Unresolved: 等待原 session 83565 含 doc-tests 成功退出，再提交/push 与三平台完整 hosted/原生日志/产物验收；M5 总审计仍待完成。

### 2026-09-10 — Windows 修复回归推进到框架 fixtures

- Command/platform: 间隔 40 秒轮询原完整回归 session 83565，/tmp/sunmao-windows-keyboard-full-tests.log；核对 HEAD e14e5b3 与工作区。
- Change: 仅监控记录，没有重启回归或修改实现。
- Result: 原进程仍活跃，编译已由格式层示例推进到 effect/instrument 模板、SVF、tempo delay、GL/WGPU GUI 等 fixtures，暂无编译错误；尚未得到完整回归退出结果。上一轮与本轮均为已验证等待。
- Unresolved: 等待原 session 83565 成功退出，再提交/push 并完成 Windows 原生输入及三平台完整 hosted/产物验收；M5 总审计待完成。

### 2026-09-10 — Windows 修复完整回归开始执行测试

- Command/platform: 两次间隔 40 秒轮询原完整回归 session 83565，日志 /tmp/sunmao-windows-keyboard-full-tests.log。
- Change: 仅监控记录，没有重启回归或修改实现。
- Result: 完整编译结束，测试已从 AU/baseview 推进到平台 integration harness，原进程仍活跃且暂无失败。macOS 上 windows_keyboard 的非 Windows 空入口不构成 Windows 原生验收；真实输入须由后续 Windows hosted flag 步骤执行。上一轮与本轮均为已验证等待。
- Unresolved: 等待原 session 83565 含全部 doc-tests 成功退出，再提交/push 并核验同提交三平台 jobs、原生输入日志及产物；M5 总审计未完成。

### 2026-09-10 — Windows 修复回归进入 facade

- Command/platform: 间隔 40 秒轮询原完整回归 session 83565，日志 /tmp/sunmao-windows-keyboard-full-tests.log。
- Change: 仅监控记录，没有重启回归或修改实现。
- Result: 原进程由 CLAP 打包器/包装层相关测试推进至 sunmao facade，最新 raw_clap_synth_callback_does_not_allocate 测试通过，暂无失败；session 83565 仍活跃。上一轮与本轮均为已验证等待。
- Unresolved: 等待原 session 83565 含全部 doc-tests 成功退出，再提交/push 与三平台 hosted 原生输入/日志/产物验收；M5 总审计仍未完成。

### 2026-09-10 — Windows 修复回归推进至 CLAP backend

- Command/platform: 间隔 40 秒轮询原完整回归 session 83565，/tmp/sunmao-windows-keyboard-full-tests.log；核对工作区与 HEAD。
- Change: 仅监控记录，没有重启回归或修改实现。
- Result: 回归由 facade 属性测试推进至 CLAP backend，日志中的 latency/tail、expression/mod 路由、参数通知与 smoothing 零分配测试均 ok；session 83565 仍活跃，暂无失败。上一轮与本轮均为已验证等待。
- Unresolved: 等待原 session 83565 含 doc-tests 成功退出，再提交/push 与三平台 hosted 原生 Windows 输入/日志/产物验收；M5 总审计仍未完成。

### 2026-09-10 — Windows 修复回归两格式 backend 通过

- Command/platform: 间隔 40 秒轮询原完整回归 session 83565，/tmp/sunmao-windows-keyboard-full-tests.log。
- Change: 仅监控记录，没有重启回归或修改实现。
- Result: CLAP backend 38 tests 全通过（44.52s），VST3 backend 28 tests 全通过（43.86s），含旧 state 迁移、transport、GUI view 与 ABI panic containment；回归进入 core，原 session 83565 仍活跃且暂无失败。上一轮与本轮均为已验证等待。
- Unresolved: 等待原 session 83565 含全部 doc-tests 成功退出，再提交/push 与三平台 hosted 原生 Windows 输入/日志/产物验收；M5 总审计未完成。

### 2026-09-10 — Windows 修复回归推进到效果器 GUI

- Command/platform: 间隔 40 秒轮询原完整回归 session 83565，/tmp/sunmao-windows-keyboard-full-tests.log。
- Change: 仅监控记录，没有重启回归或修改实现。
- Result: 回归从 core 属性测试、DSP 推进到效果器及 GL/WebView/WGPU GUI fixtures，暂无失败；原 session 83565 仍活跃，尚未返回完整回归退出结果。上一轮与本轮均为已验证等待。
- Unresolved: 等待原 session 83565 含 doc-tests 成功退出，再提交/push 与三平台 hosted 原生 Windows 输入/日志/产物验收；M5 总审计仍未完成。

### 2026-09-10 — Windows 修复回归继续效果器套件

- Command/platform: 间隔 40 秒轮询原完整回归 session 83565，/tmp/sunmao-windows-keyboard-full-tests.log。
- Change: 仅监控记录，没有重启回归或修改实现。
- Result: layout gain 5 tests、SVF 6 tests 全通过，回归推进到 tempo delay；session 83565 仍活跃，暂无失败。上一轮与本轮均为已验证等待。
- Unresolved: 等待原 session 83565 含全部 doc-tests 成功退出，再提交/push 与三平台 hosted 原生 Windows 输入/日志/产物验收；M5 总审计未完成。

### 2026-09-10 — Windows 修复回归越过 GUI 套件

- Command/platform: 间隔 40 秒轮询原完整回归 session 83565，/tmp/sunmao-windows-keyboard-full-tests.log。
- Change: 仅监控记录，没有重启回归或修改实现。
- Result: widgets fixture 9 tests 全通过，含 accessibility 描述与 audio 线程频谱发布；回归越过 GUI crate 并进入参数宏测试，原 session 83565 仍活跃，暂无失败。Windows UIA 在 macOS 上的 0 tests 不计原生证据。上一轮与本轮均为已验证等待。
- Unresolved: 等待原 session 83565 含全部 doc-tests 成功退出，再提交/push 与三平台 hosted 原生 Windows 输入/日志/产物验收；M5 总审计未完成。

### 2026-09-10 — Windows 修复回归推进至合成器示例

- Command/platform: 间隔 40 秒轮询原完整回归 session 83565，/tmp/sunmao-windows-keyboard-full-tests.log。
- Change: 仅监控记录，没有重启回归或修改实现。
- Result: state migration 5 tests 全通过，回归继续合成器 GUI fixtures；timed volume sample offsets 与 reset silences active voices 均 ok，原 session 83565 仍活跃且暂无失败。上一轮与本轮均为已验证等待。
- Unresolved: 等待原 session 83565 含全部 doc-tests 成功退出，再提交/push 与三平台 hosted 原生 Windows 输入/日志/产物验收；M5 总审计未完成。

### 2026-09-10 — Windows 输入修复本地完整 gate 通过

- Command/platform: 原完整 locked 回归 session 83565 exit 0，包含所有 doc-tests；Windows target harness check session 70014 exit 0；metadata/fmt/diff 检查通过。
- Change: 完成 Windows TranslateMessage 钩子修复、主线程原生消息队列 harness 与 blocking CI 步骤的提交准备，保留 macOS 验收文档。
- Result: 本地完整回归通过，没有用非 Windows 空入口冒充 Windows 原生执行。harness 使用系统德语布局与 TranslateMessage 生成 z/ü/Ü/é，实际结果需下一次 Windows hosted 运行证明。
- Unresolved: 提交/push 后核实同提交三平台完整 jobs、Windows 原生日志和三份 artifacts；M5 总审计尚未完成。

### 2026-09-10 — Windows 输入修复已推送并启动 hosted

- Command/platform: commit 4c4107dcb669e234db0e6c37fc15d572360b1b4f；HTTPS push session 40756 exit 0；Actions API session 78676 exit 0。
- Change: 推送 Windows 原生键盘翻译修复、验收 harness 与 blocking CI 步骤到 phase4/gui-component-library。
- Result: run 34443897665 已确认 in_progress，https://github.com/aizcutei/sunmao/actions/runs/34443897665；完整本地 gate 通过，尚无本提交 hosted 完成证据。
- Unresolved: 监控原 run 34443897665，失败读取实际日志修复，成功后核验 Windows KEYBOARD VERIFIED 与同提交三平台完整 jobs/产物；M5 总审计仍未完成。

### 2026-09-10 — Windows 输入修复三平台 hosted 运行中

- Command/platform: 两次间隔查询原 run 34443897665，API session 20235/35542 均 exit 0，HEAD 4c4107d。
- Change: 仅监控与记录，监控助手增加国际键盘步骤显示，未修改项目实现或重启 CI。
- Result: Windows job 102764416201、macOS 102764416351、Linux 102764416354 均 in_progress；Linux 已从安装依赖推进到 Test format adapters and host，三平台当前都在该步骤。Windows 原生国际键盘验收仍 pending，无失败结果。上一轮提交/启动 CI 属进展，本轮为已验证等待。
- Unresolved: 继续原 run，等待 Windows KEYBOARD VERIFIED 实际执行及同提交完整三平台 jobs/日志/产物验收；M5 总审计未完成。

### 2026-09-10 — Windows 输入修复 hosted 持续推进

- Command/platform: 间隔 40 秒查询原 run 34443897665，API session 15296/2411 均 exit 0。
- Change: 仅监控记录，没有重启 CI 或修改实现。
- Result: 三平台 jobs 均 in_progress；macOS 已推进到 standalone runtime/facade/reference examples，Linux 在 facade renderer contracts，Windows 在 format adapters and host。Windows 原生国际键盘步骤仍 pending，暂无失败。上一轮与本轮均为已验证等待。
- Unresolved: 继续监控原 run，核实 Windows KEYBOARD VERIFIED 实际执行与同提交完整三平台 jobs/日志/产物；M5 总审计未完成。

### 2026-09-10 — hosted macOS 原生键盘步骤再次通过

- Command/platform: 间隔查询原 run 34443897665，API session 11394/23593 均 exit 0。
- Change: 仅监控记录，没有重启 CI 或修改实现。
- Result: macOS 原生国际键盘步骤 completed success，当前执行 realtime callback allocation matrix；Windows 已进入 standalone runtime/facade/reference examples，Linux 在 accessibility。三平台完整 jobs 仍 in_progress，Windows 原生键盘步骤 pending，无失败。上一轮与本轮均为已验证等待，macOS 步骤成功不等于本提交完整验收。
- Unresolved: 继续原 run，核实 Windows KEYBOARD VERIFIED 实际执行及同提交完整三平台 jobs/日志/产物；M5 总审计未完成。

### 2026-09-10 — Windows hosted 键盘步骤 shell 修复

- Command/platform: run 34443897665 / commit 4c4107d；Windows job 102764416201 已 failure，check-runs/annotations 与完整日志 /tmp/sunmao-run34443897665-windows.log 已读取。
- Change: Windows 原生键盘步骤显式指定 shell: bash，修复默认 PowerShell 无法执行 set -euo pipefail；键盘实现与断言保持原样。
- Result: 失败发生于脚本首行，错误为参数 euo 不存在，cargo 与原生键盘 harness 尚未运行。macOS/X11 原生输入步骤 success，完整 macOS/Linux jobs 仍运行中。本地 metadata/fmt/diff 已通过，完整 locked 回归 session 74599 正在执行。
- Unresolved: 等待原回归通过后提交/push shell 修复，再核验 Windows 实际输入及同提交三平台完整 jobs/日志/产物；M5 总审计未完成。

### 2026-09-10 — Windows shell 修复提交前 gate 通过

- Command/platform: 完整 locked 回归 session 74599 exit 0，日志 /tmp/sunmao-windows-shell-full-tests.log；metadata/fmt/diff 各 exit 0。
- Change: 仅 CI shell 配置与进度日志，本轮不涉及平台实现或打包/示例变更。
- Result: 全部本地 gate 通过，含 doc-tests；准备提交 shell 修复并推送当前 Phase 4 分支。
- Unresolved: 后续同提交三平台 hosted 及 Windows 原生输入必须实际通过，下载日志/产物校验后方可验收；M5 总审计未完成。

### 2026-09-10 — shell 修复已推送并进入 hosted 队列

- Command/platform: HTTPS push session 74011 exit 0；Actions API exit 0，HEAD 14583b8317498192e8ab8e520f4fd23e0b6474b9。
- Change: 将已通过完整本地 gate 的 Windows CI shell 修复推送至 phase4/gui-component-library。
- Result: 新 run 34444978927 queued，https://github.com/aizcutei/sunmao/actions/runs/34444978927；旧 run 34443897665 completed failure，Windows 原生测试因 PowerShell 解释 Bash 首行失败而未执行。
- Unresolved: 监控新 run 34444978927，核验 Windows 原生测试实际运行与同提交三平台 jobs/原始日志/产物；M5 总审计未完成。

### 2026-09-10 — shell 修复三平台 hosted 已启动

- Command/platform: 原 run 34444978927 / 14583b8，两次 API 查询 session 41776/31814 均 exit 0，间隔 40 秒；核对分支与 HEAD。
- Change: 仅监控记录，未重启 CI 或修改实现。
- Result: Linux job 102767710362 已从依赖安装推进到 format adapters and host；macOS 102767710464、Windows 102767710485 均在该测试步骤。三平台 jobs in_progress，无失败，Windows 原生国际输入仍 pending。上一轮修复/push 属进展，本轮为已验证等待。
- Unresolved: 继续监控原 run 34444978927，核实 Windows KEYBOARD VERIFIED 实际执行及同提交三平台完整 jobs/日志/产物；M5 总审计仍未完成。

### 2026-09-10 — shell 修复 hosted 推进至 facade 与 runtime

- Command/platform: 间隔 40 秒轮询原 run 34444978927，API session 36763/27355 均 exit 0；HEAD 14583b8。
- Change: 仅监控记录，没有重启 CI 或修改实现。
- Result: macOS 已推进到 standalone runtime/facade/reference examples，Linux 推进到 facade renderer contracts，Windows 在 format adapters and host。三平台 jobs in_progress，Windows 原生键盘步骤 pending，无失败。上一轮与本轮均为已验证等待。
- Unresolved: 继续原 run，核实 Windows 原生输入实际执行及同提交三平台完整 jobs/原始日志/产物；M5 总审计未完成。

### 2026-09-10 — shell 修复 hosted macOS 原生键盘通过

- Command/platform: 间隔 40 秒轮询原 run 34444978927，API session 85117/56318 均 exit 0，HEAD 14583b8。
- Change: 仅监控记录，没有重启 CI 或修改实现。
- Result: macOS 原生国际键盘步骤 success，当前检查 baseview feature combinations；Linux 推进到 Phase 3 fixtures，Windows 推进到 facade renderer contracts。三平台完整 jobs in_progress，Windows 原生键盘仍 pending，无失败。上一轮与本轮均为已验证等待。
- Unresolved: 继续原 run，核实 Windows 原生输入实际执行及同提交三平台完整 jobs/原始日志/产物；M5 总审计未完成。

### 2026-09-10 — shell 修复 hosted Wayland 步骤通过

- Command/platform: 间隔 40 秒轮询原 run 34444978927，API session 91747/75912 均 exit 0；HEAD 14583b8。
- Change: 仅监控记录，没有重启 CI 或修改实现。
- Result: Linux Wayland 探针、pointer/keyboard/cursor、activation/focus、output scaling 均 success，正在执行原生 X11 国际键盘；macOS 进入 native GUI 打包验收；Windows 在 standalone runtime/facade/reference examples。三平台完整 jobs in_progress，Windows 原生键盘仍 pending，无失败。上一轮与本轮均为已验证等待。
- Unresolved: 继续原 run，核实 Windows 原生输入实际执行及同提交三平台完整 jobs/原始日志/产物；M5 总审计未完成。

### 2026-09-10 — shell 修复 hosted X11 原生键盘通过

- Command/platform: 间隔 40 秒轮询原 run 34444978927，API session 17752/33255 均 exit 0，HEAD 14583b8。
- Change: 仅监控记录，没有重启 CI 或修改实现。
- Result: Linux X11 原生国际键盘步骤 success，当前检查 baseview feature combinations；Windows 从 Phase 4 fixtures 推进到 accessibility；macOS 在 native GUI 打包验收。三平台 jobs in_progress，Windows 原生键盘步骤 pending，无失败。上一轮与本轮均为已验证等待。
- Unresolved: 继续原 run，核实 Windows 原生输入实际执行及同提交三平台完整 jobs/原始日志/产物；M5 总审计未完成。

### 2026-09-10 — Windows 原生测试布局初始化修复

- Command/platform: run 34444978927 / 14583b8 Windows job 102767710485 failure；check-runs/annotations 与完整日志 /tmp/sunmao-run34444978927-windows.log 已读取。Windows target check session 28402 exit 0；完整 locked 回归 session 60143 exit 0（含 doc-tests），metadata/fmt/diff 均 exit 0。
- Change: 原生 harness 改为建窗、SetFocus 与初始消息泵完成后再 ActivateKeyboardLayout；每次按键前后验证 GetKeyboardLayout，保留德语 z/ü/Ü/é 全部断言。
- Result: shell 修复已使测试真实执行，旧日志在 windows_keyboard.rs:127 断言实际为 Character(";")、预期 Character("ü")，与美式布局翻译相同 VK 一致；先前仅在建窗前确认布局不足以证明输入时布局正确。新初始化顺序与布局断言已通过本地 gate，但仍须 Windows hosted 实测。首次 target check 发现 WindowHandle 无 focus 方法，已改用 Win32 SetFocus/GetFocus 并复查通过。
- Unresolved: 提交/push 后监控新 run，核实 Windows 原生输入及同提交三平台完整 jobs/日志/产物；M5 总审计未完成。上一轮为已验证等待，本轮有实际失败证据与修复进展。
