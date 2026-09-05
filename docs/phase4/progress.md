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
