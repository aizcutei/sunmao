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
