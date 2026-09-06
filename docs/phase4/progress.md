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
