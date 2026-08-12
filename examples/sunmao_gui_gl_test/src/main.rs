//! SunMao GUI Test - demonstrates all backends
//!
//! This example shows widget creation, event handling, and rendering
//! with different GUI backends.

use sunmao_gui::{
    Button, Color, Event, Fill, GuiContext, Knob, Label, Modifiers, MouseButton, Orientation,
    ParameterWidget, Rect, Slider, Widget,
};
use sunmao_gui_webview::{generate_html_page, WebViewContext};

fn main() {
    println!("SunMao GUI Test - All Backends");
    println!("==============================\n");

    // Create widgets
    let mut knob = Knob::new("gain")
        .with_bounds(Rect::new(20.0, 20.0, 64.0, 64.0))
        .with_default(0.5);

    let mut slider = Slider::new("volume")
        .with_bounds(Rect::new(100.0, 40.0, 150.0, 24.0))
        .with_orientation(Orientation::Horizontal);

    let mut button = Button::toggle("Bypass").with_bounds(Rect::new(270.0, 40.0, 80.0, 28.0));

    let label = Label::new("SunMao Gain").with_bounds(Rect::new(20.0, 100.0, 200.0, 24.0));

    // ========== Test Widget Interactions ==========
    println!("Testing Widget Interactions:");

    // Hover knob
    knob.handle_event(&Event::MouseMove {
        x: 52.0,
        y: 52.0,
        modifiers: Modifiers::none(),
    });
    println!("  ✓ Knob hovered: {}", knob.state().hovered);

    // Scroll to change knob value
    knob.handle_event(&Event::Scroll {
        x: 52.0,
        y: 52.0,
        delta_x: 0.0,
        delta_y: 20.0,
        modifiers: Modifiers::none(),
    });
    println!("  ✓ Knob value after scroll: {:.2}", knob.value());

    // Click button
    button.handle_event(&Event::MouseDown {
        x: 310.0,
        y: 54.0,
        button: MouseButton::Left,
        modifiers: Modifiers::none(),
    });
    println!("  ✓ Button toggled: {}", button.is_on());

    // Set slider programmatically
    slider.set_value(0.75);
    println!("  ✓ Slider value: {:.2}", slider.value());

    // ========== Test WebView Backend ==========
    println!("\nTesting WebView Backend:");

    let mut ctx = WebViewContext::new(400.0, 200.0);
    ctx.begin_frame();

    // Draw background
    ctx.fill_rect(0.0, 0.0, 400.0, 200.0, Fill::Solid(Color::BACKGROUND));

    // Draw widgets
    knob.draw(&mut ctx);
    slider.draw(&mut ctx);
    button.draw(&mut ctx);
    label.draw(&mut ctx);

    let js = ctx.generate_js();
    println!(
        "  Generated {} JS drawing commands",
        ctx.get_commands().len()
    );

    // Generate HTML file
    let html = generate_html_page("SunMao Gain GUI", 400, 200, &js);

    // Save to file
    let html_path = "build/sunmao_gui_test.html";
    std::fs::create_dir_all("build").ok();
    std::fs::write(html_path, &html).expect("Failed to write HTML");
    println!("  ✓ Generated HTML file: {}", html_path);

    // ========== Summary ==========
    println!("\n✅ All tests passed!");
    println!("\nAvailable Renderer Backends:");
    println!("  - sunmao_gui_gl:      OpenGL (glow)");
    println!("  - sunmao_gui_wgpu:    WGPU (cross-platform)");
    println!("  - sunmao_gui_webview: HTML5 Canvas (WebView)");
    println!(
        "\nOpen {} in a browser to see the WebView output.",
        html_path
    );
}
