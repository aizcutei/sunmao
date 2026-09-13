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
### 2026-09-13 — M0 三平台验收完成

- Command/platform: [run 34761153409](https://github.com/aizcutei/sunmao/actions/runs/34761153409) / `9cce371`；
  三平台 job 全部 success，每平台 **34 步零非成功**（跳过项 Linux 5 / macOS 9 / Windows 11，均为平台不适用者）。
  推送走 SSH，直接成功。
- Result: M0 标记完成。三份 artifacts 均在 API 列表中标为可下载
  （Linux 1,000,635,181 / Windows 78,735,698 / macOS 54,190,684 bytes）；macOS 一份已实际下载，
  `unzip -t` 报 `No errors detected in compressed data`，SHA-256 `a717122b…e408c8`。
- Evidence/artifact: 同上 run 的 jobs API（逐 step 的 conclusion 全部 success/skipped）与下载的
  `phase1-macOS-ARM64` zip。
- Unresolved: **这个 commit 是纯文档提交，CI 能为它提供的证据仅限"既有 34 步仍 blocking 且绿"**——
  没有新断言可 grep，也没有新守卫可反向验证。这一条在 M0 提交时就写明过，验收后依然成立，
  不要把它读成"新能力已验证"。进入 M1。

### 2026-09-13 — M1 交互式 standalone host

- Command/platform: 本地 macOS ARM64，分支 `phase5/test-host-compat`。
  `cargo metadata --locked` exit 0、`cargo fmt --all -- --check` exit 0、`git diff --check` exit 0、
  `RUSTFLAGS=-Awarnings cargo test --locked` exit 0（/tmp/p5m1-test.log）。
  另把新 CI 步骤的脚本体原样抽出来，在本机以真实打包产物（`build_new/SunMao Gain.{vst3,clap}`、
  `SunMao Widgets GL.{vst3,clap}`）跑通一遍，exit 0。
- Result: 新增 `host` 子命令：行式命令语言，**同一套面孔既给人用也给管道用**——
  `info`/`params`/`buses`/`get`/`set`/`expect`/`process`/`reset`/`state save|load`/
  `preset save|load`/`editor open|close`/`echo`/`quit`，任一命令失败即计数，会话结束打印
  `HOST SESSION VERIFIED` 或 `HOST SESSION FAILED` 并据此决定退出码，所以一段脚本可以直接当 blocking 步骤用。
  参数寻址支持 `#index`，因为真实参数 ID 是 hash（增益旋钮是 `458499838`），只认 ID 的宿主没人能手工驱动。
  `preset.rs` 按上游转录实现 `.vstpreset` 容器（`vstpresetfile.h/.cpp` 的 header/chunk list 布局与
  `funknown.cpp` 的 `FUID::toString` 两条分支），并逐条拒绝截断、越界、缺 `Comp`、未来版本与非 preset 文件。
  **本轮抓到四件事，三件是缺陷、一件是跨平台隐患：**
  **(1)** runner 的 CLAP 宿主对**不存在的参数 ID 报成功**——`clap_plugin_params.flush` 返回 `void`，
  事件被静默丢弃，而规范把"只发 `get_info` 给过的 ID"的责任交给宿主。一个对不存在的参数报成功的测试宿主，
  看不见插件把参数弄丢。已改为先用插件自己的 `get_value` 问一次；VST3 侧本来就靠
  `setParamNormalized` 的返回值如实报错，两格式就此一致。
  **(2)** 同一次写入经两格式读回**值不同**：写 0.9 读回，VST3 差 0，CLAP 差 2.38e-8。根因是
  `vst3_rs::ParameterBridge` 存 `AtomicU64`（f64）副本，而 `sunmao_core` 的参数真身是 `AtomicU32`（f32）——
  **即 VST3 侧的回读并不能证明插件内部的值**，此前没写下来。这直接决定了 `expect` 必须**显式定义容差**
  （`DEFAULT_EXPECT_TOLERANCE = 1e-6`，位于 f32 误差之上）而不是比相等。统一精度属独立立项，本轮不动
  已三平台验收的 VST3 路径。
  **(3)** 会话提示符原本无条件写 stderr，CI 步骤合并两流后会在**每一行断言前面**加上 `> `，
  本地跑步骤时 `grep -q "^parameters 3"` 当场失败。改为仅在 stdin 是终端时提示。
  这条是自己先踩到的——如果先推 CI，它就是一次"新步骤红"而不是一次本地修复。
  **(4)** `.vstpreset` 存的是 class ID 的**字符串**，而上游 `toString` 在 `COM_COMPATIBLE`（仅 Windows）
  下把前 8 字节当 `GuidStruct` 重排。上游 `INLINE_UID` 存不同字节正是为了让字符串跨平台一致，
  `vst3_sys::make_tuid` 已照此实现；但**插件自己的 class ID 走的不是这条路**：`class_id_from_str`
  产出与平台无关的固定 FNV 字节，于是同一个 SunMao 插件在 Windows 与 mac/Linux 上得到**两个不同的 class 串**。
  本轮不改 class ID 生成方式（那会改变所有既有插件的身份），但把这条隐患**钉成断言**
  （`the_same_bytes_yield_different_class_strings_on_windows_and_elsewhere`），
  并让宿主读写都用本平台的 `toString`：同平台往返正确，跨平台不匹配被 class 串比较**明确拒绝**而非静默载错。
  既有六个子命令**行为未变**：四处逐字相同的扫描分派抽成 `scan_plugin_path`（分支与消息逐字保留），
  `load_plugin` 改 `pub(crate)`，没有别的改动。
  测试：**44 → 70**（macOS 实跑，+26：`preset` 16、`interactive` 10）。全仓 676 → **702 passed / 0 failed**，
  逐套件比对确认**只有 runner 一套变化**、其余与 M0 基线逐位相同。
- Evidence/artifact: 新 blocking 步骤 "Drive the interactive host over VST3 + CLAP"，两格式各跑一段
  18 命令会话并断言 `HOST SESSION VERIFIED`、`^buses 2`、`^parameters 3`、三次 `== 0.25 (within`，
  再比对 `.vstpreset` 以 `VST3` 开头且与裸 state **不同**、CLAP preset 与裸 state **逐字节相同**。
  **守卫的反向验证做在步骤里，而不是只在本地做过一次**：10 个反向用例
  （两格式各 4 个：未知 ID、越界 `#99`、错误期望、拼错命令；外加 VST3 preset 喂给 CLAP、截断 preset）
  每个都**必须非零退出**，否则步骤自己 exit 1 并打印 "the guard cannot fail"。
  编辑器生命周期在 `SunMao Widgets GL` 上开关各两次，Linux 沿用既有的 `xvfb-run` + **timeout 180** 约定。
- Unresolved: **本地全过不等于验收**——本条须取得同 commit 三平台 hosted 全绿，且下载原始日志
  grep 到 `HOST COMMAND SURFACE VERIFIED` 与 10 条 `rejected as it must be`，才算 M1 完成。
  两件独立立项：VST3/CLAP 参数回读精度不一致；`class_id_from_str` 的跨平台 class 串不一致。
  Phase 4 继承的四条遗留未动。
