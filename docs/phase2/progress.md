# Phase 2 进展日志

按时间追加，格式固定：

```text
### YYYY-MM-DD — <milestone>
- Command/platform:
- Result:
- Evidence/artifact:
- Unresolved:
```

### 2026-08-28 — M0 脚手架并入 workspace 与 CI

- Command/platform: macOS ARM64，分支 `phase2/advanced-plugin-contract`（HEAD `e039aae`，Phase 1 验收点之后）。
- Change:
  - 四个 acceptance fixture 骨架（tempo delay / sidechain comp / poly expr synth / state migration）并入 workspace。骨架初稿无法编译，已修：`AudioBuffer` 的通道索引是 `usize`，骨架写成 `channel as u32`（三个 fixture 共 5 处）；poly synth 的 voice 分配用 `iter_mut().find().or_else(|| self.voices.first_mut())` 触发 E0500，改为先取 index 再 `get_mut`（空闲优先，否则偷 slot 0）。
  - `.github/workflows/phase1.yml` 新增 blocking 步骤 "Test Phase 2 acceptance fixtures"：逐个 `cargo test -p`（失败时 `::error` 回显日志尾部，与 Phase 1 步骤同构），再 `cargo build` 四个 crate 以覆盖 `sunmao_export!` 的 cdylib 路径。
  - 修正 `docs/phase2/status.md` 中"单元测试通过"的错误记录。
- Result:
  - 四个 fixture 6 个单元测试通过；完整 `RUSTFLAGS=-Awarnings cargo test --locked` 104 个套件全绿、0 失败。
  - `cargo check --locked --target x86_64-pc-windows-msvc` 覆盖四个 fixture 通过。
  - Phase 1 回归无损：`tools/package_examples.sh --debug --test` 退出 0，20 个 runner 套件各 16/16，raw/packaged standalone smoke 全绿。
  - `cargo metadata --locked`、`cargo fmt --all -- --check`、`git diff --check`、workflow YAML 解析、`bash -n tools/package_examples.sh` 通过。
- Evidence/artifact: macOS ARM64 本地日志（`/tmp/phase2_m0_test2.log`、`/tmp/phase2_pkg.log`）——本地证据等级。
- Unresolved: 三平台 hosted 验证本 commit（Phase 1 既有 gate + 新增 Phase 2 fixture 步骤同时全绿）后 M0 才算完成；M1 transport 设计未开始。

### 2026-08-28 — M0 完成：hosted run #27 三平台全绿

- Command/platform: push `f351ddb` 触发 GitHub Actions #27：https://github.com/aizcutei/sunmao/actions/runs/33155476475
- Result: macOS ARM64、Windows x86_64、Ubuntu x86_64 三个 job 同一 commit 全部 success。新增的 blocking 步骤 "Test Phase 2 acceptance fixtures" 在三平台均 success；Phase 1 既有全部 gate（格式适配、standalone、GUI matrix、packager、runner、打包 helper）保持绿色，无回归。
- Evidence/artifact: run #27 上传 `phase1-macOS-ARM64`（50.7MB）、`phase1-Windows-X64`（76.7MB）、`phase1-Linux-X64`（954.4MB）。
- Unresolved: M0 完成，进入 M1。M1 底层盘点结论：`_sys` 两侧 transport 绑定已完整（`clap_sys` tsig/bar/loop + 8 flags；`vst3_sys::ProcessContext` tempo/tsig/bar/cycle + valid 位），工作从 `_rs` 层开始——`clap_rs::Transport` 需补 tsig/bar/loop/recording 访问器，vst3_rs 需向上暴露 ProcessContext，再设计 core 统一结构。

### 2026-08-28 — M1 transport/timing 实现（待 hosted 验证）

- Command/platform: macOS ARM64，分支 `phase2/advanced-plugin-contract`。
- Change（自底向上）：
  - `_sys`：无需改动，两侧绑定已完整。
  - `_rs`：`clap_rs::Transport` 补 `is_recording`/`is_loop_active`/`is_within_pre_roll`/`time_signature`/`bar_start_beats`/`bar_number`/`loop_beats`/`loop_seconds`；`vst3_rs::ProcessContext` 补对称访问器，并在 `update_transport_from_raw` 按 `ProcessContextFlags` 解析 tempo/tsig/bar/cycle，`song_pos_seconds` 由采样时间轴推导。两侧一致拒绝退化值（非有限、tempo<0、tsig 分母为 0、loop `end<=start`），loop 必须 active 才下发。
  - core：`ProcessContext` 扩展为 11 个字段并 `#[derive(Debug, Clone, PartialEq, Default)]`，带 doc-test；`None` 表示"宿主未提供"而非 0。既有 `tempo`/`is_playing`/`sample_pos` 字段名不变。
  - backend：VST3 与 CLAP 同时映射全部字段；VST3 无 bar 序号故 `bar_number: None`（测试显式断言）。AU 与 standalone runtime 用 `..Default::default()` 保持 Phase 1 子集，不回归。
  - fixture：tempo delay 增加 `sync`/`division` 参数并按 `context.tempo` 计算延迟；宿主无 tempo 或 tempo 退化时回落到毫秒时间。
  - `docs/phase2/semantics.md`：transport 行填入落地 API，并新增 bar 序号、loop 区间、秒制位置三行降级记录（含对应测试名）。
- Result:
  - 新增测试：`clap_rs` 4（42/42）、`vst3_rs` 5（46/46）、两个 backend 各 2（各 18/18）、tempo delay fixture 5（7/7）。
  - 完整 `RUSTFLAGS=-Awarnings cargo test --locked`：104 套件全绿、0 失败；`cargo metadata --locked`、`cargo fmt --all -- --check`、`git diff --check` 通过。
  - `cargo check --locked --target x86_64-pc-windows-msvc`（两个 backend + fixture）通过；`tools/package_examples.sh --debug --test` 退出 0，Phase 1 的 20 个 runner 套件仍各 16/16，standalone smoke 全绿。
  - 过程中发现并修复：两个 backend 的 transport 观测测试共用进程级 static，并行执行时互相抢结果（VST3 侧因此假失败），已加序列化锁。
- Evidence/artifact: macOS ARM64 本地日志（`/tmp/phase2_m1_test.log`、`/tmp/phase2_m1_pkg.log`）——本地证据等级。
- Unresolved: 三平台 hosted 验证本 commit 后 M1 才算完成；runner 侧尚未加 transport 宿主注入测试，随 M2 的 latency/tail 断言一并设计。

### 2026-08-28 — M1 完成：hosted run #29 三平台全绿

- Command/platform: push `66ec5d3` 触发 GitHub Actions #29：https://github.com/aizcutei/sunmao/actions/runs/33157482389
- Result: macOS ARM64、Windows x86_64、Ubuntu x86_64 三个 job 同一 commit 全部 success。transport 模型在两种格式、三个平台上通过；Phase 1 既有 gate 与 M0 的 Phase 2 fixture 步骤保持绿色。
- Evidence/artifact: run #29 上传 `phase1-macOS-ARM64`（50.7MB）、`phase1-Windows-X64`（76.7MB）、`phase1-Linux-X64`（954.7MB）。
- Unresolved: M1 完成，进入 M2。M2 盘点：`clap_rs::Plugin` 已有 `latency()`/`tail()`/`set_render_mode()` 与 `ext/{latency,tail,render}.rs`，`vst3_rs::Plugin` 已有 `latency()`/`tail()`；缺口在 `sunmao_core::SunmaoPlugin`（无对应方法）、两个 backend（未桥接）、VST3 的 `ProcessSetup.process_mode`（未向上暴露）、runner（无 latency/tail 断言）。

### 2026-08-28 — M2 latency/tail/offline render 实现（待 hosted 验证）

- Command/platform: macOS ARM64，分支 `phase2/advanced-plugin-contract`。
- Change（自底向上）：
  - `_rs`：`vst3_rs` 新增 `RenderMode`（`from_process_mode` 把 `kPrefetch` 与未知模式并入 `Realtime`）与 `Plugin::set_render_mode` 钩子，并在 `setupProcessing` 组件未激活时下发；`clap_rs` 的 latency/tail/render 扩展已存在，无需改动。
  - core：`SunmaoPlugin` 新增 `latency_samples()`/`tail()`/`set_render_mode()`；新增 `TailLength{None,Samples,Infinite}` 与 `RenderMode{Realtime,Offline}` 两个枚举（均带 doc-test），并进 `sunmao::prelude`。
  - backend：两侧同时桥接。tail 的"无限"魔数由 backend 编码——VST3 `kInfiniteTail`(=`u32::MAX`)、CLAP `>=i32::MAX`；**有限 tail 一律夹到魔数之下**，避免恰好等于魔数的有限值被宿主当成无限尾音。CLAP 的 `set_render_mode` 经 `catch_unwind` 包裹，插件 panic 转为 `false` 而非跨 ABI 展开。
  - fixture：tempo delay 上报 5ms lookahead latency（offline 时翻倍）与 tail——feedback>0 时为 `Infinite`，否则为延迟线长度。
  - `docs/phase2/semantics.md`：latency/tail/offline render 三行填入落地 API 与降级规则，均注明对应测试名。
- Result:
  - 新增测试：`vst3_rs` 1（47/47）、两个 backend 各 2（各 20/20）、fixture 2（9/9）、core doc-test 2。
  - 完整 `cargo test --locked`：104 套件全绿、0 失败；fmt/metadata/diff 通过。
  - Windows target check（两 backend + fixture）通过；`tools/package_examples.sh --debug --test` 退出 0，Phase 1 的 20 个 runner 套件仍各 16/16。
- Evidence/artifact: macOS ARM64 本地日志（`/tmp/phase2_m2_test.log`、`/tmp/phase2_m2_pkg.log`）——本地证据等级。
- Unresolved: 三平台 hosted 验证本 commit 后 M2 才算完成。runner 的 latency/tail 宿主侧断言仍未加（宿主需查询 `IAudioProcessor::getLatencySamples`/`clap.latency`），列为 M2 收尾项。

### 2026-08-28 — M2 完成：hosted run #31 三平台全绿

- Command/platform: push `52fe11c` 触发 GitHub Actions #31：https://github.com/aizcutei/sunmao/actions/runs/33159235245
- Result: macOS ARM64、Windows x86_64、Ubuntu x86_64 三个 job 同一 commit 全部 success；latency/tail/render 契约在两种格式、三个平台通过，Phase 1 既有 gate 与 Phase 2 fixture 步骤保持绿色。
- Evidence/artifact: run #31 上传 `phase1-macOS-ARM64`（50.8MB）、`phase1-Windows-X64`（76.7MB）、`phase1-Linux-X64`（954.8MB）。
- Unresolved: M2 完成，进入 M3。M3 盘点：`clap_rs::AudioPortInfo`（`ext/audio_ports.rs:14`）与 `vst3_rs` wrapper 的 `get_bus_info`/`activate_bus`/`set_bus_arrangements`、`PortType::Aux → BusTypes::kAux` 均已存在；缺口在 `sunmao_core`——`SunmaoPlugin` 只有 `input_channels()`/`output_channels()` 两个标量、无 bus 模型，`AudioBuffer` 只有扁平通道索引、无 per-bus 视图，两个 backend 的端口表也都由这两个标量推导。M2 收尾项（runner 的 latency/tail 宿主断言）一并留待 M3/M6。

### 2026-08-28 — M3 多 bus/sidechain 实现（待 hosted 验证）

- Command/platform: macOS ARM64，分支 `phase2/advanced-plugin-contract`。
- Change（自底向上）：
  - `_rs`：无需改动，两侧 bus 能力已存在（`clap_rs::AudioPortInfo`、`vst3_rs` 的 `get_bus_info`/`activate_bus`/`set_bus_arrangements` 与 `PortType::Aux`）。
  - core：新增 `BusRole{Main,Sidechain}` 与 `BusInfo{name,channels,role}`（含 `main`/`sidechain` 构造器与 doc-test）；`SunmaoPlugin` 新增 `input_buses()`/`output_buses()`，默认实现由 `input_channels()`/`output_channels()` 推导单 main bus，**Phase 1 插件行为不变**；`AudioBuffer` 新增 `with_input_bus_bounds`/`num_input_buses`/`input_bus_channels`/`input_bus`，扁平通道索引保持不变。`BusInfo`/`BusRole` 进 prelude。
  - backend：两侧同时桥接，且**以 bus 声明为通道拓扑的唯一真相**——扁平通道总数取自 bus 声明之和，因此加 sidechain 只需覆写 `input_buses()`。VST3 把 `BusRole::Sidechain` 映射为 `PortType::Aux`（speaker layout 只应用于 main bus）；CLAP 无 aux 概念，映射为 `is_main=false` 的普通端口。bus bounds 在构造/激活时预计算，音频线程不重建。
  - fixture：sidechain 压缩器声明 main+sidechain 两条 stereo 输入总线，检测器改用 key 信号；宿主未连接 sidechain 时回落到主路径。
  - `docs/phase2/semantics.md`：多 bus/sidechain 行填入落地 API 与两格式差异，注明全部测试名。
- Result:
  - 新增测试：core 3（25/25，覆盖 bus 切分、未连接 bus 读空、无 bus 布局）、fixture 3（4/4，覆盖 bus 声明、loud key 触发压缩、silent key 不压缩）。
  - 完整 `cargo test --locked`：104 套件全绿、0 失败；fmt/diff 通过。
  - Windows target check（两 backend + fixture）通过；`tools/package_examples.sh --debug --test` 退出 0，Phase 1 的 20 个 runner 套件仍各 16/16——本轮改了两个 backend 的通道拓扑推导，这条回归是关键证据。
- Evidence/artifact: macOS ARM64 本地日志（`/tmp/phase2_m3_test.log`、`/tmp/phase2_m3_pkg.log`）——本地证据等级。
- Unresolved: 三平台 hosted 验证本 commit 后 M3 才算完成。仍未做：bus 激活/去激活回调、speaker layout 动态协商（`setBusArrangements` 目前仍按声明固定接受）、runner 侧多 bus 宿主测试——这三项列为 M3 收尾或 M6 收口项。

### 2026-08-28 — M3 核心完成：hosted run #33 三平台全绿

- Command/platform: push `1daf86e` 触发 GitHub Actions #33：https://github.com/aizcutei/sunmao/actions/runs/33160731528
- Result: macOS ARM64、Windows x86_64、Ubuntu x86_64 三个 job 同一 commit 全部 success。bus 声明模型与 sidechain 路由在两种格式、三个平台通过；改动了两个 backend 的通道拓扑推导后，Phase 1 既有 gate（含 GUI matrix、standalone、packager、runner）仍保持绿色。
- Evidence/artifact: run #33 上传 `phase1-macOS-ARM64`（51.0MB）、`phase1-Windows-X64`（76.8MB）、`phase1-Linux-X64`（955.8MB）。
- Unresolved: M3 的三项延后工作（bus 激活/去激活回调、speaker layout 动态协商、runner 多 bus 宿主测试）转入 M6 收口。进入 M4；M4 盘点：`_sys` 两侧齐全（`clap_sys` 有 `CLAP_EVENT_NOTE_EXPRESSION`/`CLAP_EVENT_PARAM_MOD` 与 expression 种类常量，`vst3_sys::ievents` 有 `NoteExpressionValueEvent`），`clap_rs::Plugin` 已有 `voice_info()`；**最大缺口在 `_rs` 事件层**——`clap_rs::Event` 只有 5 个变体，note expression 与 param mod 都落入 `Unknown` 被静默丢弃，违反 semantics.md 的"严禁静默丢事件"，必须先修。

### 2026-08-28 — M4 modulation/per-note expression/voice-info 实现（待 hosted 验证）

- Command/platform: macOS ARM64，分支 `phase2/advanced-plugin-contract`。
- Change（自底向上）：
  - `_sys`：`vst3_sys` 补 `NoteExpressionTypeIDs`（SDK 的 `Steinberg::Vst::NoteExpressionTypeIDs`），此前缺失。
  - `_rs`：**修掉一处静默丢事件**——`clap_rs::Event` 此前只有 5 个变体，`CLAP_EVENT_NOTE_EXPRESSION` 与 `CLAP_EVENT_PARAM_MOD` 都落入 `Unknown` 被丢弃；现新增 `NoteExpression`/`ParamMod` 变体、`NoteExpressionKind` 与短结构体拒绝。`vst3_rs::Plugin` 新增 `note_expression()` 钩子，wrapper 分发 `kNoteExpressionValueEvent`（拒绝非有限值）。
  - core：`Event` 新增 `ParamMod`/`NoteExpression` 变体，新增 `NoteExpression`/`NoteExpressionKind`/`VoiceInfo` 与 `EventQueue::{param_mods,note_expressions}`、`SunmaoPlugin::voice_info()`、`MidiMessage::channel()`；`as_param_change()` 对 `ParamMod` 返回 `None`，使 modulation 无法污染 state。全部进 prelude。
  - backend：两侧同时桥接。**`NoteExpression` 的 `channel`/`key` 建模为 `Option`**——VST3 的 expression 事件只带 `note_id`，据实为 `None`；CLAP 带全套故为 `Some`。VST3 无 pressure 维度（宿主走独立的 `kPolyPressureEvent`），未知维度保留原始 id 为 `Unknown(i32)` 照常下发。CLAP 侧桥接 voice-info；VST3 无查询点，按降级不暴露。
  - fixture：poly synth 按 note_id 优先、channel/key 退化匹配来路由 expression，实现 tuning 弯音与 volume 缩放，并上报 voice-info。
  - `docs/phase2/semantics.md`：modulation、per-note expression、voice-info 三行填入落地 API 与降级规则。
- Result:
  - 新增测试：`clap_rs` 4（46/46）、core（25/25 + 4 doc-test）、fixture 4（6/6）。
  - 完整 `cargo test --locked`：104 套件全绿、0 失败。过程中新枚举变体暴露出三处非穷尽匹配（standalone runtime 的事件钳制、VST3 backend 测试、`clap_rs_syn_sine` 示例），均已按各自语义补齐——runtime 对 mod 只钳时序不钳值域，对 expression 钳时序并拒绝非有限值。
  - Windows target check 通过；`tools/package_examples.sh --debug --test` 退出 0，Phase 1 的 20 个 runner 套件仍各 16/16。
- Evidence/artifact: macOS ARM64 本地日志（`/tmp/phase2_m4_test4.log`、`/tmp/phase2_m4_pkg.log`）——本地证据等级。
- Unresolved: 三平台 hosted 验证本 commit 后 M4 才算完成。未做：backend 层的 expression/mod 端到端映射测试（目前覆盖在 `_rs` 与 core/fixture 两端）、VST3 note-id ↔ channel/key 的 backend 侧映射表，列为 M6 收口项。

### 2026-08-28 — M4 完成：hosted run #35 三平台全绿

- Command/platform: push `051e754` 触发 GitHub Actions #35：https://github.com/aizcutei/sunmao/actions/runs/33162478028
- Result: macOS ARM64、Windows x86_64、Ubuntu x86_64 三个 job 同一 commit 全部 success。modulation、per-note expression、voice-info 在两种格式、三个平台通过；Phase 1 既有 gate 与 Phase 2 fixture 步骤保持绿色。
- Evidence/artifact: run #35 上传 `phase1-macOS-ARM64`（51.1MB）、`phase1-Windows-X64`（76.9MB）、`phase1-Linux-X64`（956.0MB）。
- Unresolved: M4 完成，进入 M5（最后一个实现里程碑）。M5 盘点：`vst3_rs/src/state.rs` 已有 `STATE_MAGIC`+`STATE_VERSION=1` 的参数编码，CLAP 侧另有一份；**关键缺口是 `decode_header` 对版本不符直接返回 `None`（`vst3_rs/src/state.rs:94`），旧版本 state 会被整体拒绝而非迁移**，且版本头分散在两个格式层，与 semantics.md 约定的"版本头由 SunMao 层定义、格式无关"不符。

### 2026-08-28 — M5 版本化 state 与迁移（待 hosted 验证）

- Command/platform: macOS ARM64，分支 `phase2/advanced-plugin-contract`。
- Change（自底向上）：
  - `_rs`：**修掉 M5 的本体缺陷**——`vst3_rs/src/state.rs` 与 `clap_rs/src/ext/state.rs` 的 `decode_header` 此前对版本不符一律 `return None`，旧 preset 会被整体丢弃。现改为：`version <= STATE_VERSION` 接受并返回版本号，`version > STATE_VERSION` 拒绝（本 build 无法解释未来布局），magic 校验不变。
  - core：`SunmaoPlugin` 新增 `STATE_VERSION` 常量与 `migrate_state(from_version)` 钩子，并写明版本策略——条目按参数 id 匹配，故**增删参数不需要升版本**，只有既有参数的含义变化才需要。
  - fixture：state migration 演进到 v2（保留 v1 的 `level`，新增 `trim_db`），`migrate_state` 在 `from_version < 2` 时把 `trim_db` 归位到文档化的默认值。
  - `docs/phase2/semantics.md`：版本化 state 行填入落地 API 与版本策略。
- Result:
  - 新增测试：两个 `_rs` 各 3（`clap_rs` 49/49、`vst3_rs` 50/50，覆盖"旧版本被接受/未来版本被拒绝/外来 magic 被拒绝"）、fixture 2（3/3）。
  - 完整 `cargo test --locked`：104 套件全绿、0 失败；fmt/diff 通过。
  - Windows target check 通过；`tools/package_examples.sh --debug --test` 退出 0，Phase 1 的 20 个 runner 套件仍各 16/16——runner 的 state round-trip 用例是这次改动的直接回归面。
- Evidence/artifact: macOS ARM64 本地日志（`/tmp/phase2_m5_test.log`、`/tmp/phase2_m5_pkg.log`）——本地证据等级。
- Unresolved: 三平台 hosted 验证本 commit 后 M5 才算完成。未做：`clap.preset-load` 宿主驱动的 preset 载入路径、VST3 program list 映射、backend 侧调用 `migrate_state` 的端到端接线（目前钩子与解码器已就绪，backend 尚未在 load 后回调），列为 M6 收口项。

### 2026-08-28 — hosted CI #37 Windows WGPU 收尾段错误（疑似 flake，待二次取证）

- Command/platform: push `1ea2be2` 触发 GitHub Actions #37：https://github.com/aizcutei/sunmao/actions/runs/33163982575
- Result: macOS 与 Linux 全绿；Windows 失败于 "Package and exercise native GUI backends"。
- 证据（annotation）：`SunMao Gain WGPU (VST3)` 的**全部断言都通过**——窗口创建、520x220 resize、像素校验、Win32 原生鼠标输入使参数 0.5→0.78、host gesture、close/recreate、recreate 后像素校验——并打印了 "GUI test complete." 与 "Done."；随后进程以 **exit 139（段错误）** 结束。即崩溃发生在测试逻辑完成后的收尾/析构阶段。
- 判断：本 commit 只改了两个 `_rs` 的 state 解码与 core 的迁移钩子，未触及 GUI/WGPU 路径；同一步骤在 run #27/29/31/33/35 连续五轮通过。倾向于 Windows WGPU 收尾期的偶发崩溃（与 run #24 的 UIA 超时 flake 类似），但**不以"flake"为由跳过**：下一次 push 会在新 commit 上重跑同一 gate 作为第二个数据点；若复现，则深入 WGPU/D3D 析构路径而非重试。
- Evidence/artifact: check-run annotations（job 全量日志需 admin 权限）。
- Unresolved: M5 尚未取得三平台 hosted 证据。

### 2026-08-28 — M5 + M6 完成：hosted run #38 三平台全绿

- Command/platform: push `77f788c` 触发 GitHub Actions #38：https://github.com/aizcutei/sunmao/actions/runs/33164763166
- Result: macOS ARM64、Windows x86_64、Ubuntu x86_64 三个 job 同一 commit 全部 success。本 run 同时验证了 M5 的版本化 state 改动与 M6 的 property tests；`Test Phase 2 acceptance fixtures` 与含 proptest 的 `Test format adapters and host` 三平台均 success，Phase 1 既有 gate 保持绿色。#37 的 Windows WGPU 收尾段错误未复现，确认为偶发 flake（同一路径已六轮通过）。
- Evidence/artifact: run #38 上传 `phase1-macOS-ARM64`（51.1MB）、`phase1-Windows-X64`（76.9MB）、`phase1-Linux-X64`（956.1MB）。
- Unresolved: Phase 2 按 `docs/phase2/status.md` 的完成规则达成，但**未覆盖 roadmap Phase 2 的全部条目**——7 项延后工作已列入 status.md 的"M6 遗留项"表，需在 Phase 3 前单独立项或并入 Phase 3。
