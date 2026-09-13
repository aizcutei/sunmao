# Phase 5 进展日志

按时间追加，格式固定（沿用 Phase 4 的四项格式）：

```text
### YYYY-MM-DD — <milestone>
- Command/platform:
- Result:
- Evidence/artifact:
- Unresolved:
```

### 2026-09-13 — M0 脚手架与基线

- Command/platform: 本地 macOS ARM64，新分支 `phase5/test-host-compat`，从
  `phase4/gui-component-library` 尖端 `d9bed39` 切出（`main` 仍落后于已验收的 Phase 3/4
  工作，合并需仓库所有者决定，本 phase 不自行推送 main）。
  `cargo metadata --locked` exit 0、`cargo fmt --all -- --check` exit 0、
  `git diff --check` exit 0、`RUSTFLAGS=-Awarnings cargo test --locked` exit 0
  （/tmp/sunmao-p5-test.log）、`tools/package_examples.sh --debug --test` exit 0
  （/tmp/sunmao-p5-pkg.log）。
- Result: 建 `docs/phase5/{status,progress}.md`（沿用 Phase 4 的矩阵与四项日志格式）。
  **两条基线都是本轮实测，不是抄 Phase 4 的**：`cargo test` **135 套件 / 676 passed /
  0 failed / 4 ignored**；打包 **32 套件 / 640 passed / 0 failed**（`Summary:` 行逐行累加）。
  两者都与 Phase 4 收尾数字一致，说明切分支未引入漂移；后续纯重构提交应与之逐位相同。
  清点 `sunmao_unittest_runner`：8 个源文件 / 13588 行 / 47 个 `#[test]`；七个子命令
  （`scan`/`info`/`test`/`process`/`gui`/`gui-test` 与两个平台内部 helper）；`HostPlugin`
  trait 已有 23 个方法，含 `param_*`、`process_with_events`、`audio_buses`、
  `save_state`/`load_state`、`open_gui`/`resize_gui`/`set_gui_scale`/`send_gui_key`。
  **结论是 M1 需要的底层能力绝大多数已经在 trait 里，缺的是可交互的命令面而非重写宿主。**
  十条缺口逐条附证据落在 status.md，其中三条是 grep 直接可验的硬事实：runner 源码
  `grep -i preset` **零命中**（VST3 `.vstpreset` 与 CLAP preset-discovery 宿主侧路径都不存在，
  而 `clap_sys` 已有 `ext/preset_load.rs` 与 `factory/preset_discovery.rs` 转录、`vst3_sys`
  则连 preset 相关绑定都没有）；`grep fuzz .github/workflows/*.yml` **零命中**（`fuzz/` 被根
  `Cargo.toml` `exclude`，README 明写 local-only）；`println_plugin_info` 不调用
  `audio_buses()`，故 bus 拓扑只在 `test` 内部消费、没有面向人的枚举输出。
- Evidence/artifact: 本地日志 /tmp/sunmao-p5-test.log、/tmp/sunmao-p5-pkg.log、
  /tmp/sunmao-p5-gate.status。**本地结果只作开发证据**——M0 仍须同 commit 三平台
  hosted 全绿 + artifacts 可下载才标记完成。
- Unresolved: 本轮是纯文档提交，**没有新增任何断言**，因此 CI 对它能提供的证据只有
  "Phase 1–4 既有 34 个步骤在这个 commit 上仍然 blocking 且绿"，不存在"新守卫是否真的
  执行/会变红"可查——这一点如实写在这里，免得下一轮把它读成已验证了新能力。
  进入 M1：交互式 standalone host。
