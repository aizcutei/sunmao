# SunMao 长期路线图

本路线图以 VST3/CLAP/standalone 跨平台基础能力稳定为前提；AU 不参与第一阶段验收。

## Phase 1：跨平台基础插件矩阵（当前）

同一份 SunMao Rust 插件实现输出 VST3、CLAP 与 standalone，在 macOS、Windows、Linux 上完成 effect + instrument 基础处理、MIDI、Float/Int/Bool 参数、sample-offset automation、reset、版本化参数 state、无分配实时回调、Cocoa/Win32/X11 的 GL/WGPU/WebView 插件 GUI 与 standalone 顶层窗口生命周期、target-aware 打包器和 CLI/GUI 测试宿主。以 native hosted CI 和可下载 artifacts 作为最终证据；AU、Wayland、物理 Linux 音频设备、ScreenCaptureKit 实捕获和真实 DAW GUI automation recording 明确延期。

Phase 1 已完成：扩展后的 VST3 + CLAP + standalone gate 由 run #25（commit `c8401e6`，https://github.com/aizcutei/sunmao/actions/runs/33152642714 ）在三平台 hosted native jobs 上验收，artifacts 已上传；历史 VST3/CLAP baseline 为 run #21（commit `885d2a5`）。任何后续 ABI、生命周期、GUI、standalone、packager 或 runner 变更仍必须在新 commit 上重新通过三平台 hosted jobs 并重新上传证据；本地或历史 run 不能替代该 revalidation gate。Phase 2 实现目标自此可以创建。

## Phase 2：高级插件契约与实时性

增加多 audio bus/sidechain、动态 routing 与 speaker layout、modulation/per-note expression、transport/timing 完整模型、latency/tail/offline render、voice-info、plugin-owned state、preset 与 migration；继续以无分配、无阻塞 audio thread 为约束，并加入 fuzz/property tests。standalone 先保持 Phase 1 的基础设备/窗口契约，不在本阶段扩展为完整 DAW host。

## Phase 3：框架、DSP 与组件库

稳定插件构造 API、参数分组/嵌套/smoothing、effect/instrument templates；建立 filters、envelopes、oscillators、oversampling、mixing、metering 等可组合 DSP 组件，并定义版本兼容策略。

## Phase 4：GUI 组件库与平台完善

完善布局、主题、text rendering、accessibility、clipboard、IME/国际键盘、cursor/focus、scale negotiation、floating CLAP editor；明确 renderer 资源和线程归属，在 X11 生命周期稳定后加入 Wayland。

## Phase 5：完整测试宿主与外部兼容

把 `sunmao_unittest_runner` 扩展为交互式 standalone host、批量 regression/fuzz host、性能和泄漏检测；接入 CLAP/VST3 validator、代表性 DAW smoke，输出兼容性报告。

## Phase 6：发布工程

建立 SDK/模板/项目生成器与文档版本策略；加入 macOS/Windows 签名、notarization、installer/package manager、universal/multi-arch 和可复现 release artifacts。

## Phase 7：AU 恢复

待统一 core、state、GUI ownership、packager 和 CI 成熟后，单独重审 AU v2：清理 Objective-C runtime/stub workaround，补齐 AU state/幅度语义、Cocoa view discovery/lifecycle，以 macOS-only CI、`auval` 和真实 DAW 证据验收。AU 永不改变 VST3/CLAP 跨平台 gate。
