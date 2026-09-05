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
| M4 可视化与 accessibility | `VizChannel`、`SpectrumAnalyzer`/meter、accessibility 树、floating CLAP editor | **部分完成，两项受阻**（详见下方“M4 受阻项”）。已落地：`sunmao_core::viz` 三缓冲 `VizChannel`（audio 侧 publish 零 alloc，含跨真实线程的撕裂读检测）、`SpectrumAnalyzer`（峰值即起、落差衰减、NaN/越界收敛）、`accessibility_tree` + `AccessibleNode`/`AccessibleRole`（角色由 `ParameterWidget::accessible_role` **声明**而非从显示文本推断）、CLAP `suggest_title` 由静默 stub 改为真实转发。fixture 已从 crate 内 `SpectrumPublisher` 换成 `VizChannel`+`SpectrumAnalyzer` | [run #86](https://github.com/aizcutei/sunmao/actions/runs/33980911401)（commit `1ddc210`）三 job success，每 job **26 步零非成功**，artifacts 3 份可下载（macOS 53.0MB / Windows 78.3MB / Linux 971.2MB）。已下载三平台 job 日志核实新断言**真的执行且通过**：`the_editor_describes_itself_to_assistive_technology ... ok`、accessibility proptest 套件、`VizChannel` 跨线程撕裂读测试各 1 次/平台，`GUI scale negotiated` 由 16 增至 **18** 次（widgets fixture 入 GUI 矩阵后 9 插件 × 2 格式），三平台**零 FAILED 套件**。⚠️ **但其中的跨线程测试当时是 flaky 的**：run #87 在 Linux 上以 `the consumer never saw a frame` 失败，根因是该测试固定轮询 50000 次、断言依赖线程调度而非通道行为（详见 progress.md）。已改为轮询至生产者置位再做收尾 take。**#86 对其余确定性断言的验收不受影响，但"跨线程行为已三平台验证"这一条要等修复版取绿才成立** | **floating editor 已于 M5 轮补上**（见下）；accessibility OS 桥接仍未做 |
| M5 Wayland 与总验收 | Wayland、GUI 侧兼容策略、proptest/文档收尾 | **部分完成；Wayland 原生受阻（规范级，非工程取舍）**。已落地：`docs/phase3/compatibility.md` 新增 §2bis GUI 兼容（受保护面 / 视觉不承诺但语义承诺 / accessibility role 规则 / 只能记录不能承诺的宿主侧行为）；accessibility proptest 4 条；补齐 `Knob`/`Slider` 缺失的 `host_sync_never_echoes_back_to_the_host`（此前只有 `Toggle`/`Dropdown` 有，而这正是防 automation 回声环的那条不变量）；`clap_sys` 补回上游遗漏的 `uikit` 成员与 `CLAP_WINDOW_API_UIKIT` 并加 union 布局断言 | [run #88](https://github.com/aizcutei/sunmao/actions/runs/33984024056)（commit `c25dabc`）三平台 success、26 步零非成功、artifacts 3 份；已下载日志核实 `the_window_api_names_match_upstream ... ok`、`host_sync_never_echoes_back_to_the_host` 各 **4 次/平台** | Wayland 见下方「M5 Wayland：受阻链已缩短为一条」 |

## 完成规则

Phase 4 完成的唯一判定：同一 commit 三平台 hosted native jobs 全绿 + artifacts 可下载
+ 本文件 Milestone 矩阵 M0–M5 全部标记完成。本地结果任何情况下都不构成完成证据。

### 当前判定：**未满足**（只差 accessibility OS 桥接与 Wayland 原生两项）

前两条满足（run #86/#88 三平台绿、artifacts 齐备），**第三条不满足**。

已完成并各自取得三平台 hosted 绿：**M0（#73）、M1（#75）、M2（#77）、M3（#84）**，
**M4 的可视化与 accessibility 描述树（#86）**，**M5 的 GUI 兼容策略与 proptest/文档收尾（#88）**，
以及本轮补上的 **floating CLAP editor**。

**关于 floating editor：我此前把它判为受阻是错的，已纠正并交付。** 详见下方「M4 项 1」。
教训记在这里而不是藏起来：判定"受阻"之前必须把被指为障碍的代码读完——`open_blocking`
里真正阻塞的只有最后一行，而我据以下结论的是函数名。

剩余两项未交付：

| 未交付项 | 所属 | 唯一阻塞原因 | 规模 |
|---|---|---|---|
| accessibility 的 OS 桥接 | M4 | 两个插件格式都没有 accessibility 通道，它走 OS API | UIA / NSAccessibility / AT-SPI 三份互不复用的原生实现，且 **runner 无法验收**（需要 Phase 5 的交互式 host） |
| Wayland 原生 | M5 | **baseview 没有 Wayland 后端**（`baseview/src/lib.rs:6` 只有 `mod x11`，全树零 Wayland 引用） | 一个完整后端：`wl_surface`/`xdg_shell`/EGL/`wl_seat`+xkbcommon 输入/`wl_output` 缩放；CI 还需装无头 compositor 才能验收；且只有 CLAP 受益（VST3 无 Wayland 平台类型） |

## M4 的两项"受阻"：一项已纠正并交付，一项仍未做

这两项都写在 M4 范围里。**第 1 项我曾错判为受阻，本轮已交付**；第 2 项仍未做，
如实标注而不是交付一个看起来能用的版本。

### 1. floating CLAP editor —— **已交付**（此前判断过重，已纠正）

**本轮之前我把这一项判为受阻，理由是"vendored baseview 只有 `open_parented` 与阻塞式
`open_blocking`，加一个非阻塞顶层窗口模式比 M4 其余全部加起来还大"。重读三平台实现后
这个判断是错的**，如实记下来：`open_blocking` 里真正阻塞的**只有最后一步**——macOS 的
`app.run()`、Windows 的 `GetMessageW` 泵、X11 的 `join`。**建窗与事件循环早已是分开的**，
Windows 那条甚至已经把 `WindowHandle` 返回出来了。

新增 `baseview::Window::open_floating`（三平台各一条实现，与 `open_blocking` 共用建窗路径）：

- **macOS**：复用 `open_blocking` 的建窗代码，但 **`ns_app: None`**。这一个字段是关键：
  它意为"本窗口拥有 application"，非空时关窗会调 `stop_application_event_loop()`
  ——在插件里那会停掉**宿主的** run loop。同理不碰 `setActivationPolicy`/`finishLaunching`/
  `activateIgnoringOtherApps`（那些是 standalone 的 app 引导，插件无权对宿主做），
  并用 `orderFront_` 而非 `makeKeyAndOrderFront_`，编辑器弹出不该把用户从手头的事上拽走。
- **Windows**：`Self::open(false, null_mut(), ..)` 已经返回 handle，直接用。
  `DispatchMessageW` 按 HWND 派发到对应窗口过程，故宿主的消息泵自然驱动我们的窗口
  （前提是窗口建在拥有该泵的线程上——CLAP/VST3 都保证 GUI 调用在主线程）。
- **X11**：走 `open_parented` 的结构但 parent 传 `None`。X11 连嵌入窗口都自带窗口线程，
  差别只在不 join，也不传 `stop_requested`（那是给 standalone 停循环用的）。

向上贯通：`SunmaoView::supports_floating()` + `open_floating(context)`（默认 false/None，
未支持的适配器如实降级）→ `BaseviewView` 两种模式**共用同一段 `open_with` 构造**
（GL 配置、WGPU 回退、初始化校验、`ViewHandle` 接线只写一遍，一处修复两处生效）→
`backend_clap` 的 `is_api_supported(_, true)` 直接返回 `view.supports_floating()`，
**与 `gui_create` 同源，杜绝"查询说支持、创建却失败"**；窗口在 `gui_show` 才打开
（CLAP 的 create/show 分工），浮动模式下 `gui_set_parent` 一律拒绝。

**仍降级一项**：`gui_hide` 回 false。baseview 没有"隐藏但保活"，关掉再开会让宿主拿到
全新编辑器、丢失界面状态，故如实回不支持、让宿主退回 destroy/create。

测试：`a_floating_capable_view_opens_on_show_and_refuses_a_parent`、
`a_floating_capable_view_still_embeds`、
`a_suggested_title_reaches_the_plugin_and_a_non_floating_view_declines`。

**验收**：[run #90](https://github.com/aizcutei/sunmao/actions/runs/33986603921)
（commit `b111338`）三平台 success、每 job 26 步零非成功、artifacts 3 份可下载。
已下载三平台日志核实三条测试**逐条 ok**、零 FAILED 套件。**X11 那条实现本地无法编译验证**
（交叉编译缺 X11 sysroot），Linux job 是它唯一的证据，因此这一条尤其不能只看 job 结论。

### 2. accessibility —— 框架侧完成；AccessKit 翻译层完成；平台适配器接线未做

`accessibility_tree()` 把控件树转成 `AccessibleNode`（role / label / 可朗读值 /
归一化值 / bounds / focus / disabled），这是三家 OS API 共同需要、也是唯一值得测试的
那一层：6 单测 + 4 proptest + 1 doc-test + fixture 上的
`the_editor_describes_itself_to_assistive_technology`。

**本轮补上第二层：`accesskit_update()` 把这棵树翻成 AccessKit 的 `TreeUpdate`。**
上一轮我把这项记为"需要三份互不复用的原生实现"——**这个判断同样过重**：
[AccessKit](https://github.com/AccessKit/accesskit)（MIT/Apache-2.0，MSRV 1.85）
已经维护着 UIA / NSAccessibility / AT-SPI 三个适配器，egui 与 winit 都用它。
SunMao 要写的只是数据映射，**没有任何平台代码，因此在任何主机上都可测**。

放在 **off-by-default 的 `accessibility` feature** 后面，与 `text`/`clipboard` 同理，
也与本项目"AU 不进默认 feature"的既有约定一致：AccessKit 是真实的依赖面
（Linux 适配器要拉 D-Bus 栈），而 `accessibility.rs` 那棵树本身不依赖它。
只新增 2 个 crate（`accesskit` + `uuid`）。

设计要点：
- role 映射保守——每个目标 role 在三平台都有明确对应，屏幕阅读器读到的控件种类一致。
  `Graphic` 映射到 `Role::Image`（AccessKit 无 meter），读作"存在、可描述、不可交互"。
- 归一化值同时写 `min`/`max`，否则屏幕阅读器无法说出百分比；**非有限值直接不写**，
  否则会被念成乱码。
- id 按深度优先分配，父节点先占位再递归，因此子节点永远不会拿到父节点的 id。
- AccessKit 对树形很严格（节点必须是 root 或某节点的 child），故 4 条 proptest 钉住：
  唯一 id、每个非根节点恰好被一个父节点认领、无自环、focus 必指向存在的节点。

**CI 单开一步 "Test the accessibility feature"**（blocking，每 job 26→27 步）：
默认 `cargo test` 不会编译这个 feature，不单开就等于三平台上全是死代码——
run #82 已经踩过一次"进了矩阵却没真跑"的坑。

**第三层：Windows 平台适配器已接通，macOS/Linux 未接。**

Windows 走 `accesskit_windows::Adapter` + baseview 的 `WM_GETOBJECT`（不用
`SubclassingAdapter`：它要求窗口尚未可见，而 baseview 的嵌入窗口是带 `WS_VISIBLE`
创建的，会直接 panic）。要点：

- **适配器懒创建**。`Adapter::new` 会初始化 UIA，而绝大多数情况下没有任何辅助技术在跑；
  Windows 只有在真有人问时才发 `WM_GETOBJECT`，所以在那时才建。
- **`WM_GETOBJECT` 来自操作系统，在其中 panic 会带走宿主**。因此 handler 的
  `RefCell` 一律用 `try_borrow_mut`，借不到就回"没有可描述的内容"，让
  `DefWindowProc` 去答，而不是 unwind 穿过系统回调。
- 每帧绘制后调 `update_if_active`——它只在真有客户端连着时才回调工厂，因此没人监听时
  不会白白重建树。放在 `on_frame` **之后**也是必须的：那次借用结束了，发布要再借一次。
- `winapi` 与 `windows` crate 的 `HWND`/`WPARAM`/`LPARAM` 是不同类型，边界处**显式转换**
  而不是 transmute。
- **action 未接**：屏幕阅读器能读、还不能改。`ActionHandler` 如实什么都不做（trait 要求
  不支持的 action 必须无动作），这样只读支持不必等 action 管线。

macOS/Linux 侧：`ViewState::accessibility_tree()` 已经产出树，但 baseview 的
AppKit/X11 后端还没有把它交给 `accesskit_macos`/`accesskit_unix`，因此这两个平台上
feature 打开也只是多算一棵树、无人消费——**如实记为未接通，不是"已支持"**。

**尚未做到的验收**：hosted job 目前只证明这条链在三平台**编译并通过单测**
（新增 CI 步骤同时构建 `sunmao_gui`、`sunmao` facade 与 fixture 的 feature 版本）。
真正的宿主侧断言——用 runner 既有的 UIA 机制反查插件暴露的元素——还需要把 fixture
以该 feature 打包进矩阵，是下一轮的事。

## M5 Wayland：受阻链已缩短为一条

此前我把 Wayland 记为"三个事实叠加"。其中一条已经消失，如实更新：

1. **VST3 根本没有 Wayland 平台类型** —— 仍然成立。`vst3_sys/src/gui/iplugview.rs:39-42`
   转录自上游的平台类型只有 `HWND`/`NSView`/`UIView`/`X11EmbedWindowID`。VST3 插件在
   Wayland 桌面上一律经 **XWayland**，这是规范现状，不由本项目决定。
2. ~~CLAP 要求 Wayland 用浮动窗口，而我们没有浮动窗口~~ —— **前半仍成立，后半已不成立**。
   上游 `clap/ext/gui.h` 对 `CLAP_WINDOW_API_WAYLAND` 的原文确实是
   *"embed is currently not supported, use floating windows"*，但**浮动窗口本轮已经交付**
   （见「M4 受阻项」第 1 条）。这一环不再是障碍。
3. **baseview 没有 Wayland 后端** —— 仍然成立，且现在是**唯一**的障碍。
   `baseview/src/lib.rs:6` 只有 `mod x11`，全树零 Wayland 引用。

**因此 Wayland 现在是一件具体的事，而不是一条链**：给 baseview 写 Wayland 后端
（`wl_surface`、`xdg_shell`、EGL 表面、`wl_seat`+xkbcommon 输入、`wl_output` 缩放），
并给 CI 装一个无头 compositor（Ubuntu job 目前跑的是 Xvfb ＝ X11）才谈得上验收。
交付后只有 CLAP 受益。这仍是独立立项的规模，但比之前清楚得多。

**现状不是"不能在 Wayland 上用"**：经 XWayland，X11 路径在 Wayland 桌面上照常工作，
这也正是目前 Ubuntu hosted job 验证的那条路径。

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
