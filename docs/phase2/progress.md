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
