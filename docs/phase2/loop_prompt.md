# SunMao Phase 2 实现循环指令（供 claude code /loop 使用）

## 背景与目标（每轮先自检，不要盲信本段描述的状态）

仓库：`/Users/z/Codes/rust/cursor/sunmao`。Phase 1（VST3 + CLAP + standalone，三平台，基础契约 + GUI + 打包 + 测试宿主）已由 hosted run #25（commit `c8401e6`）验收，证据见 `docs/phase1/status.md`。

Phase 2 目标（见 `docs/roadmap.md`）：在不破坏 Phase 1 任何已验收能力的前提下，为同一份 SunMao 插件实现补齐高级插件契约——多 audio bus/sidechain、动态 routing 与 speaker layout、modulation/per-note expression、transport/timing 完整模型、latency/tail/offline render、voice-info、plugin-owned state、preset 与 migration——并继续满足 audio thread 无分配、无阻塞约束，加入 property/fuzz 测试。standalone 保持 Phase 1 的设备/窗口契约不扩展（但不得回归）；AU 仍然不进 gate。

分支策略：在 `phase2/advanced-plugin-contract` 分支上工作（不存在则从 Phase 1 验收 commit 或其后代切出）。不要自行向 `main` 推送；main 的合并策略由用户决定。

每轮开始必读：

1. `docs/phase2/status.md` 与 `docs/phase2/progress.md` 末尾 3 条（不存在说明处于 M0）
2. `docs/phase1/status.md` 的完成规则（Phase 2 沿用同样的证据标准）
3. `git status --short`、`git log --oneline -3`、当前分支名

## 验收 gate（每个 milestone 和最终验收都适用）

- 同一 commit 上三个 hosted native jobs（macOS ARM64、Windows x86_64、Ubuntu x86_64）全绿并上传 artifacts；本地结果一律标注平台与证据等级，不能替代 hosted 证据。
- Phase 1 的全部既有 CI 步骤保持 blocking 且绿色（gain/sine fixtures、GUI matrix、standalone smoke、packager、runner）。
- 每个新的 host-facing 能力必须：VST3 与 CLAP 同时落地；语义差异与降级行为写入 `docs/phase2/semantics.md`；`sunmao/core` 暴露统一 API 并进 `prelude`（带 doc-test）；realtime allocation matrix 扩展覆盖其成功路径；`sunmao_unittest_runner` 增加对应 host 侧测试。
- 新事件/参数/路由路径全部固定容量，audio callback 成功路径零 alloc/realloc/dealloc。
- property tests（proptest 或等价）进 gate；时间无界的 fuzz 只做本地/非 blocking。

## Milestones（按依赖顺序推进；每轮只解决当前 milestone 的一个瓶颈）

- **M0 脚手架**：创建 `docs/phase2/status.md`（目标、硬门槛、矩阵表、完成规则，格式仿 phase1）、`docs/phase2/progress.md`（同格式日志）、`docs/phase2/semantics.md`（两格式语义映射表骨架）；选定 Phase 2 acceptance fixtures 并立项（建议至少：sidechain compressor、tempo-synced delay（含 tail + latency）、poly expression synth、state v1→v2 migration fixture；若用户提供了 C++ 插件解析文档，优先把其中插件立为 fixture）；把 fixture 骨架加入 workspace 与 CI 计划。
- **M1 transport/timing**：tempo、拍号、小节/绝对位置、loop 区间、播放状态，进 `ProcessContext`；VST3 `ProcessContext` ↔ CLAP `clap_event_transport` 映射；tempo-synced delay fixture 开始消费。
- **M2 latency/tail/offline render**：latency 上报与变更通知、tail 长度、realtime/offline render 模式；VST3 `getLatencySamples`/`getTailSamples`/`setupProcessing` ↔ CLAP `latency`/`tail`/`render` 扩展（`clap_rs/src/ext/` 已有骨架，检查完成度）；lookahead + tail fixture 验证宿主侧行为（runner 增加 latency/tail 断言）。
- **M3 多 bus/sidechain/speaker layout**：静态多 bus 声明、sidechain 输入、bus 激活/去激活、常用 layout（mono/stereo，预留 surround 枚举）与动态协商；VST3 bus arrangements ↔ CLAP audio-ports/audio-ports-config；sidechain compressor fixture 全流程。
- **M4 modulation/per-note expression/voice-info**：参数 modulation（CLAP mod 与 VST3 的语义差异要写清）、per-note expression/MPE 输入路径、voice-info（CLAP 有、VST3 无 → 定义降级）；poly synth fixture。
- **M5 plugin-owned state/preset/migration**：版本化 state 的升级路径、宿主/插件双向 preset 载入、migration 框架与测试（老版本 state 注入 → 新版本读回断言）；为"外部格式兼容层"留 API 钩子但不实现具体格式。
- **M6 总验收**：property/fuzz 套件收尾、CI 扩展步骤全部转 blocking、三平台 hosted 全绿 + artifacts → 更新 `docs/phase2/status.md` 为完成、更新 roadmap，**停止 loop**。

## 每轮流程（状态机）

1. 判断所处 milestone 与瓶颈（读 status/progress）。M0 未完成先做 M0。
2. **自底向上**实现：`_sys`（缺 binding 先补）→ `_rs` 安全包装（含 ABI/生命周期/panic 边界测试）→ `sunmao/backend_*` 适配 → `sunmao/core` + facade API → fixture/example → runner host 侧测试 → CI 步骤。每层有测试再往上走。
3. 本地 gate（全部通过才可 push）：`cargo metadata --locked --no-deps`、`cargo fmt --all -- --check`、`git diff --check`、`RUSTFLAGS=-Awarnings cargo test --locked`、`cargo check --locked --target x86_64-pc-windows-msvc`（触及平台相关代码时）、`bash -n tools/package_examples.sh` + workflow YAML 解析、`tools/package_examples.sh --debug --test`（触及打包/示例时）。
4. 在 `docs/phase2/progress.md` 追加记录（格式同 phase1：`### YYYY-MM-DD — <milestone>` + Command/platform / Result / Evidence/artifact / Unresolved），提交并 push 到 phase2 分支。
5. 监控 hosted CI：本机无 `gh`，轮询 `curl -s "https://api.github.com/repos/aizcutei/sunmao/actions/runs?branch=<分支名URL编码>&per_page=1"`；失败时 job 日志需要 admin 权限，可读证据是 `::error` annotations（`.../commits/<sha>/check-runs` → `.../check-runs/<id>/annotations`）；annotations 不够时在本地精确复现失败步骤的命令定位（Phase 1 的 #23/#24 都是这样修的）。CI 运行中则本轮只记录状态等待。
6. 失败 → 自底向上定位修复，回到第 3 步；成功 → 更新 status 矩阵，进入下一瓶颈。

## 硬性规则

- 绝不回退 Phase 1 的既有修复：Linux WebView 进程级专用 GTK 线程、100ms 预算 GTK drain、Linux GUI `timeout 180` 包装、WebView 固定几何（拖动 120,138 → 400,138）、`[profile.release] strip = false`、`packaged_standalone()` 以 `--out` stem 命名、`SUNMAO_UIA_HELPER_TIMEOUT_MS`（CI 20s）、`package_examples.sh` 头注释合法性。
- 契约变更（尤其 bus/layout/事件结构）不得破坏 Phase 1 fixtures 的 state round-trip；state 格式若必须演进，走 M5 的版本化路径并写迁移测试。
- 一个格式独有的能力，必须在 `semantics.md` 里写明另一格式的降级行为并测试之；不许静默丢事件。
- 不确定 VST3/CLAP 规范细节时，先读 `vst3_sys`/`clap_sys` 里的上游头文件注释，再读 `_rs` 层，不要猜。
- AU 源码保留但不进默认 feature/gate；触及导出路径后用 `nm` 复查默认产物无 AU 符号。
- 每轮只解决一个瓶颈，写完日志结束本轮。宣称 Phase 2 完成的唯一标准：同一 commit 三平台 hosted 全绿 + artifacts + `docs/phase2/status.md` 完成规则全部满足。
