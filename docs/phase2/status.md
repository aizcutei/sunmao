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
| M1 transport/timing | core `ProcessContext` 扩展 ↔ VST3 `ProcessContext`/CLAP `clap_event_transport` | 进行中；**`_sys` 层无缺口**：`clap_sys` 有 tsig_num/denom、bar_start/bar_number、loop_start/end（beats+seconds）与 8 个 `CLAP_TRANSPORT_*` flag；`vst3_sys::ProcessContext` 有 tempo、time_sig、bar_position_music、cycle_start/end_music 与全部 valid 位。缺口在 `_rs` 与 core：`clap_rs::Transport`（`clap_rs/src/process.rs:511`）只暴露 tempo/is_playing/song_pos_seconds/song_pos_beats；`sunmao_core::ProcessContext`（`sunmao/core/src/plugin.rs:22`）只有 sample_rate/tempo/is_playing/sample_pos | 底层盘点见本行 | 扩展 `clap_rs::Transport` 与 vst3_rs 侧读取，设计统一 Transport 结构（字段可缺失 → `Option`，单位在 backend 归一化） |
| M2 latency/tail/render | 上报 + 变更通知 + render 模式 | 未开始；底层现状：vst3_rs plugin trait 已有 `latency()`/`tail()`，clap_rs 已有 latency/tail/render 扩展包装；sunmao/core 未暴露 | — | core API + backend 桥接 |
| M3 bus/sidechain/layout | 多 bus 声明/激活/协商 | 未开始；底层现状：clap_sys audio_ports(-config/-activation)、surround 齐全，clap_rs audio_ports 部分包装；vst3 bus arrangement 在 wrapper 内固定 stereo | — | core bus 模型设计 |
| M4 expression/voice-info | modulation/MPE/voice-info | 未开始；底层现状：clap_rs 已有 `voice_info()`；VST3 INoteExpressionController binding 未确认 | — | 底层 binding 盘点 |
| M5 state/preset/migration | 版本化 state 升级、preset 载入 | 未开始；底层现状：clap_sys 有 preset_load/state_context；Phase 1 已有参数 state round-trip | — | state 版本头设计 |
| M6 总验收 | property/fuzz、CI blocking 化 | 未开始 | — | — |

## 完成规则

在同一 commit 上，三平台 hosted jobs 全绿（含 Phase 1 全部既有 gate 与 Phase 2
新增 blocking 步骤）、四个 acceptance fixture 的两格式契约测试全绿、semantics.md
覆盖全部已落地能力、artifacts 可下载后，才把本文件状态改为 "Phase 2 完成"。
任何本地结果必须标注平台和证据等级。
