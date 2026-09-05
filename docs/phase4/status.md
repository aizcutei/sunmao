# Phase 4 状态

更新时间：2026-09-05

## 目标与边界

在 Phase 1（run #25 / commit `c8401e6`）、Phase 2 核心（run #38 / commit `77f788c`）、
Phase 3（run #69 / commit `b45efea`）之上，完成 GUI 组件库与平台完善。目标能力：

- renderer 资源与线程归属（GL / WGPU / WebView 三后端的设备、表面、上下文与销毁顺序），
  scale/DPI 协商（VST3 `IPlugViewContentScaleSupport` ↔ CLAP `gui.set_scale`）
- 声明式布局与主题：`Column`/`Row`/gap/padding，`Label`/`Knob`/`Slider`/`Toggle`/`Dropdown`，
  参数双向绑定（`Knob::param`）零手写回调
- text rendering 与输入：字体栅格化与文本度量、clipboard、IME/国际键盘、cursor/focus
- 可视化与 accessibility：`VizChannel` 无锁 audio→GUI 通道、`SpectrumAnalyzer`/meter 组件、
  accessibility 树、floating CLAP editor
- X11 生命周期稳定后加入 Wayland

明确不做：AU GUI 契约扩展（AU 不进默认 feature/gate，留待 Phase 7）、完整 DAW 宿主
（Phase 5）、发布签名与安装器（Phase 6）。

## 硬门槛（每个 milestone 与最终验收通用）

- 同一 commit 三平台 hosted native jobs（macOS ARM64、Windows x86_64、Ubuntu x86_64）
  全绿并上传 artifacts；本地结果只作开发证据。
- Phase 1 + Phase 2 + Phase 3 全部既有 CI 步骤保持 blocking 且绿色。
- 每个 host-facing 能力：VST3 与 CLAP 同时落地；语义差异与降级写入
  `docs/phase2/semantics.md`（附测试名）；公共 API 进 `sunmao/gui` 或新 crate
  并入 prelude（带 doc-test）。
- audio callback 成功路径零 alloc/realloc/dealloc；GUI 线程与 audio 线程只经无锁通道
  通信；新增不变量补 proptest。

## 分支基点（与 loop prompt 的偏差，需知悉）

loop prompt 写的是"不存在则从 main 切出"。**实际从 `phase3/framework-dsp-library`
的尖端 `e844215` 切出**，原因是 `main` 仍停在 Phase 2 的 `2df01ce`，Phase 3 的 33 个
commit 从未合并进 main。已用 `git merge-base --is-ancestor main HEAD` 确认新分支是
main 的严格超集，因此"从 main 切出"在祖先关系上仍然成立；若真按字面从 main 切，会丢掉
`sunmao/dsp`（M4 明确要消费它的 metering）、两个模板与已 CI 验证的清理。
**Phase 3 → main 的合并需要仓库所有者决定，本轮未自行推送 main。**

## Acceptance fixtures

| fixture | crate | 验收能力 | M0 骨架状态 |
|---|---|---|---|
| Widgets GUI | `examples/sunmao_fx_widgets_gui_gl` | M2 控件与布局、M4 可视化与无锁发布 | M0 骨架：旋钮用框架现有 `Knob`；**下拉、开关、频谱是 crate 内 skeleton**（`DropdownSkeleton`/`ToggleSkeleton`/`SpectrumSkeleton` + `SpectrumPublisher`），9 单元测试含零分配断言（macOS 本地通过） |

验收方式与 Phase 3 相同：M2 把 skeleton 换成框架 `Toggle`/`Dropdown` 与声明式布局、
M4 把 `SpectrumPublisher` 换成 `VizChannel` + `SpectrumAnalyzer`，**而这些测试语义不变**。

**该 fixture 暂未进打包矩阵，这是有意推迟而非遗漏。** M0 阶段它的下拉/开关/频谱都还是
skeleton，宿主侧可见行为不超出既有的 `sunmao_fx_gain_gui_gl`，进矩阵只增加 CI 时长与
Linux GUI 面的风险。计划在 **M2 真控件落地时接入**——Phase 3 的教训（instrument 模板
一进打包矩阵，runner 立刻在真实宿主抓出两个单测看不到的缺陷）说明这一步不能省，只是
应该在有真实宿主可见行为时做。

## Milestone 矩阵

| Milestone | 范围 | 当前判断 | 权威证据 | 下一步 |
|---|---|---|---|---|
| M0 脚手架与清理收口 | 文档、GUI fixture、workspace/CI 骨架；ABI 去重取得三平台绿 | **完成**（三平台 hosted 全绿）：清理收口由 run #71/#72 完成；GUI fixture 与新 blocking 步骤 "Test Phase 4 acceptance fixtures" 由 run #73 验收，每 job **26 步零非成功**。已下载 Linux job 日志核实该步骤**非空转**：`running 9 tests ... 9 passed; 0 failed`，其中零分配断言在 glibc 分配器下同样通过 | 脚手架：[run #73](https://github.com/aizcutei/sunmao/actions/runs/33959635350)（commit `d37a46f`）三 job success，artifacts `phase1-macOS-ARM64`（49.9MB）、`phase1-Windows-X64`（74.5MB）、`phase1-Linux-X64`（901.4MB）可下载。清理：[run #71](https://github.com/aizcutei/sunmao/actions/runs/33956858763)（commit `7dabb3d`，25 步零非成功）与 run #72（commit `e844215`，同样零非成功） | — （M0 完成；进入 M1） |
| M1 renderer 资源与线程归属 | 三后端归属与销毁顺序文档、scale/DPI 协商两格式落地 | **完成**（三平台 hosted 全绿）：`docs/phase4/ownership.md` 落地；scale/DPI 协商自 `_sys` 补起（`vst3_sys` 此前**完全没有** `IPlugViewContentScaleSupport` 绑定，IID/typedef 自上游头文件转录）→ `vst3_rs` 第二 vtable + 布局断言 → core `ViewHandle::scalable`（并删除自 Phase 1 起就无调用方、且签名 `&self` 无法改动编辑器的死钩子 `SunmaoView::set_scale_factor`）→ 两 backend 路由到**活着的** view_handle（`backend_clap` 此前从未 override `gui_set_scale`，一直告诉 CLAP 宿主“不支持缩放”）→ baseview 按 `创建尺寸 × factor` 响应 → runner `gui-test` 两格式断言 | [run #75](https://github.com/aizcutei/sunmao/actions/runs/33963105660)（commit `0b51dcb`）三 job success，每 job **26 步零非成功**，artifacts 3 份可下载。已下载三平台 job 日志核实断言**真实执行**：每平台 `GUI scale negotiated: host applied 2.0` 各 **16 次**（8 个 GUI 插件 × 2 格式），零因子拒绝亦 16 次，且**拒绝方式按格式精确分裂 8/8**（CLAP 回 `false`、VST3 回 `kInvalidArgument`），三平台完全一致 | — （M1 完成；进入 M2） |
| M2 布局与主题 | `Column`/`Row`/gap/padding、五个控件、参数双向绑定、主题 token | **完成**（三平台 hosted 全绿）：`Theme` 角色化 token（暗/亮，对比度由测试机械断言）、新增 `Toggle`/`Dropdown`（连同既有 `Knob`/`Slider`/`Label`/`Button` 共 6 控件）、声明式 `Column`/`Row`（含 6 个布局 proptest）、`ParamBinder`+`ParamHost` 两向绑定（facade 以 `ViewContextHost` 适配 `ViewContext`），fixture 换真控件后**已无逐控件回调代码**并接入打包矩阵 | [run #77](https://github.com/aizcutei/sunmao/actions/runs/33965946234)（commit `aec872f`）三 job success，每 job **26 步零非成功**，artifacts 3 份。已下载三平台日志核实新 fixture 真被宿主执行：每平台 `SunMao Widgets GL` 出现 10 次、两格式各自 `Testing:` 一次，全 run **零失败套件**。本地打包 30→**32 套件**、600→**640 断言** | — （M2 完成；进入 M3） |
| M3 text rendering 与输入 | 字体栅格化/度量、clipboard、IME/国际键盘、cursor/focus | 未开始 | — | — |
| M4 可视化与 accessibility | `VizChannel`、`SpectrumAnalyzer`/meter、accessibility 树、floating CLAP editor | 未开始 | — | — |
| M5 Wayland 与总验收 | Wayland、GUI 侧兼容策略、proptest/文档收尾 | 未开始 | — | — |

## 完成规则

Phase 4 完成的唯一判定：同一 commit 三平台 hosted native jobs 全绿 + artifacts 可下载
+ 本文件 Milestone 矩阵 M0–M5 全部标记完成。本地结果任何情况下都不构成完成证据。

## 已知遗留

- **vst3_rs 控制器包装仍有冗余**：`ControllerWrapper` 与 `GuiControllerWrapper` 有 25 个
  函数 / 260 行在归一化类型名后 ≥0.995 相同。可仿 `clap_rs` 的 `audio_ports_config_ext!`
  用 `$bound`/`$type` 宏收敛，但该处用**字段偏移算术**还原 `this`，宏化必须保持字段顺序
  与 `repr(C)` 布局不变并补布局断言测试，故须单独 commit 并单独取得三平台绿。
- **Windows WGPU 收尾段错误**（exit 139，run #37 一次）自 run #66 起在 #66/#68/#69/#71/#72
  连续五次未复现。**仍不改判为"已修复"**——连续绿不构成对间歇性失败的证明。若在 M1 复现，
  按计划深入 WGPU/D3D 析构路径，不盲目重试。
- `main` 落后于已验收的 Phase 3 工作 33 个 commit（见上文"分支基点"）。
