/goal SunMao Phase 5：完整测试宿主与外部兼容

背景：仓库 /Users/z/Codes/rust/cursor/sunmao。Phase 1（run #25/c8401e6）、Phase 2 核心（run #38/77f788c）、Phase 3（run #69/b45efea）、Phase 4（run 34746764198/28cba05，GUI 组件库＋Wayland＋AccessKit 桥接）均已三平台 hosted 验收。每轮必读：docs/phase5/status.md 与 progress.md 末尾 3 条（无则处于 M0）、docs/phase4/audit.md 的遗留清单、docs/phase2/semantics.md、docs/phase3/compatibility.md、CLAUDE.md、git status/log 与当前分支。

分支：在 phase5/test-host-compat 工作（不存在则从 phase4/gui-component-library 尖端切出；main 仍落后于 Phase 3/4，严禁自行推送 main）。

验收 Gate（每个 milestone 通用）：同一 commit 三平台（macOS ARM64 / Win x86_64 / Ubuntu x86_64）hosted native jobs 全绿并上传 artifacts，本地结果只作开发证据；Phase 1–4 既有 CI 步骤保持 blocking 且绿。**job 全绿不等于断言跑过**——本仓已两次踩到全绿而目标断言 0 次执行，故每项验收必须下载原始日志 grep 到实际断言行；新加的守卫还要反向验证它真的会变红。host-facing 能力必须 VST3 与 CLAP 同时落地，差异与降级写入 semantics.md 并附测试名；公共 API 入 prelude 带 doc-test；audio 回调成功路径零分配零加锁。

Milestones（依序推进，每轮只解一个瓶颈）

M0 脚手架与基线：建 docs/phase5/{status,progress}.md（沿用 phase4 的矩阵与四项日志格式）；清点 sunmao_unittest_runner 现有能力与缺口；把本地 gate 基线数字（套件数/断言数/打包套件数）写入 status.md，后续以此判断"纯重构应逐位相同"。

M1 交互式 standalone host：把 runner 扩为可交互宿主——加载已打包的 .vst3/.clap、枚举参数与 bus、改参数、存取 state/preset、开关编辑器；既有非交互 CI 用法必须原样不变。

M2 批量 regression host：确定性批跑（固定种子、固定 buffer 尺寸、固定块划分），输出可比对的音频与参数轨迹；golden 对拍并证明跨平台一致——浮点容差必须显式定义而非默认相等；把 fuzz/ 的 state 解码 fuzz 接成有界 CI 步骤（不得让无界 fuzz 进 blocking gate）。

M3 性能与泄漏检测：RT 安全检测（分配/加锁/系统调用）从 audio 线程扩到 GUI 线程与宿主回调；泄漏检测（开关编辑器 N 次、扫描-实例化-销毁 N 次）；基准与阈值写入 status.md，阈值回归即红。

M4 外部 validator：接入 clap-validator 与 Steinberg VST3 validator，三平台各自 blocking 步骤；失败项逐条归因——是我们的缺陷，还是 validator 的期待与规范不符，后者要引上游原文。AU 仍不进默认 feature/gate，改动导出后 nm 复查产物无 AU 符号。

M5 DAW smoke 与兼容性报告：至少一个可脚本化 DAW（如 REAPER）在三平台加载/处理/存工程/重开；输出机器可读的兼容性报告为 artifact；三平台全绿 → 更新 status.md 与 roadmap 标记 Phase 5 完成，停止 loop。

每轮流程：读 status 定位瓶颈 → 自底向上（_sys→_rs→backend→core/gui→fixture→runner→CI）逐层带测试 → 本地 gate 全过才 push：cargo metadata --locked、cargo fmt --all -- --check、git diff --check、RUSTFLAGS=-Awarnings cargo test --locked（勿用管道吞掉 cargo 退出码）、tools/package_examples.sh --debug --test（触打包/示例时）→ 追加 progress.md（固定四项格式）→ commit/push（SSH 直接可用，别问 token）→ curl 轮询 Actions API（读日志与下载 artifact 需 token：无 token 时 logs 回 403、artifact zip 回 401；token 放仓库外用 $(cat …) 引用）→ CI 运行中只记录等待；失败自底向上修复。

硬性规则：不回退既有平台修复（Linux WebView 专用 GTK 线程、100ms GTK drain、Linux GUI timeout 180、WebView 固定几何、strip=false、packaged_standalone 按 out stem 命名、SUNMAO_UIA_HELPER_TIMEOUT_MS=20s、有限 tail 夹到无限魔数之下）。state：旧版本必须接受、未来版本必须拒绝、modulation 不进 state，契约演进走版本迁移测试，不破坏既有 fixture 的 round-trip。规范细节先读 vst3_sys/clap_sys 的上游转录，不猜。并发：可能有其他 agent 共用同一工作树，push 前 git log 确认 HEAD 未被 amend，严禁 force-push 已通过 CI 的 commit。

可顺带处理但须各自单独 commit 并单独取三平台绿：Stack::focus_next 只做索引边界检查，Tab 会停在非交互控件；vst3_rs 的 ControllerWrapper 与 GuiControllerWrapper 有 25 函数／260 行重复，宏化须保持 repr(C) 字段顺序并补布局断言；Windows WGPU 收尾 exit 139（复现则深入 D3D 析构路径，不盲目重试）。

完成判定唯一标准：同 commit 三平台 hosted 全绿 + artifacts 可下载 + docs/phase5/status.md 完成规则满足。
