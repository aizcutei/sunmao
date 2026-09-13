# SunMao

一套 Rust 代码同时输出 VST3、CLAP、AU 三种插件格式。VST3 与 CLAP 支持 Windows / macOS / Linux 全平台；AU 仅 macOS。

## 分层与排查顺序

```
*_sys   底层 FFI binding（照抄上游 C/C++ 头文件，不做抽象）
  ↓
*_rs    Rust 安全包装（vst3_rs / clap_rs / au_rs，负责 ABI 正确性与 panic 隔离）
  ↓
sunmao/backend_*   把各格式映射到统一契约
  ↓
sunmao/core        跨格式的 trait、参数、状态、事件
sunmao/dsp         无分配 DSP 组件
sunmao/gui*        渲染后端（GL / WGPU / WebView）
  ↓
examples/*         fixture，同时是验收样本
tools/             打包器与测试宿主 runner
```

**排查一律自底向上**：先确认 `_sys` 的结构体布局/常量与上游头文件一致，再看 `_rs` 的 ABI 行为，最后才怀疑 `sunmao` 层。规范细节先读 `vst3_sys`/`clap_sys` 里的上游定义，不要猜。

仓库里的代码不等于正确的代码——可能未完成或有缺陷。改动前先读该层现有测试。

## 硬性约束

- **audio 回调成功路径零分配、零加锁。** 新 DSP 与 GUI 代码都受此约束；跨线程一律走无锁通道。
- **host-facing 能力必须 VST3 与 CLAP 同时落地。** 只有单格式支持的能力，要在 `docs/phase2/semantics.md` 记录降级并附测试名，严禁静默丢事件。
- **AU 不进默认 feature/gate。** 改动导出后用 `nm` 复查产物无 AU 符号。
- **state 兼容**：旧版本 state 必须接受，未来版本必须拒绝；modulation 不进 state。契约演进走版本迁移测试，不得破坏既有 fixture 的 round-trip。策略见 `docs/phase3/compatibility.md`。
- 不回退既有平台修复：Linux WebView 专用 GTK 线程、100ms GTK drain、Linux GUI timeout 180、WebView 固定几何、`strip=false`、`packaged_standalone` 按 out stem 命名、`SUNMAO_UIA_HELPER_TIMEOUT_MS=20s`、有限 tail 夹到无限魔数之下。

## 验收标准

唯一标准是**同一 commit 在三平台（macOS ARM64 / Windows x86_64 / Ubuntu x86_64）hosted native jobs 全绿且 artifacts 可下载**。本地结果只作开发证据，不能替代。

**但 job 全绿不等于断言跑过。** 本仓已两次踩到"三平台全绿而目标断言 0 次执行"（run #82 的
`GUI key verified`、run #86 的跨线程 viz 测试当时是 flaky 的）。因此验收必须下载原始日志、
grep 到实际断言行；**新写的守卫还要反向验证它真的会变红**（Phase 4 的 `compile_fail`
doc-test 就是临时摘掉属性、确认变红、再还原才算数）。

本地 gate（push 前全过）：

```bash
cargo metadata --locked
cargo fmt --all -- --check
git diff --check
RUSTFLAGS=-Awarnings cargo test --locked      # 别用管道，会吞掉 cargo 退出码
tools/package_examples.sh --debug --test      # 触打包/示例时
```

基线：`cargo test` **135 套件 / 676 passed / 0 failed**（2026-09-13 本地实测两次）；
打包 **32 套件 / 640 断言**（最后一次实测见 `docs/phase4/progress.md`，本轮未重跑）。
纯重构应当与基线逐位相同。CI 上的数字更高且三平台各不相同（144 / 142 / 162 套件），因为
hosted job 另跑 feature 组合与平台专项，不要拿它和本地数字对比。

## 进展记录

**及时把进展写进文件。** 每个阶段有 `docs/phase<N>/status.md`（矩阵）与 `progress.md`（日志，固定四项格式）；总体方向见 `docs/roadmap.md`。开工先读当前 phase 的这两份文件末尾。

## 当前状态

Phase 1（run #25 / `c8401e6`）、Phase 2 核心（run #38 / `77f788c`）、Phase 3（run #69 / `b45efea`）、Phase 4（run 34746764198 / `28cba05`）均已三平台验收。下一阶段是 **Phase 5：完整测试宿主与外部兼容**。

Phase 5 的 goal prompt 见 `docs/phase5/loop_prompt.md`；历史各 phase 同名文件保留。

`docs/design/target_syntax.md` 描述的是**目标语法，部分仍未实现**——该文件开头有逐项核对的现状对照表，照抄未实现的名字会编译失败。

已知遗留（各自单独立项，逐条见 `docs/phase4/audit.md`）：`Stack::focus_next` 会把 Tab 停在
非交互控件；`vst3_rs` 两个控制器包装 25 函数 / 260 行重复；Windows WGPU 收尾 exit 139
**不改判为已修复**；`main` 仍落后于 Phase 3/4 工作，合并需仓库所有者决定。

## 环境注意

- **推送直接 `git push` 走 SSH 即可**（2026-09-13 实测 `ssh -T git@github.com` 认证通过、push 成功）。此处原先写的是"SSH 不通、必须 HTTPS + token"，那条已经过期却被当成事实沿用过一次，导致把"我推不了"当作阻塞上报——**声称某项环境能力不可用之前先实测**。
- 本机无 `gh`，CI 用 `curl` 轮询 Actions API。**读 Actions 日志与下载 artifact 需要 token**（公开仓库也一样：无 token 时 logs 回 403、artifact zip 回 401），token 由用户放在仓库外的文件里，用 `$(cat …)` 引用以免进入命令文本；仓库 `.gitignore` 另有防御性条目。
- 可能有其他 agent 会话共用同一工作树。push 前 `git log` 确认 HEAD 未被 amend；**严禁 force-push 已通过 CI 的 commit**。
- 已知 flake：Windows WGPU GUI 偶发在断言全过、打印 `Done.` 后 exit 139（收尾段错误）。再复现应深入 WGPU/D3D 析构路径，不要盲目重试。
