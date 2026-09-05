# Renderer 资源与线程归属

Phase 4 M1 产物。记录 GL / WGPU / WebView 三个渲染后端各自**谁拥有设备、表面、上下文**，
以及**销毁顺序**。写下来的原因很实际：这三处的生命周期错误不会变成编译错误或测试失败，
只会变成宿主里的偶发崩溃——Windows WGPU 那个 exit 139 就是这一类。

本文描述的是**当前代码的实际行为**（截至 Phase 4 M1），不是理想设计。

## 通用结构

三个后端都经 `sunmao/view_baseview` 接入，形状相同：

```
SunmaoPlugin::view() -> Box<dyn SunmaoView>      插件侧，可被调用多次，每次新建
        ↓ open(parent, context)
baseview::Window::open_parented(...)             宿主窗口的子窗口
        ↓ 返回
ViewHandle { ScalableWindow { handle, base_w, base_h } }   backend 持有
        ↓ drop
baseview 关闭窗口 → WindowHandler::drop → 渲染资源释放
```

**关键点：渲染资源全部由 `WindowHandler` 拥有**，不由 `SunmaoView` 或 `ViewHandle` 拥有。
`ViewHandle` 只持有 `baseview::WindowHandle`，销毁它即请求关闭窗口；真正的设备/上下文在
baseview 的窗口线程上随 handler 一起析构。这就是为什么 `ViewHandle` **不是 `Send`**——
原生编辑器资源必须留在自己的 UI 线程上。

## OpenGL（`gl` feature）

| 资源 | 拥有者 | 创建时机 | 销毁时机 |
|---|---|---|---|
| 平台 GL context（`baseview` 的 `gl_context`） | `baseview::Window` | `open_parented` 内，窗口创建时 | 窗口关闭时，由 baseview 释放 |
| `sunmao_gui::gl::GlContext`（着色器/VBO/program） | `GlViewHandler`（即 `WindowHandler` 实现） | 首帧 `on_frame` 之前，在 builder 闭包里经 `GlContext::from_loader` | handler drop 时 |

- **每次绘制都必须 `make_current`**：`on_frame` 先 `gl_ctx.make_current()`、末尾
  `swap_buffers()`。宿主可能在同一线程上轮流驱动多个插件编辑器，不能假设 current context
  在两帧之间保持不变。
- `GlContext::from_loader` 用一个**临时的 400×300** 尺寸初始化，随后由 `on_resize` 修正；
  这不是逻辑尺寸的真相来源，真相在 `BaseviewConfig`。
- 销毁顺序：GL 对象（program/VBO）必须在平台 context 之前释放，因此 `GlContext` 作为
  handler 的字段、平台 context 由 baseview 拥有，drop 顺序天然正确。**不要**把 `GlContext`
  移出 handler 或延长它的寿命。

## WGPU（`wgpu` feature）

| 资源 | 拥有者 | 创建时机 | 销毁时机 |
|---|---|---|---|
| `wgpu::Instance` / `Adapter` / `Device` / `Queue` | `WgpuContext`（在 `WgpuHandler` 内） | builder 闭包里 `pollster::block_on(WgpuHandler::new(..))` | handler drop 时 |
| `wgpu::Surface` | 同上，绑定到 baseview 窗口 | 同上 | **必须早于窗口销毁** |
| pipeline / shader module / bind group layout | `WgpuContext` | 一次性，随 device | 随 device |

- 创建是**阻塞**的（`pollster::block_on`），发生在窗口线程上。失败时代码走
  `window.close()` + `BaseviewHandler::Failed`，不留半初始化的窗口。
- `Surface` 借用窗口。Rust 的所有权保证了 surface 不会比 `WgpuHandler` 活得久，但
  **`WgpuHandler` 必须比 baseview 窗口先析构**——这正是 Windows 上 exit 139 的嫌疑区域：
  进程退出时若窗口/HWND 已被系统回收而 D3D12 设备仍在析构，就会踩到已释放的对象。
- **已知未决**：Windows 上 WGPU GUI 偶发在断言全过、打印 `Done.` 之后 exit 139。自 run #66
  的“不卸载插件库”修复以来在 #66/#68/#69/#71/#72/#73 连续未复现，但**不改判为已修复**。
  若复现，从这一行的销毁顺序查起：确认 `WgpuHandler` 的 drop 早于窗口销毁，且进程退出路径
  不会跳过它。

## WebView（`webview` feature）

WebView 与前两者结构不同，**它有自己的线程**。

| 资源 | 拥有者 | 线程 |
|---|---|---|
| `wry::WebView` | `gtk_thread` 模块内的 `HashMap`（Linux） | **专用 GTK 线程** `sunmao-gtk-webview` |
| 调用方持有的句柄 | `WebViewProxy`（只是一个 id + channel） | 任意线程 |

- **Linux 上 GTK 必须在自己的专用线程上**（`baseview/src/webview.rs` 的 `gtk_thread`）。
  这是 Phase 1 的既有修复，**不得回退**：GTK 不能在宿主的任意线程上初始化，也不能与
  baseview 的 X11 事件循环共用线程。
- 所有跨线程调用经 `Command` channel，回复带 **10 秒 `REPLY_TIMEOUT`**——GTK 线程本身
  从不阻塞，所以超时只会在 WebKit 自己卡死时触发，调用方拿到的是诊断而不是挂起。
- **100ms 的 drain**（`webview.rs:343`，`gtk::main_iteration_do(false)` 循环）是另一个
  既有修复：销毁 WebView 后必须把 GTK 的待处理事件抽干，否则析构在事件仍在队列里时完成，
  下一次创建会拿到脏状态。同样**不得回退**。
- WebView 的几何是**固定**的（Phase 1 修复），不随窗口自适应。

## DPI scale（Phase 4 M1）

`ViewHandle::set_scale(f32)` 是唯一入口，两格式经它汇合（VST3
`IPlugViewContentScaleSupport`、CLAP `clap_plugin_gui.set_scale`，差异见
`docs/phase2/semantics.md`）。baseview 适配层的实现是**把窗口重设为 `创建尺寸 × factor`**，
即非 DPI-aware 编辑器在 Windows/X11 上应有的反应；macOS 由 AppKit 拥有 backing scale，
宿主通常根本不调这个接口，因此该路径在 macOS 上只是保持未使用，而不是被特殊分支绕过。

基准尺寸取自 `BaseviewConfig`，存在 `ScalableWindow` 里——不能从当前窗口尺寸推导，
否则连续两次 scale 会复合（1.5 之后再 1.5 得到 2.25 而非 1.5）。

## 给后续 milestone 的约束

- M2 的控件与布局**不得**持有渲染资源；控件只描述几何与绘制指令，资源仍归 handler。
- M3 的字体栅格化会引入第一处**跨帧缓存**的 GPU 资源（字形图集），必须挂在 handler 上，
  不能是进程级 static——否则多实例插件会共享并跨窗口释放。
- M4 的 `VizChannel` 是 audio→GUI 方向，**不拥有任何渲染资源**，只做无锁数据传递；
  它可以是 `Send`，而 `ViewHandle` 不行。
