# Phase 3 进展日志

按时间追加，格式固定：

```text
### YYYY-MM-DD — <milestone>
- Command/platform:
- Result:
- Evidence/artifact:
- Unresolved:
```

### 2026-08-28 — M0 脚手架并入 workspace 与 CI

- Command/platform: macOS ARM64，分支 `phase3/framework-dsp-library`（自 main `2df01ce` 切出）。
- Change:
  - 四个 Phase 3 acceptance fixture 骨架并入 workspace：`sunmao_syn_grouped_params`（M2：参数前缀标记未来分组，单音 sine + 一阶 LP + 线性 AR 包络）、`sunmao_fx_svf`（M3：inline TPT SVF，LP/BP/HP）、`sunmao_fx_os_dist`（M4：无 oversampling 的 tanh waveshaper，latency 固定 0 并被测试钉住）、`sunmao_fx_meter`（M4：passthrough + AtomicU32 位存 peak/RMS 无锁发布）。全部只用 Phase 1+2 契约，`sunmao_export!` 统一导出。
  - `.github/workflows/phase1.yml` 新增 blocking 步骤 "Test Phase 3 acceptance fixtures"（与 Phase 2 步骤同构：逐 crate `cargo test --locked -p` + 失败回显日志尾部 + `cargo build` 覆盖 cdylib 路径）。
  - 新建 `docs/phase3/{status,progress}.md`（milestone 矩阵 + 固定四项日志格式）。
- Result:
  - 四个 fixture 22 单元测试通过；完整 `RUSTFLAGS=-Awarnings cargo test --locked` 113 套件全绿、exit 0。
  - `cargo check --locked --target x86_64-pc-windows-msvc` 覆盖四个 fixture 通过。
  - `tools/package_examples.sh --debug --test` 退出 0，20 个 runner 套件各 16/16，raw/packaged standalone smoke 全绿——Phase 1 回归无损。
  - `cargo metadata --locked`、`cargo fmt --all -- --check`、`git diff --check`、workflow YAML 解析（ruby）、`bash -n tools/package_examples.sh` 通过。
  - `nm -gU` 复查四个新 cdylib：均只导出 `GetPluginFactory` + `clap_entry`，无 AU 符号。
- Evidence/artifact: macOS ARM64 本地日志（`/tmp/phase3_m0_test.log`、`/tmp/phase3_m0_pkg.log`）——本地证据等级。
- Unresolved: 三平台 hosted 验证本 commit（Phase 1+2 既有 gate + 新增 Phase 3 fixture 步骤同时全绿）后 M0 才算完成；M1（Phase 2 七项遗留收口）未开始。

### 2026-08-28 — M0 完成：hosted run #41 三平台全绿

- Command/platform: push `9f65af5` 触发 GitHub Actions #41：https://github.com/aizcutei/sunmao/actions/runs/33167456623
- Result: macOS ARM64、Windows x86_64、Ubuntu x86_64 三个 job 同一 commit 全部 success。新增的 blocking 步骤 "Test Phase 3 acceptance fixtures" 三平台均 success；Phase 1 与 Phase 2 既有 gate（GUI matrix、standalone、packager、runner、Phase 2 fixture、proptest）保持绿色。#37 的 Windows WGPU 收尾段错误未复现。
- Evidence/artifact: run #41 上传 `phase1-macOS-ARM64`（48.7MB）、`phase1-Windows-X64`（73.3MB）、`phase1-Linux-X64`（911.8MB），均可下载。
- Unresolved: M0 完成，进入 M1（Phase 2 七项遗留收口）。第 1 项 bus 激活回调的底层盘点：`_sys` 两侧齐全（`clap_sys::clap_plugin_audio_ports_activation_t`、`vst3_sys::IComponent::activate_bus`），缺口在 `_rs`——`vst3_rs::processor_activate_bus` 是固定返回 `kResultOk` 的 stub，`clap_rs` 未暴露该扩展，core 无回调。

### 2026-08-28 — M1 第 1 项：bus 激活/去激活回调（VST3 ↔ CLAP）

- Command/platform: macOS ARM64，分支 `phase3/framework-dsp-library`。
- Change:
  - `_sys`：无改动（盘点确认两侧绑定齐全，且 `clap_sys` 的
    `CLAP_EXT_AUDIO_PORTS_ACTIVATION`=`clap.audio-ports-activation/2`、draft-2
    compat 别名与 `set_active` 字段序均与上游 `audio-ports-activation.h` 一致）。
  - `_rs`：`vst3_rs::processor_activate_bus` 从固定 `kResultOk` 的 stub 改为真实
    实现——`ffi_guard` 包裹，按 `MediaType`/`BusDirection`/index 依 `audio_config()`
    校验后转发 `Plugin::activate_bus`，插件拒绝上报 `kResultFalse`、越界/负数
    `kInvalidArgument`、未 initialize `kNotInitialized`；唯一 event bus 由 wrapper
    自行接受。新增 `clap_rs/src/ext/audio_ports_activation.rs` 暴露
    `clap.audio-ports-activation/2`（含 draft-2 别名解析），按声明端口数校验
    index 后转发 `Plugin::set_audio_port_active`，`can_activate_while_processing`
    默认 `false`；扩展仅在插件声明了 audio ports 时创建，并在 destroy 时释放
    （non-GUI 与 GUI 两条 init/get_extension 路径都接线）。
  - core：`SunmaoPlugin::set_bus_active(is_input, bus_index, active) -> bool`
    默认 `true`（带 doc-test），trait 已在 prelude 中，Phase 1 插件行为不变。
  - backend：VST3 直接转发；CLAP 依"一 bus 一 port、声明序号即索引"转发并丢弃
    `sample_size`（SunMao 仅 f32，clap_rs 在 activate 阶段已拒绝其它位宽）。
  - fixture：`sunmao_fx_sidechain_comp` 消费回调——宿主关掉 key bus 后探测器回落
    主路径，而非继续读取已去激活但仍占槽位的 sidechain。
  - docs：semantics.md 新增"bus 激活/去激活"行（含两格式差异、降级与全部测试名）；
    phase2/status.md 遗留表第 1 项改为"已落地，待 hosted 验收"。
- Result:
  - 新增 11 个测试全绿：`_rs` 5（含 2 proptest：任意声明拓扑 × 任意索引含负数/
    越界，在范围内必转发且仅一次、越界必拒绝且绝不触达插件）、backend 3、
    fixture 2、core doc-test 1。
  - `RUSTFLAGS=-Awarnings cargo test --locked` 113 套件全绿、exit 0。
  - `cargo metadata --locked`、`cargo fmt --all -- --check`、`git diff --check` 通过。
  - `cargo check --locked --target x86_64-pc-windows-msvc` 覆盖 6 个改动 crate 通过
    （workspace 级交叉编译受既有 `au_sys` Apple framework 限制，与本改动无关；
    CI 为各平台原生分包构建）。
  - `tools/package_examples.sh --debug --test` 退出 0，20 个 runner 套件各 16/16。
  - `nm -gU` 复查 sidechain fixture cdylib：无 AU 符号，仅预期导出。
  - Cargo.lock 仅新增 proptest 到 clap_rs/vst3_rs 的 dev-dep 边（2 行，无版本变动）。
- Evidence/artifact: macOS ARM64 本地日志（`/tmp/phase3_m1_test2.log`、
  `/tmp/phase3_m1_pkg.log`、`/tmp/phase3_m1_win.log`）——本地证据等级。
- Unresolved: 待三平台 hosted 全绿方可把遗留表第 1 项改为"已实现"；M1 余下 6 项
  未开始，下一个瓶颈是第 2 项 speaker layout 动态协商（`setBusArrangements` 真实
  协商 ↔ CLAP `clap.audio-ports-config`，`clap_rs` 尚未暴露该扩展）。

### 2026-08-28 — M1 第 1 项验收：hosted run #42 三平台全绿

- Command/platform: push `b78aca6` 触发 GitHub Actions #42：https://github.com/aizcutei/sunmao/actions/runs/33171119003
- Result: macOS ARM64、Windows x86_64、Ubuntu x86_64 三个 job 同一 commit 全部
  success，且**逐步骤复查零非成功步骤**（不只看 job 级汇总）：Phase 1+2 既有
  gate（"Test format adapters and host"、standalone/facade、"Test Phase 2
  acceptance fixtures"、packager、GUI backends）与 "Test Phase 3 acceptance
  fixtures" 三平台均 success。新增的 bus 激活链路（`_rs` 两侧 + backend +
  fixture + 2 proptest）在三平台原生构建下全部通过。#37 的 Windows WGPU 收尾
  段错误未复现。
- Evidence/artifact: run #42 上传 `phase1-macOS-ARM64`（48.8MB）、
  `phase1-Windows-X64`（73.4MB）、`phase1-Linux-X64`（912.1MB），
  `expired=false` 均可下载。
- Unresolved: phase2/status.md 遗留表第 1 项已改为"已实现"。M1 余下 6 项未开始；
  下一个瓶颈是第 2 项 speaker layout 动态协商——`setBusArrangements` 目前按声明
  固定接受而非真实协商，且 `clap_rs` 尚未暴露 `clap.audio-ports-config`（该扩展
  的 `clap_sys` 绑定已齐全，缺口同样在 `_rs` 层）。

### 2026-08-28 — M1 第 2 项：speaker layout 动态协商（VST3 ↔ CLAP）

- Command/platform: macOS ARM64，分支 `phase3/framework-dsp-library`。
- Change:
  - 先读上游确认事实：`setBusArrangements` 原实现**并非**"固定接受"，而是"与静态
    声明逐一比对，不等就 `kResultFalse`"——两者都不是协商（插件无法提供备选）。
    `clap_sys::audio_ports_config` 绑定齐全（config/config-info/draft-0 别名/host
    rescan），缺口在 `_rs`。
  - core：新增 `BusConfig{name,inputs,outputs}` + `input_channel_counts()`/
    `output_channel_counts()`/`matches()`，以及 `SunmaoPlugin::bus_configs()`/
    `current_bus_config()`/`select_bus_config()`（默认空 ＝ 不可协商，Phase 1/2
    插件行为不变）。`BusConfig` 与 `BusInfo`/`BusRole` 一并进 `sunmao_core` 与
    `sunmao` 两个 prelude，带 doc-test。
  - `clap_rs`：新增 `ext/audio_ports_config.rs`，暴露 `clap.audio-ports-config`
    与 `clap.audio-ports-config-info/1`（含 draft-0 别名）；`select` 先拒绝未发布
    的 id，再转发插件，成功后**重建端口缓存并重算 audio-thread scratch buffer**
    （新增 `PluginInstance::resize_process_buffers`）——否则 mono→stereo 会用旧
    尺寸缓冲处理。两个扩展仅在插件发布了配置时创建，destroy 时释放，non-GUI 与
    GUI 两条路径都接线。
  - `clap_rs` 顺带修正既有缺陷：`audio_ports_get` 对**所有**端口固定上报
    `port_type=stereo`；mono 布局出现后即为错报，现按通道数给 `mono`/`stereo`/null
    （新增共享 `port_type_for`，两条 GUI/非 GUI 路径共用）。
  - `vst3_rs`：`Plugin::negotiate_bus_arrangement(in_counts,out_counts)`（默认
    全拒，保持既有语义）；`setBusArrangements` 在"等于声明布局"之外，按
    **位图 popcount** 得到提议通道数并询问插件，接受后记录到
    `input/output_bus_channels`；`getBusInfo`/`getBusArrangement`/
    `setupProcessing` 改读该记录，使协商结果真正对宿主可见并被分配采用。
  - backend：VST3 侧把提议在 `bus_configs()` 中查表后 `select_bus_config`（未发布
    的布局一律拒绝，故 VST3 宿主可达布局集与 CLAP 完全一致）；CLAP 侧以下标为
    config id 发布列表，`select` 成功后刷新 bus 列表与通道总数。抽出共享的
    `clap_ports_for` 供实时端口表与各配置共用，避免两处描述同一 bus 却不一致。
  - fixture：新增 `examples/sunmao_fx_layout_gain`（发布 mono/stereo，默认 stereo），
    并入 workspace 与 CI 的 Phase 3 fixture 列表（blocking）。**未**改动
    `sunmao_fx_gain` 等 Phase 1 参考示例，以免动到 runner smoke 契约。
  - docs：semantics.md 用"speaker layout 动态协商"整行替换原"（M3 设计中）"占位，
    记录两格式方向相反、可达集相同、限制（只协商通道数/bus 数不变/active 时拒绝）
    与 port_type 修正，附全部测试名。
- Result:
  - 新增 12 个测试全绿：fixture 5、backend_clap 4、backend_vst3 2（其一驱动真实
    `setBusArrangements` 并断言 `getBusInfo` 随协商改变）、core 跨格式可达性
    proptest 1（任意配置集 × 任意提议：VST3 查表结果必等于独立算出的真值，命中项
    的通道数必等于提议）。core doc-test 增至 8 个。
  - `RUSTFLAGS=-Awarnings cargo test --locked` 115 套件全绿、exit 0（M1 第 1 项时
    为 113）。
  - `cargo metadata --locked`、`cargo fmt --all -- --check`、`git diff --check` 通过。
  - `cargo check --locked --target x86_64-pc-windows-msvc` 覆盖 7 个改动 crate 通过。
  - `tools/package_examples.sh --debug --test` 退出 0，20 个 runner 套件各 16/16
    ——**并在补上"active 时拒绝协商"的守卫后重跑一次确认仍绿**（该守卫改变了
    `setBusArrangements` 行为，首次打包跑在守卫之前，不足为证）。
  - `nm -gU` 复查新 fixture cdylib：仅预期导出，无 AU 符号。
- Evidence/artifact: macOS ARM64 本地日志（`/tmp/phase3_m1b_test2.log`、
  `/tmp/phase3_m1b_pkg2.log`、`/tmp/w2.log`）——本地证据等级。
- Unresolved: 待三平台 hosted 全绿方可把遗留表第 2 项改为"已实现"。M1 余下 5 项
  未开始，下一个瓶颈是第 3 项 runner 宿主侧断言（latency/tail 查询、多 bus 拓扑
  枚举、向 sidechain 送信号验证路由）。已知边界：本项只协商通道数，bus 数量变化
  与 surround 位图仍未支持（semantics.md 已记）。

### 2026-08-28 — M1 第 2 项验收：hosted run #44 三平台全绿

- Command/platform: push `1478189` 触发 GitHub Actions #44：https://github.com/aizcutei/sunmao/actions/runs/33174187893
- Result: macOS ARM64、Windows x86_64、Ubuntu x86_64 三个 job 同一 commit 全部
  success，逐步骤复查零非成功步骤；"Test Phase 3 acceptance fixtures"（现含新增
  的 `sunmao_fx_layout_gain`）三平台均 success，Phase 1+2 既有 gate 保持绿色。
  layout 协商链路（core `BusConfig`、`clap_rs` audio-ports-config/config-info、
  `vst3_rs` setBusArrangements 真实协商、两 backend、跨格式可达性 proptest）在
  三平台原生构建下全部通过。#37 的 Windows WGPU 收尾段错误未复现。
- Evidence/artifact: run #44 上传 `phase1-macOS-ARM64`（49.3MB）、
  `phase1-Windows-X64`（73.7MB）、`phase1-Linux-X64`（914.5MB），均可下载。
- Unresolved: phase2/status.md 遗留表第 2 项已改为"已实现"（2/7 关闭）。下一个
  瓶颈是第 3 项 runner 宿主侧断言：latency/tail 查询、多 bus 拓扑枚举、向
  sidechain 送信号验证路由。

### 2026-08-28 — M1 第 3 项：runner 宿主侧断言（并修复一个真实缺陷）

- Command/platform: macOS ARM64，分支 `phase3/framework-dsp-library`。
- Change:
  - runner host 层：`HostPlugin` 新增 `reported_latency()`/`reported_tail()`/
    `audio_buses()`（均返回 `Option`，用以区分"格式未暴露该能力"与"暴露了但值为 0"），
    新增 `HostBusInfo{name,channels,is_input,is_main}`。VST3 侧走
    `getLatencySamples`/`getTailSamples`/`getBusCount`+`getBusInfo`（含 UTF-16 名称解码）；
    CLAP 侧走 `clap.latency`/`clap.tail`/`clap.audio-ports`（含 `CLAP_AUDIO_PORT_IS_MAIN`）。
  - runner 测试：套件从 16 项扩到 19 项——`latency_tail`（查询 + 合理性上界 + 各格式
    无限尾音魔数校验；对 Tempo Delay 额外断言**非零**）、`bus_topology`（枚举并与
    `info()` 的扁平通道总数**交叉校验**，两者来自不同调用，此前无任何东西保证一致；
    另断言有输出必有 main bus）、`sidechain_routing`（只往 key bus 送信号，比较
    silent-key 与 loud-key 两趟输出——若 backend 把 key bus 映射到错误通道偏移，
    插件会 key 到静音、两趟输出相同，单跑任一趟都发现不了）。
  - 打包与 CI：`sunmao_fx_tempo_delay` 与 `sunmao_fx_sidechain_comp` 并入
    `tools/package_examples.sh` 的 EXAMPLES 与 workflow 的 packager/runner 调用
    （matrix 加 `delay-binary`/`sidechain-binary` 三平台路径）。**必要性**：全部
    Phase 1 示例都是零 latency、无 tail、单输入 bus，不加这两个 fixture 的话三个新
    断言在 CI 里永远只走 skip 分支，等于没测。打包 bundle id 用连字符
    （packager 拒绝 bundle identifier 中的下划线），与 fixture 自身的 CLAP id 无关。
  - **修复真实缺陷（由新断言发现）**：`sunmao_backend_clap::activate` 会把插件
    `take()` 进 audio processor，而 `latency()`/`tail()` 只看 `self.plugin.as_ref()`
    并 `unwrap_or(0)`——**插件激活期间（正是宿主查询的时刻）一律上报 0**。宿主会因此
    不做延迟补偿、并可能切掉尾音。VST3 backend 直接持有插件故无此问题，两格式行为分叉。
    现于 `activate` 移交所有权前缓存（`initialize` 已跑完，值反映激活采样率），
    激活期间回落缓存值；`deactivate` 后插件重新成为权威。既有单测
    `latency_and_infinite_tail_reach_the_clap_contract` 只覆盖未激活状态，故未能发现。
- Result:
  - runner 本地 24 套件（原 20，新增 2 fixture × 2 格式）各 19/19、exit 0。关键读数：
    Tempo Delay CLAP `latency=221, tail=2147483647`（i32::MAX）、VST3
    `latency=221, tail=4294967295`（u32::MAX）——两格式 latency 一致（44.1kHz 下
    5ms lookahead）且各用本格式魔数；Sidechain Comp 两格式均 `bus_topology (2 in / 1 out)`
    与 `sidechain_routing (silent=0.0500, loud=0.0063)`。
  - 新增 backend 回归测试 `latency_and_tail_survive_activation`（激活期间可读 + 去激活后恢复）。
  - `RUSTFLAGS=-Awarnings cargo test --locked` 115 套件全绿、exit 0。
  - `cargo metadata --locked`、`cargo fmt --all -- --check`、`git diff --check`、
    workflow YAML 解析、`bash -n tools/package_examples.sh` 通过。
  - `cargo check --locked --target x86_64-pc-windows-msvc -p sunmao_unittest_runner
    -p sunmao_backend_clap` 通过。
  - `nm -gU` 复查两个新打包 fixture cdylib：无 AU 符号。
- Evidence/artifact: macOS ARM64 本地日志（`/tmp/m1c_test.log`、`/tmp/m1c_pkg3.log`、
  `/tmp/m1c_win.log`）——本地证据等级。
- Unresolved: 待三平台 hosted 全绿方可把遗留表第 3 项改为"已实现"。M1 余下 4 项未开始，
  下一个瓶颈是第 4 项 backend 层 expression/mod 端到端映射测试。注意 CI 时长因新增
  4 个 runner 套件而增加。

### 2026-08-28 — run #46 Windows GUI 输入 flake：诊断与加固（非本项改动所致）

- Command/platform: hosted run #46（commit `031717e`）：https://github.com/aizcutei/sunmao/actions/runs/33176536976
- Result: macOS ARM64 与 Ubuntu x86_64 **success**；Windows x86_64 **failure**，
  且失败步骤是既有的 "Package and exercise native GUI backends"，
  **本项新增断言所在的 "Package and exercise VST3 + CLAP + standalone" 步骤在
  Windows 上 success**。本机无 gh 且日志下载需 admin 权限（403），故经
  `/check-runs/<id>/annotations` 取到失败详情：
  `SunMao Sine Synth GL (CLAP)` → pixels/resize/focus 全过
  （`foreground active=true, raised=true, input focused=true`、96 DPI、client 520x220、
  drag (64,110)→(456,110)、`input depth 0`＝GL 表面无子窗口属正常），随后
  `GUI input verification failed: parameter 'Volume' stayed at 0.500000`。
- 诊断（未采信"flake"即跳过）：
  - **不是**已记录的 Windows WGPU exit 139 收尾段错误（那是断言全过后崩溃；此处是断言本身失败），
    因此不套用"再复现则查 WGPU/D3D 析构"的结论。
  - 本 commit 未触碰任何 GUI 代码、GUI 布局或参数枚举；对 CLAP 路径的唯一改动是
    latency/tail 缓存（`activate` 期间读值），与合成输入无因果关系。同一 fixture 的
    同一断言在 run #42、#44 及更早多轮通过。
  - `gui_test_render_delay` 确实在 500ms 内持续 `pump_events()`，且拖动前已完成
    pixel 验证（说明已绘制），故排除"消息未泵送"与"尚未首绘"两个假设。
  - 历史同症状：run #24 Windows `Gain WebView (VST3)` 同样 "stayed at 0.5"，
    根因是冷 runner 上 UIA helper 5s 超时（修法是放宽超时而非删断言）；
    Linux 亦有过因 WebKitGTK 字体度量导致拖动 y 坐标打偏的同症状。
    即：该症状属"合成输入与控件竞态"这一既有 flake 家族。
- Fix（加固而非重试，且不削弱断言）：GUI 输入验证改为**有界重试**
  （`SUNMAO_GUI_INPUT_ATTEMPTS`，默认 3 次，沿用 `env_duration_ms` 的 env 约定）。
  输入若真的到不了插件则每次都失败、仍然红；只有"第一次按下被控件丢弃"这类竞态
  会被吸收。每次失败打印 `attempt n/N`，成功且 n>1 打印 `took n attempts`，
  使真实回归与竞态在日志里可区分。与 run #24 放宽超时的处置同精神，
  未回退任何 Phase 1 GUI 修复。
- Result（本地验证）：macOS `gui-test --verify-pixels --verify-input` 对
  `SunMao Sine Synth GL (CLAP)` 通过——`Volume` 0.500000 → 0.922414 一次命中
  （无 "took n attempts"），gesture 证据 begin +1/value +13/end +1；成功路径未变。
  `RUSTFLAGS=-Awarnings cargo test --locked` 115 套件全绿；
  `package_examples.sh --debug --test` 退出 0、24 套件各 19/19；
  metadata/fmt/diff-check、Windows 交叉 check 通过。
- Unresolved: 本机为 macOS，无法在 Windows 上直接复现以定位控件丢弃首次按下的确切
  层次（baseview 命中测试 / D3D 首帧 / SendInput 时序），故这是**竞态加固而非根因修复**；
  已在日志中留下可区分证据。第 3 项仍待三平台 hosted 同 commit 全绿方可验收。

### 2026-08-28 — M1 第 3 项验收：hosted run #47 三平台全绿

- Command/platform: push `0e79bd2` 触发 GitHub Actions #47：https://github.com/aizcutei/sunmao/actions/runs/33177884493
- Result: macOS ARM64、Windows x86_64、Ubuntu x86_64 三个 job 同一 commit 全部
  success，逐步骤复查零非成功步骤。#46 的 Windows GUI 输入失败未复现；新增的 4 个
  runner 套件（Tempo Delay 与 Sidechain Comp 各 ×2 格式）与 3 个新断言在三平台
  原生构建下全部通过。
- Evidence/artifact: run #47 上传 `phase1-macOS-ARM64`（49.3MB）、
  `phase1-Windows-X64`（73.7MB）、`phase1-Linux-X64`（914.5MB），均可下载。
- Unresolved: phase2/status.md 遗留表第 3 项已改为"已实现"（3/7 关闭）。
  **诚实标注**：日志下载需 admin 权限（403），故无法从 API 判定 Windows 这次是
  "重试第 2/3 次才命中"还是"首次即命中"——即无法区分"加固生效"与"竞态未复现"。
  若后续运行出现 `took n attempts` 日志，即为加固确实在吸收竞态的证据；若再出现
  三次全失败，则为真实回归，需深入 baseview 命中测试 / D3D 首帧 / SendInput 时序。
  下一个瓶颈是第 4 项 backend 层 expression/mod 端到端映射测试。

### 2026-08-28 — M1 第 4 项：backend 端到端 expression/mod 映射测试（发现并修复 VST3 expression 完全失效）

- Command/platform: macOS ARM64，分支 `phase3/framework-dsp-library`。
- Change:
  - backend_clap 新增 `raw_clap_expression_and_mod_events_reach_the_core_queue`：
    构造原始 `clap_event_note_expression_t` 与 `clap_event_param_mod_t`，经真实
    `clap_input_events_t` vtable 与真实 CLAP ABI `process` 进入插件，在插件的
    `process` 内读取 core `EventQueue` 并断言——expression 的 kind/channel/key/
    note_id/value/offset 全部保真（CLAP 携带 channel/key，故为 `Some`）、
    param mod 由数值 id 正确译回字符串 id 且 amount/offset 保真、
    且 **modulation 不出现在 `param_changes()`**（否则会流入插件 state）。
  - backend_vst3 新增 `raw_vst3_expression_events_reach_the_core_queue`：构造原始
    `NoteExpressionValueEvent` 经真实 `IEventList` 进入，断言 kind/note_id/value/
    offset 保真、**VST3 侧 channel/key 为 `None`**（文档化的降级）、未知 type id
    保留为 `Unknown(9999)` 而非被丢弃，并与一个交错的 MIDI note 一起断言
    **三路归并按 sample offset 排序**（expression@2 → midi@3 → expression@4）。
  - **修复真实缺陷（由该测试发现）**：VST3 backend 的 `note_expression` 回调直接
    push 进 `self.event_queue`，而 `process` 在合并本块事件前会
    `self.event_queue.clear()`——**clear 发生在回调之后，于是每个 VST3 note
    expression 都被静默丢弃，插件永远收不到**。MIDI 不受影响是因为它走
    `pending_midi` 暂存再合并，expression 没有对应暂存。修法：新增
    `pending_expressions`（与 `pending_midi` 同容量策略与 `try_reserve_exact`
    预分配，故 audio callback 仍零分配），`append_timed_events` 改为 param/MIDI/
    expression 三路按 offset 归并（并列时序 param → MIDI → expression，确定性），
    并在 deactivate/reset/overflow 三处与 `pending_midi` 一同清空。
  - 说明该缺陷为何一直未被发现：`_rs` 层测试证明 vst3_rs 正确分发
    `kNoteExpressionValueEvent`，core/fixture 测试（`sunmao_syn_poly_expr`）直接调
    `plugin.process` 证明 core 正确处理 expression——**缺口恰在中间的 backend**，
    而 Phase 2 从未在该层做端到端测试。这正是本项存在的意义。
- Result:
  - 两个新测试通过；`sunmao_backend_vst3` 24 测试全绿。
  - `RUSTFLAGS=-Awarnings cargo test --locked` 115 套件全绿、exit 0。
  - 零分配未回退：`cargo test --release --locked` 的 realtime allocation matrix
    四个 crate 全绿，`unified_vst3_audio_processing_does_not_use_the_allocator` 与
    `unified_vst3_effect_processing_does_not_use_the_allocator` 通过。
  - `cargo metadata --locked`、`cargo fmt --all -- --check`、`git diff --check` 通过；
    `cargo check --locked --target x86_64-pc-windows-msvc` 覆盖两个 backend 通过；
    `tools/package_examples.sh --debug --test` 退出 0、24 套件各 19/19。
- Evidence/artifact: macOS ARM64 本地日志（`/tmp/m1d_test.log`、`/tmp/m1d_pkg.log`、
  `/tmp/m1d_win.log`）——本地证据等级。
- Unresolved: 待三平台 hosted 全绿方可把遗留表第 4 项改为"已实现"。M1 余下 3 项未开始，
  下一个瓶颈是第 5 项 `migrate_state` backend 接线。**遗留观察**：测试中发现
  `DenseEventList` 等既有测试用 COM 结构未标 `#[repr(C)]`，其 vtbl 在首字段属侥幸；
  本次新增的 `ExprEventList` 已显式标注，既有的未改（不在本项范围，且当前行为正确）。

### 2026-08-28 — M1 第 4 项验收：hosted run #49 三平台全绿

- Command/platform: push `03a53eb` 触发 GitHub Actions #49：https://github.com/aizcutei/sunmao/actions/runs/33180224229
- Result: macOS ARM64、Windows x86_64、Ubuntu x86_64 三个 job 同一 commit 全部
  success，逐步骤复查零非成功步骤。VST3 expression 修复（`pending_expressions`
  暂存 + 三路 offset 归并）与两个 backend 端到端测试在三平台原生构建下全部通过；
  realtime allocation matrix 保持绿色，零分配未回退。#46 的 Windows GUI 输入
  竞态未复现（本轮亦无 `took n attempts` 证据可查，原因同前：日志需 admin 权限）。
- Evidence/artifact: run #49 上传 `phase1-macOS-ARM64`（49.5MB）、
  `phase1-Windows-X64`（73.9MB）、`phase1-Linux-X64`（915.6MB），均可下载。
- Unresolved: phase2/status.md 遗留表第 4 项已改为"已实现"（4/7 关闭）。
  下一个瓶颈是第 5 项：backend 在 state load 后按版本回调 `migrate_state`
  （两格式接线 + 测试）。

### 2026-08-28 — M1 第 5 项：`migrate_state` backend 接线（根因在 `_rs` 的硬编码版本号）

- Command/platform: macOS ARM64，分支 `phase3/framework-dsp-library`。
- Change:
  - 自底向上定位：core 的 `migrate_state` 钩子与 `STATE_VERSION` 早已就绪，
    但 **`vst3_rs/src/state.rs` 与 `clap_rs/src/ext/state.rs` 都把版本号硬编码为
    `const STATE_VERSION: u32 = 1` 写入 blob，并只与该常量比对**——插件即便声明
    `STATE_VERSION = 2`（`sunmao_state_migration` fixture 正是如此），写出的 blob
    仍标为 1，读入时版本恒等于当前版本，`migrate_state` **在任何情况下都不会被调用**。
    所以本项不是"只差 backend 一行转发"，缺口同时在 `_rs` 与 backend 两层。
  - `_rs` 两层：`Plugin` trait 新增 `const STATE_VERSION: u32 = 1` 与
    `fn state_loaded(&mut self, from_version: u32)`；encode 改写入 `P::STATE_VERSION`，
    `decode_header` 改与 `P::STATE_VERSION` 比对（更旧接受、更新拒绝），
    load 成功后在**全部参数值应用完毕**才回调 `state_loaded`（保证插件从完整旧状态迁移）。
    VST3 侧三个 state 入口（processor、controller、GUI controller）全部接线，其中
    controller 无插件实例故只透传版本、由 processor 侧负责迁移。
  - backend 两侧：`const STATE_VERSION: u32 = P::STATE_VERSION;` 上抛插件版本，
    `state_loaded` 转发 `SunmaoPlugin::migrate_state`。
  - 测试（各走真实 stream ABI，不走内部辅助函数）：
    `clap_state_from_an_older_build_triggers_migration`（自建 `clap_istream_t`，
    经真实 `clap.state` 扩展载入 v1/v2/v3 三种 blob，断言 v1→`migrate_state(1)`、
    v2→不迁移、v3→拒绝且不迁移）、`clap_saved_state_carries_the_plugin_state_version`
    （经真实 `clap.state` save 断言写出的版本是插件的 2 而非常量 1）、
    `vst3_state_from_an_older_build_triggers_migration`（自建 `IBStream`，经真实
    `IComponent::setState`/`getState` 做同样三段断言 + 写出版本断言）。
- Result:
  - `RUSTFLAGS=-Awarnings cargo test --locked` 115 套件全绿、exit 0；
    `sunmao_state_migration` fixture 的 state round-trip 未破（硬性规则）。
  - `cargo metadata --locked`、`cargo fmt --all -- --check`、`git diff --check` 通过；
    `cargo check --locked --target x86_64-pc-windows-msvc` 覆盖两个 `_rs` 与两个 backend 通过；
    `tools/package_examples.sh --debug --test` 退出 0、24 套件各 19/19。
- Evidence/artifact: macOS ARM64 本地日志（`/tmp/m1e_test.log`、`/tmp/m1e_pkg.log`、
  `/tmp/m1e_win.log`）——本地证据等级。
- Unresolved: 待三平台 hosted 全绿方可把遗留表第 5 项改为"已实现"。
  **诚实标注一次性兼容影响**：修复前的构建写出的 blob 一律标为版本 1（即便插件已是 v2），
  修复后会被当作 v1 读入并触发 `migrate_state(1)`。`sunmao_state_migration` 的迁移是
  幂等的（把 trim 设为常量）故无害，但非幂等迁移的插件需自行权衡；已记入 semantics.md。
  M1 余下 2 项，下一个瓶颈是第 6 项 preset-load / program list。

### 2026-08-28 — M1 第 5 项验收：hosted run #51 三平台全绿

- Command/platform: push `7999b73` 触发 GitHub Actions #51：https://github.com/aizcutei/sunmao/actions/runs/33182548781
- Result: macOS ARM64、Windows x86_64、Ubuntu x86_64 三个 job 同一 commit 全部
  success，逐步骤复查零非成功步骤。state 版本改由插件提供后，既有 fixture 的
  state round-trip 在三平台保持绿色（硬性规则），三个新增的真实 stream ABI
  迁移测试全部通过。
- Evidence/artifact: run #51 上传 `phase1-macOS-ARM64`（49.5MB）、
  `phase1-Windows-X64`（73.9MB）、`phase1-Linux-X64`（915.7MB），均可下载。
- Unresolved: phase2/status.md 遗留表第 5 项已改为"已实现"（5/7 关闭）。
  下一个瓶颈是第 6 项：`clap.preset-load` 与 VST3 program list，统一为
  "插件侧载入回调 + 状态应用"，program list 可选实现。

### 2026-08-28 — M1 第 6 项：preset 载入（CLAP 落地，VST3 program list 按边界不实现）

- Command/platform: macOS ARM64，分支 `phase3/framework-dsp-library`。
- Change:
  - 先读上游确认边界：`clap_sys` 有 `clap.preset-load/2` 与 draft 别名、
    location kind 常量在 `factory::preset_discovery`；**`vst3_sys` 完全没有
    `IUnitInfo`/`IProgramListData` 绑定**，故 VST3 program list 若要做需先补 `_sys`。
    按 loop 边界"program list 可选实现"，本项只落 CLAP 侧回调腿。
  - core：新增 `PresetLocation::{File{path,key}, Internal{key}}` 与
    `SunmaoPlugin::{SUPPORTS_PRESET_LOAD, load_preset}`（默认不支持、返回 false），
    进两个 prelude，带 doc-test。
  - `clap_rs`：新增 `ext/preset_load.rs` 暴露 `clap.preset-load/2`（含 draft 别名解析），
    **仅在 `SUPPORTS_PRESET_LOAD` 为真时创建扩展**——否则宿主会拿到一个必然失败的
    loader。backend 层防御：file 位置但路径为空指针、路径非 UTF-8、未知 location_kind
    一律在触达插件前拒绝；非 UTF-8 **不做有损转换**，否则可能载入与宿主所指不同的文件。
  - backend_clap：`ClapPresetLocation` ↔ `SunmaoPresetLocation` 同形转译，插件返回值
    如实上报。
  - fixture：`sunmao_state_migration` 消费该能力（preset 本质就是参数状态），
    实现两个 factory preset 与"未知 key / file 位置一律拒绝"。
- Result:
  - 新增测试：backend 2（走真实扩展：两种位置原样送达、拒绝上报 false、
    空路径与未知 kind 不触达插件；未支持的插件不暴露扩展）、fixture 2、core doc-test 1。
  - `RUSTFLAGS=-Awarnings cargo test --locked` 115 套件全绿、exit 0。
  - metadata/fmt/diff-check 通过；`cargo check --locked --target x86_64-pc-windows-msvc`
    覆盖 5 个改动 crate 通过；`tools/package_examples.sh --debug --test` 退出 0、
    24 套件各 19/19。
- Evidence/artifact: macOS ARM64 本地日志（`/tmp/m1f_test.log`、`/tmp/m1f_pkg.log`、
  `/tmp/m1f_win.log`）——本地证据等级。
- Unresolved: 待三平台 hosted 全绿方可把遗留表第 6 项改为"已实现"。
  **诚实标注**：这是本阶段少见的**单格式能力**——VST3 侧没有等价调用可接，
  不是"接了但降级"，而是该格式宿主根本没有 preset 接口可调（其路径是 `setState`）。
  若将来要做 VST3 program list，需先在 `vst3_sys` 补 `IUnitInfo`/`IProgramListData`
  绑定，属独立工作量。M1 余下第 7 项（无界 fuzz 脚手架）。

### 2026-08-28 — M1 第 6 项验收：hosted run #53 三平台全绿

- Command/platform: push `e1455dd` 触发 GitHub Actions #53：https://github.com/aizcutei/sunmao/actions/runs/33184652119
- Result: 三平台三个 job 同一 commit 全部 success，逐步骤复查零非成功步骤。
- Evidence/artifact: run #53 上传三平台 artifacts（49.5MB / 73.9MB / 915.8MB），均可下载。
- Unresolved: 遗留表第 6 项已改为"已实现"（6/7 关闭）。余下第 7 项：无界 fuzz 脚手架
  （按边界仅本地/非 blocking，入口写入 README）。

### 2026-08-28 — M1 第 7 项：无界 fuzz 脚手架（本地/非 blocking）

- Command/platform: macOS ARM64，分支 `phase3/framework-dsp-library`。
- Change:
  - 新建 `fuzz/` crate 并在根 `Cargo.toml` 加 `exclude = ["fuzz"]`——**边界要求"仅本地/
    非 blocking"，排除出 workspace 是让 gate 连构建都不会碰它的可靠做法**（已用
    `cargo metadata` 复核：`sunmao_fuzz` 不在 workspace 包列表中）。
  - 选题依据：值得 fuzz 的是"解析非本插件产生的字节"的路径。state 正是如此——
    来自工程文件/preset，用户可能编辑或截断，且在 C ABI 后解码（panic 即 UB）。
    两个目标：任意字节 → 真实 `clap.state` load、任意字节 → 真实
    `IComponent::setState`（都走真实插件 ABI 而非内部解码函数，连 wrapper 的防御一起测）。
  - 结构：fuzz body 放 `src/lib.rs`，由两个 driver 共用——`src/main.rs` 是**稳定版、
    零外部依赖**的无界随机 driver（xorshift64*，打印 seed 可复现），
    `fuzz_targets/*.rs` 是三行 libfuzzer 包装。共用 body 意味着 coverage-guided
    目标不会与日常实际跑的代码悄悄分叉。
  - 入口写入根 `README.md`（Build And Verify 下新增 Fuzzing 小节）与 `fuzz/README.md`。
    `fuzz/.gitignore` 排除 `target`/`corpus`/`artifacts`（根 `.gitignore` 只有 `/target`，
    否则 `fuzz/target` 会被提交）。
- Result:
  - **实跑验证**：`cargo run --release -- --iterations 3000000` 三百万例、
    约 486k 例/秒、**无崩溃**，exit 0；格式化后再跑 5 万例复核仍绿。
  - 主 gate 不受影响：`cargo metadata --locked`、`cargo fmt --all -- --check`、
    `git diff --check`、`RUSTFLAGS=-Awarnings cargo test --locked` 115 套件全绿、
    `tools/package_examples.sh --debug --test` 退出 0（24 套件各 19/19）。
    `fuzz` 目录单独 `cargo fmt --all -- --check` 通过。
- Evidence/artifact: macOS ARM64 本地日志（`/tmp/fuzz_long.log`、`/tmp/m1g_test.log`、
  `/tmp/m1g_pkg.log`）——本地证据等级。
- Unresolved: **诚实标注**：环境未安装 `cargo-fuzz`，故 `fuzz_targets/` 的 libfuzzer
  包装本身**未被执行**——被执行的是它们调用的 fuzz body（经稳定版 driver）。
  已在 `fuzz/README.md` 明示该限制。稳定版 driver **不是 coverage-guided**，
  只是随时可用的基线；深扫仍应用 `cargo +nightly fuzz run`。
  本项无需 hosted 验证其功能（非 blocking），但仍需三平台 hosted 确认它
  **没有拖累既有 gate**。M1 七项落地完毕，下一步进 M2。

### 2026-08-28 — M1 完成：7/7 遗留项全部三平台 hosted 验收

- Command/platform: 第 7 项 push `f0c2f2e` 触发 GitHub Actions #55：https://github.com/aizcutei/sunmao/actions/runs/33186447997
- Result: 三平台三个 job 同一 commit 全部 success、逐步骤零非成功步骤。fuzz crate
  排除出 workspace 后**未对既有 gate 产生任何影响**（这正是本项需要 hosted 确认的点）。
  M1 七项各自在独立 commit 上取得三平台 hosted 绿：
  run #42（bus 激活）、#44（layout 协商）、#47（runner 宿主断言）、
  #49（backend expression 端到端）、#51（`migrate_state` 接线）、
  #53（preset 载入）、#55（无界 fuzz 脚手架）。
- 收口过程中发现并修复的、原本"标记完成但实际失效"的缺陷（均非本轮新引入）：
  1. **VST3 note expression 从未真正到达插件**——backend 在宿主回调之后
     `event_queue.clear()`，每个 expression 都被静默丢弃（Phase 2 M4 标记完成时即失效）。
  2. **CLAP 在插件激活期间 latency/tail 上报 0**——`activate` 把插件 `take()` 进
     processor 后回落 `unwrap_or(0)`，而这正是宿主查询的时刻；VST3 无此问题，两格式分叉。
  3. `clap_rs` 对所有端口固定上报 `port_type=stereo`（mono 布局出现后即为错报）。
  4. 两个 `_rs` 层把 state 版本硬编码为 1，插件声明的 `STATE_VERSION` 从未写入或比对，
     `migrate_state` 因此永远不可能触发。
  共同教训：**Phase 2 的测试覆盖了 `_rs` 与 core/fixture 两端，缺口都在中间的 backend
  适配层**；M1 第 3、4 项要求的"宿主侧断言"与"backend 端到端映射测试"正是补这个位置，
  且一落地就各自抓出一个真实缺陷。后续 milestone 的新能力应默认补 backend 层端到端测试。
- Evidence/artifact: run #55 上传三平台 artifacts（49.5MB / 73.9MB / 915.8MB），均可下载。
- Unresolved: M1 完成。下一个瓶颈是 M2：参数分组/嵌套（VST3 `IUnitInfo` ↔ CLAP module
  路径）、零分配参数 smoothing、effect/instrument template（新插件样板 ≤50 行），
  由 `examples/sunmao_syn_grouped_params` fixture 消费验证。
  **注意**：VST3 侧参数分组需要 `IUnitInfo`，而 `vst3_sys` **尚无该绑定**
  （做 preset program list 时已确认），M2 将需要自 `_sys` 层补起。

### 2026-08-29 — M2 第一项：参数分组/嵌套（自 `_sys` 层补起）

- Command/platform: macOS ARM64，分支 `phase3/framework-dsp-library`。
- Change:
  - **自底向上**：`vst3_sys` **完全没有 `IUnitInfo` 绑定**（做 preset 时已确认），
    故先补 `vst/ivstunits.rs`——`UnitInfo`/`ProgramListInfo` 结构、`kRootUnitId`/
    `kNoParentUnitId`/`kNoProgramListId`/`kAllProgramInvalid` 常量、12 个方法按
    上游顺序排列的 vtbl。**IID 自上游 `vst/ivstunits.h` 转录而非凭记忆**：
    实际值 `0x3D4BD6B5,0x913A4FD2,0xA886E768,0xA5EB92C1` 与我记忆中的后三段不同，
    若照记忆写会导致宿主永远查不到该接口且**静默无表现**。
  - core：`ParamDescriptor.group`（`/` 分隔路径，空＝顶层）+ `params::group_segments`
    规范化辅助（带 doc-test，丢弃空段而非报错——为斜杠这种纯外观问题让插件加载失败不值得）。
  - macros：`#[group = "..."]` 与 `#[param(group = "...")]`，并把 `group` 注册进
    derive 的 helper attributes（否则编译期报 "cannot find attribute"）。
  - `vst3_rs`：新增 `units.rs`——`UnitTable::from_paths` 把路径集合展开为 unit 树，
    **中间层级即使无参数直接命名也会创建**，且保证父先于子（6 个单测钉住）。
    `ParamInfo.group` + `.group()` builder；`get_parameter_info` 的 `unit_id`
    由 `unit_table_for(params).unit_for(group)` 得出（原为硬编码 0）。
    `IUnitInfo` 经**带回指针的 shim** 暴露：两个 controller wrapper 现有的
    `from_connection` 等恢复逻辑依赖字段偏移，再插一个指针会平移既有偏移，
    故改用独立分配 + owner 回指针，风险更低；仅在存在分组时创建，
    无分组插件对 `IUnitInfo` 仍返回 `kNoInterface`。
  - `clap_rs` **修正既有缺陷**：`params` 扩展此前把 `info.module` 无条件清零
    （两条 GUI/非 GUI 路径都是），即插件声明的层级根本到不了宿主；现按 `ParameterInfo.module` 写入。
  - backend 两侧桥接；fixture `sunmao_syn_grouped_params` 换用真实分组
    （`Osc`、`Osc/Tuning`、`Filter`、`Amp/Envelope`——含嵌套与共享分组）。
- Result:
  - 新增测试 11：`vst3_rs::units` 6、backend_clap 1（走真实 `clap.params` 断言路径逐字送达）、
    backend_vst3 2（走真实 `IUnitInfo`+`IEditController`：unit 树/单层名/越界拒绝/
    每参数 `unit_id` 正确/无分组留 root；以及扁平插件不暴露接口）、fixture 2。
  - `RUSTFLAGS=-Awarnings cargo test --locked` 115 套件全绿、exit 0。
  - metadata/fmt/diff-check 通过；`cargo check --locked --target x86_64-pc-windows-msvc`
    覆盖 8 个改动 crate 通过；`tools/package_examples.sh --debug --test` 退出 0、
    24 套件各 19/19。
- Evidence/artifact: macOS ARM64 本地日志（`/tmp/m2a_test.log`、`/tmp/m2a_pkg.log`、
  `/tmp/m2a_win.log`）——本地证据等级。
- Unresolved: 待三平台 hosted 全绿。M2 余下两项：零分配参数 smoothing、
  effect/instrument template（新插件样板 ≤50 行）。
