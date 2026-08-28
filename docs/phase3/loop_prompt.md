# SunMao Phase 3 实现循环指令（供 claude code /goal 或 /loop 使用）

背景：仓库 /Users/z/Codes/rust/cursor/sunmao。Phase 1（run #25/`c8401e6`）与 Phase 2 核心（run #38/`77f788c`，M0–M6 各自三平台 hosted 绿）已验收，main 已合并至 `2df01ce`。每轮必读：docs/phase3/status.md 与 progress.md 末尾 3 条（无则处于 M0）、docs/phase2/status.md 的"M6 遗留项"表、git status/log 与当前分支。

分支：在 phase3/framework-dsp-library 工作（不存在则从 main 切出），严禁自行推送 main。

验收 Gate（每个 milestone 通用）：同一 commit 三平台（macOS ARM64、Win x86_64、Ubuntu x86_64）hosted native jobs 全绿并上传 artifacts，本地结果只作开发证据；Phase 1+2 全部既有 CI 步骤保持 blocking 且绿色；host-facing 能力必须 VST3 与 CLAP 同时落地，差异/降级写入 docs/phase2/semantics.md（附测试名）；公共 API 进 sunmao/core 或新 crate 并入 prelude（带 doc-test）；audio callback 成功路径零 alloc；新增不变量补 proptest。

## Milestones（依序推进，每轮只解一个瓶颈）

M0 脚手架：仿 phase2 建 docs/phase3/{status,progress}.md（同格式矩阵与日志）；为 M2–M4 立 fixture（建议：参数分组 synth、SVF 滤波 fx、oversampled 失真、metering fx）并入 workspace 与 CI。

M1 Phase 2 收口（7 项遗留，一项不能漏，每项完成即更新 phase2/status.md 遗留表）：
1. bus 激活/去激活回调（VST3 activateBus ↔ CLAP audio-ports-activation）；
2. speaker layout 动态协商（setBusArrangements 真实协商而非固定接受 ↔ CLAP audio-ports-config）；
3. runner 宿主侧断言：latency/tail 查询、多 bus 拓扑枚举、向 sidechain 送信号验证路由；
4. backend 层 expression/mod 端到端映射测试（宿主原始事件 → core 队列）；
5. backend 在 state load 后按版本回调 migrate_state（两格式接线 + 测试）；
6. clap.preset-load 与 VST3 program list（统一为"插件侧载入回调 + 状态应用"，program list 可选实现）；
7. 无界 fuzz 脚手架（cargo-fuzz 或等价，仅本地/非 blocking，入口写入 README）。

M2 参数系统与构造 API：参数分组/嵌套（宿主可见层级：VST3 IUnitInfo ↔ CLAP module 路径）、参数 smoothing（线性/指数，零分配，与 automation/modulation 协同）、effect/instrument template（新插件样板 ≤50 行）；fixture 消费验证。

M3 DSP 基础组件：新建 sunmao/dsp crate——filters（一阶/SVF/biquad）、envelopes（ADSR/follower）、band-limited oscillators（sine/saw/pulse）；纯 no-alloc process API，每组件带单测 + proptest（稳定性、denormal、参数边界）；既有 fixture 换用组件实现且测试语义不变。

M4 oversampling/mixing/metering：2x/4x oversampling（latency 接入 Phase 2 契约并被 runner 断言）、dry/wet 与增益工具、peak/RMS metering（GUI 可读的无锁发布）；oversampled fixture 验证 latency 上报正确。

M5 版本兼容策略与总验收：为 sunmao/dsp 与 core API 写 semver/state 兼容策略文档；proptest 与文档收尾；三平台 hosted 全绿 → 更新 status.md 与 roadmap 标记 Phase 3 完成，停止 loop。

## 每轮流程

读 status 定位瓶颈 → 自底向上（_sys→_rs→backend→core/facade→fixture→runner→CI）逐层带测试 → 本地 gate 全过才 push：cargo metadata --locked、cargo fmt --all -- --check、git diff --check、RUSTFLAGS=-Awarnings cargo test --locked、Windows target check（触平台代码时）、tools/package_examples.sh --debug --test（触打包/示例时）→ 追加 progress.md（固定四项格式）→ commit/push → curl 轮询 GitHub Actions API 监控（本机无 gh；失败读 /commits/<sha>/check-runs → /check-runs/<id>/annotations，必要时本地精确复现失败命令）→ CI 运行中只记录等待；失败自底向上修复，成功更新矩阵进下一瓶颈。

## 硬性规则

- 不回退 Phase 1/2 既有修复：Linux WebView 专用 GTK 线程、100ms GTK drain、Linux GUI timeout 180、WebView 固定几何、strip=false、packaged_standalone 按 out stem 命名、SUNMAO_UIA_HELPER_TIMEOUT_MS=20s、打包脚本头注释合法、有限 tail 夹到无限魔数之下、modulation 不入 state、旧版本 state 接受/未来版本拒绝。
- 契约变更不得破坏既有 fixture 的 state round-trip；演进必须走版本迁移测试。
- 单格式独有能力必须在 semantics.md 记降级并测试，严禁静默丢事件；规范细节先读 vst3_sys/clap_sys 上游头文件，不猜。
- AU 不进默认 feature/gate；改动导出后用 nm 复查无 AU 符号。
- 已知 flake：Windows WGPU GUI 偶发在断言全过、打印 Done. 后 exit 139（收尾段错误，run #37 一次未复现）；再复现则深入 WGPU/D3D 析构路径，不要盲目重试。
- 完成判定唯一标准：同 commit 三平台 hosted 全绿 + artifacts 可下载 + docs/phase3/status.md 完成规则满足。
