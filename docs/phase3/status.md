# Phase 3 状态

更新时间：2026-08-28

## 目标与边界

Phase 1（run #25 / commit `c8401e6`）与 Phase 2 核心（run #38 / commit
`77f788c`，M0–M6 各自三平台 hosted 绿）之上，完成框架构造 API 与 DSP 组件库，
并收口 Phase 2 的 7 项遗留。目标能力：

- Phase 2 收口：bus 激活/去激活回调、speaker layout 动态协商、runner 宿主侧
  latency/tail/多 bus 断言、backend 层 expression/mod 端到端测试、
  `migrate_state` backend 接线、preset-load 统一路径、无界 fuzz 脚手架
- 参数分组/嵌套（VST3 `IUnitInfo` ↔ CLAP module 路径）、零分配参数 smoothing、
  effect/instrument template（新插件样板 ≤50 行）
- `sunmao/dsp` crate：filters（一阶/SVF/biquad）、envelopes（ADSR/follower）、
  band-limited oscillators（sine/saw/pulse），纯 no-alloc process API
- 2x/4x oversampling（latency 接入 Phase 2 契约）、dry/wet 与增益工具、
  peak/RMS metering（无锁发布）
- semver/state 兼容策略文档

明确不做：AU 契约扩展（不进默认 feature/gate）、Wayland、完整 GUI toolkit、
卷积/FFT 级 DSP、外部 preset 格式解析。

## 硬门槛（每个 milestone 与最终验收通用）

- 同一 commit 三平台 hosted native jobs（macOS ARM64、Windows x86_64、Ubuntu
  x86_64）全绿并上传 artifacts；本地结果只作开发证据。
- Phase 1 + Phase 2 全部既有 CI 步骤保持 blocking 且绿色。
- 每个 host-facing 能力：VST3 与 CLAP 同时落地；语义差异与降级行为写入
  `docs/phase2/semantics.md`（附测试名）；公共 API 进 `sunmao/core` 或新 crate
  并入 prelude（带 doc-test）。
- audio callback 成功路径零 alloc/realloc/dealloc；新增不变量补 proptest。

## Acceptance fixtures

| fixture | crate | 验收能力 | M0 骨架状态 |
|---|---|---|---|
| Grouped Params Synth | `examples/sunmao_syn_grouped_params` | M2 参数分组/smoothing/instrument template | 单音 sine + 一阶 LP + 线性 AR 包络，参数平铺（前缀标记未来分组），4 单元测试（macOS 本地通过） |
| SVF Filter | `examples/sunmao_fx_svf` | M3 sunmao/dsp filter 组件替换 | inline TPT SVF（LP/BP/HP），6 单元测试含稳定性/边界（macOS 本地通过） |
| OS Distortion | `examples/sunmao_fx_os_dist` | M4 oversampling + latency 契约 | 无 oversampling 的 tanh waveshaper，latency 固定 0，6 单元测试（macOS 本地通过） |
| Meter | `examples/sunmao_fx_meter` | M4 无锁 metering 发布 | passthrough + AtomicU32 位存 peak/RMS，6 单元测试含跨线程读取（macOS 本地通过） |
| Layout Gain | `examples/sunmao_fx_layout_gain` | M1 第 2 项 speaker layout 动态协商 | M1 新增（非 M0 骨架）：发布 mono/stereo 两个 `BusConfig`，5 单元测试（macOS 本地通过） |

M3/M4 的验收方式是把骨架的 inline DSP 换成 `sunmao/dsp` 组件且**测试语义不变**；
M2 的验收方式是 grouped synth 换用分组 + smoothing + template 后测试仍绿。

## Milestone 矩阵

| Milestone | 范围 | 当前判断 | 权威证据 | 下一步 |
|---|---|---|---|---|
| M0 脚手架 | 文档、4 fixtures、workspace/CI 骨架 | **完成**（三平台 hosted 全绿） | [run #41](https://github.com/aizcutei/sunmao/actions/runs/33167456623)（commit `9f65af5`）三 job success，"Test Phase 3 acceptance fixtures" 三平台均 success，artifacts 齐备 | — |
| M1 Phase 2 收口 | 7 项遗留（见 phase2/status.md 遗留表），每项完成即更新该表 | 进行中：**2/7 项已验收**（第 1 项 bus 激活/去激活回调 11 测试；第 2 项 speaker layout 动态协商 12 测试；各自三平台 hosted 全绿）。原盘点（2026-08-28）已确认 `_sys` 层无缺口（`clap_sys::clap_plugin_audio_ports_activation_t`、`audio_ports_config` 全家族、`vst3_sys::IComponent::activate_bus`），缺口在 `_rs`/core，activation 与 `audio_ports_config`（layout 协商）两侧现均已补齐 | 第 1 项：[run #42](https://github.com/aizcutei/sunmao/actions/runs/33171119003)（commit `b78aca6`）；第 2 项：[run #44](https://github.com/aizcutei/sunmao/actions/runs/33174187893)（commit `1478189`）。两次均三 job success、全部 blocking 步骤零非成功、artifacts 齐备 | 第 3 项 runner 宿主侧断言（latency/tail 查询、多 bus 拓扑枚举、sidechain 送信号验证路由） |
| M2 参数系统与构造 API | 分组/嵌套、smoothing、effect/instrument template | 未开始 | — | 读 vst3_sys `IUnitInfo` 与 clap_sys params module 路径约定 |
| M3 DSP 基础组件 | `sunmao/dsp`：filters/envelopes/oscillators | 未开始 | — | 新 crate + SVF fixture 换组件实现 |
| M4 oversampling/mixing/metering | 2x/4x oversampling、dry-wet/增益、peak/RMS metering | 未开始 | — | oversampler latency 接 Phase 2 契约并被 runner 断言 |
| M5 版本兼容策略与总验收 | semver/state 兼容文档、proptest/文档收尾 | 未开始 | — | — |

## 完成规则

在同一 commit 上，三平台 hosted jobs 全绿（含 Phase 1+2 全部既有 gate 与
Phase 3 新增 blocking 步骤）、Phase 2 遗留表 7 项全部关闭、4 个 Phase 3
fixture 完成各自 milestone 演进且测试绿、semantics.md 覆盖全部已落地能力、
artifacts 可下载后，才把本文件状态改为 "Phase 3 完成"。任何本地结果必须标注
平台和证据等级。

## 当前状态

- M0 完成：hosted run #41（commit `9f65af5`）三平台 native job 全绿，Phase 1+2
  既有 gate 与新增的 "Test Phase 3 acceptance fixtures" 步骤同时 success，
  artifacts `phase1-macOS-ARM64`（48.7MB）、`phase1-Windows-X64`（73.3MB）、
  `phase1-Linux-X64`（911.8MB）可下载。
- M1 进行中：第 1、2 项均已验收。第 2 项（speaker layout 动态协商）hosted
  run #44（commit `1478189`）三平台 native job 全绿、全部 blocking 步骤零非成功，
  artifacts `phase1-macOS-ARM64`（49.3MB）、`phase1-Windows-X64`（73.7MB）、
  `phase1-Linux-X64`（914.5MB）可下载。第 1 项（bus 激活/去激活回调）hosted run #42
  （commit `b78aca6`）三平台 native job 全绿，三个 job 的全部 blocking 步骤零非
  成功（Phase 1+2 既有 gate 与 "Test Phase 3 acceptance fixtures" 同时 success），
  artifacts `phase1-macOS-ARM64`（48.8MB）、`phase1-Windows-X64`（73.4MB）、
  `phase1-Linux-X64`（912.1MB）可下载；#37 的 Windows WGPU 收尾段错误未复现。
  其余 5 项未开始，下一个瓶颈是第 3 项 runner 宿主侧断言。
- 分支：`phase3/framework-dsp-library`（自 main `2df01ce` 切出）。
