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
