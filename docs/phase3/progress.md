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
