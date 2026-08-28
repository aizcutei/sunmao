# SunMao Phase 1 收尾循环指令（供 claude code /loop 使用）

## 背景（每轮先自检，不要盲信本段描述的状态）

仓库：`/Users/z/Codes/rust/cursor/sunmao`，分支 `phase1/vst3-clap-cross-platform`。

目标：一份 SunMao Rust 实现导出 VST3、CLAP、standalone 三种产物，在 macOS ARM64、Windows x86_64、Ubuntu x86_64 三个 hosted CI native job 上**同一 commit 全绿并上传 artifacts**。AU 不在本阶段范围（opt-in feature，默认产物必须 AU-free）。

历史基线：hosted run #21（commit `885d2a5`）已让旧版 VST3/CLAP gate 三平台全绿；当前工作区包含未提交的 ABI/生命周期/GUI/packager/runner hardening 和 standalone gate 扩展（约 118 个文件），需要新 commit 的 hosted 重新验证后才能宣布 Phase 1 完成。

每轮开始必读：

1. `docs/phase1/status.md`（验收标准与证据表）
2. `docs/phase1/progress.md` 末尾 3 条记录（上一轮做到哪里）
3. `git status --short`、`git log --oneline -3`

## 每轮流程：判断当前处于哪个状态，只推进一个瓶颈，然后结束本轮

### 状态 A：有未提交改动，且尚未通过完整本地 gate

依次运行（任何一步失败即转入修复，修复后从头重跑）：

1. `cargo metadata --locked --no-deps > /dev/null`
2. `cargo fmt --all -- --check`
3. `git diff --check`
4. `RUSTFLAGS=-Awarnings cargo test --locked`（全 workspace，含 doc-tests）
5. `bash -n tools/package_examples.sh`；确认 `.github/workflows/phase1.yml` YAML 可解析
6. `tools/package_examples.sh --debug --test`（VST3/CLAP runner 测试 + 8 个 raw/packaged standalone `--smoke`；本机 WindowServer 允许时追加 GUI smoke）

全部通过 → 转状态 B。

### 状态 B：本地 gate 全绿、改动未提交

1. 先在 `docs/phase1/progress.md` 追加一条记录（格式见"硬性规则"）。
2. 提交全部 Phase 1 改动（一个或少数几个逻辑 commit；message 说明 hardening + standalone gate 扩展的内容）。
3. 确认远端没有正在运行的 hosted run（push 会取消它），然后 `git push origin phase1/vst3-clap-cross-platform` → 转状态 C。

### 状态 C：已 push，hosted CI 运行中

本机没有 `gh` CLI。二选一：

- 首选：`brew install gh`，`gh auth login` 后用 `gh run list` / `gh run watch` 监控。
- 否则轮询公共 API：
  `curl -s "https://api.github.com/repos/aizcutei/sunmao/actions/runs?branch=phase1%2Fvst3-clap-cross-platform&per_page=1"`，看最新 run 的 `status` 和 `conclusion`。

仍在运行 → 本轮只记录一句状态，结束本轮等待下一轮。已完成 → `success` 转状态 E，`failure`/`cancelled` 转状态 D。

### 状态 D：hosted CI 失败

1. 取证据：job 完整日志需要 repo admin 权限，可读证据是 `::error` annotations（GUI 测试失败会回显日志尾部；runner 各阶段有 phase marker）。用
   `curl -s "https://api.github.com/repos/aizcutei/sunmao/commits/<sha>/check-runs"` 找到失败 check-run 的 id，再取
   `curl -s "https://api.github.com/repos/aizcutei/sunmao/check-runs/<id>/annotations"`。
2. 按 CLAUDE.md 的层次**自底向上**定位：`_sys` → `_rs` → `sunmao/backend_*` → `gui*/view_baseview/baseview` → `tools`/CI 脚本。
3. 把失败根因、修复方案写入 `docs/phase1/progress.md`。
4. 修复后回到状态 A：完整本地 gate 必须重跑后才能再 push（hosted CI 一轮约 8 分钟且证据获取困难，不要拿 CI 当调试器）。

### 状态 E：三平台 job 同一 commit 全绿

1. 用 API 确认三个 job 均 success，且 `phase1-macOS-*`、`phase1-Windows-*`、`phase1-Linux-*` artifacts 上传成功（记录大小作为证据）。
2. 更新 `docs/phase1/status.md`：当前工作区状态改为 "Phase 1 完成"，附 run URL、commit sha、artifacts 清单；同步矩阵表各行。
3. 在 `docs/phase1/progress.md` 追加完成记录；如 README / `docs/roadmap.md` 有过期表述一并修正。
4. 提交并 push 文档更新（该 commit 会再触发一次 CI，属预期；gate 证据是上一个全绿 run，无需等待）。
5. **停止 loop**，向用户宣布 Phase 1 完成并给出 run URL 与 artifacts 证据。

## 硬性规则

- `progress.md` 追加格式固定：`### YYYY-MM-DD — <milestone>`，正文四项 `Command/platform:`、`Result:`、`Evidence/artifact:`、`Unresolved:`。每轮至少写一条。
- 绝不在没有"同一 commit 三平台 hosted 全绿 + artifacts 可下载"证据时宣称 Phase 1 完成；一切本地结果必须标注平台与证据等级。
- 不得回退以下来之不易的修复（回归历史见 progress.md CI #16–#21 各条）：
  - Linux WebView 的进程级专用 GTK 线程（WebKitGTK 永久线程亲和，第二次在新线程建 WebView 会同步 IPC 死锁）；
  - 100ms 时间预算的 GTK event drain（无界 drain 会因 frame clock re-arm 永不退出）；
  - Linux GUI 测试的 `timeout --signal=TERM --kill-after=15 180` 包装与 phase marker；
  - WebView 面板固定几何（显式 line-height、24px slider box，拖动坐标 120,138 → 400,138）；
  - 根 Cargo.toml 的 `[profile.release] strip = false`（macOS ld LINKEDIT 对齐 bug）；
  - `tools/package_examples.sh` 头部注释必须保持合法 shell 注释（曾因缺 `#` 无参自递归 fork bomb 拖死 runner）。
- audio callback 成功路径保持零 alloc/realloc/dealloc；不要为修 GUI 或 standalone 弄坏 realtime allocation matrix 测试。
- AU 源码保留但不得进入默认 feature/构建/gate；改动导出相关代码后用 `nm` 复查默认产物无 `RustAUFactory|au_component_factory|SunmaoAUCocoa` 符号。
- 每轮只解决一个瓶颈，写完日志就结束本轮。对 CI 步骤行为不确定时，先读 `.github/workflows/phase1.yml` 与 `tools/package_examples.sh`，不要猜。
