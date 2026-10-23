//! Isolated native Markdown evidence using synthetic math, prices and code.
//! Run with `--features appshots-fixture --example markdown-math-fixture -- DIR`.
use gpui::{
    AppContext, Bounds, Context, IntoElement, ParentElement, Render, Styled, Window, WindowBounds,
    WindowOptions, div, point, px, size,
};
use harness_ui::{appearance, icons, markdown, settings, theme, theme_library, typography};
use std::{path::PathBuf, sync::Arc, time::Duration};

const SOURCE: &str = r#"# Inline equations and ordinary prices

The sample mean has variance $\operatorname{Var}(\bar X)=\sigma^2/m$.

- **$x_i$** and $\alpha + \beta$ keep their surrounding Markdown styles.
- Standard error: $\sigma/\sqrt m$; fraction: $\frac{a+b}{n}$.

Prices stay literal: $5 and $10; $5–$10; $.50–$.75; escaped \$5.

Code stays literal: `$x^2$`.

| Quantity | Formula |
|---|---|
| Sample mean | $\bar X$ |
| Standard error | $\sigma/\sqrt m$ |

Existing display-math control:
\[ y=\frac{a+b}{n} \]
"#;

struct MathFixture {
    tree: markdown::BlockTree,
}

impl Render for MathFixture {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = theme::Theme::of(cx);
        div()
            .size_full()
            .p(px(24.0))
            .bg(theme.bg)
            .text_color(theme.text)
            .child(markdown::render::render_tree(
                &self.tree,
                &markdown::render::RenderOptions::settled("math-fixture".into()),
                &theme,
                window,
                &|_| None,
            ))
    }
}

fn main() -> anyhow::Result<()> {
    let output = PathBuf::from(std::env::args().nth(1).expect("output directory"));
    std::fs::create_dir_all(&output)?;
    let data = tempfile::tempdir()?;
    let runtime = tokio::runtime::Runtime::new()?;
    let _runtime = runtime.enter();
    let failure = Arc::new(std::sync::Mutex::new(None));
    let result = failure.clone();
    gpui_platform::application()
        .with_assets(icons::Assets)
        .run(move |cx| {
            gpui_tokio::init(cx);
            gpui_base::init(cx);
            let settings = settings::UiSettings::default();
            settings::init(settings.clone(), data.path().to_owned(), cx);
            let fonts = typography::register_fonts(cx);
            typography::init(
                settings.ui_font_family.clone(),
                settings.ui_font_size,
                settings.terminal_font_family.clone(),
                settings.terminal_font_size,
                settings.code_font_family.clone(),
                settings.code_font_size,
                fonts,
                cx,
            );
            theme_library::init(data.path().to_owned(), cx);
            appearance::init(
                appearance::AppearanceMode::Dark,
                settings.theme_selection,
                settings.accent,
                settings.surface,
                cx,
            );
            let windows: Vec<(gpui::AnyWindowHandle, &str)> =
                [(900.0, 720.0, "wide"), (480.0, 820.0, "narrow")]
                    .into_iter()
                    .map(|(width, height, name)| {
                        let window = cx
                            .open_window(
                                WindowOptions {
                                    window_background: theme::Theme::of(cx)
                                        .window_background_appearance(),
                                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                                        point(px(40.0), px(60.0)),
                                        size(px(width), px(height)),
                                    ))),
                                    ..Default::default()
                                },
                                |_, cx| {
                                    cx.new(|_| MathFixture {
                                        tree: markdown::parse_full(SOURCE),
                                    })
                                },
                            )
                            .unwrap();
                        (window.into(), name)
                    })
                    .collect();
            cx.activate(true);
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                for (window, name) in windows {
                    let capture: anyhow::Result<()> = (|| {
                        window.update(cx, |_, window, cx| {
                            window.draw(cx).clear();
                            window
                                .render_to_image()?
                                .save(output.join(format!("math-{name}.png")))?;
                            Ok(())
                        })?
                    })();
                    if let Err(error) = capture {
                        *result.lock().unwrap() = Some(format!("{error:#}"));
                    }
                    let _ = window.update(cx, |_, window, _| window.remove_window());
                }
                let _ = cx.update(|cx| cx.quit());
            })
            .detach();
        });
    if let Some(error) = failure.lock().unwrap().take() {
        anyhow::bail!(error);
    }
    Ok(())
}
