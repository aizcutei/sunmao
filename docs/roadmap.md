# SunMao 长期路线图

本路线图以 VST3/CLAP/standalone 跨平台基础能力稳定为前提；AU 不参与第一阶段验收。

## Phase 1：跨平台基础插件矩阵（当前）

同一份 SunMao Rust 插件实现输出 VST3、CLAP 与 standalone，在 macOS、Windows、Linux 上完成 effect + instrument 基础处理、MIDI、Float/Int/Bool 参数、sample-offset automation、reset、版本化参数 state、无分配实时回调、Cocoa/Win32/X11 的 GL/WGPU/WebView 插件 GUI 与 standalone 顶层窗口生命周期、target-aware 打包器和 CLI/GUI 测试宿主。以 native hosted CI 和可下载 artifacts 作为最终证据；AU、Wayland、物理 Linux 音频设备、ScreenCaptureKit 实捕获和真实 DAW GUI automation recording 明确延期。

Phase 1 已完成：扩展后的 VST3 + CLAP + standalone gate 由 run #25（commit `c8401e6`，https://github.com/aizcutei/sunmao/actions/runs/33152642714 ）在三平台 hosted native jobs 上验收，artifacts 已上传；历史 VST3/CLAP baseline 为 run #21（commit `885d2a5`）。任何后续 ABI、生命周期、GUI、standalone、packager 或 runner 变更仍必须在新 commit 上重新通过三平台 hosted jobs 并重新上传证据；本地或历史 run 不能替代该 revalidation gate。Phase 2 实现目标自此可以创建。

## Phase 2：高级插件契约与实时性（核心已完成）

增加多 audio bus/sidechain、动态 routing 与 speaker layout、modulation/per-note expression、transport/timing 完整模型、latency/tail/offline render、voice-info、plugin-owned state、preset 与 migration；继续以无分配、无阻塞 audio thread 为约束，并加入 fuzz/property tests。standalone 先保持 Phase 1 的基础设备/窗口契约，不在本阶段扩展为完整 DAW host。

进展：transport/timing、latency/tail/offline render、多 bus/sidechain、modulation/per-note expression/voice-info、版本化 state 与迁移已落地，六个里程碑各自在独立 commit 上取得三平台 hosted 绿（run #27/#29/#31/#33/#35/#38），property tests 已进 gate；验收状态与证据见 `docs/phase2/status.md`，两格式语义差异与降级见 `docs/phase2/semantics.md`。**尚未覆盖**：bus 激活/去激活回调、speaker layout 动态协商、runner 宿主侧断言、`migrate_state` 的 backend 接线、`clap.preset-load`/VST3 program list、无界 fuzz——见 status.md 的"M6 遗留项"表，需在 Phase 3 之前单独立项或并入 Phase 3。

## Phase 3：框架、DSP 与组件库（已完成）

稳定插件构造 API、参数分组/嵌套/smoothing、effect/instrument templates；建立 filters、envelopes、oscillators、oversampling、mixing、metering 等可组合 DSP 组件，并定义版本兼容策略。

进展：**Phase 3 完成**——总验收 [run #69](https://github.com/aizcutei/sunmao/actions/runs/33940874765)
（commit `b45efea`）在同一 commit 上三平台 hosted native jobs 全绿、每 job 25 步零非成功、
artifacts 可下载。Phase 2 的 7 项遗留已全部收口，参数分组/嵌套与 smoothing、`sunmao/dsp` 的
filters/envelopes/oscillators/oversampling/mixing/metering 均已落地，五个里程碑分别在独立
commit 上取得三平台 hosted 绿（run #41/#42/#44/#47/#49/#51/#53/#55/#57/#59/#61/#63/#66/#68/#69）。
API 与 state 的兼容策略见 [`docs/phase3/compatibility.md`](phase3/compatibility.md)，
验收状态与证据见 `docs/phase3/status.md`。"新插件样板 ≤50 行"已达标（effect 42 行、
instrument 49 行，由 `sunmao/tests/template_size.rs` 机械强制）。

## Phase 4：GUI 组件库与平台完善（M0–M3 完成；M4/M5 部分完成，两项受阻）

完善布局、主题、text rendering、accessibility、clipboard、IME/国际键盘、cursor/focus、scale negotiation、floating CLAP editor；明确 renderer 资源和线程归属，在 X11 生命周期稳定后加入 Wayland。

进展：**M0–M3 各自在独立 commit 上取得三平台 hosted 绿**（run #73 / #75 / #77 / #84）——
renderer 归属文档与两格式 scale/DPI 协商、`Column`/`Row` 声明式布局与六个控件与
`ParamBinder` 双向绑定与主题 token、字体栅格化/度量与 clipboard 与 IME/国际键盘与
焦点模型。**M4 的可视化与 accessibility 树**（无锁 `VizChannel` 三缓冲、
`SpectrumAnalyzer`、renderer 无关的 accessibility 描述树）已由
[run #86](https://github.com/aizcutei/sunmao/actions/runs/33980911401) 三平台绿验收；
**M5 的 GUI 兼容策略**（`docs/phase3/compatibility.md` §2bis）与 proptest/文档收尾亦已落地。

**两项未交付，都不是工期问题，需单独立项：**

1. **floating CLAP editor 与 Wayland 卡在同一块 baseview 工作上。** vendored baseview 只有
   `open_parented` 与阻塞式 `open_blocking`，没有非阻塞顶层窗口模式；且 Linux 后端 X11 独占。
   而 CLAP 上游明确说 Wayland **不支持嵌入、必须用浮动窗口**，VST3 则**根本没有 Wayland
   平台类型**（只有 `X11EmbedWindowID`，故 VST3 在 Wayland 上一律走 XWayland）。
   即：Wayland 原生 = baseview Wayland 后端 + 非阻塞顶层窗口，且只有 CLAP 能受益。
2. **accessibility 的三份 OS 桥接未做。** 框架侧的描述树已完成并测试，但发布给
   UIA / NSAccessibility / AT-SPI 是三份互不复用的原生实现；**runner 也无法验收**——
   两个插件格式都没有 accessibility 通道，它走 OS API。

依据与文件行号见 `docs/phase4/status.md` 的「M4 受阻项」与「M5 Wayland 受阻链」。

## Phase 5：完整测试宿主与外部兼容

把 `sunmao_unittest_runner` 扩展为交互式 standalone host、批量 regression/fuzz host、性能和泄漏检测；接入 CLAP/VST3 validator、代表性 DAW smoke，输出兼容性报告。

## Phase 6：发布工程

建立 SDK/模板/项目生成器与文档版本策略；加入 macOS/Windows 签名、notarization、installer/package manager、universal/multi-arch 和可复现 release artifacts。

## Phase 7：AU 恢复

待统一 core、state、GUI ownership、packager 和 CI 成熟后，单独重审 AU v2：清理 Objective-C runtime/stub workaround，补齐 AU state/幅度语义、Cocoa view discovery/lifecycle，以 macOS-only CI、`auval` 和真实 DAW 证据验收。AU 永不改变 VST3/CLAP 跨平台 gate。
