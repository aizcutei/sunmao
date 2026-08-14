# Phase 1 状态

更新时间：2026-08-13

## 目标与边界

一期只验收 VST3 + CLAP。每个 acceptance fixture 使用一份 Rust/SunMao 实现同时导出两种格式，目标平台为 macOS、Windows x86_64、Linux x86_64。AU 源码和显式 AU feature 保留，但不进入一期构建、测试、打包或完成条件。

硬门槛：

- gain effect 与 sine instrument；0/mono/stereo 基础声道契约；audio/MIDI 处理。
- Float、Int、Bool 参数元数据、sample-offset automation、reset、参数 state round-trip、错误和密集事件边界。
- audio callback 成功路径无 alloc/realloc/dealloc。
- Cocoa、Win32、X11 上 GL/WGPU/WebView 的 attach、非空像素、输入、`520x220` resize、close/recreate。
- `sunmao_packager` 的 Mach-O/PE/ELF 与架构检查、VST3/CLAP 目标布局、staging/rollback。
- `sunmao_unittest_runner` 的 scan/info/test/process/gui/gui-test，失败/超时非零退出。
- 同一 commit 的 macOS、Windows、Ubuntu hosted CI 全绿并上传 bundles、日志和检查报告。

明确延期：AU、Wayland/floating editor、多 bus、完整 latency/tail/offline/voice-info、preset/migration、完整 GUI toolkit、签名/notarization/installers/universal binary、Linux 物理声卡、ScreenCaptureKit 真实捕获、真实 DAW GUI gesture automation recording。

## 当前基线

- 本地分支：`phase1/vst3-clap-cross-platform`。
- 已 push：https://github.com/aizcutei/sunmao/tree/phase1/vst3-clap-cross-platform
- Hosted CI #1（`0b0319e`）：失败，https://github.com/aizcutei/sunmao/actions/runs/31660899211
- Hosted CI #2（`750fae3`）：失败，https://github.com/aizcutei/sunmao/actions/runs/31661879049
- Hosted CI #3（`d172c77`）：失败，https://github.com/aizcutei/sunmao/actions/runs/31664299209
- Hosted CI #4（`0a625d9`）：失败，https://github.com/aizcutei/sunmao/actions/runs/31665587024
- Hosted CI #5（`51db6b8`）：失败，https://github.com/aizcutei/sunmao/actions/runs/31669974672
- Hosted CI #6（`98b2c31`）：失败，https://github.com/aizcutei/sunmao/actions/runs/31670494262
- Hosted CI #7（`6b9a8bc`）：失败，https://github.com/aizcutei/sunmao/actions/runs/31671132074
- Hosted CI #8（`1cac6e2`）：失败，https://github.com/aizcutei/sunmao/actions/runs/31672115274
- Hosted CI #9（`bebd200`）：失败，https://github.com/aizcutei/sunmao/actions/runs/31672658169
- Hosted CI #10（`e1a75ac`）：失败，https://github.com/aizcutei/sunmao/actions/runs/31673224274
- Hosted CI #11（`d6cc964`）：失败，https://github.com/aizcutei/sunmao/actions/runs/31674003019
- Hosted CI #12（`6fb253d`）：失败，https://github.com/aizcutei/sunmao/actions/runs/31674923751
- Hosted CI #13（`5dd3e78`）：失败，https://github.com/aizcutei/sunmao/actions/runs/31675432896
- Hosted CI #14（`a7451af`）：失败，https://github.com/aizcutei/sunmao/actions/runs/31675953940
- Hosted CI #15（`10164d8`）：被 #16 push 取消，https://github.com/aizcutei/sunmao/actions/runs/31676802616
- Hosted CI #16（`048c110`）：失败，https://github.com/aizcutei/sunmao/actions/runs/31679754064
- #16 里 Windows 首次通过全部 GUI backends 步骤；失败发生在 "Exercise repository packaging helper"：`tools/package_examples.sh` 头部 Usage 注释缺 `#`，脚本无参自递归（fork bomb），拖死 runner（Windows "lost communication"，macOS 卡 36 分钟）。已修复并加 step 级 timeout。
- Hosted CI #17（`f5a0938`）：macOS 与 Windows job 首次全绿（含 packaging helper）；Linux 卡死在 GUI backends 步骤——`048c110` 引入的无界 GTK drain 循环（WebKitGTK frame clock 持续 re-arm，`events_pending()` 永不为空）。已改为 100ms 时间预算的有界 drain。
- Hosted CI #18（`33b23dd`）：macOS + Windows 继续全绿；Linux GUI backends 步骤仍挂起（30 分钟+），说明 hang 不止在 drain（疑似 WebView close/recreate 深处）。因无 repo admin 权限拿不到 job log，改为给 Linux GUI 调用加 3 分钟 `timeout` + 阶段化打点，让 ::error 注解暴露卡点；**Phase 1 仍未完成**。
- `cargo metadata --locked --no-deps`、`cargo fmt --all -- --check`、`git diff --check` 已在本机通过。
- 默认 VST3/CLAP 二进制经 `nm` 确认无 `RustAUFactory|au_component_factory|SunmaoAUCocoa`。

## 矩阵状态

| 能力 | 当前判断 | 权威证据 | 下一步 |
|---|---|---|---|
| workspace metadata / fmt / diff | 本机通过 | command log | hosted job 复验 |
| `_sys`/`_rs` ABI 与生命周期 | 本机 selected tests 通过 | cargo test logs | Windows/Ubuntu hosted |
| SunMao core/backend | Int/Bool normalized API、offset automation、固定容量、state 已实现并本机通过 | cargo test + runner 16/16 | hosted 复验 |
| packager | layout/format/arch/staging-rollback 本机 unit+cli tests 通过 | `sunmao_packager` tests | hosted 复验 |
| runner | scan/info/test/process/gui-test 本机通过；缺失插件与未知命令非零退出 | runner logs | hosted 复验 |
| macOS GUI | 本机 GL/WGPU/WebView × VST3/CLAP 全绿，含 520x220、输入、gesture、close/recreate | `.phase1-run.gui.9d55cbf2/*.gui-test.log`（未跟踪） | hosted macOS job |
| Linux GUI | 未在本机运行；macOS cross 缺少 X11 sysroot | 无 Ubuntu hosted 证据 | Ubuntu hosted job |
| Windows GUI | 仅有 x86_64 MSVC cross-check，无原生 GUI runtime | 无 Windows hosted 证据 | Windows hosted job |
| hosted CI | #18 macOS+Windows 全绿；Linux GUI 步骤仍挂起，已加 per-test timeout 以定位 | [run 31766414064](https://github.com/aizcutei/sunmao/actions/runs/31766414064) | 等待新 run 的 ::error 注解 |

## 当前验证摘要

本机 macOS（Apple Silicon host）已通过：

- selected `cargo test --locked`：clap/vst3 sys+rs、core/macros、backends、packager、view_baseview、runner、gain/sine、baseview `--all-features`。
- release realtime allocator matrix：gain/sine × clap/vst3 sys+rs + unified backends。
- gain+sine × VST3+CLAP `sunmao_unittest_runner test`：各 16/16。
- GL/WGPU/WebView × VST3+CLAP native `gui-test --auto-close --verify-pixels --verify-input`：12/12，含 recreate。
- runner 失败路径：缺失插件、未知命令、无参数均为非零退出。

**不能**将以上结果写成 Phase 1 完成。完成条件要求同一 commit 上 macOS、Windows、Ubuntu hosted jobs 全绿且 artifacts 可下载。

## 完成规则

在同一 commit 上，三个 hosted native jobs 全绿、基础 fixture 的两种格式 process/state/automation 全绿、renderer GUI matrix 全绿且 artifact 可下载后，才把此文件的状态改为“Phase 1 完成”。任何本地、guest 或 container 结果都必须明确标注平台和证据等级。

当前状态：**hosted CI 未全绿，Phase 1 未完成**。
