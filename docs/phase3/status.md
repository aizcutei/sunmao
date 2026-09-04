# Phase 3 状态

更新时间：2026-09-04

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
| OS Distortion | `examples/sunmao_fx_os_dist` | M4 oversampling + latency 契约 | M4 演进：tanh waveshaper 跑在 `sunmao/dsp` 4x `Oversampler` 内，dry/wet 用 `DryWet` 在过采样域内混合（避免 dry/wet 相对延迟成梳状），latency 由 `OversamplingFactor::latency_samples()`（24）上报且在 prepare 之前即可回答，8 单元测试含群延迟对齐与 13kHz 混叠抑制（macOS 本地通过） |
| Meter | `examples/sunmao_fx_meter` | M4 无锁 metering 发布 | M4 演进：换用 `sunmao/dsp` `Meter`/`MeterHandle`（原子位存发布、-20 dB/s 峰值回落、100 ms RMS 时间常数），handle 在构造期即可取得（编辑器可先于激活打开），增益经 `db_to_gain` 以 dB 表达，10 单元测试含跨线程读取（macOS 本地通过） |
| Layout Gain | `examples/sunmao_fx_layout_gain` | M1 第 2 项 speaker layout 动态协商 | M1 新增（非 M0 骨架）：发布 mono/stereo 两个 `BusConfig`，5 单元测试（macOS 本地通过） |
| Template Effect / Instrument | `examples/sunmao_template_{effect,instrument}` | M2 新插件样板 | **两者均达标**（effect 42 行、instrument 49 行，预算 ≤50）。M5 期间收口：`#[param(default/range/name)]` 让 derive 生成 `Default`、`SunmaoPlugin::IS_INSTRUMENT` 一次声明两个宿主开关、facade 的 `MonoVoice::render`（按事件 sample offset 分段渲染）、`AudioBuffer::fill_mono{,_range}`。instrument 现已进打包矩阵，由 runner 的 synth 套件在两格式上实测（历史：M2 时 86 行、M3 组件化后 81 行，均未达标）。行数预算由 `sunmao/tests/template_size.rs` 机械强制 |

M3/M4 的验收方式是把骨架的 inline DSP 换成 `sunmao/dsp` 组件且**测试语义不变**；
M2 的验收方式是 grouped synth 换用分组 + smoothing + template 后测试仍绿。

## Milestone 矩阵

| Milestone | 范围 | 当前判断 | 权威证据 | 下一步 |
|---|---|---|---|---|
| M0 脚手架 | 文档、4 fixtures、workspace/CI 骨架 | **完成**（三平台 hosted 全绿） | [run #41](https://github.com/aizcutei/sunmao/actions/runs/33167456623)（commit `9f65af5`）三 job success，"Test Phase 3 acceptance fixtures" 三平台均 success，artifacts 齐备 | — |
| M1 Phase 2 收口 | 7 项遗留（见 phase2/status.md 遗留表），每项完成即更新该表 | **完成**：7/7 项全部验收（各自三平台 hosted 绿）（第 1 项 bus 激活/去激活回调 11 测试；第 2 项 speaker layout 动态协商 12 测试；第 3 项 runner 宿主侧断言 3 测试 + 套件 16→19、打包 20→24 套件；各自三平台 hosted 全绿）。原盘点（2026-08-28）已确认 `_sys` 层无缺口（`clap_sys::clap_plugin_audio_ports_activation_t`、`audio_ports_config` 全家族、`vst3_sys::IComponent::activate_bus`），缺口在 `_rs`/core，activation 与 `audio_ports_config`（layout 协商）两侧现均已补齐 | 第 1 项：[run #42](https://github.com/aizcutei/sunmao/actions/runs/33171119003)（commit `b78aca6`）；第 2 项：[run #44](https://github.com/aizcutei/sunmao/actions/runs/33174187893)（commit `1478189`）；第 3 项：[run #47](https://github.com/aizcutei/sunmao/actions/runs/33177884493)（commit `0e79bd2`）。三次均三 job success、全部 blocking 步骤零非成功、artifacts 齐备 | M1 七项全部落地后进入 M2（参数分组/smoothing/template） |
| M2 参数系统与构造 API | 分组/嵌套、smoothing、effect/instrument template | **完成**（三项均三平台 hosted 全绿）（各自三平台 hosted 全绿）。smoothing 的指数 ramp 不终止与 `is_smoothing` 残余偏移两个缺陷由新增 proptest 抓出并修正（详见 progress.md）。自 `_sys` 补起：`vst3_sys` 新增 `IUnitInfo` 绑定（IID 自上游头文件转录），并修正 `clap_rs` 把 `info.module` 无条件清零的既有缺陷 | 分组：[run #57](https://github.com/aizcutei/sunmao/actions/runs/33189876572)（commit `94b3542`）；smoothing：[run #59](https://github.com/aizcutei/sunmao/actions/runs/33192422381)（commit `d0a13d3`）；template：[run #61](https://github.com/aizcutei/sunmao/actions/runs/33194134348)（commit `3374c5f`）。三次均三 job success、零非成功步骤、artifacts 齐备 | — （M2 完成；instrument 模板 86 行未达 ≤50，待 M3 的 oscillator/envelope 落地后重测） |
| M3 DSP 基础组件 | `sunmao/dsp`：filters/envelopes/oscillators | **完成**：filters/envelopes/oscillators 三家族齐备且三平台 hosted 全绿。filters（三平台 hosted 全绿；一阶/SVF/biquad + 4 proptest；SVF fixture 已换组件实现且**测试零改动**通过；顺带修掉 f32 biquad 低 cutoff DC 增益 14% 失准）。**envelopes/oscillators 已验收**（ADSR/follower、PolyBLEP sine/saw/pulse，+3 proptest；instrument 模板换用组件后 86→81 行，**仍未达 ≤50**，原因见 progress.md） | filters：[run #63](https://github.com/aizcutei/sunmao/actions/runs/33196120193)（commit `9db6ff0`）三 job success、零非成功步骤、artifacts 齐备 | — （M3 完成；进入 M4） |
| M4 oversampling/mixing/metering | 2x/4x oversampling、dry-wet/增益、peak/RMS metering | **完成**（三平台 hosted 全绿）：`sunmao/dsp` 新增 `oversampling`（2x/4x 级联半带 FIR，33 tap 使 4x 群延迟为整数 24）、`mixing`（dB 换算、`apply_gain`、线性/等功率 `DryWet`）、`metering`（`Meter`/`MeterHandle` 原子发布）三模块 + 7 proptest；两个 fixture 换组件实现；runner 新增第 19 项 `latency_alignment`（两格式读 latency 后以冲激实测峰值帧，容差 1）。proptest 抓出 `EqualPower` 全湿时 `cos(π/2)` 为 -4.4e-8 的负 dry 增益并修正 | [run #68](https://github.com/aizcutei/sunmao/actions/runs/33867319967)（commit `04d036a`）三 job success，每 job 25 步零非成功，"Test Phase 3 acceptance fixtures" 与两个 "Package and exercise ..." 步骤三平台均 success，artifacts `phase1-macOS-ARM64`（52.3MB）、`phase1-Windows-X64`（78.1MB）、`phase1-Linux-X64`（960.1MB）可下载 | — （M4 完成；进入 M5） |
| M5 版本兼容策略与总验收 | semver/state 兼容文档、proptest/文档收尾 | **进行中**：`docs/phase3/compatibility.md` 落地（API 面 / 破坏性定义 / `sunmao_dsp` 数值语义承诺 / 弃用流程 / state 格式与载入规则 / 何时升 `STATE_VERSION` / 验证方式），README 与 roadmap 已链接；proptest +8 把该文件的 state 与 id 承诺机械钉住（两 `_rs` 各 4 / 3 项 + core 3 项，含 CLAP 端到端"被拒的 state 不触达插件"）。核对代码时修正文档两处失实（CLAP blob 存 plain value 而非归一化值，stepped 参数写步进索引；因此两格式 blob 不通用不只因 magic）。**并修掉一个由 proptest 抓出的真实 DSP 缺陷**：`flush_denormal` 被独立施加到耦合递推的每个状态上会破坏其衰减——`Svf` 归零由 43k 样本劣化到 683 万样本（慢 158 倍），`Biquad` 永久停在 6.2e-20 极限环；两者改为成组 flush（阈值提为 prelude 里的 `DENORMAL_FLOOR`，带 doc-test），并把该测试从"400k 样本后残留 < 1e-18"改写为按各滤波器离散极点半径算预算的 `every_filter_settles_within_its_own_time_constant`，cutoff 改对数均匀采样（原均匀采样命中该角落的概率约 1/4000，这正是缺陷长期潜伏的原因） | 本地：macOS ARM64 123 套件 / 504 测试全绿、Windows 交叉 check、打包 28 套件各 20/20（见 progress.md）——本地证据等级 | 推送后等待三平台 hosted；随后收口 instrument 模板行数目标再做总验收 |

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
  `phase1-Linux-X64`（912.1MB）可下载；
  第 3 项（runner 宿主侧断言）已验收——hosted run #47（commit `0e79bd2`）三平台
  全绿、零非成功步骤，artifacts 齐备；该项当即发现并修复了 CLAP 激活期间 latency/
  tail 上报 0 的真实缺陷。第 4 项（backend 层 expression/mod 端到端映射测试）已验收——hosted
  run #49（commit `03a53eb`）三平台全绿、零非成功步骤；该项同样当即发现并修复了一个
  更严重的缺陷：**VST3 note expression 从未真正到达插件**（backend 在回调之后 clear
  事件队列，Phase 2 M4 标记完成时即已失效）。第 5 项（`migrate_state` backend 接线）已验收——hosted
  run #51（commit `7999b73`）三平台全绿、零非成功步骤；同样发现根因不在 core：两个
  `_rs` 层把 state 版本硬编码为 1，插件声明的 `STATE_VERSION` 从未写入或比对，
  `migrate_state` 因此永远不可能触发。第 6 项（preset 载入）已验收——hosted run #53
  （commit `e1455dd`）三平台全绿、零非成功步骤；CLAP 侧完整落地，VST3 program list
  按边界不实现（`vst3_sys` 无相应绑定，VST3 宿主经 `setState` 走状态应用那一半），
  已在 semantics.md 记为单格式能力。第 7 项（无界 fuzz 脚手架）已验收——hosted run #55（commit `f0c2f2e`）三平台全绿；`fuzz/` 排除出 workspace，本地实跑 300 万例无崩溃。**M1 收口完成，进入 M2。**
- M4 已验收：hosted run #68（commit `04d036a`）三平台 native job 全绿，每个 job 25 个步骤
  零非成功，artifacts 齐备。该 run 也是 Windows/Linux 首次在 CI 打包并 exercise
  `sunmao_fx_os_dist` 与 `sunmao_fx_meter`（"Package and exercise VST3 + CLAP + standalone"
  三平台 success），`latency_alignment` 断言因此不再只走 skip 路径。另外，run #66 修掉的
  Windows WGPU 收尾段错误在本 run 的 "Package and exercise native GUI backends" 上**再次
  未复现**（连续两次绿）——但仍按"间歇性失败单次绿不构成证明"处理，留待 M5 收尾时决定是否
  把该项从"已知 flake"降级为"已修复"。
- M5 进行中：兼容策略文档与其 proptest 守卫已落地（见 M5 行），等待三平台 hosted。
- **M2 的"新插件样板 ≤50 行"已收口**（Phase 3 唯一一条写进 milestone 却长期未满足的目标）：
  instrument 从 81 行降到 **49 行**、effect 从 50 降到 42，靠的是四个对任何插件都有用的
  API 而非删注释——`#[param(default/range/name)]`（derive 据此生成 `Default`）、
  `SunmaoPlugin::IS_INSTRUMENT`、`MonoVoice`（facade 层，按事件 sample offset 渲染）、
  `AudioBuffer::fill_mono{,_range}`。`template_size.rs` 已从"钉住 81 行并提示差距"改为
  直接断言两者都在预算内。把 instrument 接进打包矩阵后，runner 立刻在真实宿主里抓出两个
  单测看不到的缺陷（模板从未实现 `reset()`；runner 自己的 note-off 断言只接受"瞬间静音"
  的无包络合成器），详见 progress.md。
- 分支：`phase3/framework-dsp-library`（自 main `2df01ce` 切出）。
