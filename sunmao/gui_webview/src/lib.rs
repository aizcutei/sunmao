//! Compatibility crate for SunMao's WebView renderer.
//!
//! The implementation lives in `sunmao_gui::webview`; this crate keeps the
//! original package-level API without maintaining a second renderer copy.

pub use sunmao_gui::webview::{DrawCommand, WebViewContext};

/// Generate a complete HTML page containing a canvas and drawing code.
pub fn generate_html_page(title: &str, width: u32, height: u32, draw_commands: &str) -> String {
    let title = escape_html_text(title);
    let draw_commands = escape_script_end_tags(draw_commands);
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>{title}</title>
    <style>
        body {{
            margin: 0;
            background: #1a1a1e;
            display: flex;
            justify-content: center;
            align-items: center;
            min-height: 100vh;
        }}
        canvas {{
            background: #25252a;
            border-radius: 8px;
        }}
    </style>
</head>
<body>
    <canvas id="canvas" width="{width}" height="{height}"></canvas>
    <script>
{draw_commands}
    </script>
</body>
</html>"#
    )
}

fn escape_html_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn escape_script_end_tags(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    let mut remaining = value;
    while let Some(index) = remaining.to_ascii_lowercase().find("</script") {
        escaped.push_str(&remaining[..index]);
        escaped.push_str("<\\/script");
        remaining = &remaining[index + "</script".len()..];
    }
    escaped.push_str(remaining);
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use sunmao_gui::{Color, GuiContext, TextAlign};

    #[test]
    fn compatibility_context_uses_the_hardened_renderer() {
        let mut context = WebViewContext::new(100.0, 50.0);
        context.draw_text(
            "</script><script>alert('x')</script>",
            0.0,
            10.0,
            12.0,
            Color::WHITE,
            TextAlign::Left,
        );
        let html = generate_html_page("<SunMao>", 100, 50, &context.generate_js());

        assert!(html.contains("<title>&lt;SunMao&gt;</title>"));
        assert_eq!(html.matches("</script>").count(), 1);
        assert!(!html.contains("<script>alert"));
    }

    #[test]
    fn raw_script_terminators_are_neutralized() {
        let html = generate_html_page("test", 1, 1, "const x = '</ScRiPt>';\n");
        assert_eq!(html.matches("</script>").count(), 1);
        assert!(html.contains("<\\/script>"));
    }
}
