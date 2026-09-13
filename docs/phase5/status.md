# Phase 5 状态

更新时间：2026-09-13

## 目标与边界

在 Phase 1（run #25 / `c8401e6`）、Phase 2 核心（run #38 / `77f788c`）、Phase 3
（run #69 / `b45efea`）、Phase 4（run 34746764198 / `28cba05`）之上，把
`sunmao_unittest_runner` 从"CI 探针"扩成**完整测试宿主**，并接入外部 validator 与
真实 DAW，输出机器可读的兼容性证据。目标能力：

- 交互式 standalone host：加载已打包的 `.vst3`/`.clap`、枚举参数与 bus、改参数、
  存取 state/preset、开关编辑器
- 批量 regression host：固定种子 / 固定 buffer / 固定块划分的确定性批跑，输出可比对的
  音频与参数轨迹，golden 对拍并**显式定义浮点容差**
- 性能与泄漏检测：RT 安全检测从 audio 线程扩到 GUI 线程与宿主回调；开关编辑器 N 次、
  扫描-实例化-销毁 N 次的泄漏检测
- 外部 validator：`clap-validator` 与 Steinberg VST3 validator，三平台各自 blocking
- DAW smoke：至少一个可脚本化 DAW 在三平台加载/处理/存工程/重开

明确不做：AU 契约扩展（AU 不进默认 feature/gate，留待 Phase 7）、发布签名与安装器
（Phase 6）、新的 GUI 组件（Phase 4 已收口）。

## 硬门槛（每个 milestone 与最终验收通用）

- 同一 commit 三平台 hosted native jobs（macOS ARM64、Windows x86_64、Ubuntu x86_64）
  全绿并上传 artifacts；本地结果只作开发证据。
- Phase 1–4 全部既有 CI 步骤保持 blocking 且绿色。
- **job 全绿不等于断言跑过。** 本仓已两次踩到"三平台全绿而目标断言 0 次执行"
  （run #82 的 `GUI key verified`、run #86 的跨线程 viz 测试）。每项验收必须下载原始
  日志 grep 到实际断言行；新加的守卫还要**反向验证它真的会变红**。
- host-facing 能力 VST3 与 CLAP 同时落地，差异与降级写入 `docs/phase2/semantics.md`
  并附测试名；公共 API 入 prelude 带 doc-test。
- audio 回调成功路径零 alloc/零加锁；跨线程一律无锁通道。

## 分支基点

`phase5/test-host-compat` 从 `phase4/gui-component-library` 的尖端 `d9bed39` 切出。
`main` 仍落后于已验收的 Phase 3/4 工作（合并需仓库所有者决定，见
`docs/phase4/status.md` 的"分支基点"一节），本 phase **不自行推送 main**。

## 本地 gate 基线（M0 实测）

commit `d9bed39`（Phase 4 尖端，工作树干净），macOS ARM64 本地：

| 项目 | 命令 | 结果 |
|---|---|---|
| metadata | `cargo metadata --locked` | exit 0 |
| fmt | `cargo fmt --all -- --check` | exit 0 |
| whitespace | `git diff --check` | exit 0 |
| 测试 | `RUSTFLAGS=-Awarnings cargo test --locked` | exit 0，**135 套件 / 676 passed / 0 failed / 4 ignored** |
| 打包 | `tools/package_examples.sh --debug --test` | exit 0，**32 套件 / 640 passed / 0 failed**（本轮实测，与 Phase 4 M2 起的数字一致） |

**这组数字的用途**：纯重构提交应当与它**逐位相同**；任何偏移都必须在 progress.md 里
有对应的新增测试来解释。CI 上的数字更高且三平台各不相同（Phase 4 收尾实测
144 / 142 / 162 套件、833 / 811 / 882 passed），因为 hosted job 另跑 feature 组合与
平台专项——**不要拿 CI 数字和本地数字对比**。

## `sunmao_unittest_runner` 能力清单与缺口（M0 清点）

代码规模：`main.rs` 2797 行、`gui_window.rs` 3508、`gui.rs` 1062、
`host/vst3_host.rs` 2318、`host/clap_host.rs` 1538、`host/au_host.rs` 904、
`host/scanner.rs` 767、`host/mod.rs` 694，合计 **13588 行**；源码里 `#[test]` **47 处**，其中在 macOS 上实际运行 **44 个**（其余被平台 `cfg` 挡掉）——后一个数才是基线可比的数。

### 已有：子命令

| 子命令 | 能力 | 交互性 |
|---|---|---|
| `scan <dir>` / `scan --system` | 目录/系统扫描，列出插件类 | 非交互 |
| `info <plugin>` | 名称/厂商/版本/ID/通道数/类型/路径 | 非交互 |
| `test <plugin>` | 固定测试序列（见下），一次性跑完打印 PASS/FAIL | 非交互 |
| `process <plugin>` | 固定 1 秒 440Hz 正弦（合成器则一个 NoteOn），512 块，打印 peak/RMS | 非交互 |
| `gui` | 自带 GUI 界面 | 交互（但不是插件宿主控制台） |
| `gui-test [--auto-close] [--verify-pixels] [--verify-input …] <plugin>` | 编辑器生命周期、像素回读、输入注入 | 非交互 |
| `__macos-capture-window` / `__windows-uia-range-drag` | 平台内部 helper | 非交互 |

### 已有：`HostPlugin` trait（`host/mod.rs`）

`info` / `initialize` / `param_count` / `param_info` / `param_get` / `param_set` /
`process` / `process_with_events`（`NoteOn`/`NoteOff`/`ParamValue` 带 sample offset）/
`reported_latency` / `reported_tail` / `audio_buses`（`HostBusInfo`，含 main/aux 区分）/
`reset` / `save_state` / `load_state` / `shutdown` / `service_host_requests` /
`gui_gesture_evidence` / `open_gui` / `resize_gui` / `set_gui_scale` / `send_gui_key` /
`close_gui` / `plugin_library`。

**M1 需要的底层能力，绝大多数已经在这个 trait 里了**——缺的是把它们暴露成可交互的
命令面，而不是重写宿主。

### 已有：`test` 子命令覆盖的断言

load、initialize、params（含有效性）、process_silence / impulse / sine440 / sine1000 /
dc_offset、param_set_get、`run_parameter_automation_test`、reset、process_after_reset、
sample_rates（多采样率）、state_roundtrip、`run_latency_tail_test`、
`run_bus_topology_test`、`run_sidechain_routing_test`、
`run_reported_latency_alignment_test`、`run_synth_processing_tests`、shutdown。

### 缺口（逐条对应后续 milestone）

| # | 缺口 | 证据 | 归属 |
|---|---|---|---|
| 1 | **没有交互模式**。每个子命令都是"进程启动 → 跑死脚本 → 退出"，无法在一次会话里改一个参数再听结果 | `main.rs` 的 `match args[1]`，六个命令全部一次性返回 `bool` | M1 |
| 2 | **没有 preset 概念**。全仓 runner 源码 `grep -i preset` **零命中**：VST3 `.vstpreset` 与 CLAP preset-discovery 都没有宿主侧路径；state 只有裸字节 `save_state`/`load_state` | 见上 grep | M1 |
| 3 | **bus 枚举只在 `test` 内部消费**，没有面向人的枚举输出；`info` 只打印扁平通道数 | `println_plugin_info` 不碰 `audio_buses()` | M1 |
| 4 | **`process` 的激励是写死的**：固定 44100 / 1 秒 / 440Hz / 512 块，没有种子、没有块划分控制、没有参数轨迹 | `cmd_process` 内联常量 | M2 |
| 5 | **没有 golden 对拍**，也没有任何显式浮点容差；`process` 只打印 peak/RMS，无法跨平台逐位或带容差比对 | 同上 | M2 |
| 6 | **fuzz 完全不进 CI**。`fuzz/` 被根 `Cargo.toml` `exclude`，README 明说 "local-only and never part of CI"；`grep fuzz .github/workflows/*.yml` 零命中 | `fuzz/README.md`、workflow grep | M2 |
| 7 | **RT 安全检测只覆盖 audio 线程**。零分配断言是 fixture crate 内的 `GlobalAlloc` 计数器，宿主侧与 GUI 线程、宿主回调都没有等价守卫 | `examples/*` 的 `*_do_not_allocate` 测试 | M3 |
| 8 | **没有泄漏检测**。没有"开关编辑器 N 次"或"扫描-实例化-销毁 N 次"的循环与占用量断言 | runner 源码无对应循环 | M3 |
| 9 | **没有外部 validator**。`clap-validator` 与 Steinberg VST3 validator 均未接入 | workflow grep 零命中 | M4 |
| 10 | **没有 DAW smoke，也没有机器可读的兼容性报告 artifact**。现有 artifact 是构建产物与日志，不是结构化报告 | workflow 的 upload-artifact 步骤 | M5 |

### CI 现状（Phase 4 尖端）

`.github/workflows/phase1.yml` 单 workflow、**34 个 `- name:` 步骤**、三平台 matrix。
runner 在其中出现于三处：`cargo build -p sunmao_unittest_runner`、macOS 的
"default binaries are AU-free" `nm` 复查（runner 自身也查）、以及
"Package and exercise VST3 + CLAP + standalone" / "Package and exercise native GUI
backends" 两个 blocking 步骤里被当作宿主调用。

## Milestone 矩阵

| Milestone | 范围 | 当前判断 | 权威证据 | 下一步 |
|---|---|---|---|---|
| M0 脚手架与基线 | 建 `docs/phase5/{status,progress}.md`；清点 runner 能力与缺口；记录本地 gate 基线 | **完成**（三平台 hosted 全绿）：文档、能力清单与两条实测基线落地 | [run 34761153409](https://github.com/aizcutei/sunmao/actions/runs/34761153409)（commit `9cce371`）三 job success，每 job **34 步零非成功**（跳过项分别为 5/9/11，均为平台不适用者），三份 artifacts 可下载（Linux 1,000,635,181 / Windows 78,735,698 / macOS 54,190,684 bytes；macOS 一份已下载，`unzip -t` 报 No errors detected）。**该 commit 是纯文档提交，没有新增断言**，故 CI 对它能提供的证据仅限“Phase 1–4 既有 34 步仍 blocking 且绿” | — （M0 完成；进入 M1）|
| M1 交互式 standalone host | 加载已打包 `.vst3`/`.clap`、枚举参数与 bus、改参数、存取 state/preset、开关编辑器；既有非交互 CI 用法原样不变 | **完成**（三平台 hosted 全绿）：新增 `host` 子命令（行式命令语言，人可交互、管道可脚本化），`preset.rs` 按上游转录实现 `.vstpreset` 容器，`HostPlugin::class_id` 补上 VST3 class ID，CLAP 宿主不再对未知参数 ID 报成功。既有六个子命令未改行为（四处重复的扫描分派抽成 `scan_plugin_path`，分支逐字相同） | [run 34763956140](https://github.com/aizcutei/sunmao/actions/runs/34763956140)（commit `ac41dff`）三 job success，每 job **35 步零非成功**（新步骤 "Drive the interactive host over VST3 + CLAP" 三平台各 success），三份 artifacts 可下载（Linux 1,000,635,643 / Windows 78,737,182 / macOS 54,191,174 bytes；macOS 一份已下载，`unzip -t` 报 No errors detected，SHA-256 `2c01b112…1fa10`）。**三平台原始日志已下载并逐条 grep，且把 GitHub 回显的脚本正文（ANSI `36;1m` 前缀）剔除后计数**：每平台真实输出 `HOST COMMAND SURFACE VERIFIED` **1** 次、`rejected as it must be` **10** 次（十个反向用例逐个非零退出）、`HOST SESSION VERIFIED` **4** 次（两格式各一段 18 命令会话 + 两格式各一段 5 命令编辑器会话）、`editor opened`/`editor closed` 各 **4** 次 | — （M1 完成；两项新发现各自独立立项，见下）|
| M2 批量 regression host | 确定性批跑（固定种子/buffer/块划分）、音频与参数轨迹、golden 对拍 + 显式浮点容差、有界 fuzz 进 CI | **完成**（三平台 hosted 全绿）：`regress` 子命令与 `regress.rs`；goldens 入库 `tools/regression_goldens/`；两个新 blocking 步骤（golden 对拍、有界 fuzz）。块划分刻意不均匀且首尾钉死在 max/1 | [run 34766022434](https://github.com/aizcutei/sunmao/actions/runs/34766022434)（commit `c059e41`）三 job success，每 job **37 步零非成功**。三平台原始日志剔除脚本回显后逐条核实，**三平台数字完全一致**：`REGRESSION MATCHED GOLDEN` 各 2 次（两格式，各 147 项比对），**worst deviation 三平台均为 `0e0`**，`cross-format audio identical across all block records`、`REGRESSION GOLDENS VERIFIED`、`STATE DECODE FUZZ VERIFIED: 200000 cases` 各 1 次，`perturbed golden rejected`／`future-version trace rejected` 各 1 次。三份 artifacts 可下载（Linux 1,000,654,355 / Windows 78,753,789 / macOS 54,209,946 bytes；Windows 一份已下载，`unzip -t` 通过，SHA-256 `f36237e604a24860…`），且新增的 `host-session`/`regression`/fuzz 日志确已在包内 | — （M2 完成；进入 M3）|
| M3 性能与泄漏检测 | RT 安全检测扩到 GUI 线程与宿主回调；泄漏检测；基准与阈值写入本文件 | 未开始 | — | — |
| M4 外部 validator | `clap-validator` + Steinberg VST3 validator 三平台 blocking；失败项逐条归因 | 未开始 | — | — |
| M5 DAW smoke 与兼容性报告 | 可脚本化 DAW 三平台加载/处理/存工程/重开；机器可读兼容性报告 artifact | 未开始 | — | — |

## M1 抓到的两项新发现（各自独立立项）

两项都由"把宿主真正驱动一遍"暴露出来，都已在三平台 hosted 日志里取得硬件证据，
都**没有在本轮顺手改掉**——两者的修法都会动到已三平台验收过的路径。

### 1. VST3 class ID 的字符串形式跨平台不一致 —— **已由 CI 实证**

`.vstpreset` 与宿主工程记录的都是 class ID 的**字符串**，而上游 `FUID::toString`
（`funknown.cpp`）在 `COM_COMPATIBLE` 下把前 8 字节当 `GuidStruct` 重排；`fplatform.h`
只在 `defined (_WIN32)` 下置 `COM_COMPATIBLE 1`。上游 `INLINE_UID` 存**不同的字节**正是
为了抵消这一点，让字符串在所有平台一致——`vst3_sys::base::types::make_tuid` 已照此实现。
**但插件自己的 class ID 走的不是这条路**：它是一组与平台无关的固定字节。

run 34763956140 的三平台日志给出了直接证据，同一个 `SunMao Gain`：

```
macOS / Linux : class     53756E4D616F46784761696E21212121 (native UID layout)
Windows       : class     4D6E75536F6178464761696E21212121 (COM UID layout)
```

（前者就是 ASCII `SunMaoFxGain!!!!`；后者是同一组字节按 COM `GuidStruct` 重排的结果。）

**后果**：macOS 上保存的工程/preset 记录的 class 串，Windows 宿主算出来的对不上，
插件会"找不到"。这不是本轮引入的，是一直存在的条件。**本轮不改**，因为改 class ID 的
生成方式会改变所有既有插件的身份、使已发布的工程与 preset 失效，须单独立项、单独取三平台绿，
并想清楚 Windows 既有安装的迁移。当前宿主的行为是自洽的：读写都用本平台的 `toString`，
同平台往返正确，跨平台不匹配被 class 串比较**明确拒绝**而不是悄悄载入错的 state。
已钉成断言：`preset::tests::the_same_bytes_yield_different_class_strings_on_windows_and_elsewhere`、
`both_uid_layouts_print_the_same_canonical_string`。

**M2 之后这条不再只是日志证据，而是产物证据**：`host-session` 目录已进成功 artifact，
从 run 34766022434 的 Windows 包里取出真实的 `gain.vst3.preset`，按上游布局逐字段解出——

```
header  : b'VST3'        version : 1        list@ : 100   file len : 128
class   : 4D6E75536F6178464761696E21212121   （按十六进制还原成字节即 b'MnuSoaxFGain!!!!'）
```

而 macOS/Linux 写出的同一插件的 preset 里是 `53756E4D616F46784761696E21212121`（即 `SunMaoFxGain!!!!`）。
**两个平台产出的 `.vstpreset` 文件带着不同的 class ID**，这是能直接打开文件看到的事实。
顺带核对了容器布局本身：`48 + 52 = 100` 的 list 偏移与 `100 + 4 + 4 + 20 = 128` 的总长，
与 `vstpresetfile.cpp` 的写入顺序逐字段吻合。

### 2. VST3 与 CLAP 的参数回读精度不一致

写 0.9 再读回：VST3 差 0，CLAP 差 2.38e-8。根因是 `vst3_rs::ParameterBridge` 存
`AtomicU64`（f64）副本，而 `sunmao_core` 的参数真身是 `AtomicU32`（f32）。
**这意味着 VST3 侧的回读并不能证明插件内部的值。** 三平台日志里该差值**逐位相同**，
是确定性差异而非噪声。本轮的应对是让宿主的 `expect` **显式定义容差**
（`DEFAULT_EXPECT_TOLERANCE = 1e-6`，位于 f32 误差之上）而不是比相等；统一精度属独立立项，
不动已三平台验收的 VST3 路径。

### 3. VST3 对**离散参数**的回读返回未量化的值 —— M2 首跑即抓到

M1 记的是精度差（f64 副本 vs f32 真身）。M2 的确定性批跑把同一根因的**更严重**形态暴露出来：
`SunMaoGain` 的两个 **stepped** 参数（`Polarity`、`Bypass`）在两格式的回读**语义不同**，
而不只是精度不同——

```
vst3:  final 2646080969 8.01757812500000000e-1
clap:  final 2646080969 1.00000000000000000e0
```

**同一次自动化之后，VST3 报 0.80，CLAP 报 1.0。** 而两份 trace 的 **24 条 `block` 记录逐字节相同**，
也就是说插件**处理音频时用的是量化后的 1.0**——错的是 VST3 的回读，不是 DSP。
根因与 M1 同一处：`vst3_rs::ParameterBridge` 存的是宿主写进来的原值，没有过插件自己的量化。

后果比 M1 那条重：宿主问"Bypass 现在是多少"，拿到的是 0.80 而不是"已旁通"。
连续参数不受影响（`Gain` 两格式完全一致，`6.67968750000000000e-1`）。

**本轮不修**，因为 M2 的瓶颈是回归床本身；但 goldens **刻意记录当前行为**，
这样下一轮修掉它时，`tools/regression_goldens/*.trace` 的 diff 本身就是修复的证据。
这也正是回归床存在的理由：它第一次真跑就抓到了这个。

## 从 Phase 4 继承的已知遗留

各自单独立项、各自单独取三平台绿（逐条见 `docs/phase4/audit.md`）：

- `Stack::focus_next` 只做索引边界检查，Tab 会停在非交互控件上
- `vst3_rs` 的 `ControllerWrapper` 与 `GuiControllerWrapper` 有 25 函数 / 260 行重复；
  该处用字段偏移算术还原 `this`，宏化必须保持 `repr(C)` 字段顺序并补布局断言
- Windows WGPU 收尾 exit 139（run #37 一次，自 run #66 未再复现）——**不改判为已修复**
- `main` 落后于已验收的 Phase 3/4 工作，合并需仓库所有者决定

## 完成规则

Phase 5 完成的唯一判定：同一 commit 三平台 hosted native jobs 全绿 + artifacts 可下载
+ 本文件 Milestone 矩阵 M0–M5 全部标记完成。本地结果任何情况下都不构成完成证据。

### 当前判定：**Phase 5 进行中（M0、M1、M2 完成，下一步 M3）**
