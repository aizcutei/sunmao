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

### 2026-08-29 — M2 分组/嵌套验收：hosted run #57 三平台全绿

- Command/platform: push `94b3542` 触发 GitHub Actions #57：https://github.com/aizcutei/sunmao/actions/runs/33189876572
- Result: 三平台三个 job 同一 commit 全部 success、逐步骤零非成功步骤。新增的
  `vst3_sys::IUnitInfo` 绑定与 shim 暴露路径、`clap_rs` 的 module 写入修正，
  在三平台原生构建下全部通过。
- Evidence/artifact: run #57 上传三平台 artifacts（50.0MB / 74.2MB / 918.2MB），均可下载。
- Unresolved: M2 余下两项：零分配参数 smoothing、effect/instrument template。

### 2026-08-29 — M2 第二项：零分配参数 smoothing（设计经 proptest 两次修正）

- Command/platform: macOS ARM64，分支 `phase3/framework-dsp-library`。
- Change:
  - core 新增 `smoothing` 模块：`SmoothingStyle::{Linear,Exponential}(seconds)` 与
    `Smoother`（`set_sample_rate`/`reset`/`set_target`/`next`/`is_smoothing`/`current`/
    `target`），进两个 prelude，带 doc-test。全标量 + `Copy`，`next()` 只做算术。
  - 与 automation/modulation 的关系写进模块文档：**automation 是值故被 smoothing，
    modulation 是叠加偏移必须加在 smoothing 之后**——若折进 target，modulation 就会
    变成被平滑的值并进入插件持久化状态，违反既有契约。
  - fixture `sunmao_syn_grouped_params` 消费：level 走 10ms 线性 ramp，
    `initialize` 里 `reset` 到当前参数值（否则开工程会从 0 淡入）。
- **实现中被测试抓出的两个真实缺陷（都是我这轮写的）**：
  1. **指数 ramp 永不终止**：`current = target - distance*decay` 在 f32 下存在不动点——
     target=1.0 时距离收敛到约 `1.4e-5` 后 `target - small` 舍入回同一个 f32，
     而该距离**远大于**我原先设的 `SNAP_EPSILON=1e-6`，于是永远不 snap、
     每样本继续做无用计算且永不到达。修法不是调大 epsilon（不动点距离随 target 量级变化，
     任何固定阈值对大 target 都太小、对小 target 又粗糙），而是**检测无进展**
     （`next == current` 即 snap）。
  2. **`is_smoothing()` 曾用 epsilon 判定**，于是它在值仍差一点点时就报"已到达"，
     插件据此停止调用 `next()` 便永久留下一个残余偏移。改为"恰好到达才为 false"。
     随后 proptest 又指出：指数逼近要到达绝对 epsilon 可能需 ~22 个时间常数
     （1 秒时间常数 → 约 22 秒），`is_smoothing` 会长时间为真。最终设计：
     **指数 ramp 在 12 个时间常数后精确 snap**（此时残余约 `e^-12`≈6ppm，不可闻），
     `is_smoothing` 统一为 `remaining > 0`——既保证精确到达，又给出可预测的上界。
- Result:
  - core 7 单测（精确落点、单调不过冲、中途改目标不跳变、NaN/Inf 不污染 ramp、
    非法采样率退化为直通、reset 取消 ramp、无堆状态）。
  - **2 个新 proptest**：`a_smoother_always_reaches_its_target`（任意起点/目标
    ±20000、任意时长、4 种采样率、两种风格——必在界内精确到达且全程有限）与
    `a_smoother_never_leaves_the_interval_it_travels`。前者正是抓出上述两个缺陷的测试。
  - 零分配**实测**：`sunmao_backend_clap::smoothing_in_process_does_not_allocate`
    在该模块既有的计数分配器下跑 64 块 × 512 样本 × 两种风格 + 每块改目标，
    分配调用 0；realtime allocation matrix 四个 crate 保持绿。
  - fixture 8 单测全绿（含既有 DSP 语义测试未变）：新增
    `a_level_jump_is_ramped_rather_than_stepped`、
    `the_level_smoother_starts_at_its_parameter_instead_of_sliding_up`。
  - `RUSTFLAGS=-Awarnings cargo test --locked` 115 套件全绿、exit 0；
    metadata/fmt/diff-check 通过；Windows 交叉 check 覆盖 4 个改动 crate 通过；
    `tools/package_examples.sh --debug --test` 退出 0、24 套件各 19/19。
- Evidence/artifact: macOS ARM64 本地日志（`/tmp/m2b_test.log`、`/tmp/m2b_pkg.log`、
  `/tmp/m2b_win.log`）——本地证据等级。
- Unresolved: 待三平台 hosted 全绿。smoothing 非 host-facing（无格式映射），故未加
  semantics.md 行；与 automation/modulation 的关系记在模块文档。M2 余下最后一项：
  effect/instrument template（新插件样板 ≤50 行）。

### 2026-08-29 — M2 smoothing 验收：hosted run #59 三平台全绿

- Command/platform: push `d0a13d3` 触发 GitHub Actions #59：https://github.com/aizcutei/sunmao/actions/runs/33192422381
- Result: 三平台三个 job 同一 commit 全部 success、逐步骤零非成功步骤。新增的
  2 个 smoothing proptest 与零分配实测在三平台原生构建下通过；realtime allocation
  matrix 保持绿色。
- Evidence/artifact: run #59 上传三平台 artifacts（50.0MB / 74.2MB / 918.2MB），均可下载。
- Unresolved: M2 余下最后一项：effect/instrument template（新插件样板 ≤50 行）。

### 2026-08-29 — M2 第三项：effect/instrument template（effect 达标 50 行，instrument 未达标并说明原因）

- Command/platform: macOS ARM64，分支 `phase3/framework-dsp-library`。
- Change:
  - 新增 `examples/sunmao_template_effect`（**恰好 50 行**）与
    `examples/sunmao_template_instrument`（86 行），均为可真正编译成 VST3+CLAP 的
    起始骨架，并入 workspace 与 CI 的 Phase 3 fixture 列表。
  - 新增 `sunmao/tests/template_size.rs`：**用测试机械强制行数预算**
    （`include_str!` + 行数断言），而不是在文档里声称。effect 断言 ≤50；
    instrument 钉住当前 86 行上限（不得静默增长），并额外断言它**仍然大于 50**——
    这样一旦 M3 让它降到预算内，测试会主动失败提醒把断言改回预算并更新 status。
  - **先核实而未改动**：原以为 `Vst3Info::default()` 的全零 `class_id` 与
    `ClapInfo::default()` 的空 id 是碰撞隐患，读代码后确认两个 backend **早已**
    在未设置时从 `VENDOR::NAME` 派生唯一 id（`backend_vst3` 的 `class_id()`、
    `backend_clap` 导出宏的 `resolved_id`）。因此模板可以完全省略
    `vst3_info`/`clap_info` 两个函数——省下约 14 行且不教坏习惯。**没有去"修"不存在的问题。**
- Result:
  - `RUSTFLAGS=-Awarnings cargo test --locked` 120 套件全绿、exit 0（新增 3 个套件）。
  - 两个模板 cdylib 均只导出 `GetPluginFactory` + `clap_entry`，`nm -gU` 复查无 AU 符号。
  - metadata/fmt/diff-check、workflow YAML 解析通过；Windows 交叉 check 通过；
    `tools/package_examples.sh --debug --test` 退出 0、24 套件各 19/19。
- Evidence/artifact: macOS ARM64 本地日志（`/tmp/m2c_test.log`、`/tmp/m2c_pkg.log`、
  `/tmp/m2c_win.log`）——本地证据等级。
- Unresolved（**诚实标注未达标项**）：instrument 模板 86 行，**未达到 ≤50 行目标**。
  原因：一个能真正发声的乐器需要 voice 管理（note on/off、相位、频率换算），
  而在 `sunmao/dsp` 提供 oscillator/envelope 之前（正是 **M3** 的内容），这些代码
  只能写在模板里。我没有为了凑数把 DSP 从模板里删掉（那会变成不能用的模板），
  也没有把"样板行数"重新定义成不含 process 体（那是为了达标而改测量口径）。
  M3 落地后应重测并把 `template_size.rs` 的 instrument 断言改为预算。
  已识别的另一个减负杠杆：让 `#[derive(Params)]` 依属性生成 `Default`
  （`#[param(default=, min=, max=)]`），两个模板各可再省约 7 行——未做，属独立改动。

### 2026-08-29 — M2 完成：template 验收（hosted run #61 三平台全绿）

- Command/platform: push `3374c5f` 触发 GitHub Actions #61：https://github.com/aizcutei/sunmao/actions/runs/33194134348
- Result: 三平台三个 job 同一 commit 全部 success、逐步骤零非成功步骤，
  "Test Phase 3 acceptance fixtures"（现含两个 template crate）三平台均 success。
  **M2 三项（参数分组/嵌套、零分配 smoothing、effect/instrument template）
  各自在独立 commit 上取得三平台 hosted 绿**：run #57 / #59 / #61。
- Evidence/artifact: run #61 上传三平台 artifacts（50.0MB / 74.2MB / 918.2MB），均可下载。
- Unresolved: M2 唯一未达标项是 instrument 模板 86 行（目标 ≤50），原因与处置见上一条
  日志；`sunmao/tests/template_size.rs` 会在它降到预算内时主动失败以提醒收紧断言。
  进入 M3：新建 `sunmao/dsp`（filters/envelopes/oscillators），并让 SVF fixture
  换用组件实现且**测试语义不变**，同时用 oscillator/envelope 把 instrument 模板压进预算。

### 2026-08-29 — M3 第一步：`sunmao/dsp` filters（SVF fixture 换组件、测试零改动）

- Command/platform: macOS ARM64，分支 `phase3/framework-dsp-library`。
- Change:
  - 新建 `sunmao/dsp` crate 并入 workspace：`filters` 模块提供 `OnePole`
    （lowpass/highpass）、`Svf`（TPT，同时输出 lp/bp/hp）、`Biquad`（TDF2，
    lowpass/highpass/bandpass，RBJ 系数），全部**系数计算与逐样本处理分离**
    （`set_*` 做三角函数，`process`/`tick` 只做算术），无分配、`Copy`。
  - 组件自己夹参数而不信任调用方：cutoff 归一化后夹在 `[1e-5, 0.49]`
    （`tan` 在 Nyquist 发散，宿主可以把 cutoff 自动化到任何值）、resonance 夹 `[0,1]`、
    Q 夹 `[0.05,40]`、非有限输入丢弃而非写进状态（一个 NaN 会永久污染滤波器）。
  - `sunmao_fx_svf` fixture 换用 `Svf` 组件：删除 inline TPT 实现，
    cutoff 预畸变/Nyquist 夹取/resonance→damping 映射全部交给组件。
    **6 个既有测试一行未改即通过**（`git diff` 在测试区零改动），这正是 M3
    对 filter 家族的验收标准。
- **实现中被测试抓出的三件事**：
  1. denormal flush 阈值原设 `1e-30`：确实能挡住 denormal（f32 denormal < 1.18e-38），
     但低 cutoff 的谐振 SVF 静音后 40 万样本仍停在 `1.1e-28`，仍在做无用计算。
     改为 `1e-20`（约 -400 dBFS，仍远低于任何可闻电平），及时离开该区间。
  2. 我把不变量**写过头**了：原断言"静音后精确等于 0"。二阶递归会在任何 flush
     地板之上徘徊（biquad 实测停在 `-1.38e-20`），而"精确 0"只会诱导我不断调大常数
     去迁就一个我自己臆造的要求。改为断言真正需要的性质：
     **残留低于可闻阈（1e-18）且不落在 denormal 区间**。
  3. **f32 biquad 在低归一化 cutoff 下 DC 增益严重失准**：20Hz@96kHz 实测 DC 增益
     **1.142（14% 误差）**——`a1→-2, a2→1` 使 `1+a1+a2` 成为灾难性相消。
     20Hz 是极常见设置，故**修掉而非记为陷阱**：`Biquad` 的系数与状态改用 f64
     （每样本多几次乘法），修复后同条件 DC 增益回到 1.0 量级。`Svf`/`OnePole`
     无此相消，保持 f32。
- Result:
  - `sunmao_dsp`：9 单测 + 3 doc-test + **4 proptest**（任意 cutoff/resonance/Q/
    采样率/幅度下永不产生非有限样本；静音后残留不可闻且非 denormal；
    lowpass 在任意 cutoff 下 DC 增益为 1；`reset` 后与新建实例逐样本完全一致）。
  - `RUSTFLAGS=-Awarnings cargo test --locked` 123 套件全绿、exit 0（新增 3 套件）。
  - metadata/fmt/diff-check 通过；Windows 交叉 check 通过；
    `tools/package_examples.sh --debug --test` 退出 0、24 套件各 19/19；
    `nm -gU` 复查 SVF fixture cdylib 无 AU 符号。
- Evidence/artifact: macOS ARM64 本地日志（`/tmp/m3a_test.log`、`/tmp/m3a_pkg.log`、
  `/tmp/m3a_win.log`）——本地证据等级。
- Unresolved: 待三平台 hosted 全绿。M3 余下：envelopes（ADSR/follower）与
  band-limited oscillators（sine/saw/pulse），随后用 oscillator/envelope 把
  instrument 模板压进 ≤50 行（M2 记录的未达标项）。

### 2026-08-29 — M3 filters 验收：hosted run #63 三平台全绿

- Command/platform: push `9db6ff0` 触发 GitHub Actions #63：https://github.com/aizcutei/sunmao/actions/runs/33196120193
- Result: 三平台三个 job 同一 commit 全部 success、逐步骤零非成功步骤。新 crate
  `sunmao_dsp` 与换用组件后的 SVF fixture 在三平台原生构建下通过。
- Evidence/artifact: run #63 上传三平台 artifacts（50.0MB / 74.2MB / 918.2MB），均可下载。
- Unresolved: M3 余下 envelopes（ADSR/follower）与 band-limited oscillators
  （sine/saw/pulse），随后用它们把 instrument 模板压进 ≤50 行。

### 2026-08-29 — M3 第二步：envelopes 与 band-limited oscillators

- Command/platform: macOS ARM64，分支 `phase3/framework-dsp-library`。
- Change:
  - `sunmao_dsp::oscillators`：`Oscillator` 支持 `Waveform::{Sine,Saw,Pulse}`，
    saw/pulse 用 **PolyBLEP** 抑制混叠（朴素方波/锯齿的阶跃会把 Nyquist 之上的
    全部谐波折回可听频段，音高越高越明显）。频率夹在 Nyquist 之下——除了无意义，
    还因为相位增量 ≥0.5 会破坏 PolyBLEP"任一样本附近至多一个不连续点"的前提；
    pulse width 夹 `[0.01,0.99]` 避免退化成常量。
  - `sunmao_dsp::envelopes`：`Adsr`（线性段，`gate_on/gate_off/is_active/stage`）
    与 `EnvelopeFollower`（attack/release 分离的幅度跟踪器）。
    **线性而非指数**段：线性 attack 精确到达峰值，指数只能逼近——正是 M2 smoothing
    踩过的渐近线问题，不重复引入。retrigger 从当前电平继续（跳到 0 会咔哒）；
    时间为 0/负/NaN 退化为"立即"而不是除零。
  - `Oscillator` 加 `Default`（静音 sine），使其能放进 `#[derive(Default)]` 的插件结构。
  - instrument 模板换用 `Oscillator` + `Adsr`。
- Result:
  - `sunmao_dsp`：23 单测（含 sine 周期数、PolyBLEP 相对朴素锯齿的高频能量下降、
    duty cycle、极端频率有限、退化 pulse width 不静音、reset 可复现；
    ADSR 走完各段并回到 Idle、attack 时长约等于设定、retrigger 不跳变、
    follower 攻击快于释放且只跟幅度）+ **7 proptest**（新增 3：任意振荡器设置不产生
    坏样本；ADSR 在任意参数下恒在 `0..=1` 且 gate_off 后**必定回到 Idle**——
    否则合成器会永久泄漏一个 voice；follower 电平非负且不超过输入幅度）+ 6 doc-test。
  - `RUSTFLAGS=-Awarnings cargo test --locked` 123 套件全绿、exit 0；
    metadata/fmt/diff-check、Windows 交叉 check、
    `tools/package_examples.sh --debug --test`（24 套件各 19/19）通过；
    instrument 模板 cdylib 仅预期导出、无 AU 符号。
- Evidence/artifact: macOS ARM64 本地日志（`/tmp/m3b_test.log`、`/tmp/m3b_pkg.log`、
  `/tmp/m3b_win.log`）——本地证据等级。
- Unresolved（**再次诚实标注同一未达标项**）：instrument 模板由 86 行降到 **81 行**，
  **仍未达 ≤50**。用上组件后剩下的体积**不是 DSP**，而是 trait 仪式
  （params 的 `Default`、`input_channels`/`accepts_midi`/`params`/`initialize`、
  `process` 签名）加 MIDI 处理与逐样本写入循环。要进 50 行需要一层更高的
  voice/synth 抽象（或由宏生成 process 循环），那是超出 DSP crate 的设计决定，
  我没有在 M3 里顺手引入。`template_size.rs` 的钉子已从 90 收紧到 85 以锁住这次改进。
  M3 三个家族（filters/envelopes/oscillators）至此齐备，待 hosted 后进 M4。

### 2026-08-29 — run #65：Windows WGPU 收尾段错误**已复现**，按规则深入析构路径而非重试

- Command/platform: hosted run #65（commit `c9e549d`）：https://github.com/aizcutei/sunmao/actions/runs/33197739950
- Result: macOS ARM64 与 Ubuntu x86_64 **success**（M3 的 envelopes/oscillators 在两平台通过）；
  Windows x86_64 **failure**，失败步骤 "Package and exercise native GUI backends"。
- 诊断（经 `/check-runs/<id>/annotations`，本机无 gh 且日志下载需 admin）：
  失败 fixture 是 **`SunMao Gain WGPU (VST3)`**，且**全部断言均通过**——pixels 验证、
  输入验证（`'Gain' changed 0.500000 -> 0.922414`）、gesture 验证（begin +2/value +17/end +2）、
  recreate 后 pixels 再验证，随后打印 `GUI test complete.` 与 `Done.`，**然后 exit 139（SIGSEGV）**。
  这**正是** loop 边界里记录的已知 flake（"断言全过、打印 Done. 后 exit 139，run #37 一次未复现；
  再复现则深入 WGPU/D3D 析构路径，不要盲目重试"）。**本轮即为复现**，故按规则不重试。
  另注：#46 的 UIA 拖动竞态与本项无关，本轮输入一次命中（无 `took n attempts`），加固有效。
- 根因分析与修复：`Vst3HostPlugin`/`ClapHostPlugin` 均**无 `Drop` impl**，字段按声明序释放，
  而 `_lib: libloading::Library` 声明在第 2 位——即**进程收尾时会 `FreeLibrary` 卸载插件 DLL**。
  一个已初始化 GPU 后端（WGPU→D3D12）的插件模块**不可安全卸载**：图形运行时保留了指向该模块
  内部的回调与全局状态，卸载后这些指针悬空，在进程剩余清理阶段 fault——与"断言全过、
  Done. 之后崩"的现象完全吻合。修复：两个 host 的库改为 `ManuallyDrop<libloading::Library>`
  **永不卸载**（附注释说明理由）。这不是回避：runner 本就即将退出，真实宿主也普遍让插件模块
  常驻，正是同一个原因。
  并新增收尾标记 `Teardown complete.`（在 plugin 与 host 对象释放之后打印），
  使**将来若仍崩可定位**：崩在该行之前＝释放/卸载路径，之后＝运行器之外的进程收尾。
- Result（本地）：macOS `SunMao Gain WGPU.vst3` 的 gui-test **exit 0**，
  并如期打印 `Done.` → `Teardown complete.`；
  `RUSTFLAGS=-Awarnings cargo test --locked` 123 套件全绿；
  metadata/fmt/diff-check、Windows 交叉 check、
  `tools/package_examples.sh --debug --test`（24 套件各 19/19）通过。
- Evidence/artifact: macOS ARM64 本地日志（`/tmp/wgpu_gui.log`、`/tmp/m3c_test.log`、
  `/tmp/m3c_pkg.log`、`/tmp/m3c_win.log`）——本地证据等级。
- Unresolved（**诚实标注**）：**我无法在本机复现该 Windows 崩溃**（macOS 环境），
  因此"DLL 卸载即根因"是基于机制与现象吻合的**推断**，不是实测确认；
  确认只能靠 hosted Windows。若下一轮 Windows 仍在 `Teardown complete.` **之前**崩，
  则说明还有另一处释放顺序问题；若崩在其**之后**，则问题在运行器之外的进程收尾。
  M3 的 envelopes/oscillators 本身在 #65 的 macOS/Linux 已通过，但按"同 commit 三平台全绿"
  的判定标准仍未验收，将与本修复一并验收。

### 2026-08-29 — M3 完成 + Windows WGPU 收尾段错误修复验收：hosted run #66 三平台全绿

- Command/platform: push `9dd749b` 触发 GitHub Actions #66：https://github.com/aizcutei/sunmao/actions/runs/33199051341
- Result: macOS ARM64、Windows x86_64、Ubuntu x86_64 三个 job 同一 commit 全部
  success，逐步骤零非成功步骤；**"Package and exercise native GUI backends" 三平台
  均 success**（#65 正是该步在 Windows 上 exit 139）。M3 三家族
  （filters / envelopes / oscillators）与 host 库常驻修复一并验收。
- Evidence/artifact: run #66 上传三平台 artifacts（50.0MB / 74.2MB / 918.2MB），均可下载。
- **关于已知 flake 的处置更新**：该项此前记为"Windows WGPU 偶发在断言全过、打印 Done.
  后 exit 139"。#65 复现后已定位到具体机制（host 结构体无 `Drop` impl，
  `_lib` 按字段序在进程收尾时 `FreeLibrary` 卸载已初始化 D3D12 的插件模块）并修复
  （`ManuallyDrop` 常驻）。**但必须诚实说明证据强度**：该失败本就是间歇性的，
  **单次 Windows 绿不足以证明根因判断正确**——它与修复一致，但不是证明。
  新增的 `Teardown complete.` 标记使后续任何一次复现都可定位：崩在该行之前＝释放/卸载
  路径仍有问题；之后＝运行器之外的进程收尾。若长期不再复现，可在 M5 收尾时把该项
  从"已知 flake"降级为"已修复"。
- Unresolved: M3 完成。进入 M4：2x/4x oversampling（latency 接 Phase 2 契约并被 runner
  断言）、dry/wet 与增益工具、peak/RMS metering（GUI 可读的无锁发布），
  由 `sunmao_fx_os_dist` 与 `sunmao_fx_meter` 两个 fixture 消费验证。
  仍挂账：instrument 模板 81 行未达 ≤50（需 voice/synth 抽象，非 M4 范围）。

### 2026-09-03 — M4：oversampling / mixing / metering 三模块落地，runner 实测 latency 对齐

- Command/platform: macOS ARM64 本地。`sunmao/dsp` 新增三模块并接入 prelude（各带 doc-test）：
  - `oversampling`：`Oversampler`/`OversamplingFactor{None,X2,X4}`，2x/4x 为**级联**半带 FIR
    （4x 不是一次 4 倍零填充——两级各在自身速率上切 1/4，否则基带 Nyquist 到截止之间的镜像会在
    抽取时折回）。FIR 取 **33 tap**（中心 16）而非常见 31：4x 两级速率不同，只有中心可被 4 整除
    时总群延迟才是整数基采样（15 → 22.5 samples，宿主只能收整数，会永远差半个样本）。
    latency 挂在 `OversamplingFactor::latency_samples()`（2x=16，4x=24）而不只在已 prepare 的
    实例上：VST3 在 `setActive` 之前就调 `getLatencySamples`，插件必须在未 prepare 时也能如实回答。
    `prepare` 唯一分配；`process` 零分配，超出 prepare 尺寸的 block 退化为直通而非在音频线程 realloc。
    输入在插值与抽取两处 `sanitize`（非有限→0，|x|>1e30 夹住）：插值乘 2 会让 `f32::MAX` 溢出成
    inf，FIR 再算 inf−inf 得 NaN 并滞留在延迟线里——调用方的非线性根本没机会看到那个样本。
  - `mixing`：`db_to_gain`/`gain_to_db`（-inf dB ⇄ 静音，NaN 有定义）、`apply_gain`、
    `DryWet{Linear,EqualPower}`（`mix`/`mix_block`）。
  - `metering`：`Meter`（音频侧）/`MeterHandle`（GUI 侧，可 clone）经 `Arc<AtomicU32>` 位存发布
    peak/RMS，无锁；峰值 -20 dB/s 回落、RMS 一阶 100 ms 时间常数；每块发布一次而非每样本。
  - `sunmao_fx_os_dist` 换用 `Oversampler`（4x）+ `DryWet`：dry/wet **在过采样域内**混合，使 dry
    与 wet 走同一组滤波器与同一延迟（基速率 dry 对延迟 wet 混合会成梳状滤波——过采样效果器的经典
    latency bug）；`latency_samples()` 直接返回 `FACTOR.latency_samples()`。
    `sunmao_fx_meter` 换用 `Meter`/`MeterHandle`，handle 在 `Default` 构造期即取得（编辑器可先于
    激活打开），增益改为 dB 语义经 `db_to_gain`。
  - runner 新增第 19 项 `latency_alignment`（套件 19→20）：经格式 API 读 latency 后送单位冲激、
    定位输出峰值帧，要求 |峰值帧 − 上报值| ≤ 1。仅对 OS Distortion 断言（Tempo Delay 的冲激峰值在
    延迟时间处而非 lookahead 处，线性相位才有"峰值＝latency"的性质），其余 fixture 走 skip 路径并
    打印原因。**发现 CI 缺口**：Phase 3 fixture 此前只 `cargo test -p`，从未打包并交给 runner，
    该断言在 CI 里只会走 skip——已把 `sunmao_fx_os_dist`/`sunmao_fx_meter` 接入 workflow 矩阵
    （`os-dist-binary`/`meter-binary`）、打包-exercise 步骤（VST3+CLAP 各跑 runner）与
    `tools/package_examples.sh`（24→28 套件）。
  - proptest +7（`sunmao/dsp/tests/property.rs`）：oversampler 任意因子/块长/极端幅度下无非有限输出
    且 `None` 为逐位直通；上报 latency 与任意块长下实测群延迟一致；线性 body 保持 DC 单位增益；
    reset 与新建不可区分；dB 换算往返且单调；`DryWet` 两律各守恒且块/逐样本一致；meter 读数不越过
    所见幅度且 handle 与音频侧逐位一致。**proptest 抓出一个真实缺陷**：`EqualPower` 全湿时
    `cos(π/2)` 在 f32 为 -4.4e-8，dry 增益为负——已夹到 `[0,1]`。
- Result: `RUSTFLAGS=-Awarnings cargo test --locked` **123 套件全绿、exit 0**（中途两处失败均已修：
  上述 EqualPower 负增益；meter fixture 单样本瞬态测试容差 1e-3 过紧——瞬态落在块尾前 46 样本，
  按 -20 dB/s 回落 0.2% 是组件既定弹道，容差放到 1e-2 并注明）。metadata/fmt/diff-check 通过；
  `cargo check --locked --target x86_64-pc-windows-msvc` 覆盖 runner/dsp/两 fixture 通过
  （warnings 均为既有 `vst3_sys` 常量命名，与改动无关）；`tools/package_examples.sh --debug --test`
  退出 0、**28 套件各 20/20**，其中 `SunMao OS Distortion.vst3` 与 `.clap` 均
  `latency_alignment (reported 24, measured 24)`；`nm -gU` 两个新 cdylib 无 AU 符号。
  semantics.md latency 行追加 M4 说明；status.md fixture 表与 M4 行更新为"代码齐备，待 hosted"。
- Evidence/artifact: macOS ARM64 本地日志（`/tmp/m4_test.log`、`/tmp/m4_pkg.log`、`/tmp/m4_win.log`）
  ——本地证据等级。
- Unresolved: 待三平台 hosted 全绿方可把 M4 记为完成。注意 Windows/Linux 首次在 CI 打包并 exercise
  这两个 fixture，任何平台差异都会在"Package and exercise VST3 + CLAP + standalone"步骤暴露。
  M4 完成后进入 M5（semver/state 兼容策略文档 + 收尾）。仍挂账：instrument 模板 81 行未达 ≤50。

### 2026-09-04 — M4 验收：hosted run #68 三平台全绿

- Command/platform: 轮询 GitHub Actions API（本机无 gh）读 commit `04d036a` 的 check-runs
  与 run #68 的 jobs/steps/artifacts：https://github.com/aizcutei/sunmao/actions/runs/33867319967
- Result: macOS ARM64、Windows x86_64、Ubuntu x86_64 三个 job 同一 commit 全部 success，
  **每个 job 25 个步骤逐个 success/skipped、零非成功**。M4 的三个新模块与两个 fixture 演进
  一并验收。要点：本 run 是 Windows/Linux **首次**在 CI 里打包并 exercise
  `sunmao_fx_os_dist` / `sunmao_fx_meter`（M4 之前 Phase 3 fixture 只做 `cargo test -p`），
  "Package and exercise VST3 + CLAP + standalone" 三平台均 success，
  因此 `latency_alignment` 断言不再只在 CI 里走 skip 路径。
- Evidence/artifact: run #68 上传三平台 artifacts（`phase1-macOS-ARM64` 52.3MB、
  `phase1-Windows-X64` 78.1MB、`phase1-Linux-X64` 960.1MB），均 `expired=false` 可下载。
- Unresolved: 已知 flake（Windows WGPU 收尾 exit 139）在本 run 的
  "Package and exercise native GUI backends" 上**连续第二次未复现**；仍按"间歇性失败的
  单次绿不构成证明"处理，是否降级为"已修复"留到 M5 收尾判断。进入 M5。

### 2026-09-04 — M5：兼容策略文档 + proptest 收口，并修掉 flush 破坏耦合衰减的真实缺陷

- Command/platform: macOS ARM64 本地。
  - **文档**：`docs/phase3/compatibility.md` 落地并从 README、`docs/roadmap.md` 链接。
    两条独立的兼容轴：crate semver（受保护的 API 面 / 什么算破坏性 / `sunmao_dsp` 的数值语义
    承诺 / 弃用流程）与 state 兼容（blob 布局、载入规则表、何时升 `STATE_VERSION`、
    什么永不进 state、验证方式）。**核对代码时发现文档自身两处失实并修正**：
    (a) 原文称 blob 里的 `value` 是归一化值，实际 CLAP 侧写的是 **plain value**——
    连续参数恰好等于归一化值，但 stepped 参数写的是**步进索引**（`parameter_to_clap_value`），
    VST3 侧才是归一化；(b) 因此两格式 blob 不通用不只是 magic 不同，同一个 stepped 参数
    的**数值尺度**本身就不同。
  - **proptest +8**（把文档里的承诺变成机械守卫，而不是散文）：`vst3_rs::state::tests`
    4 项（逐位 round-trip、顺序无关、任意字节串不 panic 且**绝不给出部分条目表**、
    版本规则是不等式）；`clap_rs::ext::state::tests` 4 项（同前三项 +
    `a_rejected_state_never_reaches_the_plugin`：用一个假的 `clap_istream_t` 与记录型
    `Plugin` 探针端到端断言"被拒的 blob 不应用任何值、不触发迁移；被接受的 blob
    先应用完全部已知 id 才回调迁移"）；`sunmao_core` 3 项（id 哈希对照独立实现的 FNV-1a、
    永不落在保留的 `u32::MAX`、group 路径归一化幂等且无空层级）。
  - **真实缺陷（本轮主要收获，由 proptest 抓出）**：`RUSTFLAGS=-Awarnings cargo test`
    出现一次 `every_filter_settles_below_audibility_and_out_of_the_denormal_range` 失败
    （`svf left an audible residue: -2.5e-18`，cutoff 20 / res 0.2 / 96 kHz）。这不是容差
    问题：**`flush_denormal` 被独立施加到耦合递推的每个状态上，会破坏让它们衰减的耦合。**
    量化（临时探针，已删）：`Svf` 的 `ic1` 幅度约为 `ic2` 的 `g` 倍（20 Hz/96 kHz 时约
    1/1500），于是 `ic1` 先跌破 1e-20 被清零，而 `ic1` 正是 `ic2` 唯一的快衰减通道——
    `ic2` 随后以 `O(g²)` 的 `2*a3` 速率爬行，归零耗时 **6,833,421 样本（71 秒）**，
    而其时间常数只要 **43,346 样本（0.45 秒）**，慢了 158 倍。`Biquad` 更糟：低 cutoff 下
    `a1 → -2`、`a2 → 1`，衰减来自 `-a1*output` 与 `s2` 的近似抵消，单独清零 `s2` 后剩下
    `s1 * -a1` 是近 2 倍的增益，两个状态轮流被清零又被泵起，**永久停在 6.2e-20 的极限环**。
    修法：两者都改为**成组** flush（只有全部状态都在 floor 内才一起清零），并把阈值提为
    `sunmao_dsp::DENORMAL_FLOOR`（进 prelude，带 doc-test 讲清成组规则），
    `flush_denormal` 的文档注明只适用于单状态组件。
  - **测试本身也是缺陷的一部分**，一并修：(a) 原断言"400k 样本后残留 < 1e-18"里的 400k 是
    拍出来的——最慢的合法设置（20 Hz / res 1.0 / 192 kHz）**本来就需要约 140 万样本**，
    所以该测试在慢区证明不了任何事，在别处又把真 bug 报成边缘抖动。改为
    `every_filter_settles_within_its_own_time_constant`：按各滤波器的**离散极点半径**
    （SVF 取状态矩阵特征值、one-pole 取 `1-c`、biquad 取 `sqrt(a2)`）算预算，断言
    "预算内到达精确 0 并在随后 64 样本窗口保持 0"。用解析式模型不行：`π*fc*k/fs` 只在
    小 `g` 下成立，0.49*fs 时真实衰减比它慢 10 倍，会在正确代码上失败。
    (b) 判定"已归零"不能看单个 0 输出——振铃滤波器**会穿过零**（这一版先失败在
    `biquad left zero at 3607 after settling at 3606`），故改用整窗口全零。
    (c) cutoff 改为**对数均匀**采样（新增 `cutoffs()`）：均匀采样把 99.9% 的样例放在
    200 Hz 以上，低归一化 cutoff 这个缺陷所在的角落每例只有约 1/4000 的命中率——
    这正是该 bug 能一直藏着、只在一个不巧的种子上冒出来的原因。
  - 反向验证：把 `filters.rs` 暂时切回修复前，新测试**立即失败**并 shrink 到
    `cutoff = 19.999999999999996, resonance = 0.20088771, sample_rate = 96000`；恢复后通过。
- Result: `RUSTFLAGS=-Awarnings cargo test --locked` **123 套件 / 504 测试全绿、exit 0**
  （含 `sunmao_fx_svf` fixture 的既有测试**零改动**通过——组件行为的改变只在 -350 dBFS 以下）。
  metadata/fmt/diff-check 通过；`cargo check --locked --target x86_64-pc-windows-msvc`
  覆盖 dsp/两 `_rs`/core/三 fixture 零 error、零新 warning。
- Evidence/artifact: macOS ARM64 本地日志（`/tmp/m5_test.log`、`/tmp/m5_pkg.log`）——本地证据等级。
- Unresolved: 待三平台 hosted 全绿方可把 M5 的这一步记为完成。**Phase 3 总验收前仍挂账一项**：
  instrument 模板 81 行未达 M2 的"新插件样板 ≤50 行"，这是 Phase 3 唯一写进 milestone
  却未满足的目标，下一轮单独收口（需要 voice/synth 层抽象与参数默认值的声明式表达），
  不以"完成规则未提及"为由跳过。
