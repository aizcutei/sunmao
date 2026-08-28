# Phase 2 VST3 ↔ CLAP 语义映射

每个 Phase 2 host-facing 能力在两种格式落地时，本表记录语义差异与降级行为。
"降级"指一种格式缺少对应概念时 SunMao 统一 API 的行为；严禁静默丢事件。

| 能力 | SunMao API | VST3 | CLAP | 差异与降级 |
|---|---|---|---|---|
| transport/timing | `ProcessContext` 的 `tempo`/`is_playing`/`is_recording`/`is_loop_active`/`sample_pos`/`time_signature`/`song_pos_beats`/`song_pos_seconds`/`bar_start_beats`/`bar_number`/`loop_beats`（M1 落地） | `ProcessData.processContext`（state flags 标记各字段有效性） | `clap_process.transport`（`clap_event_transport`，flags 同样按位标记） | 两侧字段均可缺失，统一 API 一律 `Option`，`None` 表示"宿主未提供"而非 0。单位已归一：CLAP beattime 定点（`CLAP_BEATTIME_FACTOR`）与 VST3 `TQuarterNotes` 都换算为**四分音符** `f64`。宿主完全不给 transport（CLAP `transport` 为 NULL、VST3 `process_context` 为 NULL）时全部音乐字段为 `None` 且 `is_playing` 保持 true。测试：`clap_transport_fields_reach_the_plugin`、`vst3_transport_fields_reach_the_plugin`、两侧 `a_host_without_a_*_leaves_every_musical_field_absent` |
| bar 序号 | `ProcessContext::bar_number` | 无对应字段（只有 `bar_position_music`，小节的音乐起点） | `clap_event_transport.bar_number` | **VST3 降级为 `None`**；`bar_start_beats` 两侧都可用，需要序号的插件应改用它或自行累计。测试：`vst3_transport_fields_reach_the_plugin` 显式断言 `bar_number == None` |
| loop 区间 | `ProcessContext::loop_beats`（四分音符） | `cycle_start_music`/`cycle_end_music` + `kCycleValid`/`kCycleActive` | `loop_start_beats`/`loop_end_beats` + `CLAP_TRANSPORT_IS_LOOP_ACTIVE` | 两侧一致：loop 未激活或区间非法（`end <= start`、非有限）时为 `None`，绝不下发负长度区间。CLAP 另有秒制 loop 字段，统一 API 只承诺 beats（两格式均原生提供），秒制保留在 `clap_rs::Transport::loop_seconds`。测试：`loop_region_requires_an_active_loop_and_a_sane_range`、`cycle_region_requires_an_active_cycle_and_a_sane_range` |
| 秒制位置 | `ProcessContext::song_pos_seconds` | 无秒字段，由 `project_time_samples / sample_rate` 推导 | `song_pos_seconds`（定点） | VST3 侧为推导值而非宿主原值；`sample_rate <= 0` 时 `None` |
| latency | （M2 设计中） | `IAudioProcessor::getLatencySamples`，变更需 `IComponentHandler::restartComponent(kLatencyChanged)` | `clap.latency` 扩展 + `host_latency.changed()`（只能在重启/去激活窗口生效） | 变更通知时机不同：统一 API 定义"仅在非 processing 状态可变" |
| tail | （M2 设计中） | `IAudioProcessor::getTailSamples`（`kInfiniteTail` 表无限） | `clap.tail`（`i32::MAX` 表无限） | 无限值编码不同，backend 归一化为枚举 |
| offline render | （M2 设计中） | `ProcessSetup.processMode`（realtime/prefetch/offline） | `clap.render`（realtime/offline） | VST3 多 prefetch 模式：映射到统一 API 的 realtime，并在 semantics 测试断言 |
| 多 bus/sidechain | （M3 设计中） | `getBusCount/getBusInfo/activateBus/setBusArrangements`，sidechain 为 `kAux` bus | `clap.audio-ports`（`is_main` 区分）+ `audio-ports-config` | VST3 有显式 aux 语义；CLAP 靠端口顺序/名称约定：统一 API 显式标注 `BusRole::Sidechain` |
| speaker layout | （M3 设计中） | `SpeakerArrangement` 位图 | `clap.audio-ports` 的 `port_type` + channel count（surround 扩展另有位图） | 一期只承诺 mono/stereo 协商，surround 枚举预留 |
| 参数 modulation | （M4 设计中） | 无原生概念（automation 即值变化） | `CLAP_EVENT_PARAM_MOD`（相对偏移，不落 state） | VST3 降级：mod 事件叠加为临时值变化且不写回 state；必须测试两侧 state 不受 mod 污染 |
| per-note expression | （M4 设计中） | `INoteExpressionController` + `NoteExpressionValueEvent`（note id 关联） | `CLAP_EVENT_NOTE_EXPRESSION`（note_id/port/channel/key 匹配） | note id 生命周期不同；统一 API 定义 SunMao note id 并在 backend 双向映射 |
| voice-info | （M4 设计中） | 无对应接口 | `clap.voice-info` | VST3 降级：能力查询返回 None，宿主测试断言不崩溃 |
| 版本化 state | （M5 设计中） | `IComponent::setState/getState` 字节流 | `clap.state` 字节流 | 两侧都是不透明字节流：版本头由 SunMao 层定义，格式无关 |
| preset 载入 | （M5 设计中） | program lists / `IUnitInfo`（宿主驱动） | `clap.preset-load`（宿主给路径/位置） | 模型差异大：一期统一 API 只承诺"插件侧载入回调 + 状态应用"，VST3 program list 映射为可选实现 |

## 记录规则

- 每个 milestone 落地时把对应行的"设计中"替换为最终 API 名，并补充实测差异。
- 单元/宿主测试必须覆盖表中每条降级行为；测试名写入对应行。
