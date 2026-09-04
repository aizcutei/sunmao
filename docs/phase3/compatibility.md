# SunMao 版本兼容策略（Phase 3 M5）

本文件定义两条互相独立的兼容轴：**API 兼容**（写插件的人升级 `sunmao`/`sunmao_dsp`
之后代码是否还能编译、行为是否不变）与 **state 兼容**（用户在旧版插件里存下的 preset
在新版插件里是否还能载入、听起来是否一样）。两者的版本号也是独立的：前者是 crate 的
semver，后者是每个插件自己声明的 `SunmaoPlugin::STATE_VERSION`。升级 crate 版本**不**
要求升 state 版本，反之亦然。

## 1. crate 与 semver

workspace 所有 crate 共用 `[workspace.package] version`（当前 `0.1.0`）。在 `0.y.z`
阶段按 Cargo 的 semver 约定：**`y` 是破坏性版本位**，`z` 是兼容位。也就是说
`0.1.x → 0.1.(x+1)` 必须对下述"公共 API 面"保持源码兼容；任何破坏性变更都要升到
`0.2.0` 并在本文件末尾的变更记录里写明迁移方式。

### 1.1 公共 API 面（受 semver 保护）

| crate | 受保护的面 | 说明 |
|---|---|---|
| `sunmao` | `sunmao::prelude::*` 中的每个名字；`sunmao_export!`、`#[derive(Params)]` 的输入语法 | prelude 是插件作者唯一被要求知道的导入路径。不在 prelude 里的路径（`sunmao::backend_clap`、`sunmao::__private`、各 `_rs`/`_sys` crate）**不受保护**，随时可能变。 |
| `sunmao_core` | `SunmaoPlugin` trait 的既有方法签名与默认实现、`AudioBuffer`/`EventQueue`/`Event`/`ProcessContext`/`TailLength`/`RenderMode`/`BusConfig`、参数类型 `FloatParam`/`IntParam`/`BoolParam` 与 `ParamDescriptor`、`Smoother`/`SmoothingStyle` | `SunmaoPlugin` 只允许**新增带默认实现的方法**；改既有方法签名、去掉默认实现、新增无默认实现的方法都是破坏性变更。 |
| `sunmao_dsp` | `sunmao_dsp::prelude::*` 中的每个类型与函数，及其 `process`/`tick`/`next` 系列方法的**数值语义**（见 1.3） | 模块内未被 prelude 导出的项（如 `oversampling` 的 `HALFBAND_TAPS`）是实现细节。 |

### 1.2 什么算破坏性

除常规的"签名变化、删除、可见性收窄"之外，SunMao 还把下面几类视为破坏性，因为它们
会改变用户**听到的**结果或宿主**看到的**结构，即便源码仍能编译：

- 改变某个 `ParamDescriptor` 的**归一化映射**（min/max/步进/枚举顺序）——它决定宿主
  automation 曲线与 state 里 `f64` 值的含义。
- 改变 `Smoother` 的 ramp 形状或终止条件、`Meter` 的弹道常数、`Oversampler` 的
  latency（见 1.3）。
- 改变 `sunmao_export!` 生成的 VST3 class id / CLAP id 派生方式，或 `Vst3Info`/`ClapInfo`
  默认值——宿主用它们识别插件，变了就是"另一个插件"。
- 改变 state blob 的 header 布局或 magic（见第 2 节）——这属于框架级 state 破坏，需要
  框架自己提供迁移，而不是让每个插件升 `STATE_VERSION`。

### 1.3 `sunmao_dsp` 的数值语义承诺

`sunmao_dsp` 组件不是"任何实现都行"的黑箱：插件作者会围绕它们的具体数值行为写测试
（Phase 3 的四个 fixture 就是这么做的）。因此在同一破坏性版本内承诺：

- **Latency 常数不变。** `OversamplingFactor::latency_samples()`（2x=16、4x=24）是宿主
  用来对齐轨道的数字；改滤波器长度会改这个数，必须升破坏性版本。
- **DC 与静态增益不变。** 滤波器在 DC 的增益、`db_to_gain`/`gain_to_db` 的映射、
  `DryWet` 两个 law 的系数公式不变。
- **稳定性边界只能放宽不能收紧。** 一个版本里被接受（不产生非有限输出）的参数组合，
  下一个兼容版本里仍必须被接受。这由 `sunmao/dsp/tests/property.rs` 的 proptest 钉住：
  收紧任何一个 strategy 的范围都视为破坏性变更。
- **denormal floor（`DENORMAL_FLOOR` = 1e-20）不变，且必须按状态组整体判定。**
  阈值本身是承诺的一半；另一半是**多状态组件要把它的状态当作一组来判定**——单独把某个
  状态清零会破坏让这组状态衰减的耦合。这不是理论顾虑：独立 flush 曾让 20 Hz / resonance
  0.2 / 96 kHz 的 `Svf` 归零需要 680 万样本（71 秒）而非其时间常数对应的 4.3 万样本，
  也曾让 121 Hz / 96 kHz 的 `Biquad` 永久停在 6.2e-20 的极限环上（`a1 ≈ -2` 使被清零的
  那一侧每样本把另一侧放大近两倍）。守卫是
  `every_filter_settles_within_its_own_time_constant`：它按各滤波器**离散极点半径**算出
  预算，因此"衰减被拖慢"会直接失败，而不是表现为某个容差的边缘抖动。
- **静音后必须归零，而不只是变小。** 组件喂静音足够长（自身时间常数量级）之后输出必须
  是精确的 `0.0` 并保持，不允许停在任何非零残留上。
- **允许变的：** 频响的 ripple/过渡带细节、浮点舍入、`reset()` 之后的瞬态形状、
  低于约 -350 dBFS 的尾部残留轨迹，只要上述承诺与既有单测/proptest 仍通过。

### 1.4 弃用流程

先在一个兼容版本里加 `#[deprecated(note = "use X")]` 并在 prelude **同时**导出新旧
两个名字，下一个破坏性版本才删除旧名字。fixture 与 template 必须在弃用的同一版本改用
新名字，让示例永远不出现 deprecation warning。

## 2. state 兼容

### 2.1 格式（框架层，插件不可见）

两种格式的 blob 布局相同、magic 不同（VST3 `SMV3PRM\0`、CLAP `SMCLPRM\0`）：

```
magic[8] | version: u32 LE | count: u32 LE | count × ( id: u32 LE | value: f64 LE )
```

- `version` 是写出该 blob 的插件的 `STATE_VERSION`，不是框架版本。
- `id` 是参数字符串 id 的 **FNV-1a 32 位哈希**（`stable_param_id`），`u32::MAX` 保留为
  无效 id（哈希到那里的字符串被确定性地重映射）。因此 state 与参数在结构体里的
  **顺序无关**、与显示名无关，只与字符串 id 有关。哈希算法本身属于持久化契约：换掉它会
  静默孤立所有已存 preset 而不是报错，因此由 `sunmao/core/tests/property.rs` 的
  `the_id_hash_stays_fnv_1a_over_the_id_bytes`（对照独立实现的 FNV-1a）与
  `a_parameter_id_never_hashes_to_the_reserved_value` 机械钉住。
- `value` 存在**该格式自己的取值域**里，两者并不总是同一个数：
  - VST3 blob 写**归一化**值（`0..=1` 的 `f64`），因为 VST3 参数本身就是归一化的；
  - CLAP blob 写 CLAP 的**plain value**。连续参数的 plain value 恰好等于归一化值，
    但**stepped 参数的 plain value 是步进索引**（`0..=step_count`，见
    `parameter_to_clap_value`）。
  两侧都在解码阶段整体拒绝非有限值；CLAP 侧还在**应用任何值之前**校验每个已知 id 的值
  落在该参数声明的 `min..=max` 内（`state_value_valid`），越界即整体拒绝。
- 两种格式的 blob **互不通用**——不只是 magic 不同：同一个 stepped 参数在两边的数值
  尺度就不同。框架不做跨格式转换（宿主本来也只读自己格式的 preset）。
- 上限：`count ≤ MAX_STATE_PARAMETERS`，越界与截断的 blob 在**应用任何值之前**拒绝，
  绝不留下半应用的状态。

一旦要改这个布局（比如加 `Vec<u8>` 的插件自有 blob），由框架在 header 里升自己的格式
版本并在两个 `_rs` 层同时提供解码旧布局的路径，**不得**借插件的 `STATE_VERSION` 表达。

### 2.2 载入规则（插件层）

设插件当前 `STATE_VERSION = N`，blob 里写的是 `v`：

| 情形 | 行为 | 测试 |
|---|---|---|
| `v == N` | 按 id 匹配应用；不回调 `migrate_state` | `state_is_versioned_and_keyed_by_parameter_id`（两格式各一） |
| `v < N` | 按 id 匹配应用（老参数恢复，blob 里没有的新参数保持默认），**全部应用完毕后**回调 `migrate_state(v)` | `a_state_from_an_older_build_is_accepted`、`clap_state_from_an_older_build_triggers_migration`、`vst3_state_from_an_older_build_triggers_migration`、fixture `a_v1_state_leaves_the_new_parameter_at_its_documented_default` |
| `v > N` | 整体拒绝（本 build 无法解释未来含义），宿主收到失败返回值，参数保持载入前的值 | `a_state_from_a_newer_build_is_rejected`（两格式各一） |
| magic 不符 / 截断 / 非有限值 / 越界 | 整体拒绝，同上 | `a_foreign_magic_is_rejected`、`malformed_state_is_rejected_before_values_are_exposed`、`out_of_range_state_values_are_rejected` |

上表每一行都是按例子写的单测。同样的规则另有 proptest 守卫（`clap_rs`
`ext::state::tests`、`vst3_rs` `state::tests`），覆盖的是**任意** blob 而不是挑出来的
几个：`any_parameter_set_round_trips_bit_for_bit`（存进去的值逐位取回，不给"约等于"
留空间）、`decoding_arbitrary_bytes_never_panics_and_never_yields_a_partial_list`
（任意字节串要么被拒，要么给出**完整**条目表）、
`a_blob_is_readable_exactly_when_it_is_not_from_the_future`（版本规则是不等式而非等式）、
`the_decoded_map_does_not_depend_on_parameter_order`（VST3 侧的顺序无关性）、
`a_rejected_state_never_reaches_the_plugin`（CLAP 侧端到端：被拒的 blob 不应用任何值、
不触发迁移；被接受的 blob 先应用完全部已知 id 才回调迁移）。

`migrate_state` 总是看到一个**完整**的旧状态：所有可匹配的参数值已经就位，插件据此
把"旧含义"翻译成"新含义"。它跑在宿主的 state 载入线程（非音频线程），可以分配。

### 2.3 什么时候升 `STATE_VERSION`

**只有既有参数的含义变了才升**。具体地：

| 变更 | 是否升版本 | 原因 |
|---|---|---|
| 新增参数 | 否 | 旧 blob 没有该 id，载入后保持默认值——这就是设计好的行为；测试用例即 `a_v1_state_leaves_the_new_parameter_at_its_documented_default` |
| 删除参数 | 否 | 旧 blob 里多出的 id 无匹配，被忽略 |
| 改显示名、单位、分组路径、smoothing 时间 | 否 | 都不进 state（分组路径只影响宿主看到的层级，其归一化由 `group_segments` 负责，见 `a_group_path_normalizes_to_named_levels_and_stays_normalized`） |
| 改字符串 id | **否，且禁止**——等价于删旧参数 + 加新参数，用户 preset 里的该值会静默丢失。要改名请保留旧 id 并只改显示名 | id 哈希是 state 的键 |
| 改 min/max/步进/枚举顺序、改 default 且希望旧 preset 跟着变、改参数的物理含义（如线性增益改 dB） | **是** | 存进 blob 的那个数的**解释**变了，必须在 `migrate_state(from)` 里按旧范围反算再按新范围正算。改 `step_count` 尤其要注意：它同时改变 CLAP blob 里 plain value 的尺度（2.1）与归一化值到步进的舍入 |

`STATE_VERSION` 只增不减；`migrate_state` 必须处理 `from` 为**任何**比当前小的值
（用 `if from < 2 { ... } if from < 3 { ... }` 的链式写法，让一个 v1 blob 顺次经过每一步）。
迁移应当**幂等**：因为 Phase 3 M1 之前的构建把版本硬编码为 1 写出，修复后那些 blob 会
再次触发 `migrate_state(1)`，非幂等迁移会在这些 preset 上重复应用（详见
`docs/phase2/semantics.md` 的"state 版本与迁移"行）。

### 2.4 不进 state 的东西

以下内容**永不**写入 blob，升级它们不影响 state 兼容：modulation 偏移（`ParamMod` 是
叠加通道，`Event::as_param_change()` 对它返回 `None`）、smoothing 的当前值与目标值、
latency/tail、bus 激活状态与 speaker layout、`Meter` 读数、任何 GUI 状态。插件若想
持久化 GUI 布局等信息，需等框架层格式升级（2.1）后走插件自有 blob，而不是伪装成参数。

### 2.5 演进契约的验证方式

- 任何改动 state 编解码的 PR 必须让 `sunmao_state_migration` fixture 的 round-trip 与
  迁移测试原样通过，并新增对应版本的 blob 样本测试；这是硬性规则"契约变更不得破坏既有
  fixture 的 state round-trip；演进必须走版本迁移测试"的落点。
- `fuzz/`（cargo-fuzz，非 blocking）对两格式的 state 解码器做无界 fuzz，入口见仓库 README。
- proptest（`sunmao/dsp/tests/property.rs`、`sunmao/core/tests/property.rs` 与两 `_rs`
  的 property 测试）是数值语义承诺（1.3）与 state 规则（2.1/2.2）的机械守卫：收紧任一
  strategy 的范围等于收紧承诺，必须走破坏性版本。本文件的每条承诺都应当能指到一个测试
  名；指不到的承诺是文档债，不是承诺。

## 3. 变更记录

| crate 版本 | 类型 | 内容 | 迁移 |
|---|---|---|---|
| 0.1.0 | 初始 | Phase 1–3 全部 API；state 格式 header v1（magic + version + count） | — |
| 0.1.0（未发布） | 缺陷修复 | `Svf` 与 `Biquad` 改为**成组** flush 其状态对（新增 `DENORMAL_FLOOR` 到 prelude）。可听行为不变——差异只在 -350 dBFS 以下的尾部；变的是静音后归零所需时间（`Svf` 在 20 Hz/0.2/96 kHz 由 680 万样本降到 4.3 万，`Biquad` 由"永不归零"变为归零） | 无需迁移（0.1.0 尚未发布）。若已依赖旧的尾部残留轨迹：那不是承诺范围（见 1.3"允许变的"） |
