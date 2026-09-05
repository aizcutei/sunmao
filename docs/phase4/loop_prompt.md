/goal SunMao Phase 4：GUI 组件库与平台完善

背景：仓库 /Users/z/Codes/rust/cursor/sunmao。Phase 1（run #25/c8401e6）、Phase 2 核心（run #38/77f788c）、Phase 3（run #69/b45efea，含 Phase 2 全部 7 项遗留收口与 sunmao/dsp）均已三平台 hosted 验收。每轮必读：docs/phase4/status.md 与 progress.md 末尾 3 条（无则处于 M0）、docs/phase3/status.md、docs/phase2/semantics.md、CLAUDE.md 的"声明式 GUI 框架"目标语法、git status/log 与当前分支。

分支：在 phase4/gui-component-library 工作（不存在则从 main 切出），严禁自行推送 main。

验收 Gate（每个 milestone 通用）：同一 commit 三平台（macOS ARM64、Win x86_64、Ubuntu x86_64）hosted native jobs 全绿并上传 artifacts，本地结果只作开发证据；Phase 1+2+3 全部既有 CI 步骤保持 blocking 且绿色；host-facing 能力必须 VST3 与 CLAP 同时落地，差异/降级写入 docs/phase2/semantics.md（附测试名）；公共 API 进 sunmao/gui 或新 crate 并入 prelude（带 doc-test）；audio callback 成功路径零 alloc；GUI 线程与 audio 线程只经无锁通道通信；新增不变量补 proptest。

Milestones（依序推进，每轮只解一个瓶颈）

M0 脚手架与清理收口：仿 phase3 建 docs/phase4/{status,progress}.md（同格式矩阵与日志）；先让待验证的 ABI 去重（vst3_rs 两个 export 宏合一 −321 行、clap_rs 26 个 _gui 冗余包装删除 −430 行、死代码与 no-op 守卫、7 个未用依赖）取得三平台 hosted 绿再进 M1；补 GUI fixture（旋钮＋下拉＋开关＋频谱）并入 workspace 与 CI。

M1 renderer 资源与线程归属：明确 GL/WGPU/WebView 三后端的设备/表面/上下文归属与销毁顺序，写入 docs/phase4/ownership.md；scale/DPI 协商（VST3 IPlugViewContentScaleSupport ↔ CLAP gui.set_scale）两格式落地并被 runner 断言；Windows WGPU 收尾 exit 139 若复现则在此定位 D3D 析构路径。

M2 布局与主题：实现 CLAUDE.md 目标语法的 Column/Row/gap/padding 与 Label/Knob/Slider/Toggle/Dropdown，参数双向绑定（Knob::param）零手写回调；主题 token 与暗/亮色；控件级单测＋布局 proptest；fixture 消费验证。

M3 text rendering 与输入：字体栅格化与文本度量、clipboard、IME/国际键盘（macOS/Win/X11 各至少一条真实输入路径）、cursor/focus 模型；runner 断言按键→参数变化可观测。

M4 可视化与 accessibility：VizChannel 无锁 audio→GUI 通道＋SpectrumAnalyzer/meter 组件（消费 Phase 3 metering，零分配发布）；accessibility 树（复用既有 UIA helper）；floating CLAP editor（gui.set_transient/suggest_title），VST3 侧记降级。

M5 Wayland 与总验收：X11 生命周期稳定后加 Wayland；semver/state 兼容策略补 GUI 部分；proptest 与文档收尾；三平台 hosted 全绿 → 更新 status.md 与 roadmap 标记 Phase 4 完成，停止 loop。

每轮流程：读 status 定位瓶颈 → 自底向上（_sys→_rs→backend→core/gui→fixture→runner→CI）逐层带测试 → 本地 gate 全过才 push：cargo metadata --locked、cargo fmt --all -- --check、git diff --check、RUSTFLAGS=-Awarnings cargo test --locked（勿用管道吞掉 cargo 退出码）、Windows target check（触平台代码时）、tools/package_examples.sh --debug --test（触打包/示例时）→ 追加 progress.md（固定四项格式）→ commit/push → curl 轮询 GitHub Actions API 监控（本机无 gh；失败读 /commits/<sha>/check-runs → /check-runs/<id>/annotations，必要时本地精确复现失败命令）→ CI 运行中只记录等待；失败自底向上修复，成功更新矩阵进下一瓶颈。

硬性规则：不回退 Phase 1/2/3 既有修复（Linux WebView 专用 GTK 线程、100ms GTK drain、Linux GUI timeout 180、WebView 固定几何、strip=false、packaged_standalone 按 out stem 命名、SUNMAO_UIA_HELPER_TIMEOUT_MS=20s、打包脚本头注释合法、有限 tail 夹到无限魔数之下、modulation 不入 state、旧版本 state 接受/未来版本拒绝）。契约变更不得破坏既有 fixture 的 state round-trip，演进必须走版本迁移测试。单格式独有能力必须在 semantics.md 记降级并测试，严禁静默丢事件；规范细节先读 vst3_sys/clap_sys 上游头文件，不猜。AU 不进默认 feature/gate，改动导出后用 nm 复查无 AU 符号。新 GUI 代码不得在 audio 线程分配或加锁。推送：本机 SSH 到 GitHub 不通（出口节点丢弃 SSH），用 HTTPS＋token 经环境变量传入，推完清 keychain。并发：可能有其他 agent 会话共用同一工作树，push 前 git log 确认 HEAD 未被 amend，严禁 force-push 已验收 commit。

剩余已知冗余（可在 M1 顺带处理，但须单独 commit 并三平台绿）：vst3_rs ControllerWrapper 与 GuiControllerWrapper 有 25 个函数／260 行在归一化类型名后 ≥0.995 相同，宜仿 clap_rs 的 audio_ports_config_ext! 用 $bound/$type 宏收敛；注意该处用字段偏移算术还原 this，宏化必须保持字段顺序与 repr(C) 布局不变，并补布局断言测试。

完成判定唯一标准：同 commit 三平台 hosted 全绿 + artifacts 可下载 + docs/phase4/status.md 完成规则满足。
