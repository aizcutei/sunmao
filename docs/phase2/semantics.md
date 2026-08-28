# Phase 2 VST3 ↔ CLAP 语义映射

每个 Phase 2 host-facing 能力在两种格式落地时，本表记录语义差异与降级行为。
"降级"指一种格式缺少对应概念时 SunMao 统一 API 的行为；严禁静默丢事件。

| 能力 | SunMao API | VST3 | CLAP | 差异与降级 |
|---|---|---|---|---|
| transport/timing | （M1 设计中） | `ProcessData.processContext`（state flags 标记各字段有效性） | `clap_process.transport`（`clap_event_transport`，flags 同样按位标记） | 两侧字段均可缺失：统一 API 用 `Option` 表达；单位差异（VST3 quarter notes / CLAP beattime 定点）在 backend 归一化 |
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
