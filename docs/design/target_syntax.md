# Sunmao 目标语法设计文档

> **状态：设计目标，非现有 API。** 本文描述 Sunmao 期望达到的用户侧语法，用于指引框架演进方向。
> 下表是截至 Phase 3 完成（commit `b45efea`）逐项核对过的实现现状——**写代码前先查这张表**，
> 表中标 ✗ 的名字在仓库里不存在，照抄会编译失败。

## 实现现状对照

| 文档中的写法 | 现状 | 真实 API |
|---|---|---|
| `#[derive(Params)]` + `#[param(name/range/default/unit)]` | ✓ 已实现 | 同名可用 |
| `FloatParam` / `IntParam` / `BoolParam` | ✓ 已实现 | `sunmao_core::params` |
| `EnumParam<T>` / `#[derive(Enum)]` | ✗ 未实现 | 暂用 `IntParam` 表达离散档位 |
| `#[id = "..."]` 显式参数 id | ✗ 主动拒绝 | derive 宏遇到 `#[id]` 或 `#[param(id = ...)]` 直接报编译错误，要求删掉；id 串取自字段名，再由 `stable_param_id` FNV 哈希成 `u32` |
| （文档未提）参数分组 | ✓ 已实现 | `#[group = "..."]`，宿主侧对应 VST3 `IUnitInfo` / CLAP module 路径 |
| `param.value()` / `param.smoothed()` | ✗ 名称不符 | `param.get()`；平滑用 `Smoother` + `SmoothingStyle` |
| `sunmao::export!` | ✗ 名称不符 | `sunmao::sunmao_export!` |
| `const AUDIO_IO: AudioIoLayout` | ✗ 未实现 | `fn input_channels()` / `fn output_channels()` / `fn accepts_midi()` |
| `fn initialize(&mut self, &AudioConfig)` | ✗ 签名不符 | `fn initialize(&mut self, sample_rate: f64, max_block_size: u32)` |
| `type Editor` / `EditorView` / `Column`/`Row`/`.child()` | ✗ 未实现 | `fn view() -> Option<Box<dyn SunmaoView>>`；控件为命令式 `Widget`/`Layout` |
| `BiquadFilter` | ✗ 名称不符 | `Biquad` + `BiquadKind`（另有 `OnePole`、`Svf`） |
| `StftProcessor` / `FeedbackNetwork` / `DelayLine` | ✗ 未实现 | 无 |
| `VizChannel` | ✗ 未实现 | 计量数据用 `Meter` / `MeterHandle` 无锁发布 |
| `buffer.process_wet_dry(...)` | ✗ 未实现 | `DryWet` + `MixLaw` |

`sunmao_dsp` 现有全部组件：`OnePole`/`Svf`/`Biquad`、`Adsr`/`EnvelopeFollower`、
`Oscillator`/`Waveform`、`Oversampler`/`OversamplingFactor`、`DryWet`/`MixLaw`/`apply_gain`/
`db_to_gain`/`gain_to_db`、`Meter`/`MeterHandle`、`flush_denormal`/`DENORMAL_FLOOR`。

声明式 GUI（`EditorView`、`Column`/`Row`、`Knob::param` 自动绑定）是 **Phase 4** 的目标，
见 `docs/roadmap.md`。

---

# Sunmao Framework - 目标语法设计文档

> 本文档定义 Sunmao 音频插件框架的目标语法（target syntax），用于指引 AI Agent 进行框架实现。所有 API 为**设计目标**，尚未实现。

---

## 目录

1. [设计原则](#设计原则)
2. [核心概念](#核心概念)
3. [完整示例：STFT + Feedback Reverb 插件](#完整示例)
4. [参数系统 (Params)](#参数系统)
5. [DSP 组件库](#dsp-组件库)
6. [声明式 GUI 框架](#声明式-gui-框架)
7. [进阶：DSP Graph DSL](#进阶dsp-graph-dsl)
8. [可视化数据共享](#可视化数据共享)

---

## 设计原则

1. **简洁优先** —— 用户以最少的样板代码表达意图，框架承担 buffer 管理、平滑、overlap-add 等脏活。
2. **类型安全** —— 编译期确定 buffer 尺寸（const 泛型）、参数类型、IO 布局。
3. **声明式 GUI** —— 参数与控件自动双向绑定，无需手写事件回调。
4. **渐进复杂度** —— 简单插件保持极简；复杂插件按需引入 STFT/FDN/Graph 等高级组件。
5. **无锁实时安全** —— audio 线程与 GUI 线程之间通过框架提供的无锁通道通信。

---

## 核心概念

| 概念 | 说明 |
|------|------|
| `SunmaoPlugin` | 插件主 trait，定义元信息、处理逻辑、GUI 关联 |
| `Params` | 派生宏定义的参数结构，自动实现序列化/自动化 |
| `AudioBuffer` | 音频缓冲区，提供高层处理方法 |
| `ProcessContext` | 采样率、transport、block 信息 |
| DSP 组件 | `StftProcessor`、`FeedbackNetwork`、`DelayLine`、`BiquadFilter` 等 |
| `EditorView` | 声明式 GUI 视图 trait |
| `VizChannel` | audio→GUI 的无锁可视化数据通道 |

---

## 完整示例

带 GUI 的多参数 STFT + Feedback Reverb 插件。

```rust
use sunmao::prelude::*;

// ============ 参数定义 ============
#[derive(Params)]
struct ReverbParams {
    #[id = "mix"]
    #[param(name = "Dry/Wet", range = 0.0..=1.0, default = 0.5, smooth = 50.ms())]
    mix: FloatParam,

    #[id = "size"]
    #[param(name = "Room Size", range = 0.0..=1.0, default = 0.7, smooth = 100.ms())]
    size: FloatParam,

    #[id = "damp"]
    #[param(name = "Damping", range = 0.0..=1.0, default = 0.3)]
    damping: FloatParam,

    // 频域处理开关
    #[id = "freeze"]
    #[param(name = "Freeze")]
    freeze: BoolParam,

    // 带单位和显示格式的参数
    #[id = "predelay"]
    #[param(name = "Pre-Delay", range = 0.0..=200.0, default = 20.0, unit = "ms")]
    predelay: FloatParam,

    // 离散选择
    #[id = "mode"]
    #[param(name = "Mode", default = ReverbMode::Hall)]
    mode: EnumParam<ReverbMode>,
}

#[derive(Enum, PartialEq, Clone, Copy)]
enum ReverbMode {
    #[name = "Room"] Room,
    #[name = "Hall"] Hall,
    #[name = "Plate"] Plate,
    #[name = "Shimmer"] Shimmer,
}

// ============ 插件状态 ============
struct SunmaoReverb {
    params: Arc<ReverbParams>,

    // 开箱即用的 DSP 组件
    stft: StftProcessor<2048, 512>,          // 窗口大小 2048, hop 512
    feedback: FeedbackNetwork<8>,            // 8 路 FDN
    predelay_line: DelayLine,
    lowpass: BiquadFilter,

    // audio→GUI 可视化通道
    scope: VizChannel<SpectrumFrame>,
}

impl Default for SunmaoReverb {
    fn default() -> Self {
        Self {
            params: Arc::new(ReverbParams::default()),
            stft: StftProcessor::hann(),           // 默认 Hann 窗
            feedback: FeedbackNetwork::hadamard(), // Hadamard 矩阵混合
            predelay_line: DelayLine::seconds(0.2),
            lowpass: BiquadFilter::lowpass(8000.0, 0.707),
            scope: VizChannel::new(),
        }
    }
}

// ============ 插件实现 ============
impl SunmaoPlugin for SunmaoReverb {
    const NAME: &'static str = "Sunmao Reverb";
    const VENDOR: &'static str = "My Company";
    const URL: &'static str = "https://example.com";
    const AUDIO_IO: AudioIoLayout = AudioIoLayout::stereo();

    type Params = ReverbParams;
    type Editor = ReverbEditor;   // 关联 GUI，见下方

    fn params(&self) -> Arc<Self::Params> {
        self.params.clone()
    }

    // 采样率变化 / 初始化时调用，用于分配 buffer
    fn initialize(&mut self, config: &AudioConfig) {
        self.stft.prepare(config.sample_rate);
        self.feedback.prepare(config.sample_rate, config.max_block_size);
        self.predelay_line.prepare(config.sample_rate);
        self.lowpass.prepare(config.sample_rate);
    }

    fn reset(&mut self) {
        self.stft.reset();
        self.feedback.reset();
        self.predelay_line.clear();
    }

    // --- 时域处理路径 ---
    fn process(
        &mut self,
        buffer: &mut AudioBuffer,
        _events: &EventQueue,
        ctx: &ProcessContext,
    ) -> ProcessStatus {
        // 参数平滑值可以按 block 或 per-sample 读取
        let mix = self.params.mix.smoothed();
        let size = self.params.size.smoothed();
        let freeze = self.params.freeze.value();

        // 更新 DSP 组件参数
        self.feedback.set_decay(size);
        self.lowpass.set_cutoff(1000.0 + self.params.damping.value() * 15000.0);

        // 预延迟
        self.predelay_line.set_delay_ms(self.params.predelay.smoothed());

        // Dry/Wet 自动混合处理块
        buffer.process_wet_dry(mix, |wet| {
            self.predelay_line.process(wet);

            // STFT 域处理：闭包接收频谱帧
            self.stft.process(wet, |spectrum| {
                if freeze {
                    spectrum.freeze();
                } else {
                    // 对每个 bin 应用衰减
                    spectrum.for_each_bin(|bin, mag, phase| {
                        (mag * size, phase)
                    });
                }
                // 推送给 GUI 显示
                self.scope.push(spectrum.snapshot());
            });

            // 时域 feedback 网络
            self.feedback.process(wet);
            self.lowpass.process(wet);
        });

        ProcessStatus::Normal
    }
}

// ============ GUI 定义（声明式）============
#[derive(Editor)]
#[editor(size = (480, 320), resizable = true)]
struct ReverbEditor;

impl EditorView for ReverbEditor {
    type Params = ReverbParams;

    fn view(params: &Arc<ReverbParams>, cx: &mut Context) -> impl View {
        Column::new(cx)
            .gap(12.0)
            .padding(16.0)
            .child(
                Label::new(cx, "SUNMAO REVERB").font_size(20.0)
            )
            .child(
                // 一行放多个旋钮，自动绑定参数
                Row::new(cx)
                    .gap(20.0)
                    .child(Knob::param(cx, &params.mix))
                    .child(Knob::param(cx, &params.size))
                    .child(Knob::param(cx, &params.damping))
                    .child(Knob::param(cx, &params.predelay))
            )
            .child(
                Row::new(cx)
                    .gap(20.0)
                    .child(Dropdown::param(cx, &params.mode))
                    .child(Toggle::param(cx, &params.freeze).label("Freeze"))
            )
            // 频谱可视化组件（从 process 侧读取共享数据）
            .child(
                SpectrumAnalyzer::new(cx)
                    .height(120.0)
                    .source(SpectrumSource::PostProcess)
            )
    }
}

sunmao::export!(SunmaoReverb);
```

---

## 参数系统

### 设计说明

参数使用 **attribute 语法**而非位置构造函数，避免记忆参数顺序：

```rust
#[param(name = "Room Size", range = 0.0..=1.0, default = 0.7, smooth = 100.ms())]
size: FloatParam,
```

### 参数类型

| 类型 | 用途 |
|------|------|
| `FloatParam` | 浮点连续参数 |
| `IntParam` | 整数参数 |
| `BoolParam` | 开关 |
| `EnumParam<T>` | 离散枚举选择（需 `#[derive(Enum)]`） |

### 支持的 attribute 字段

| 字段 | 说明 | 可选 |
|------|------|------|
| `name` | 显示名称 | 否 |
| `range` | 取值范围 `min..=max` | Float/Int 必填 |
| `default` | 默认值 | 否 |
| `smooth` | 平滑时间，如 `50.ms()` | 是 |
| `unit` | 单位字符串，如 `"ms"`、`"dB"` | 是 |

### 值读取 API

```rust
param.value()      // 当前原始值（不平滑）
param.smoothed()   // 平滑后的值（block 或 per-sample）
```

### 枚举参数

```rust
#[derive(Enum, PartialEq, Clone, Copy)]
enum ReverbMode {
    #[name = "Room"] Room,
    #[name = "Hall"] Hall,
    #[name = "Plate"] Plate,
    #[name = "Shimmer"] Shimmer,
}
```

---

## DSP 组件库

框架提供开箱即用的 DSP 组件，隐藏 buffer 管理、overlap-add、循环缓冲等复杂度。

### StftProcessor

```rust
stft: StftProcessor<2048, 512>,   // const 泛型：窗口大小 2048，hop 512

StftProcessor::hann()             // Hann 窗构造
StftProcessor::hamming()          // Hamming 窗
StftProcessor::blackman()         // Blackman 窗

stft.prepare(sample_rate);
stft.reset();

// 处理：框架负责 windowing / FFT / overlap-add / FIFO
stft.process(buffer, |spectrum| {
    spectrum.for_each_bin(|bin_index, magnitude, phase| {
        (new_magnitude, new_phase)   // 返回修改后的值
    });
    spectrum.freeze();               // 冻结频谱
    let snap = spectrum.snapshot();
```

> **注：原文档到此截断**（`## DSP 组件库` 的 `StftProcessor` 一节写到一半）。
> 目录承诺的 `声明式 GUI 框架`、`进阶：DSP Graph DSL`、`可视化数据共享` 三节从未写出。
> 未代为补写——这些是设计决策，应由作者定。
