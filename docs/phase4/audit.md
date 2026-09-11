# Phase 4 最终审计

2026-09-10，基准提交 d533e543fd9db72f3d9165aad491f0fb6b465074。
本文件记录逐项核对结果；未核对的要求不视为完成。三平台 run 34446209752 全部成功，三份 artifacts 已下载并通过 SHA-256/ZIP CRC，但不能据此覆盖未实现的要求。

| 原始要求 | 当前证据 | 判断与后续工作 |
|---|---|---|
| macOS/Windows/X11 各一条真实国际输入路径 | baseview/tests/{macos_keyboard,windows_keyboard,x11_keyboard}.rs；各平台 blocking CI；Windows 完整原始日志第 2878 行成功标记；三个平台各自完整 run 与产物校验见 status/progress | 已验收国际布局与 compose；不包含完整 CJK IME |
| floating CLAP editor 的 set_transient | clap_rs/src/ext/gui.rs 默认 gui_set_transient 返回 false；SunmaoClapWrapper 的 GuiHandler 实现未 override；baseview 未提供 transient 所属窗口接线；本轮先修正 clap_rs 入口保留 owner 的 api，新增兼容默认回调 | 未完成。需先核对上游 gui.h，再贯通格式层、统一 view 契约和原生窗口，覆盖 show 前设置与平台差异 |
| SpectrumAnalyzer/meter 消费 Phase 3 metering | sunmao/gui/src/widgets/spectrum.rs 只有 SpectrumSource/StaticSpectrum/SpectrumAnalyzer；widgets fixture 已新增 MeterSource，消费输出第一声道的 MeterHandle，显示 peak/RMS（-60..0 dBFS）；prelude/doc-test、数值属性、真实 process 到显示的零分配/reset 测试本地通过 | 已由 `89b0d18` / [run 34451013385](https://github.com/aizcutei/sunmao/actions/runs/34451013385) 三平台完整 jobs 成功、实际属性/doc-test/零分配端到端日志与三份产物 SHA-256/ZIP CRC 验收；测量输出第一声道，peak/RMS 独立原子读取，不承诺一致快照 |
| floating CLAP editor 的 suggest_title | backend 仅保存 suggested_title，gui_show/open_floating 未消费；既有测试只证明字符串保存 | 未完成。需实际传入原生窗口标题并在原生验收读取标题；与 transient 创建参数一起补齐 |
| GUI semver/state 策略 | docs/phase3/compatibility.md 的 2bis 节存在 | 待逐条对实现/迁移测试核对，不能仅凭文档存在关闭 |
| 文档与矩阵一致 | status fixture 段仍写 skeleton/未打包，和 M2/M4 实现冲突；semantics accessibility 仍写平台适配未做 | 待修正文档并核实当前平台接线证据 |
| 其余 M0–M5 要求及跨层不变量 | 既有 status/progress 中有历史证据 | 待最终逐项核查 API/prelude/doc-tests、布局/主题、文本/剪贴板/焦点、ownership/DPI、Wayland、无锁/零分配与全部既有 gates |

下一瓶颈：M4 floating CLAP editor 的 transient 契约；MeterSource 已独立三平台验收。
