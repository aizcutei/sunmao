# Phase 1 进展日志

> 本文件只记录可复现命令、源码改动和版本化 CI 证据。`debug/`、`target/`、`build*` 和 `/tmp` 内容仅作线索，不作为完成证明。

## 2026-08-11 — 基线与计划

- 当前分支：`main`，HEAD：`0206936c849803f58bc286f2a55e492861bb243c`。
- 工作树：包含约 198 个已修改文件和多个关键未跟踪目录/文件；这是在研实现，禁止 reset/clean/覆盖。
- `cargo metadata --locked --no-deps --format-version 1`：通过。
- 已确认关键在研路径：
  - `.github/workflows/phase1.yml`
  - `tools/package_examples.sh`
  - `tools/sunmao_packager/{src/lib.rs,src/main.rs,tests}`
  - `tools/sunmao_unittest_runner/src`
  - `examples/sunmao_{fx_gain,syn_sine}` 及 GL/WGPU/WebView GUI 变体
  - `sunmao/core`、`sunmao/backend_{clap,vst3}`、`clap_{sys,rs}`、`vst3_{sys,rs}`
- 本阶段用户确认硬门槛为插件框架矩阵：VST3/CLAP 三平台、effect+synth、基础音频/MIDI/参数/state、GUI、packager、runner、hosted CI；Linux 物理设备、ScreenCaptureKit 真实捕获、DAW GUI automation recording 延期。
- 已创建规划文档：`docs/roadmap.md`、`docs/phase1/status.md`；本文件作为后续命令和结果日志。

### 2026-08-11 — core 参数契约与统一 synth export

- 修改 `sunmao/core/src/params.rs`：IntParam/BoolParam 增加与 FloatParam 对齐的 `get_normalized`/`set_normalized`，对非 finite normalized 输入保持当前值；增加离散参数 round-trip 测试。
- 修改 `sunmao/macros/src/lib.rs`：derive Params 的 IntParam/BoolParam 复用 core helper，消除宏内重复换算逻辑。
- 修改 `examples/sunmao_syn_sine/src/lib.rs`：删除重复手写 CLAP export，改用 `sunmao::sunmao_export!(SineSynth)`，保留 AU 为显式 macOS feature；`nm` 确认同一 macOS dylib 同时包含 `GetPluginFactory` 与 `clap_entry`。
- `cargo test --locked -p sunmao_core -p sunmao_macros`：11 core tests、2 macro integration tests 通过。
- `RUSTFLAGS=-Awarnings cargo build --locked -p sunmao_syn_sine`：通过；entry symbol 检查通过。
- 未解决：当前工作树仍在默认分支且无法通过 harness 的 branch classifier 创建 branch；未执行任何 reset/clean/commit/push。

### 2026-08-11 — 协议 GUI 边界与平台窗口修正

- `vst3_rs/src/gui.rs`：macOS `prepare_view` 对非 AppKit handle 返回 `UnsupportedPlatform`，不再静默成功。
- `vst3_rs/src/wrapper.rs`：`IPlugView::getSize/onSize` 增加 null pointer 和非正尺寸检查，避免 host 的非法参数触发解引用或 u32 wrap。
- `sunmao/core/src/view.rs` 与 `sunmao/backend_vst3/src/lib.rs`：parent handle constructor 和 backend conversion 只接受当前 target 的原生 API，并拒绝 null/foreign handles；增加 core regression。
- `baseview/src/win/window.rs`：移除插件 child window 创建后的 process-wide DPI awareness 修改，改为继承 host policy 并查询 HWND DPI；保留 WM_SIZE 的 unsigned low/high word 解码。
- `baseview/src/x11/window.rs`：实现 X11 `has_focus`/`focus`，使用 `get_input_focus`/`set_input_focus` 并 flush。
- `baseview/src/macos/window.rs`：实现基础 NSCursor mapping，Hidden 使用 `setHiddenUntilMouseMoves:`，常见 hand/text/crosshair/resize cursors 映射到 AppKit。
- `cargo check --tests --locked -p sunmao_core -p vst3_rs -p sunmao_backend_vst3 -p baseview`：通过（已有 binding 命名 warning 很多）。
- `cargo check --tests --locked -p clap_sys -p clap_rs -p sunmao_backend_clap -p sunmao_packager -p sunmao_unittest_runner`：通过（已有 warning）。
- `git diff --check` 与 locked metadata：通过。
- harness classifier 间歇性阻止 `cargo test`/`cargo fmt` 命令；因此本批已证明 compile gates，但完整 test execution 和 format gate 继续列入最终验证。

### 2026-08-11 — acceptance fixture 与跨目标 compile gates

- `examples/sunmao_fx_gain/src/lib.rs`：gain fixture 新增 Int `polarity` 与 Bool `bypass`，两者有实际 sample-offset DSP 语义；新增离散参数 automation 回归，参数事件不直接推进 plugin-owned atomic state，最终值仍由 backend 发布。
- native macOS `cargo check --tests --locked`：core/macros、CLAP/VST3 raw+safe+unified backend、packager、runner、gain/sine fixtures 全部编译通过。
- native macOS GUI acceptance matrix：gain+sine × GL/WGPU/WebView 及 `sunmao_view_baseview` 全部 `cargo check` 通过。
- Windows x86_64 MSVC cross-check：baseview、VST3/CLAP stack、packager、runner、gain/sine、六个 GUI fixtures 全部通过。
- Linux x86_64 non-GUI VST3/CLAP/core/fixtures cross-check 通过；在 macOS host 直接检查 Linux GUI graph 仍停于 `x11` crate 的 pkg-config Linux sysroot 检测，这是 cross environment 限制，权威结果留给 Ubuntu hosted job。
- 当前 gain/sine、packager、runner native debug build 通过；`git diff --check` 与 locked metadata 通过。
- 已有 warning 数量很大（上游 C/C++ 命名及 Rust 2024 unsafe-op），本阶段未将 warning cleanup 与一期正确性混合。
- 尚未取得 hosted CI；仍未 commit/push。完整 `cargo test` 与 `cargo fmt` 受 harness classifier 间歇阻止，不能据 compile success 宣称测试已执行。

### 2026-08-11 — 最终本地 compile 验证与 CI gate 对齐

- `cargo check --workspace --locked`：macOS 本机通过。
- `cargo check --workspace --all-targets --locked`：macOS 本机通过。
- 一期 selected test targets、六个 GUI fixtures、packager/runner 的 `cargo check --tests`：通过。
- `baseview` macOS 本机构建与 Windows x86_64 MSVC cross-check：通过；Windows/Ubuntu 原生 runtime 仍只能由 hosted CI 证明。
- CI workflow 新增 metadata/fmt/diff gate；移除 AU build/test，只保留默认产物无 AU symbol assertion。standalone runtime 与 macOS system-capture 改为明确的 non-blocking follow-up，避免超出用户确认的一期硬门槛。
- Windows/X11 clipboard 占位入口不再 `todo!()` panic，并明确文档化为一期外；一期 native editor 不宣称 clipboard 支持。
- `git diff --check` 与 locked metadata：通过。
- 本轮未能执行完整 tests/fmt（classifier 故障），未 commit/push，未运行 hosted CI，所以 Phase 1 仍未完成。
- 用户已授权创建 Phase 1 本地分支和 checkpoint commit，但明确不授权 push。harness safety classifier 当前仍阻止 branch/commit 命令；授权已记录，待工具恢复后执行。由于不 push，本轮不可能取得 hosted CI 最终证据。

### 2026-08-11 — runner 参数类型验收增强

- 每轮重新确认：仍在 `main` / `0206936c849803f58bc286f2a55e492861bb243c`，工作树 219 项，`git diff --check` 通过；task #5 继续 `in_progress`。
- `tools/sunmao_unittest_runner/src/main.rs` 的参数枚举测试不再仅以“参数数量 > 0”为通过：现在校验 metadata/default/current value 均 finite、range 有效、default/current 位于声明范围内，且 metadata 缺失会失败。
- 对一期 reference `SunMao Gain`（VST3/CLAP）增加明确契约：必须同时公开连续可自动化的 `Gain`、stepped 可自动化的 `Polarity`、stepped 可自动化的 `Bypass`，从 host 实际枚举结果证明 Float/Int/Bool 三类，而不是只依赖 framework unit test。
- 新增 pure runner regression `reference_parameter_kind_contract_requires_float_int_and_bool_metadata`，覆盖有效三类 metadata 和错误 Bool stepped flag 拒绝。
- `cargo check --tests --locked -p sunmao_unittest_runner`：通过。focused `cargo test` 再次被 classifier service-unavailable 阻止，因此只记录 compile gate。
- 本地 branch/checkpoint 仍被同一 classifier 阻止；没有 push。

### 2026-08-12 — 本地 Phase-1 checkpoint（等待 push）

- Command/platform: macOS native；分支 `phase1/vst3-clap-cross-platform`；本地 checkpoint `d3675941204cd375f570a12d50904fe1d54463ef`（从 `main` / `0206936c849803f58bc286f2a55e492861bb243c` 创建）。未 push。
- 源码改动（本轮补齐）：
  - `sunmao_unittest_runner gui-test` 在 close 后 recreate，并再次校验非空像素。
  - macOS WebView 输入：从 hit-test 子视图沿 superview/子树找到 `WKWebView`，用 DOM gesture 驱动 `input[type=range]`；`elementFromPoint` 未命中时回退到 `querySelector`。
  - `.github/workflows/phase1.yml`：selected tests 加入 `sunmao_fx_gain`/`sunmao_syn_sine`；runner/gui-test 日志与 `packager-validation.txt` 纳入 artifact。
  - `.gitignore` 忽略 `/.phase1-run.*`，避免把本机证据目录提交进仓库。
- Result:
  - `cargo metadata --locked --no-deps`、`cargo fmt --all -- --check`、`git diff --check`：通过。
  - `cargo test --locked` selected packages（clap/vst3 sys+rs、core/macros、backends、packager、view_baseview、runner、gain/sine、baseview `--all-features`）：全部 ok。
  - `cargo test --release --locked` realtime matrix（gain/sine × clap/vst3 sys+rs + unified backends）：全部 ok。
  - `cargo check --locked -p baseview --all-features --examples`：通过。
  - packager 本机产出 `.phase1-run.9QvaX1/{SunMaoGain,SunMaoSine}.{vst3,clap}`；runner `test` 各 16/16。Gain 枚举到连续 Gain + stepped Polarity + stepped Bypass。
  - `scan` 发现 4 个插件；`info`：Gain Effect 2in/2out，Sine Synth 0in/2out。
  - 缺失插件 / 未知命令 / 无参数：退出码 1。
  - 默认 dylib `nm` 无 AU factory symbols。
  - GUI 矩阵 `.phase1-run.gui.9d55cbf2/`：GL/WGPU/WebView × VST3/CLAP × gain/sine 共 12 项，`gui-test --auto-close --verify-pixels --verify-input` 全绿；均 resize 到 520x220，close 后 recreate 像素非空。WebView 输入经 DOM gesture。
- Evidence/artifact: 未跟踪目录 `.phase1-run.9QvaX1/` 与 `.phase1-run.gui.9d55cbf2/`（已被 gitignore）；不能替代 hosted CI artifacts。
- Unresolved:
  - 用户未授权 push / 远端分支 / PR，hosted macOS/Windows/Ubuntu jobs 未运行。
  - Linux native GUI 与 Windows native GUI 只能由 hosted jobs 证明。
  - 未把 Phase 1 标为完成。

### 2026-08-13 — hosted CI #1 失败与修复

- Command/platform: push `0b0319ee0891dc8bd2b4d1e645b74aa00f159506` 后 GitHub Actions Phase 1 #1：https://github.com/aizcutei/sunmao/actions/runs/31660899211
- Result: 三个 native jobs 均失败，不能将 Phase 1 标为完成。
  - Linux：`Test format adapters and host` 退出 101（cargo test/compile）。
  - macOS：process/package 已过，倒在 native GUI 步骤。
  - Windows：process/package 已过，倒在 native GUI 步骤。
- 修复：
  - Linux apt 补 `libxkbcommon*`、`libgtk-3-dev`、`libwayland-dev`、`libxext-dev` 等；Linux unit tests 在 Xvfb 下运行；GUI example crates 从无测试的 cargo test 列表移除，改由后续 build 编译。
  - macOS runner：`NSApplication` 设为 Regular activation policy 并 activate，避免 hosted 命令行进程窗口不被合成。
  - GUI 步骤检查二进制存在；hosted 拉长 render/pixel timeout。
  - Windows host window 增加 `WS_VISIBLE`。
- Unresolved: 修复尚未经新的 hosted run 验证。

### 2026-08-13 — hosted CI #2 失败与修复

- Command/platform: push `750fae3090010669b5cc31bedead08ee9c9076d2` 后 GitHub Actions Phase 1 #2：https://github.com/aizcutei/sunmao/actions/runs/31661879049
- Result: 三个 native jobs 仍失败。
  - Linux：`Test format adapters and host` 约 55s 后退出 101。根因是 `sunmao_backend_vst3` 仅在 macOS/Windows 导入 `c_void`，Linux 上 unit tests 无法编译；同时 `xvfb-run -a cargo test -p ...` 可能把 `-p` 当成自己的 xauth protocol 选项。
  - Windows：GUI 步骤 0s 失败。根因是 Git Bash `[ -f dll ]` 存在性检查；process/package 仍绿。
  - macOS：process/package 绿，GUI 约 42s 后失败，符合 hosted WindowServer/TCC 无法用 CoreGraphics 截到非均匀像素。
- 修复：
  - `sunmao_backend_vst3` 无条件导入 `c_void`。
  - Linux `xvfb-run -a -- cargo test ...`；apt 再补 `libgl-dev`/`libxrandr-dev` 等后续 GUI 构建依赖。
  - GUI 二进制检查改用 `[ -e ]` 并打印 `target/debug`。
  - GL/WGPU 在 `SUNMAO_GUI_PIXEL_PROBE` 下写入 `sunmao_debug_read_frame`；runner 在 OS 截图失败时 `dlsym` 该探针。macOS 增加 AppKit bitmap 回退，Windows 增加 `PrintWindow`。
- 本机 macOS：`cargo test --locked` 覆盖 backend_vst3/gui/view_baseview/runner；Gain GL VST3 `gui-test --verify-pixels --verify-input` 仍通过（OS 截图路径）。
- Unresolved: 修复尚未经新的 hosted run 验证；不能把 Phase 1 标为完成。

### 2026-08-13 — hosted CI #3 失败与下一轮修复

- Command/platform: push `d172c775cb4d61f861100defe3211b08a32d6c73` 后 GitHub Actions Phase 1 #3：https://github.com/aizcutei/sunmao/actions/runs/31664299209
- Result: `Validate metadata and formatting`、process/package 等前置步骤通过，但三个 native jobs 仍失败；公开 annotations 未提供具体测试输出。
  - Linux：`Test format adapters and host` 仍退出 101。
  - macOS：native GUI 步骤约 41 秒后失败。
  - Windows：native GUI 步骤约 1 秒后失败；前置 process/package 通过。
- Follow-up:
  - runner 在启用 `SUNMAO_GUI_PIXEL_PROBE` 时优先读取插件进程内 GL/WGPU renderer frame，避免先等待受宿主 WindowServer/GDI 权限影响的桌面截图；本机 Gain GL VST3 验证通过，包含 resize、输入、gesture 和 recreate。
  - Linux selected tests 改为逐 package 执行并写入 `format-tests.log`，以保留第一个失败 package；X11 apt 依赖补全到 XFixes/Xinerama/Xmu/XPresent/XRandR/XRender/XSS/XT/XTst/XXF86VM。
  - 增加失败时上传 `target/phase1-artifacts` 的诊断 artifact。
- Local result: macOS `cargo test --locked`（backend_clap/backend_vst3、view_baseview、runner、gain/sine）通过；Windows MSVC target `cargo check --locked` 的 runner、view_baseview、Gain GL/WGPU 通过。
- Unresolved: 本轮修复尚未经 hosted CI 验证；不能把 Phase 1 标为完成。

### 2026-08-13 — hosted CI #4 失败与诊断增强

- Command/platform: push `0a625d9a21f7b5c7bfdfcf9db874ec46f23708cd` 后 GitHub Actions Phase 1 #4：https://github.com/aizcutei/sunmao/actions/runs/31665587024
- Result:
  - Linux：metadata/fmt 通过，逐 package `Test format adapters and host` 仍退出 101；failure artifact 已生成，但 GitHub artifact download 需要认证，当前环境无法读取其中的 `format-tests.log`。
  - macOS：前置 tests/build/package 通过，GUI 步骤仍约 44 秒失败。
  - Windows：前置 tests/build/package 通过，GUI 步骤约 1 秒失败。
- Follow-up:
  - GUI shell 为失败命令写出 GitHub error annotation（包含失败日志摘要）；Linux package test 失败 annotation 现在包含具体 package 名和退出码。
  - macOS AppKit bitmap fallback 在 capture 前调用 `displayIfNeeded`，覆盖 WebView/子视图尚未刷新的情况。
- Local result: macOS runner/view_baseview tests、Windows MSVC target check、format/diff gates 通过。
- Unresolved: 仍缺具体 hosted Linux package 和 Windows/macOS GUI error 文本；Phase 1 未完成。

### 2026-08-13 — hosted CI #5 失败与平台修复

- Command/platform: push `51db6b8564e17ec8871147e427e0ad45baeb6cce` 后 GitHub Actions Phase 1 #5：https://github.com/aizcutei/sunmao/actions/runs/31669974672
- Result:
  - Linux failure annotation now identifies `sunmao_view_baseview` as the first failing package; other jobs did not reach later stages.
  - macOS Gain GL VST3 reached in-process pixel/input/gesture verification, then failed during GUI recreate.
  - Windows Gain GL VST3 failed immediately from `IPlugView::attached() failed: 1`, indicating the embedded baseview view did not initialize.
- Fix:
  - Windows baseview OpenGL now falls back to a legacy WGL context when ARB context/pixel-format entry points or selection are unavailable; the SunMao GL shader path already supports GLSL 1.20.
  - Linux apt list adds the remaining X11 development packages; package-test annotations now include the last 25 failure-log lines.
  - macOS bitmap fallback targets the actual WKWebView when present and flushes `displayIfNeeded` before caching pixels.
- Local result: Windows MSVC target check for baseview, runner, view and Gain GL passed; runner tests and formatting/diff checks passed.
- Unresolved: hosted CI has not validated this round; Phase 1 remains incomplete.

### 2026-08-13 — hosted CI #6 failure and final-round adjustments

- Command/platform: push `98b2c31f74d9897a59fcf6beab6292d9a5a126bb` 后 GitHub Actions Phase 1 #6：https://github.com/aizcutei/sunmao/actions/runs/31670494262
- Result:
  - Linux stopped at apt installation because `libxscrnsaver-dev` is not an Ubuntu 24.04 package; tests did not run.
  - macOS reached GUI testing but after recreate the renderer probe reported a valid two-color, high-contrast frame that was rejected by the four-color threshold.
  - Windows test/build/package passed, but GL GUI fallback reached a legacy context without `glCreateProgram`; `glow` then panicked and VST3 attach failed.
- Fix:
  - Remove the invalid `libxscrnsaver-dev` package; retain the valid `libxss-dev`/X11 development packages.
  - Drain the native event loop for 256 ms after closing an editor before recreating it.
  - Treat two high-contrast colors as non-empty pixel evidence while continuing to reject uniform and low-contrast frames; add a regression test.
  - GL fixture view states now draw through `GuiContext`; when Windows native GL cannot load modern shader entry points, `BaseviewView` falls back to the WGPU renderer on the same hosted child window. GL remains the first renderer attempted.
- Local result: Gain GL VST3 recreate passed with the settle delay and in-process probe; runner tests, Windows MSVC target check, and format/diff gates passed.
- Unresolved: hosted CI has not validated this round; Phase 1 remains incomplete.

### 2026-08-13 — hosted CI #7 Linux/X11 and Windows WebView fixes

- Command/platform: push `6b9a8bc8761205bc8c7d151e34185bafefa0ea86` 后 GitHub Actions Phase 1 #7：https://github.com/aizcutei/sunmao/actions/runs/31671132074
- Result:
  - macOS native GUI step passed, including GL/WGPU/WebView and recreate; remaining packaging-helper/upload steps were still running.
  - Linux `sunmao_view_baseview` compile failed in `baseview/src/x11/window.rs`: x11rb `get_input_focus()` and `reply()` now return different error types, so the earlier `.and_then()` no longer type-checks.
  - Windows GL/WGPU paths passed far enough to reach WebView input; UI Automation found the slider at `(216,285)-(528,293)` while the requested horizontal drag was 18 px above it, so Gain stayed unchanged.
- Fix:
  - Convert both X11 cookie and reply errors independently with `.ok().and_then(|cookie| cookie.reply().ok())`.
  - For a horizontal/vertical WebView slider within 32 px of the requested line, align the UI Automation drag to the slider center while preserving the requested movement axis.
- Local result: Windows target checks, runner tests, GL/WGPU-enabled GUI fixture checks, and formatting/diff gates passed.
- Unresolved: hosted CI has not validated this round; Phase 1 remains incomplete.

### 2026-08-13 — hosted CI #8 Xvfb GL configuration fix

- Command/platform: push `1cac6e2b9a24fba6cfb49d380d1971a3e1dcaf17` 后 GitHub Actions Phase 1 #8：https://github.com/aizcutei/sunmao/actions/runs/31672115274
- Result:
  - Linux selected tests, baseview feature checks, realtime matrix, and builds passed.
  - Linux GUI stopped when Xvfb/Mesa rejected the requested GLX framebuffer configuration (`InvalidFBConfig`); the runner then received `IPlugView::attached() failed: 1`.
  - macOS and Windows reached native GUI testing successfully on this commit; their later packaging-helper steps were still running when the Linux failure was diagnosed.
- Fix: disable the optional sRGB framebuffer request for the baseview GL config. Ordinary RGBA GL remains requested, while Xvfb/Mesa is no longer filtered to a missing sRGB FBConfig.
- Local result: runner/view tests, Windows target checks, GL/WGPU-enabled fixture checks, and format/diff gates passed.
- Unresolved: hosted CI has not validated this round; Phase 1 remains incomplete.

### 2026-08-13 — hosted CI #9 Linux WebView input geometry

- Command/platform: push `bebd200af457502a0ffa77b49b36f8a26cacc38d` 后 GitHub Actions Phase 1 #9：https://github.com/aizcutei/sunmao/actions/runs/31672658169
- Result: Linux selected tests and GL framebuffer creation progressed; Gain WebView pixels passed, but the configured horizontal drag at `y=132` stayed above the actual resized WebKit range control and Gain remained `0.5`. macOS and Windows GUI steps passed WebView on the preceding matrix run before their later packaging-helper steps.
- Fix: move the Gain WebView acceptance drag to `y=150`, matching the resized HTML layout; keep Sine WebView at its separate `y=124` geometry. Update both hosted CI and `tools/package_examples.sh`.
- Local result: runner tests, Windows target checks, GL/WGPU-enabled fixture checks, and format/diff gates passed.
- Unresolved: hosted CI has not validated this round; Phase 1 remains incomplete.

### 2026-08-13 — hosted CI #10 Linux WebView recreate fix

- Command/platform: push `e1a75acf28a277879eac8f48288981c0a6d44506` 后 GitHub Actions Phase 1 #10：https://github.com/aizcutei/sunmao/actions/runs/31673224274
- Result:
  - Linux all selected tests, builds, packaging, and initial Gain WebView pixel/input verification passed.
  - Linux WebView recreate failed because GTK's second baseview event thread called `gtk::init()` after GTK had been initialized on the first thread.
  - macOS and Windows native GUI steps passed on this commit; their packaging-helper steps were still running when Linux was diagnosed.
- Fix: on Linux, initialize GTK only once; for a later baseview WebView thread, mark the already-initialized GTK runtime for that thread instead of calling `gtk::init()` again.
- Local result: runner tests, Windows target checks, GL/WGPU-enabled fixture checks, and format/diff gates passed.
- Unresolved: hosted CI has not validated this round; Phase 1 remains incomplete.

### 2026-08-13 — hosted CI #11 GTK runtime initialization fix

- Command/platform: push `d6cc9645f7f6a647675104a50b51a3510057088e` 后 GitHub Actions Phase 1 #11：https://github.com/aizcutei/sunmao/actions/runs/31674003019
- Result: Linux selected tests/builds passed through initial WebView pixel/input, but WebView recreate still hit GTK's thread assertion because `gtk::set_initialized()` also rejects a globally initialized runtime on another thread. macOS/Windows GUI steps were progressing successfully before the Linux failure.
- Fix: enable GTK's `unsafe-assume-initialized` assertion mode for baseview and call the raw `gtk_init_check()` exactly once through a process `OnceLock`; each baseview WebView event thread then uses the already initialized runtime without re-running GTK initialization.
- Local result: `baseview --all-features`, view-baseview, runner tests and formatting/diff gates passed.
- Unresolved: hosted CI has not validated this round; Phase 1 remains incomplete.

### 2026-08-13 — hosted CI #12 GDK initialization assertion

- Command/platform: push `6fb253d8fc8b0ac9647b3f82dc2288f6bf7fae81` 后 GitHub Actions Phase 1 #12：https://github.com/aizcutei/sunmao/actions/runs/31674923751
- Result: Linux tests/builds passed; Linux WebView failed at first creation because the raw GTK initialization did not mark the separately compiled GDK runtime as initialized (`GDK has not been initialized`). macOS/Windows were still progressing successfully.
- Fix: enable `unsafe-assume-initialized` on the direct Linux GDK dependency as well as GTK; raw `gtk_init_check()` remains the single underlying initialization.
- Local result: baseview all-features, view-baseview, runner tests and formatting/diff gates passed.
- Unresolved: hosted CI has not validated this round; Phase 1 remains incomplete.

### 2026-08-13 — hosted CI #13 Linux WebKit close ordering

- Command/platform: push `5dd3e78a751ee51588b0e81164d19d8f7b584ea4` 后 GitHub Actions Phase 1 #13：https://github.com/aizcutei/sunmao/actions/runs/31675432896
- Result: Linux tests/builds and initial WebView pixel/input passed, but close/recreate produced an X11 `BadWindow` while the old WebKit child was being destroyed after its parent window.
- Fix: make `WebviewHandler` own the WebView as `Option<WebView>` and drop it on `WindowEvent::WillClose`, before baseview tears down the parent X11 window.
- Local result: baseview all-features, view-baseview WebView, runner tests and formatting/diff gates passed.
- Unresolved: hosted CI has not validated this round; Phase 1 remains incomplete.

### 2026-08-13 — hosted CI #14 Windows WGPU cleanup and Linux X11 lifecycle

- Command/platform: push `a7451af39db5496f642e55c1d7d135653c672053` 后 GitHub Actions Phase 1 #14：https://github.com/aizcutei/sunmao/actions/runs/31675953940
- Result:
  - Windows GL/WGPU GUI rendered and completed its WGPU CLAP lifecycle, then exited 139 during cleanup; the retained legacy WGL context created during fallback was unsafe to tear down beside the WGPU surface.
  - Linux WebView still reported an X11 `BadWindow` during close/recreate despite dropping the WebView on `WillClose`; the GTK frame-clock warning remained.
- Fix: remove the legacy WGL context fallback. When modern WGL is unavailable, `BaseviewView` now receives no GL context and directly selects its WGPU compatibility handler, avoiding a stale GL context during WGPU cleanup.
- Local result: Windows target checks, baseview/view/runner tests and formatting/diff gates passed.
- Unresolved: Linux WebView close/recreate still needs hosted validation after the cleanup change; Phase 1 remains incomplete.

### 2026-08-13 — hosted CI #15 WGPU cleanup and X11 event draining

- Command/platform: push `10164d85b8e7791e21b43bf759e8ffa360ff2647` 后 GitHub Actions Phase 1 #15：https://github.com/aizcutei/sunmao/actions/runs/31676802616
- Result:
  - Windows reached and completed the WGPU CLAP GUI lifecycle, then still exited 139 during final cleanup.
  - Linux still reproduced WebKitGTK/X11 close-recreate instability; macOS GUI remained green through the matrix.
- Fix: after dropping a WebView on `WillClose`, explicitly drain platform WebView events before baseview destroys the X11 parent; retain the no-legacy-WGL change so WGPU cleanup has no stale GL context.
- Local result: baseview all-features, view-baseview WebView, runner tests, Windows target checks, and formatting/diff gates passed.
- Unresolved: hosted CI has not validated this round; Phase 1 remains incomplete.

### 2026-08-13 — hosted CI #16 packaging helper fork bomb

- Command/platform: push `048c1101` 后 GitHub Actions Phase 1 #16：https://github.com/aizcutei/sunmao/actions/runs/31679754064
- Result:
  - Windows first time passed every prior gate including "Package and exercise native GUI backends" (GL fallback → WGPU, WebView input, close/recreate all green).
  - The job then died in "Exercise repository packaging helper": the runner lost communication after the step ran ~60 minutes; macOS ran the same step 36+ minutes in #15 before cancellation. Linux/macOS were cancelled by fail-fast.
- Root cause: lines 11–12 of `tools/package_examples.sh` were Usage examples missing their `#` comment prefix, so every invocation re-executed `tools/package_examples.sh` with no arguments — unbounded recursion that fork-bombed the host. Reproduced locally with a clean shell (fork failures + repeated release builds); this also explains the earlier local "resource exhaustion" incident. The step has never completed on any hosted runner since the initial checkpoint commit.
- Fix: restore the `#` prefixes; add `timeout-minutes: 20` to the packaging-helper step so a future hang fails with diagnostics instead of killing the runner.
- Local result: `tools/package_examples.sh --debug --test` now completes in ~25s with all 16 bundles passing 16/16 runner tests.
- Unresolved: hosted CI has not validated this round; Phase 1 remains incomplete.

### 2026-08-14 — hosted CI #17 unbounded Linux GTK drain

- Command/platform: push `f5a0938` 后 GitHub Actions Phase 1 #17：https://github.com/aizcutei/sunmao/actions/runs/31764419303
- Result:
  - macOS and Windows jobs are fully green for the first time, including the repaired packaging helper.
  - Linux hung inside "Package and exercise native GUI backends" (30+ minutes; run #16 showed the same step hanging 70 minutes before cancellation).
- Root cause: the GTK event drain added in `048c110` loops `while gtk::events_pending()`, but WebKitGTK keeps re-arming frame-clock sources, so the queue never empties and the WillClose path spins forever. Runs #14/#15 (without the drain) failed the same step in ~30 seconds with `BadWindow` instead.
- Fix: bound the drain (`poll_events` / `pump_platform_events`) with a 100ms wall-clock budget; pending WebKit destroy work is still delivered before the X11 parent is torn down.
- Local result: fmt/diff gates and `baseview --all-features` tests pass (the Linux branch itself only compiles on Linux hosts).
- Unresolved: hosted CI has not validated this round; Phase 1 remains incomplete.

### 2026-08-14 — hosted CI #18 Linux GUI hang persists; add per-test timeout

- Command/platform: push `33b23dd` 后 GitHub Actions Phase 1 #18：https://github.com/aizcutei/sunmao/actions/runs/31766414064
- Result: macOS and Windows stayed fully green; Linux still hung in "Package and exercise native GUI backends" (30+ minutes) even with the bounded GTK drain, so the hang is deeper than the drain loop — most likely inside the WebView close/recreate path that previously crashed with `BadWindow` before ever being reached.
- Constraint: job logs require repo admin rights, so the only readable evidence is the `::error` annotations that echo each GUI test's log tail.
- Fix (diagnostic): wrap each Linux GUI invocation in `timeout --signal=TERM --kill-after=15 180` and add phase markers around close/settle/reopen in the runner, so the next run's annotation shows the last completed phase instead of a silent 75-minute job timeout.
- Local result: runner builds; fmt/diff gates pass.
- Unresolved: actual Linux hang not yet identified; Phase 1 remains incomplete.

### 2026-08-14 — hosted CI #19 Linux WebView recreate deadlock; dedicated GTK thread

- Command/platform: push `041741c` 后 GitHub Actions Phase 1 #19：https://github.com/aizcutei/sunmao/actions/runs/31768164175
- Result: macOS and Windows green again. Linux failed fast as designed (exit 124), and the annotation pinpointed the hang: `SunMao Gain WebView (VST3)` passed pixels, input, and host gestures, closed cleanly, then hung after `Reopening GUI...` — i.e. inside the second WebView creation.
- Root cause: GTK/WebKitGTK are permanently bound to the first thread that initializes GTK. baseview spawns a fresh event thread per `open_gui`, so recreating a WebView from the second thread deadlocks inside WebKit's synchronous UI-process IPC (the earlier `Gdk-CRITICAL frame_clock` warning was the symptom). The previous `gtk::set_initialized` workaround only silenced the assertion; it could not give the new thread GTK affinity.
- Fix: `baseview::WebView` on Linux now spawns one process-lifetime GTK thread that owns every `wry::WebView`; create/load/eval/resize/destroy are marshaled over a channel with a 10s reply timeout, the public type is a proxy, and `Drop` blocks until WebKit teardown ran so the X11 parent may be destroyed immediately afterwards. macOS/Windows keep the direct path.
- Local result: baseview + view_baseview all-features build/test, fmt/diff gates, and a macOS WebView gui-test (including recreate) pass.
- Unresolved: hosted Linux validation of the GTK-thread design; Phase 1 remains incomplete.

### 2026-08-14 — hosted CI #20 GTK thread works; sine WebView input geometry

- Command/platform: push `c4465c2` 后 GitHub Actions Phase 1 #20：https://github.com/aizcutei/sunmao/actions/runs/31770072820
- Result: macOS and Windows green. On Linux the dedicated GTK thread fixed the recreate deadlock — Gain WebView passed its entire lifecycle for the first time — and the matrix progressed to `SunMao Sine Synth WebView (VST3)`, where the XTEST drag at y=124 missed the Volume slider (parameter stayed at 0.5). WebKitGTK font metrics place the slider lower than WebKit macOS; macOS only passed because its input check uses a DOM-gesture fallback.
- Fix: pin the synth panel geometry (explicit line-heights, 24px slider box, zero slider margin) so the slider center lands at y=138 in the 520x220 view on every engine, and update the drag coordinates in `phase1.yml` and `tools/package_examples.sh` to 120,138 → 400,138.
- Local result: macOS gui-test passes with the new coordinates (input, host gesture, recreate); fmt/diff gates pass.
- Unresolved: hosted Linux validation of the remaining WebView fixtures; Phase 1 remains incomplete.

## 待记录

后续每次执行按以下格式追加：

```text
### YYYY-MM-DD — <milestone>
- Command/platform:
- Result:
- Evidence/artifact:
- Unresolved:
```
