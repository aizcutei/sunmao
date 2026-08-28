# Phase 1 状态

更新时间：2026-08-28

## 目标与边界

一期验收 VST3 + CLAP + standalone。每个 acceptance fixture 使用一份
Rust/SunMao 实现导出两种插件格式和一个设备无关的 standalone 入口，目标
平台为 macOS、Windows x86_64、Linux x86_64。AU 源码和显式 AU feature 保留，
但不进入一期构建、测试、打包或完成条件。

硬门槛：

- gain effect 与 sine instrument；0/mono/stereo 基础声道契约；audio/MIDI 处理。
- Float、Int、Bool 参数元数据、sample-offset automation、reset、参数 state round-trip、错误和密集事件边界。
- audio callback 成功路径无 alloc/realloc/dealloc。
- Cocoa、Win32、X11 上 GL/WGPU/WebView 的 attach、非空像素、输入、`520x220` resize、close/recreate。
- `sunmao_packager` 的 Mach-O/PE/ELF 与架构检查、VST3/CLAP 目标布局、staging/rollback。
- standalone runtime/facade 的设备无关 DSP/MIDI smoke；macOS `.app`、Windows `.exe`、Linux 可执行文件的 target-aware 打包和 executable 类型校验。
- `sunmao_unittest_runner` 的 scan/info/test/process/gui/gui-test，失败/超时非零退出。
- 六个 GL/WGPU/WebView standalone editor 的 raw 与 packaged `--gui-smoke` 生命周期通过。
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
- Hosted CI #18（`33b23dd`）：macOS + Windows 继续全绿；Linux GUI backends 步骤仍挂起（30 分钟+），说明 hang 不止在 drain。给 Linux GUI 调用加 3 分钟 `timeout` + 阶段化打点后：
- Hosted CI #19（`041741c`）：注解定位到卡点——`SunMao Gain WebView (VST3)` 的 recreate 路径，`Reopening GUI...` 后卡死。根因：GTK/WebKitGTK 有永久线程亲和性，baseview 每次 open_gui 都新建 event 线程，第二次在新线程上创建 WebView 触发 WebKit 同步 IPC 死锁（伴随 Gdk frame-clock CRITICAL）。修复：Linux 上引入进程级专用 GTK 线程，所有 wry WebView 的创建/操作/销毁经 channel 编组到该线程执行，公开 `WebView` 变为代理（带 10s 应答超时防新死锁）。
- Hosted CI #20（`c4465c2`）：GTK 线程修复生效——Gain WebView 在 Linux 上首次通过完整生命周期（含 close/recreate）；新的失败点是 `SunMao Sine Synth WebView (VST3)` 输入验证：XTEST 在 y=124 拖动未命中 WebKitGTK 布局下的 Volume slider（字体度量差异）。修复：pin 布局（显式 line-height + 24px slider box）使几何跨引擎一致，拖动坐标改为 y=138。
- Hosted CI #21（`885d2a5`）：**旧版 VST3/CLAP gate 三平台全绿**，https://github.com/aizcutei/sunmao/actions/runs/31771576307 。macOS（04:59–05:05）、Windows（04:59–05:07）、Linux（04:59–05:06）全部 success，同一 commit 上传 artifacts：`phase1-macOS-ARM64`（30MB）、`phase1-Windows-X64`（55MB）、`phase1-Linux-X64`（545MB），含 bundles、runner test/gui-test 日志与检查报告；该 run 没有验证当前修正后 standalone 范围。
- Hosted CI #23（`6c0bede`，含全部 hardening + standalone gate 扩展）：Windows/Linux 全绿（含 standalone），macOS 失败于 "Package and exercise VST3 + CLAP + standalone"——workflow 的 `packaged_standalone()` 用 display name 拼 `.app` 内可执行文件路径，而 packager 以 `--out` stem（`module_stem`）命名。已改为 `basename "$output_base"`。
- Hosted CI #24（`d660597`）：macOS/Linux 全绿；Windows 失败于 Gain WebView (VST3) 输入验证——WebView2 外部 UIA helper 固定 5s 超时在冷 runner 上不够（同一 fixture 在 #23 通过）。已把 helper 超时改为 `SUNMAO_UIA_HELPER_TIMEOUT_MS` 可配（默认 15s），workflow 固定 20s。
- Hosted CI #25（`c8401e6`）：**扩展后的 VST3 + CLAP + standalone gate 三平台全绿**，https://github.com/aizcutei/sunmao/actions/runs/33152642714 。macOS（07:45–07:53）、Windows（07:45–07:56）、Linux（07:45–07:56）全部 success，同一 commit 上传 artifacts：`phase1-macOS-ARM64`（50.7MB）、`phase1-Windows-X64`（76.7MB）、`phase1-Linux-X64`（954.4MB），含 VST3/CLAP bundles、standalone 应用、runner test/gui-test 日志与 packager 报告。
- `cargo metadata --locked --no-deps`、`cargo fmt --all -- --check`、`git diff --check` 已在本机通过。
- 默认 VST3/CLAP 二进制经 `nm` 确认无 `RustAUFactory|au_component_factory|SunmaoAUCocoa`。

## 矩阵状态

| 能力 | 当前判断 | 权威证据 | 下一步 |
|---|---|---|---|
| workspace metadata / fmt / diff | 三平台 hosted 通过 | run #21 三 job success | — |
| `_sys`/`_rs` ABI 与生命周期 | 三平台 hosted 通过 | run #21 "Test format adapters and host" | — |
| SunMao core/backend | Int/Bool normalized API、offset automation、固定容量、state 三平台 hosted 通过 | run #21 tests + runner 16/16 | — |
| 高层 facade API | `sunmao::prelude`、统一 VST3/CLAP 导出、一行 standalone 入口、GL/WGPU/WebView renderer feature 三平台 hosted 通过；六个 GUI acceptance fixture 只依赖 `sunmao`（AU opt-in 除外） | run #25 三 job success | — |
| packager | VST3/CLAP/standalone layout/format/arch/staging-rollback 三平台 hosted 通过 | run #25 packager tests + raw/packaged standalone artifacts | — |
| runner | scan/info/test/process/gui-test 三平台 hosted 通过；缺失插件与未知命令非零退出 | run #21 runner steps | — |
| macOS GUI | hosted GL/WGPU/WebView × VST3/CLAP 全绿，含 520x220、输入、gesture、close/recreate | run #21 macOS job + `phase1-macOS-ARM64` artifact | — |
| Linux GUI | hosted X11 GL/WGPU/WebView 全绿（xvfb + XTEST，WebView 经专用 GTK 线程） | run #21 Linux job + `phase1-Linux-X64` artifact | — |
| Windows GUI | hosted Win32 GL→WGPU fallback、WebView2 全绿（UIA 输入） | run #21 Windows job + `phase1-Windows-X64` artifact | — |
| standalone runtime/API | 设备无关 processor、panic/事件/参数边界、facade 宏、raw/packaged `--smoke` 三平台 hosted 通过 | run #25 "Test standalone runtime..." + packaging steps | — |
| standalone GUI | GL/WGPU/WebView 顶层窗口 raw/packaged `--gui-smoke` 三平台 hosted 通过 | run #25 GUI backends step + artifacts | — |
| hosted CI | 扩展后的 VST3 + CLAP + standalone workflow 三平台全绿 | [run 33152642714](https://github.com/aizcutei/sunmao/actions/runs/33152642714)（commit `c8401e6`） | — |
| 当前工作区 hardening | 全部 hardening 已提交并在三平台 hosted 重新验证 | run #25 三 job success | — |

## 当前验证摘要

本机 macOS（Apple Silicon host）已通过：

- 完整 `RUSTFLAGS=-Awarnings cargo test --locked`：workspace unit tests 与 doc-tests 全绿；关键套件包括 runner 44/44、VST3 wrapper 41/41、CLAP safe wrapper 38/38、两个统一 backend 各 16/16。
- 六个 GL/WGPU/WebView gain/sine acceptance fixture 均只依赖 `sunmao` facade，并分别在 macOS native 与 `x86_64-pc-windows-msvc` 条件编译通过；`sunmao --all-features` 通过。
- `tools/package_examples.sh --debug --test`：10 个示例的 VST3/CLAP runner 各 16/16，8 个 raw 与 8 个 packaged standalone smoke 全部通过。
- `cargo metadata`、`cargo fmt`、`git diff --check`、workflow YAML 与 packaging shell 语法检查通过。
- selected `cargo test --locked`：clap/vst3 sys+rs、core/macros、backends、packager、view_baseview、runner、gain/sine、baseview `--all-features`。
- release realtime allocator matrix：gain/sine × clap/vst3 sys+rs + unified backends。
- gain+sine × VST3+CLAP `sunmao_unittest_runner test`：各 16/16。
- 历史 run #21 的 GL/WGPU/WebView × VST3+CLAP native `gui-test --auto-close --verify-pixels --verify-input`：12/12，含 recreate；该结果对应旧版 commit `885d2a5`，不是当前未提交工作区的重新验证。
- runner 失败路径：缺失插件、未知命令、无参数均为非零退出。

Hosted 权威证据（run #21，commit `885d2a5`）：

- 三个 hosted native jobs（macOS ARM64、Windows x86_64、Ubuntu x86_64）同一 commit 全绿。
- gain + sine × VST3 + CLAP 的 process/state/automation/runner 测试在三平台全部通过。
- GL/WGPU/WebView renderer GUI matrix 在 Cocoa/Win32/X11 全部通过：非空像素、输入、host gesture、520x220 resize、close/recreate。
- 三平台 artifacts（bundles、runner test/gui-test 日志、packager 报告）已由 run #21 上传且可下载（需仓库登录权限）。

## 完成规则

在同一 commit 上，三个 hosted native jobs 全绿、基础 fixture 的两种格式
process/state/automation 全绿、standalone raw/packaged smoke 全绿、renderer
和顶层 standalone GUI matrix 全绿且 artifact 可下载后，才把此文件的状态改
为“Phase 1 完成”。任何本地、guest 或 container 结果都必须明确标注平台和
证据等级。

## 当前状态

- **Phase 1 完成。** Hosted run #25（commit `c8401e6`，https://github.com/aizcutei/sunmao/actions/runs/33152642714 ）在同一 commit 上满足扩展后的全部完成条件：三个 hosted native jobs（macOS ARM64、Windows x86_64、Ubuntu x86_64）全绿；gain/sine × VST3/CLAP process/state/automation 与 runner 测试通过；raw/packaged standalone smoke 通过；GL/WGPU/WebView 嵌入式与顶层 standalone GUI matrix（非空像素、输入、host gesture、520x220 resize、close/recreate）通过；`phase1-macOS-ARM64`（50.7MB）、`phase1-Windows-X64`（76.7MB）、`phase1-Linux-X64`（954.4MB）artifacts 可下载。
- **历史 baseline：** 旧版 VST3/CLAP gate 由 run #21（commit `885d2a5`）验收，证据仍然有效。
- 后续任何 ABI、生命周期、GUI、standalone、packager 或 runner 变更都必须按 roadmap 的 revalidation gate 在新 commit 上重新取得三平台 hosted 证据；Phase 2 实现目标自此可以创建。
