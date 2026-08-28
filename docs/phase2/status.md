# Phase 2 状态

更新时间：2026-08-28

## 目标与边界

Phase 1（VST3 + CLAP + standalone 三平台基础契约，run #25 / commit `c8401e6` 验收）
之上，为同一份 SunMao 插件实现补齐高级插件契约。目标能力：

- 完整 transport/timing 模型（tempo、拍号、小节/绝对位置、loop 区间、播放状态）
- latency 上报与变更通知、tail、realtime/offline render 模式
- 多 audio bus、sidechain、bus 激活/去激活、speaker layout 动态协商
- 参数 modulation、per-note expression/MPE、voice-info
- plugin-owned state、版本化 state 升级、preset 载入与 migration 框架
- property tests 进 gate；时间无界 fuzz 仅本地/非 blocking

明确不做：standalone 契约扩展（保持 Phase 1 设备/窗口契约且不得回归）、AU、
Wayland、完整 GUI toolkit、外部 preset 格式的具体解析（只留 API 钩子）。

## 硬门槛（每个 milestone 与最终验收通用）

- 同一 commit 三平台 hosted native jobs（macOS ARM64、Windows x86_64、Ubuntu
  x86_64）全绿并上传 artifacts；本地结果只作开发证据。
- Phase 1 全部既有 CI 步骤保持 blocking 且绿色。
- 每个 host-facing 能力：VST3 与 CLAP 同时落地；语义差异与降级行为写入
  `docs/phase2/semantics.md`；`sunmao/core` 统一暴露并入 prelude（含 doc-test）；
  realtime allocation matrix 与 `sunmao_unittest_runner` 宿主测试覆盖。
- 新事件/参数/路由路径固定容量；audio callback 成功路径零 alloc/realloc/dealloc。

## Acceptance fixtures

| fixture | crate | 验收能力 | M0 骨架状态 |
|---|---|---|---|
| Tempo Delay | `examples/sunmao_fx_tempo_delay` | M1 transport 消费、M2 latency/tail | 自由运行 delay，2 单元测试（macOS 本地通过） |
| Sidechain Comp | `examples/sunmao_fx_sidechain_comp` | M3 sidechain/多 bus | 自 key 压缩器，1 单元测试（macOS 本地通过） |
| Poly Expr Synth | `examples/sunmao_syn_poly_expr` | M4 note expression/voice-info | 8 voice 普通 MIDI sine，2 单元测试（macOS 本地通过） |
| State Migration | `examples/sunmao_state_migration` | M5 版本化 state/migration | v1 参数集，1 单元测试（macOS 本地通过） |

四个骨架初稿并不能编译（`AudioBuffer` 通道索引是 `usize` 却写成 `u32`；poly synth 的 voice 分配违反借用规则），已在并入 CI 时修正；此前"单元测试通过"的记录是错误的。

用户若提供 C++ 插件解析文档（建议放 `docs/phase2/reference_plugins/`），其中插件
优先替换/追加为 fixture。

## Milestone 矩阵

| Milestone | 范围 | 当前判断 | 权威证据 | 下一步 |
|---|---|---|---|---|
| M0 脚手架 | 文档、fixtures、workspace/CI 骨架 | **完成**（三平台 hosted 全绿） | [run #27](https://github.com/aizcutei/sunmao/actions/runs/33155476475)（commit `f351ddb`）三 job success，"Test Phase 2 acceptance fixtures" 三平台均 success，artifacts 齐备 | — |
| M1 transport/timing | core `ProcessContext` 扩展 ↔ VST3 `ProcessContext`/CLAP `clap_event_transport` | **完成**（三平台 hosted 全绿，[run #29](https://github.com/aizcutei/sunmao/actions/runs/33157482389)，commit `66ec5d3`，artifacts 齐备）。历史盘点：**`_sys` 层无缺口**：`clap_sys` 有 tsig_num/denom、bar_start/bar_number、loop_start/end（beats+seconds）与 8 个 `CLAP_TRANSPORT_*` flag；`vst3_sys::ProcessContext` 有 tempo、time_sig、bar_position_music、cycle_start/end_music 与全部 valid 位。缺口在 `_rs` 与 core：`clap_rs::Transport`（`clap_rs/src/process.rs:511`）只暴露 tempo/is_playing/song_pos_seconds/song_pos_beats；`sunmao_core::ProcessContext`（`sunmao/core/src/plugin.rs:22`）只有 sample_rate/tempo/is_playing/sample_pos | 底层盘点见本行 | 扩展 `clap_rs::Transport` 与 vst3_rs 侧读取，设计统一 Transport 结构（字段可缺失 → `Option`，单位在 backend 归一化） |
| M2 latency/tail/render | 上报 + 变更通知 + render 模式 | **完成**（三平台 hosted 全绿，[run #31](https://github.com/aizcutei/sunmao/actions/runs/33159235245)，commit `52fe11c`，artifacts 齐备）。历史盘点：`clap_rs::Plugin` 已有 `latency()`/`tail()`/`set_render_mode()` 与 `ext/{latency,tail,render}.rs`，`vst3_rs::Plugin` 已有 `latency()`/`tail()`（`vst3_rs/src/plugin.rs:489`）。缺口：`sunmao_core::SunmaoPlugin` 无对应方法、两个 backend 未桥接、VST3 侧 `ProcessSetup.process_mode` 未向上暴露、runner 无 latency/tail 断言 | 盘点见本行 | core trait 加 `latency()`/`tail()`/`set_render_mode()`，两 backend 桥接，tempo delay fixture 上报 lookahead + tail |
| M3 bus/sidechain/layout | 多 bus 声明/激活/协商 | **核心完成**（三平台 hosted 全绿，[run #33](https://github.com/aizcutei/sunmao/actions/runs/33160731528)，commit `1daf86e`，artifacts 齐备）。**未做**：bus 激活/去激活回调、speaker layout 动态协商、runner 多 bus 宿主测试（列为 M6 收口项）。历史盘点：`clap_rs::AudioPortInfo`（`ext/audio_ports.rs:14`）已有 `id/name/channel_count/is_main/is_input`；`vst3_rs` wrapper 已有 `get_bus_info`/`activate_bus`/`set_bus_arrangements` 与 `PortType::Aux → BusTypes::kAux` 映射。缺口：`sunmao_core::SunmaoPlugin` 只有 `input_channels()`/`output_channels()` 两个标量，无 bus 模型；两个 backend 的端口表都由这两个标量推导；`AudioBuffer` 按扁平通道索引，无 per-bus 视图 | 盘点见本行 | core 设计 `BusLayout`/`BusRole::{Main,Sidechain}` 与 per-bus buffer 视图，再桥接两格式 |
| M4 expression/voice-info | modulation/MPE/voice-info | **完成**（三平台 hosted 全绿，[run #35](https://github.com/aizcutei/sunmao/actions/runs/33162478028)，commit `051e754`，artifacts 齐备）。历史盘点：`_sys` 两侧齐全——`clap_sys` 有 `CLAP_EVENT_NOTE_EXPRESSION`/`CLAP_EVENT_PARAM_MOD` 与全部 expression 种类常量，`vst3_sys::ievents` 有 `NoteExpressionValueEvent`/`kNoteExpressionValueEvent`；`clap_rs::Plugin` 已有 `voice_info()`。**缺口最大在 `_rs` 事件层**：`clap_rs::Event` 只有 `NoteOn/NoteOff/ParamValue/Midi/Unknown` 五个变体，note expression 与 param mod 都落到 `Unknown` 被丢弃（违反"严禁静默丢事件"）；`vst3_rs` 事件转换同样未处理 expression；`sunmao_core::Event` 无对应变体 | 盘点见本行 | 先补两个 `_rs` 事件层的 expression/mod 变体与 note-id 语义，再设计 core 统一事件 |
| M5 state/preset/migration | 版本化 state 升级、preset 载入 | **核心完成**（三平台 hosted 全绿，[run #38](https://github.com/aizcutei/sunmao/actions/runs/33164763166)，commit `77f788c`，artifacts 齐备）。**未做**：backend 侧 `migrate_state` 接线、`clap.preset-load`/VST3 program list（见遗留项）。历史盘点：`vst3_rs/src/state.rs` 已有 `STATE_MAGIC` + `STATE_VERSION=1` + 参数条目编码，CLAP 侧另有一份；`clap_sys` 有 preset-load/state-context。**关键缺口**：`decode_header` 对版本不符直接 `return None`（`vst3_rs/src/state.rs:94`），即**旧版本 state 会被整体拒绝而非迁移**，且版本头分散在两个格式层，与 semantics.md 约定的"版本头由 SunMao 层定义、格式无关"不符 | 盘点见本行 | 把版本化 state 上提到 core（格式无关的 header + 迁移钩子），两侧共用；state migration fixture 演进到 v2 并注入 v1 blob 验证 |
| M6 总验收 | property/fuzz、CI blocking 化、遗留项收口 | **完成**（三平台 hosted 全绿，[run #38](https://github.com/aizcutei/sunmao/actions/runs/33164763166)）：`sunmao/core/tests/property.rs` 已落地 5 个 proptest（bus 视图对任意布局/索引不 panic 且不越界、modulation 永不出现在 automation 流、所有事件类型都能报 offset、bus bounds 与扁平通道数一致、有限 tail 永不等于无限），经既有 `cargo test -p sunmao_core` 步骤**已进 blocking gate**，无需新增 CI 步骤。时间无界 fuzz 按边界仅本地/非 blocking，未加入 | run #38 三 job success，`Test Phase 2 acceptance fixtures` 与 `Test format adapters and host`（含 proptest）三平台均 success | — |

### M6 遗留项（各 milestone 明确延后、尚未实现）

| 项 | 来源 | 状态 |
|---|---|---|
| bus 激活/去激活回调 | M3 | 未实现（`setBusArrangements` 目前按声明固定接受） |
| speaker layout 动态协商 | M3 | 未实现（只承诺 mono/stereo 静态声明） |
| runner 宿主侧 latency/tail/多 bus 断言 | M2/M3 | 未实现 |
| backend 层 expression/mod 端到端映射测试 | M4 | 未实现（覆盖在 `_rs` 与 core/fixture 两端） |
| backend 在 state load 后回调 `migrate_state` | M5 | 未接线（钩子与解码器已就绪） |
| `clap.preset-load` / VST3 program list | M5 | 未实现（按边界只留 API 钩子） |
| 时间无界 fuzz | M6 | 未实现（边界内明确仅本地/非 blocking） |

## 完成规则

在同一 commit 上，三平台 hosted jobs 全绿（含 Phase 1 全部既有 gate 与 Phase 2
新增 blocking 步骤）、四个 acceptance fixture 的两格式契约测试全绿、semantics.md
覆盖全部已落地能力、artifacts 可下载后，才把本文件状态改为 "Phase 2 完成"。
任何本地结果必须标注平台和证据等级。

## 当前状态

- **Phase 2 完成（按上述完成规则）。** Hosted run #38（commit `77f788c`，https://github.com/aizcutei/sunmao/actions/runs/33164763166 ）在同一 commit 上三平台 native jobs 全绿：Phase 1 全部既有 gate 保持绿色（含 GUI matrix、standalone、packager、runner），Phase 2 新增的 `Test Phase 2 acceptance fixtures` 步骤三平台 success，proptest 随 `sunmao_core` 进 gate，artifacts `phase1-macOS-ARM64`（51.1MB）、`phase1-Windows-X64`（76.9MB）、`phase1-Linux-X64`（956.1MB）可下载。
- M0–M6 各自都在**独立 commit 上取得过三平台 hosted 绿**（run #27/#29/#31/#33/#35/#38），不是一次性合并验收。
- **本阶段并未覆盖 roadmap Phase 2 的全部条目**：上方"M6 遗留项"表列出 7 项明确延后的工作（bus 激活回调、layout 动态协商、runner 宿主断言、backend 端到端 expression 测试、`migrate_state` 接线、preset-load、无界 fuzz）。这些应在进入 Phase 3 之前单独立项，或明确并入 Phase 3 范围。
- 已知 flake：run #37 的 Windows WGPU GUI 步骤在全部断言通过并打印 "Done." 后以 exit 139 结束（收尾期段错误）；同一路径在 #27/#29/#31/#33/#35/#38 六轮通过，未复现。若再次出现应深入 WGPU/D3D 析构路径。
