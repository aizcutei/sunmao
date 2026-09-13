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
| M3 性能与泄漏检测 | RT 安全检测扩到 GUI 线程与宿主回调；泄漏检测；基准与阈值写入本文件 | **完成**（三平台 hosted 全绿）：`stress` 子命令 + `rss.rs` + `stress.rs`；`clap.params.flush` 音频线程零分配断言。**只覆盖 RT 安全三项里的「分配」**，加锁与系统调用如实未做 | [run 34773295928](https://github.com/aizcutei/sunmao/actions/runs/34773295928)（commit `1d40eef`）三 job success，每 job **38 步零非成功**。三平台日志剔除脚本回显后核实：`injected leak detected as it must be` 与 `injected editor leak detected as it must be` **各平台各 1 次**（两条守卫都在真硬件上真的变红过），`STRESS LIFECYCLES VERIFIED` 各 1 次 | — （M3 完成；进入 M4）。**但 Linux 的编辑器差分留了一个未归因的数字，见下** |
| M4 外部 validator | `clap-validator` + Steinberg VST3 validator 三平台 blocking；失败项逐条归因 | **CLAP 侧本地完成，待三平台验收；VST3 validator 未接入**：新增 blocking 步骤 "Validate CLAP plugins with clap-validator"（0.4.1，三平台各取官方预编译包），对 `target/phase1-artifacts/*.clap` 逐个验证（CI 上是 **8 个**，见下方更正） | 三平台各 **8/8 全部 0 failed**；本地对 16 个也全部 0 failed | 取三平台绿；日志须 grep 到 `CLAP VALIDATOR VERIFIED` 与每个插件的 `, 0 failed,` |
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

### 3. VST3 对**离散参数**的回读返回未量化的值 —— M2 首跑抓到，**已修复**

M1 记的是精度差（f64 副本 vs f32 真身）。M2 的确定性批跑把同一根因的**更严重**形态暴露出来：
`SunMaoGain` 的两个 **stepped** 参数（`Polarity`、`Bypass`）在两格式的回读**语义不同**，
而不只是精度不同——修复前 VST3 报 0.80、CLAP 报 1.0，而两份 trace 的 **24 条 `block` 记录逐字节相同**，
即插件**处理音频时用的是量化后的 1.0**，错的是 VST3 的回读。
宿主问"Bypass 现在是多少"，拿到的是 0.80 而不是"已旁通"。

**修法（自底向上）**：`vst3_rs` 的 wrapper 在 `plugin.set_param` 之后立刻 `plugin.get_param`，
把**采纳值**而不是请求值写进 `ParameterBridge`；`backend_vst3::get_param` 改为读
`self.params`（插件真身）而不是 bridge——否则这条链是循环的，量化永远观察不到。
`backend_vst3::set_param` 同时镜像采纳值，**这正是编辑器的 `ViewContext::set_param` 本来就在做的事**，
所以这不是新发明的约定，而是把已有约定补齐到另一条路径上。

bridge 的七个写入点逐个核对过，只改了**有插件实例**的四个，外加 process 起头那次
"从 bridge 同步进插件"的写回：不带 GUI 的 `ControllerWrapper` **没有插件实例**（源码注释原本就写明），
它无从量化，只能先写原值，由 processor 在下一个块把采纳值写回——**收敛需要一个块，这是如实延迟**。
写回发生在音频线程，`ParameterBridge::set` 是原子 swap 且仅在值真的改变时才递增 generation，
因此零分配、零加锁，且不会把同步触发成每块循环。

**证据是 golden 的 diff 本身**：重新生成后 `SunMaoGain.vst3.trace` **只变了两行**
（两个 stepped 参数 0.80/0.787 → 1.0），CLAP 的 golden **一字未动**（它本来就是对的），
没有任何 `block` 记录变化、连续参数也没动。修复后两份 trace **只差 `format` 一行**，
CI 的跨格式断言因此从"`block` 行相同"升级成了"除 `format` 外逐行相同"。

两个新测试都做过**反向验证**：把 `get_param` 改回读 bridge，两者立刻变红（`left: 0.8, right: 1.0`），还原后转绿。

**已三平台验收**：[run 34769367466](https://github.com/aizcutei/sunmao/actions/runs/34769367466)（commit `74588d0`）三 job success、每 job 37 步零非成功。三平台原始日志剔除脚本回显后核实，两条新测试各 `... ok`、且升级后的跨格式断言 `cross-format traces identical in every record but the format line` 三平台各实际输出一次。

## M3 的阈值与它们的来历

阈值不是拍脑袋定的，每条都附实测。三平台实测数字待 CI 回填。

| 检查 | 判据 | 预算 | macOS ARM64 实测 |
|---|---|---|---|
| scan-instantiate-destroy | 常驻内存增长 / 迭代 | **64 KiB/iteration** | macOS 1.5–3 KiB、Linux 3–4 KiB、Windows 0–0.3 KiB（64 次迭代，8 次预热） |
| editor-excess | **差分**，且只看每个循环的**后半段** | **1 MiB/iteration**（粗筛，理由见下） | 36 KiB/iteration（32 次迭代，余量 28×） |
| `clap.params.flush`（音频线程） | 分配器调用次数 | **0** | 0（16 次调用） |

### 为什么编辑器用差分而不是绝对预算

**因为绝对预算量的是窗口系统，不是插件。** 本轮实测记录如下，都在 macOS ARM64：

- 只建窗口、不开编辑器的循环：**~277 KiB/iteration**
- 每次迭代补上 autorelease pool 之后：**~272 KiB/iteration**（编辑器循环则从 ~870 降到 ~484）
- 再补上事件泵之后：**~684 KiB/iteration**

**同一个循环，仪器拿法不同，数字差 2.5 倍。** 这里没有一个绝对阈值是诚实的。
但两件事必须做，因为不做就是量错了：**每次迭代 drain 一个 autorelease pool**
（Cocoa 的 autoreleased 对象本来就要等 pool 排空才释放，紧循环不排空就是把临时对象堆起来当泄漏报），
以及**每次迭代泵事件**（销毁窗口是请求不是动作，AppKit/X11/Win32 都在派发事件时才真正完成，
不派发就是把一堆待销毁的排队起来当泄漏报）。

**两个循环都付同样的窗口系统代价，相减就把它消掉了。** 减完剩下的才是编辑器没还回来的东西——
实测 **0 B**：`window-open-close 686 KiB/iteration` 对 `editor-open-close 675 KiB/iteration`。
这句话比「编辑器每次泄漏 870 KiB」有用得多，而且它是对的。

差分的算术单独抽成 `stress::editor_excess` 并单测，其中一条专门构造「共享 21 MiB 噪声 +
编辑器多留 4 MiB」，要求它**必须**被抓出来——否则这个减法可能永远为真。

### CI 教会了这个检查两件事，两件都改了做法

**第一次三平台运行是红的**，macOS 绿、Linux 与 Windows 红，两边红的原因还不一样。两条都不是
产品缺陷，都是**检查本身设计错了**，如实记在这里。

**Windows：反向用例是空的。** 原本用「预算设成 0」来证明检测器会变红。Windows 上
`scan-instantiate-destroy` 跑 64 次迭代常驻内存**一个字节都没动**，于是 `growth(0) <= allowed(0)`
成立、判为 Stable、退出 0——**那条反向用例在 Windows 上什么也没断言，却一直是绿的**。
改成 `--inject-leak-bytes`：每次迭代真的泄漏并**触摸**指定字节（只保留不触摸只是保留地址空间，
常驻内存不一定涨）。现在三平台都是确定性的，不依赖被测对象恰好有增长。

**Linux：全程平均分不清「缓存填满」和「真泄漏」。** Linux 报
`editor-excess 3.20 MiB / 24 次 = 136 KiB/iteration`，而只开窗口的循环是 0 B。
软件 GL 栈（`LIBGL_ALWAYS_SOFTWARE=1`）每建一次 context 都会占一些并不还，但那是**填满就停**的，
而泄漏是**每次都付**。看全程总量的规则分不出这两者。改为**只判每个循环的后半段**：
缓存到后半段已经填完，还在按次付的才是真没还。单测里专门有一条同总量、不同分布的对照
（前半 48 MiB 后半 0 = 缓存，前后各 24 MiB = 泄漏），要求前者放行、后者抓出。

**顺带把编辑器预算改诚实了。** 同一台机器、同一个插件，在 24/48/64 三种迭代数下测出
12 KiB、150 KiB、0 KiB per iteration——**两条独立 RSS 轨迹相减，继承了两条的噪声**，
几百 KiB 以下和抖动没法区分。所以编辑器差分的预算定在 **1 MiB/iteration** 并**明说它是粗筛**：
它抓得住会终结一次会话的那类泄漏，抓不住精细的。真正精确的仪器是 instantiate 循环
（实测个位数 KiB 对 64 KiB 预算）。两条守卫现在都由注入式自检在每次 CI 运行里证明能变红。

### 一个绿着但没归因的数字：Linux 编辑器差分 ~146 KiB/iteration

run 34773295928 三平台的 editor-excess（都在 1 MiB 预算内，所以是绿的）：

| 平台 | VST3 | CLAP |
|---|---|---|
| macOS ARM64 | 4.00 KiB/iteration | 7.00 KiB/iteration |
| Windows x86_64 | 20.75 KiB/iteration | 0 B/iteration |
| **Linux x86_64** | **145.75 KiB/iteration** | **66.25 KiB/iteration** |

**Linux 明显比另外两个平台高一个数量级，而且这是在「只看后半段」之后测的**——
也就是说它**不是填满就停的缓存**，后半段仍在按次付。改判据的初衷正是把缓存排除掉，
它没有被排除掉，所以这个数字是真的持续增长。

**但现有仪器无法归因**：可能是 X11/GL 编辑器路径真有慢泄漏，也可能是
`LIBGL_ALWAYS_SOFTWARE=1` 的 llvmpipe 每个 context 确实不还。差分只能告诉我们
「开编辑器比只开窗口多花这么多」，分不清多花的是谁花的。

**因此不要把这一行的绿读成「Linux 编辑器无泄漏」**——它只表示「在 1 MiB/iteration 的粗筛下没被拦下」。
归因需要在 Linux 上换一台真 GPU 或换 valgrind/heaptrack 之类的工具单独做，**单独立项**。

### 本轮**没有**做的：加锁与系统调用检测

M3 的原始范围写的是「分配/加锁/系统调用」。本轮只做了**分配**那一项，另两项如实记为未做：

- **加锁**：没有可移植的办法拦截任意 `Mutex`/`RwLock`。可行的做法是给框架自己的锁加一层
  仪表，但 SunMao 的音频路径按设计**根本没有锁**（`ParameterBridge` 的读写是原子的，
  加锁只出现在 connect/disconnect），所以能仪表的对象目前是空集。
- **系统调用**：三平台各需一套完全不同的机制（Linux seccomp/ptrace、macOS dtrace、Windows ETW），
  这不是本轮能连同验收一起交付的量级。

写在这里而不是含糊带过，是因为 status.md 的 M3 行若只说「完成」，下一个人会以为
audio 线程的加锁和系统调用已经有守卫了。

## M4：clap-validator 抓到的三项，逐条归因

接入 clap-validator 0.4.1 后，第一次运行就有失败。**逐条归因是 M4 的要求，不是可选项**——
下面三项里两项是我们的缺陷、一项是 validator 自己的问题。

### 1. `state-reproducibility-{basic,binary,buffered}` —— **我们的缺陷，已修**

三条测试同一个根因。**我最初的判断是错的**：看到「After reloading the state, these parameter
values changed」以为是 state 往返坏了，还去查了跨进程往返——结果是好的。
读了 validator 的源码才明白，它比较的是 `before_load` 与 `after_load`，
而报错的重点在后半句：**`without a rescan request`**。

值确实正确加载了（0.056 就是它随机出来的那个值）。**问题是加载之后没有通知宿主。**
CLAP 要求插件在 state 加载后调用 `clap_host_params::rescan(CLAP_PARAM_RESCAN_VALUES)`；
`clap_sys` 早就有这个绑定，但 `clap_rs` 全仓只在测试桩里出现过 `rescan: None`——**从来没调用过**。
后果是真实可见的：宿主打开工程后，自动化轨道与通用 UI 上仍是旧值，直到别的事情触发一次 rescan。

**两格式同时落地**（host-facing 能力的硬性要求）：VST3 的对应物是
`IComponentHandler::restartComponent(kParamValuesChanged)`，同样**从未调用过**。
已在 `clap_rs::HostHandle::rescan_parameter_values()` 与
`vst3_rs::HostHandle::restart_parameter_values()` 各加一个方法，并在两边 state 加载成功后调用
（VST3 挂在**控制器**侧——component handler 在控制器上，processor 根本没有 host 字段，
第一次写在 processor 上编译就失败了，正好说明了该挂哪里）。

### 2. `param-fuzz-basic` / `param-fuzz-sample-accurate`（`SunMao OS Distortion`）—— **我们的缺陷，已修**

输出里出现**次正规数**（3e-45）。次正规数的算术在部分硬件上慢得惊人，把它交给宿主等于
把停顿传染给下游。`sunmao/dsp` 本来就有 `flush_denormal`，文档里写的正是这个危害，
但过采样失真这个示例**没有在输出上用它**——过采样器的滤波器衰减到零的尾巴正好落在次正规区间。
已在每个通道处理完后 flush。

**但这一条没有单元测试守卫，如实说明**：我写了一个「输出不得有次正规数」的测试，
**反向验证时它并不会变红**（把 flush 去掉仍然通过）——我试了正弦爆发后静音、又试了 trim 打到下限，
都没能复现 validator 用 50 组随机参数排列才撞到的那个组合。
**与其留一个看起来像守卫、实测证明不会失败的装饰，不如删掉它并写明这件事。**
这一条的守卫就是 CI 里的 clap-validator 步骤——它确实抓到过，现在确实是绿的。

### 3. `param-fuzz-bounds` 报 crashed —— **validator 自己的问题**

报错原文就写着 "This is a bug in the validator"：它想创建的临时文件已经存在。
这是前一条测试失败后留下的残留文件导致的连锁反应；把第 2 项修掉之后，这一条自动消失。
**不是我们的缺陷，也不需要为它改任何代码。**

### 更正：CI 实际验证的是 8 个，不是 16 个

提交信息与本文件最初都写的是「每个打包的 `.clap`」，并以本地 16 个全过为据。
**三平台日志核实后发现 CI 上只有 8 个**：

```
SunMaoGain / SunMaoMeter / SunMaoOsDistortion / SunMaoSidechainComp
SunMaoSine / SunMaoTemplateInstrument / SunMaoTempoDelay / SunMaoWidgetsGL
```

原因是 validator 步骤插在「Package and exercise native GUI backends」**之前**，
而 GainGL / GainWGPU / GainWebView / SineGL 等 GUI 后端变体是那一步才打包的，
跑 validator 的时候它们还不存在。本地 `build_new/` 里 16 个都在，所以本地看不出差别。

**8 个已覆盖全部三类 fixture**（效果、合成器、GUI），也包含本轮修掉次正规数的 `OsDistortion`，
所以结论本身不受影响；但「每个打包的 `.clap`」这句话对 CI 不成立，故更正。
**把 validator 步骤挪到 GUI 打包之后以覆盖 16 个，单独立项**——挪动步骤顺序要重新取三平台绿，
不在本轮顺手做。

### 未接入：Steinberg VST3 validator

M4 的范围包含它，本轮**没做**。它不像 clap-validator 那样提供预编译产物，需要在 CI 上
用 CMake 构建 VST3 SDK（含子模块）后才能拿到 `validator` 可执行文件，三平台各一份。
这是下一轮的工作，**在它接入之前 M4 不能标记完成**。

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

### 当前判定：**Phase 5 进行中（M0–M3 完成，下一步 M4）**
