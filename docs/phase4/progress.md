# Phase 4 进展日志

按时间追加，格式固定：

```text
### YYYY-MM-DD — <milestone>
- Command/platform:
- Result:
- Evidence/artifact:
- Unresolved:
```

### 2026-09-05 — M0 脚手架：GUI fixture 并入 workspace 与 CI

- Command/platform: macOS ARM64，分支 `phase4/gui-component-library`。
  **基点是 `phase3/framework-dsp-library` 的尖端 `e844215`，不是 main**——main 仍停在
  Phase 2 的 `2df01ce`，Phase 3 的 33 个 commit 从未合并。已用
  `git merge-base --is-ancestor main HEAD` 确认新分支是 main 的严格超集，故"从 main 切出"
  在祖先关系上成立；若按字面从 main 切会丢掉 `sunmao/dsp`（M4 要消费其 metering）、
  两个模板与已 CI 验证的清理。Phase 3 → main 的合并留给仓库所有者。
- Change:
  - 新建 Phase 4 acceptance fixture `examples/sunmao_fx_widgets_gui_gl`，覆盖 Phase 4 要
    交付的四类控件：旋钮（连续，用框架现有 `Knob`）、下拉（离散，`IntParam`——`EnumParam`
    尚不存在）、开关（布尔）、频谱（audio→GUI 数据，非控件）。下拉/开关/频谱是 **crate 内
    skeleton**（`DropdownSkeleton`/`ToggleSkeleton`/`SpectrumSkeleton`），与 Phase 3 fixture
    先携 inline DSP 的做法一致；M2/M4 用框架组件替换它们时**测试语义必须不变**。
  - audio→GUI 走 `SpectrumPublisher`：每 band 一个 `AtomicU32` 位存 f32，audio 侧每块
    relaxed store 一次，GUI 侧绘制时读。无锁、无分配。8 个 band 用 `sunmao/dsp` 的 `Svf`
    带通分析，音色用 `OnePole`——即 Phase 3 组件在 Phase 4 的第一个消费方。
  - `.github/workflows/phase1.yml` 新增 blocking 步骤 "Test Phase 4 acceptance fixtures"
    （与 Phase 3 步骤同构：逐 crate `cargo test --locked -p` + 失败回显日志尾部 +
    `cargo build` 覆盖 cdylib 路径）。每 job 步骤数 25 → 26。
  - 新建 `docs/phase4/{status,progress}.md`。
- Result:
  - fixture 9 单元测试通过，其中 `process_and_spectrum_publish_do_not_allocate` 复用
    backends/`voice.rs` 的 `GlobalAlloc` 计数器把"audio 路径零分配"**机械钉住**。
    **该断言经反向验证有效**：临时在计数区间内插入一次 `vec![0; 64]`，测试如期失败并
    报告 "allocated 2 times"，随后还原——一个不会失败的零分配测试没有价值。
  - 完整 `RUSTFLAGS=-Awarnings cargo test --locked` **exit 0，126 套件 / 536 测试全绿**
    （Phase 3 基线 124 / 527，增量恰为本 fixture 的 lib + doc-test 两套件与 9 测试）。
  - `cargo check --locked --target x86_64-pc-windows-msvc -p sunmao_fx_widgets_gui_gl`
    exit 0。
  - `tools/package_examples.sh --debug --test` exit 0，**30 套件 / 600 断言，与 Phase 3
    基线逐位相同**——Phase 1/2/3 回归无损。
  - `cargo metadata --locked`、`cargo fmt --all -- --check`、`git diff --check`、
    `bash -n tools/package_examples.sh` 全过。
  - `nm -gU` 复查新 cdylib：导出 `_GetPluginFactory`、`_clap_entry`、`_bundleEntry`/
    `_bundleExit` 与两个 pixel probe 钩子，共 7 个符号，**AU 符号 0 个**。
- Evidence/artifact: macOS ARM64 本地日志（`/tmp/p4_test.log`、`/tmp/p4_pkg.log`、
  `/tmp/p4_win.log`）——本地证据等级，不构成验收。
- Unresolved:
  - 本 commit 需三平台 hosted 全绿（Phase 1+2+3 既有 gate 与新增 Phase 4 步骤同时绿）
    M0 才算完成。
  - fixture **暂未进打包矩阵**，是有意推迟到 M2 真控件落地时（理由见 status.md）；
    Phase 3 的教训是这一步不能省，只是应在有真实宿主可见行为时做。
  - M1（renderer 资源与线程归属、scale/DPI 协商）未开始。
  - M0 的"清理收口"一项在开 Phase 4 之前即已完成：[run #71](https://github.com/aizcutei/sunmao/actions/runs/33956858763)
    （commit `7dabb3d`，ABI 去重 −1567 行）与 run #72（commit `e844215`，CLAUDE.md 精简）
    各自三平台 25 步零非成功、artifacts 齐备。
