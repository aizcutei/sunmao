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
### 2026-09-13 — M1 三平台验收完成

- Command/platform: [run 34763956140](https://github.com/aizcutei/sunmao/actions/runs/34763956140) / `ac41dff`；
  三平台 job 全部 success，每平台 **35 步零非成功**，新步骤 "Drive the interactive host over VST3 + CLAP"
  三平台各 success。三份 job 原始日志已下载（macOS 787,289 / Windows 770,621 / Linux 1,890,744 bytes）。
- Result: M1 标记完成。**计数时先把 GitHub 回显的脚本正文剔除**——它带 ANSI `36;1m` 前缀，
  不剔除的话 `grep -c` 会把"脚本里写着这句话"算成"这句话被打印过"，正是本仓踩过两次的那类错觉。
  剔除后三平台数字完全一致：`HOST COMMAND SURFACE VERIFIED` 各 **1** 次，
  `rejected as it must be` 各 **10** 次，`HOST SESSION VERIFIED` 各 **4** 次
  （两格式各一段 18 命令会话 + 两格式各一段 5 命令编辑器会话），`editor opened`/`editor closed` 各 **4** 次。
  **十个反向用例是在 CI 里真的失败给看的**，不是本地跑过一次就算数：每个都必须非零退出，
  否则步骤自己 exit 1 并打印 "the guard cannot fail"。
- Evidence/artifact: 三份 artifacts 均可下载（Linux 1,000,635,643 / Windows 78,737,182 /
  macOS 54,191,174 bytes）；macOS 一份已实际下载，`unzip -t` 报 `No errors detected`，
  SHA-256 `2c01b112…1fa10`。**两项新发现在三平台硬件上各自取得直接证据**：
  (1) 同一个 `SunMao Gain` 的 VST3 class 串在 macOS/Linux 是 `53756E4D616F46784761696E21212121`
  （即 ASCII `SunMaoFxGain!!!!`）而在 Windows 是 `4D6E75536F6178464761696E21212121`——
  这正是上游 `COM_COMPATIBLE` 分支把前 8 字节当 `GuidStruct` 重排的结果，**预测与实测逐字符吻合**，
  也反过来证明 `preset::fuid_to_string` 的两条分支都照着上游写对了；
  (2) 写 0.9 读回的差值三平台**逐位相同**（VST3 差 0、CLAP 差 0.00000002384185793236071），
  是确定性的 f64/f32 差异而非噪声。两项都写进 `status.md` 的"M1 抓到的两项新发现"，各自独立立项。
- Unresolved: **`host-session` 目录没有进成功 artifact**（upload 步骤是显式路径清单），
  所以 preset 文件与会话日志目前只能从 job 日志看，不能下载下来直接检查——下一个功能提交一并补上，
  因为那正是 class 串问题的可检查证据。两项新发现未修：VST3 class 串跨平台不一致（改动会变更所有
  既有插件身份，须想清楚 Windows 既有安装的迁移）、VST3/CLAP 参数回读精度不一致。
  Phase 4 继承的四条遗留未动。下一步 **M2：批量 regression host**。
### 2026-09-13 — M2 批量 regression host

- Command/platform: 本地 macOS ARM64，分支 `phase5/test-host-compat`。
  `cargo metadata --locked`、`cargo fmt --all -- --check`、`git diff --check`、
  `RUSTFLAGS=-Awarnings cargo test --locked` 全部 exit 0（/tmp/p5m2-test.log）。
  两个新 CI 步骤的脚本体照旧原样抽出来在本机跑通，各 exit 0。
- Result: 新增 `regress` 子命令与 `regress.rs`。**三件事刻意钉死**：种子、最大块长、
  以及由种子导出的**不均匀块划分**（首块钉死 max、末块钉死 1——把极端留给随机意味着某些种子永远测不到）。
  轨迹是文本：每块 peak/RMS + 4 个定点采样，加自动化日程与结束时的全参数回读。
  goldens 入库 `tools/regression_goldens/`，**比对一律带显式容差**，且容差写在 trace 自己的头里，
  文件因此自带它被检查的规则；比对**即使全过也打印见到的最大偏差**，否则容差没人能有依据地设。
  **自己的两个单测抓出自己两个真 bug：**
  **(1)** `Rng::new` 原本用 `seed | 1` 躲开 xorshift 的零不动点，这会**丢掉最低位**——
  相邻种子产出逐字节相同的运行。一个"两个不同种子其实是同一次运行"的回归床，比只有一个种子还糟。
  改用 SplitMix64 finalizer，保留全部 64 位。已把这条钉成 `neighbouring_seeds_produce_different_runs`。
  **(2)** trace 原本用 `{:.17}` 写浮点，我在文档里写的是"17 位有效数字可无损往返 f64"——
  **`{:.17}` 是 17 位小数，不是 17 位有效数字**：它把 `f32::MIN_POSITIVE` 渲染成 `0.00000000000000000`。
  那样的 golden 会把每个小采样记成 0，然后几乎匹配任何东西。改为 `{:.17e}`，并留下测试
  `the_text_form_preserves_every_bit_of_an_awkward_double` 逐位核对。
  **另修一处本来就坏的东西**：`fuzz/Cargo.lock` 是陈旧的，`--locked` 直接拒绝它——
  fuzz 从没进过 CI，所以没人撞到。要把它接成 blocking 步骤就必须先让它的 lock 诚实（少了 4 个已不再需要的传递依赖）。
  测试：runner **70 → 88**（+18），全仓 702 → **720 passed / 0 failed**。
- Evidence/artifact: 两个新 blocking 步骤。
  "Compare deterministic regression runs against goldens"：两格式各 147 项比对全中、
  worst deviation **0e0**；再 `diff` 两份新产出的 trace 的 `block` 行，断言**跨格式音频逐字节相同**；
  两个反向用例（扰动某块 peak、把 trace 版本号改成 2）必须非零退出，否则步骤自己 exit 1。
  "Fuzz the state decoders (bounded)"：`fuzz/` 被排除出 workspace、驱动默认跑到天荒地老，
  两者都是有意的，也都意味着它不能原样进 gate——**固定迭代数 200000 与固定种子**，
  并**断言那个具体的数字**而不只是 "no crash"；再加一个 16 次的短跑，要求它**不**满足 20 万次的断言，
  免得这条断言退化成"任何成功的运行都算数"。`host-session`/`regression`/fuzz 日志已加进成功 artifact。
  **M2 首次真跑就抓到一个框架缺陷**：VST3 对 stepped 参数的回读返回未量化的值（详见 status.md 第 3 条）。
  goldens 刻意记录当前行为，好让下一轮的修复以 golden diff 的形式自证。
- Unresolved: 须取得同 commit 三平台 hosted 全绿，并 grep 到 `REGRESSION GOLDENS VERIFIED`
  与 `STATE DECODE FUZZ VERIFIED`，M2 才算完成。**goldens 是在 macOS ARM64 上生成的**，
  另外两平台能否在 1e-6 内对上，要等 CI 给答案——比对会打印 worst deviation，
  所以万一对不上，一轮就能拿到该设多少的依据。三项独立立项未修：VST3 class 串跨平台不一致、
  VST3/CLAP 参数回读精度、VST3 离散参数回读未量化。Phase 4 继承的四条遗留未动。
### 2026-09-13 — M2 三平台验收完成

- Command/platform: [run 34766022434](https://github.com/aizcutei/sunmao/actions/runs/34766022434) / `c059e41`；
  三平台 job 全部 success，每平台 **37 步零非成功**。三份 job 原始日志已下载并剔除脚本回显后逐条核实。
  （M1 的验收记录提交 `5f48ac2` 亦已由 run 34764944510 三平台 success，分支保持全绿。）
- Result: M2 标记完成。三平台数字完全一致：`REGRESSION MATCHED GOLDEN` 各 2 次（两格式各 147 项比对）、
  `cross-format audio identical across all block records`、`REGRESSION GOLDENS VERIFIED`、
  `STATE DECODE FUZZ VERIFIED: 200000 cases from seed 20816` 各 1 次，
  两个反向用例（扰动 golden、未来版本 trace）各被拒 1 次。
  **本轮最值得记的一个数字：goldens 是在 macOS ARM64 上生成的，而 Linux x86_64 与 Windows x86_64
  的 worst deviation 都是 `0e0`。** 不是"落在 1e-6 容差内"，是**逐位相同**——跨三平台、跨两种架构。
  **但不要把这条读成"SunMao 的 DSP 一律跨平台逐位一致"**：本 fixture 是纯乘法增益，
  没有超越函数、没有可能被不同编译器合并成 FMA 的表达式。显式容差仍然是正确的契约，
  等 DSP 更重的 fixture 进来时它才会真正派上用场；今天只是它恰好没被用到。
- Evidence/artifact: 三份 artifacts 可下载（Linux 1,000,654,355 / Windows 78,753,789 /
  macOS 54,209,946 bytes）；Windows 一份已实际下载，`unzip -t` 通过，SHA-256 `f36237e604a24860…`。
  新加的 `host-session`/`regression`/fuzz 日志确已在包内（20 个文件，含真实的 `.vstpreset` 与 `.state`）。
  **因此 M1 的 class 串发现从"日志证据"升级成了"产物证据"**：从 Windows 包里取出真实的
  `gain.vst3.preset`，按上游布局解出 `class : 4D6E75536F6178464761696E21212121`
  （十六进制还原成字节是 `MnuSoaxFGain!!!!`），而 macOS/Linux 的同名文件里是
  `53756E4D616F46784761696E21212121`（`SunMaoFxGain!!!!`）——**两个平台产出的 `.vstpreset`
  带着不同的 class ID，打开文件就能看到**。顺带核对容器布局本身：list 偏移 `48 + 52 = 100`、
  总长 `100 + 4 + 4 + 20 = 128`，与 `vstpresetfile.cpp` 的写入顺序逐字段吻合。
- Unresolved: 三项独立立项仍未修，优先级已排好：**(1) VST3 对 stepped 参数回读未量化**——
  M2 首跑抓到的，无迁移代价、改法清楚，下一轮就修，goldens 的 diff 将是修复的证据；
  (2) VST3/CLAP 参数回读精度（同一根因）；(3) VST3 class 串跨平台不一致——**改动会变更所有既有插件的身份、
  使已发布的工程与 preset 失效，与 `main` 合并同属须由仓库所有者拍板的事，本 phase 不自行决定**。
  Phase 4 继承的四条遗留未动。下一步 **M3：性能与泄漏检测**。
### 2026-09-14 — 修复：VST3 对离散参数的回读返回未量化的值（M2 抓到的缺陷）

- Command/platform: 本地 macOS ARM64。`cargo metadata --locked`、`cargo fmt --all -- --check`、
  `git diff --check`、`RUSTFLAGS=-Awarnings cargo test --locked` 全部 exit 0（/tmp/p5m3-test.log，
  **722 passed / 0 failed**）；`tools/package_examples.sh --debug --test` exit 0（32 套件 / 640 断言，与基线一致）。
  新 CI 步骤脚本体本机跑通。
- Result: **先写失败的测试，再改代码。** `vst3_rs` 加一个会量化的测试替身
  （`set_param` 存 `value >= 0.5`，正是 `sunmao_core::BoolParam` 的行为），断言
  `set_param_normalized(0.8)` 之后 `get_param_normalized` 必须是 1.0——**改之前它如期失败**
  （`left: 0.8, right: 1.0`）。
  **自底向上改了两层**：`vst3_rs` 的 wrapper 在 `plugin.set_param` 之后立刻 `plugin.get_param`，
  把**采纳值**而非请求值写进 `ParameterBridge`；`backend_vst3::get_param` 改读 `self.params`
  （插件真身）而不是 bridge——否则这条链是循环的，量化永远观察不到。
  **这不是新发明的约定**：编辑器的 `Vst3ParamsViewContext::set_param` 本来就是"应用→回读→发布采纳值"，
  只是 `Plugin::set_param` 那条路径没照做。
  **bridge 的七个写入点逐个核对**，只改了有插件实例的四个，外加 process 开头"从 bridge 同步进插件"那次的写回。
  不带 GUI 的 `ControllerWrapper` **没有插件实例**（源码注释本来就写明），无从量化，
  只能先写原值、由 processor 在下一块写回采纳值——**收敛需要一个块，如实延迟，不是静默丢值**。
  写回在音频线程：`ParameterBridge::set` 是原子 swap，且**仅在值真的改变时**才递增 generation，
  所以零分配零加锁，也不会把同步触发成每块循环（两块内收敛）。
  **顺带修好一个说谎的测试替身**：`HostGuiTestPlugin::set_param` 是空实现、`get_param` 恒返回 0.0。
  旧契约下 wrapper 回显请求值，所以这条从来没被发现；新契约下它立刻暴露——
  一个默默丢弃状态的替身，不能替插件出现在参数管线的测试里。已改为真的存值。
  它所在的测试其实考的是 connect/disconnect，参数值只是标记物，改后语义不变。
- Evidence/artifact: **证据是 golden 的 diff 本身**——重新打包后重新生成，
  `SunMaoGain.vst3.trace` **只变了两行**（两个 stepped 参数 `8.017…e-1`/`7.871…e-1` → `1.0e0`），
  CLAP 的 golden **一字未动**（它本来就是对的），**没有任何 `block` 记录变化、连续参数也没动**。
  修复后两份 trace **只差 `format` 一行**，于是把 CI 的跨格式断言从"`block` 行相同"
  升级成"除 `format` 外逐行相同"——这是 M2 建回归床的意义第一次兑现：
  一次行为修改的范围，由 golden 逐行说清楚，而不是靠我口述。
  两个新测试都做过反向验证：把 `get_param` 改回读 bridge，两者立刻变红，还原后转绿。
  逐套件比对确认只有 `vst3_rs`（66→67）与 `sunmao_backend_vst3`（28→29）两套变化。
- Unresolved: 须取得同 commit 三平台 hosted 全绿。仍未修两项：连续参数的 f32/f64 精度差
  （同一根因，但两格式都不算错，宿主的 `expect` 以显式容差应对）、
  **VST3 class 串跨平台不一致（须由仓库所有者拍板，会变更所有既有插件身份）**。
  Phase 4 继承的四条遗留未动。下一步 **M3：性能与泄漏检测**。
