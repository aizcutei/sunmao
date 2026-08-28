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
| bus 激活/去激活回调 | M3 | **已实现**（Phase 3 M1，三平台 hosted 全绿：[run #42](https://github.com/aizcutei/sunmao/actions/runs/33171119003)，commit `b78aca6`，artifacts 齐备）：`SunmaoPlugin::set_bus_active` ↔ VST3 `IComponent::activateBus` / CLAP `clap.audio-ports-activation/2`，两侧按声明校验索引，插件拒绝如实上报，见 semantics.md 的"bus 激活/去激活"行 |
| speaker layout 动态协商 | M3 | **已实现**（Phase 3 M1，三平台 hosted 全绿：[run #44](https://github.com/aizcutei/sunmao/actions/runs/33174187893)，commit `1478189`，artifacts 齐备）：`SunmaoPlugin::bus_configs()`/`current_bus_config()`/`select_bus_config()` ↔ VST3 `setBusArrangements` 真实协商（按提议通道数在已发布布局中查找）/ CLAP `clap.audio-ports-config` + `audio-ports-config-info/1`，见 semantics.md 的"speaker layout 动态协商"行。fixture `sunmao_fx_layout_gain`；12 测试（含跨格式可达性 proptest）三平台绿 |
| runner 宿主侧 latency/tail/多 bus 断言 | M2/M3 | **已实现**（Phase 3 M1，三平台 hosted 全绿：[run #47](https://github.com/aizcutei/sunmao/actions/runs/33177884493)，commit `0e79bd2`，artifacts 齐备）：`HostPlugin::reported_latency()`/`reported_tail()`/`audio_buses()` 两格式实现 + runner 新增 3 个测试（`latency_tail`/`bus_topology`/`sidechain_routing`，套件 16→19 项）；`sunmao_fx_tempo_delay` 与 `sunmao_fx_sidechain_comp` 并入打包与 CI runner 调用，否则断言只会走 skip 分支。**该断言当即发现并修复了一个真实缺陷**（CLAP 激活期间 latency/tail 上报 0，见 semantics.md latency 行）。24 套件各 19/19（三平台）|
| backend 层 expression/mod 端到端映射测试 | M4 | **已实现**（Phase 3 M1，三平台 hosted 全绿：[run #49](https://github.com/aizcutei/sunmao/actions/runs/33180224229)，commit `03a53eb`，artifacts 齐备）：两格式各一个端到端测试（原始宿主事件走真实 `IEventList`/`clap_input_events_t` → core 队列）。**该测试当即发现 VST3 note expression 从未真正到达插件**（backend 在回调后 clear 队列），已修复并记入 semantics.md |
| backend 在 state load 后回调 `migrate_state` | M5 | **已实现**（Phase 3 M1，三平台 hosted 全绿：[run #51](https://github.com/aizcutei/sunmao/actions/runs/33182548781)，commit `7999b73`，artifacts 齐备）：`_rs` 两层改为写入/比对**插件自己的** `STATE_VERSION`（此前硬编码 1，插件即便声明 v2 也写成 1，`migrate_state` 根本不可能触发），载入后经新增的 `Plugin::state_loaded(from_version)` 回调 backend，再转 `SunmaoPlugin::migrate_state`；仅在 `from_version < STATE_VERSION` 时触发，同版本不迁移、更高版本拒绝。两格式各一个走真实 `IBStream`/`clap_istream_t` 的端到端测试 |
| `clap.preset-load` / VST3 program list | M5 | **已实现**（Phase 3 M1，三平台 hosted 全绿：[run #53](https://github.com/aizcutei/sunmao/actions/runs/33184652119)，commit `e1455dd`，artifacts 齐备）：统一为 `SunmaoPlugin::load_preset(PresetLocation)` + `SUPPORTS_PRESET_LOAD`，CLAP 侧经 `clap.preset-load/2`（含 draft 别名）落地；**VST3 program list 按边界不实现**——`vst3_sys` 无 `IUnitInfo`/`IProgramListData` 绑定，VST3 宿主经 `setState` 走"状态应用"那一半，不存在静默丢事件。见 semantics.md 的"preset 载入"行 |
| 时间无界 fuzz | M6 | **已实现**（Phase 3 M1，三平台 hosted 全绿：[run #55](https://github.com/aizcutei/sunmao/actions/runs/33186447997)，commit `f0c2f2e`，artifacts 齐备）：`fuzz/` crate（**workspace `exclude`**，gate 永不构建/运行），fuzz body 在 `src/lib.rs` 由稳定版随机 driver 与 `cargo-fuzz` target 共用；目标是两格式的 state 解码（任意字节 → 真实 `clap.state` load / `IComponent::setState`）。入口写入根 README 与 `fuzz/README.md` |

## 完成规则

在同一 commit 上，三平台 hosted jobs 全绿（含 Phase 1 全部既有 gate 与 Phase 2
新增 blocking 步骤）、四个 acceptance fixture 的两格式契约测试全绿、semantics.md
覆盖全部已落地能力、artifacts 可下载后，才把本文件状态改为 "Phase 2 完成"。
任何本地结果必须标注平台和证据等级。

## 当前状态

- **Phase 2 完成（按上述完成规则）。** Hosted run #38（commit `77f788c`，https://github.com/aizcutei/sunmao/actions/runs/33164763166 ）在同一 commit 上三平台 native jobs 全绿：Phase 1 全部既有 gate 保持绿色（含 GUI matrix、standalone、packager、runner），Phase 2 新增的 `Test Phase 2 acceptance fixtures` 步骤三平台 success，proptest 随 `sunmao_core` 进 gate，artifacts `phase1-macOS-ARM64`（51.1MB）、`phase1-Windows-X64`（76.9MB）、`phase1-Linux-X64`（956.1MB）可下载。
- M0–M6 各自都在**独立 commit 上取得过三平台 hosted 绿**（run #27/#29/#31/#33/#35/#38），不是一次性合并验收。
- **7 项遗留已于 Phase 3 M1 全部关闭**（各自在独立 commit 上取得三平台 hosted 绿：run #42/#44/#47/#49/#51/#53/#55）。收口过程中发现并修复了两个"标记完成但实际失效"的契约缺陷：**VST3 note expression 从未真正到达插件**（backend 在回调后 clear 事件队列）与 **CLAP 在插件激活期间 latency/tail 上报 0**（activate 把插件 take 走后回落 `unwrap_or(0)`）；另修正 `clap_rs` 对所有端口固定上报 `port_type=stereo`，以及两个 `_rs` 层把 state 版本硬编码为 1 导致 `migrate_state` 永不可能触发。详见 docs/phase3/progress.md。
- 已知 flake：run #37 的 Windows WGPU GUI 步骤在全部断言通过并打印 "Done." 后以 exit 139 结束（收尾期段错误）；同一路径在 #27/#29/#31/#33/#35/#38 六轮通过，未复现。若再次出现应深入 WGPU/D3D 析构路径。
