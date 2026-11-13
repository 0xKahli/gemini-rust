use std::collections::HashMap;
use egui::{Color32, ColorImage, Context, Frame, Image, Margin, Rounding, Stroke, TextureHandle, TextureOptions, Ui};
use latex_rust::font::MathFont;
use latex_rust::{latex_to_png, Color, Dim, PngOptions};

pub struct LatexCache {
    font: Option<MathFont>,
    textures: HashMap<String, TextureHandle>,
}

impl Default for LatexCache {
    fn default() -> Self {
        let font = MathFont::stix_two_math().ok();
        Self {
            font,
            textures: HashMap::new(),
        }
    }
}

impl LatexCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Renders a LaTeX math expression to an egui TextureHandle (cached).
    pub fn get_or_render(&mut self, ctx: &Context, expr: &str, is_display: bool) -> Option<TextureHandle> {
        let key = format!("{}:{}", if is_display { "disp" } else { "inline" }, expr);
        if let Some(tex) = self.textures.get(&key) {
            return Some(tex.clone());
        }

        let font = self.font.as_ref()?;
        let mut opts = PngOptions::new();
        opts.display = is_display;
        opts.dpi = Dim::from_i64(144);
        // Crisp readable light blue-white math color
        opts.color = Color::rgb(220, 235, 255);
        opts.font_size_pt = if is_display { Dim::from_i64(16) } else { Dim::from_i64(13) };

        match latex_to_png(expr.trim(), font, &opts) {
            Ok(png_bytes) => {
                if let Ok(img) = image::load_from_memory(&png_bytes) {
                    let rgba = img.into_rgba8();
                    let size = [rgba.width() as usize, rgba.height() as usize];
                    let color_img = ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                    let tex = ctx.load_texture(
                        format!("latex_{:x}", seahash(expr.as_bytes())),
                        color_img,
                        TextureOptions::LINEAR,
                    );
                    self.textures.insert(key, tex.clone());
                    Some(tex)
                } else {
                    None
                }
            }
            Err(_) => None,
        }
    }
}

/// Simple fast hash for unique texture names
fn seahash(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0x123456789abcdef0;
    for b in bytes {
        h = h.wrapping_mul(31).wrapping_add(*b as u64);
    }
    h
}

enum Segment<'a> {
    Markdown(&'a str),
    LatexDisplay(&'a str),
}

/// Parses content into alternating Markdown and LaTeX display math segments.
fn split_latex_segments(content: &str) -> Vec<Segment<'_>> {
    let mut segments = Vec::new();
    let mut cur = 0;
    let bytes = content.as_bytes();
    let len = bytes.len();

    while cur < len {
        // Look for $$ or \[
        if let Some(rel_start) = content[cur..].find("$$") {
            let start = cur + rel_start;
            // Add markdown before $$
            if start > cur {
                segments.push(Segment::Markdown(&content[cur..start]));
            }
            let formula_start = start + 2;
            if let Some(rel_end) = content[formula_start..].find("$$") {
                let end = formula_start + rel_end;
                segments.push(Segment::LatexDisplay(&content[formula_start..end]));
                cur = end + 2;
            } else {
                // Unterminated $$, treat remainder as markdown
                segments.push(Segment::Markdown(&content[start..]));
                break;
            }
        } else if let Some(rel_start) = content[cur..].find("\\[") {
            let start = cur + rel_start;
            if start > cur {
                segments.push(Segment::Markdown(&content[cur..start]));
            }
            let formula_start = start + 2;
            if let Some(rel_end) = content[formula_start..].find("\\]") {
                let end = formula_start + rel_end;
                segments.push(Segment::LatexDisplay(&content[formula_start..end]));
                cur = end + 2;
            } else {
                segments.push(Segment::Markdown(&content[start..]));
                break;
            }
        } else {
            // Remainder is pure markdown
            segments.push(Segment::Markdown(&content[cur..]));
            break;
        }
    }

    segments
}

/// Renders a message content stream, automatically detecting and rendering LaTeX display math.
pub fn render_content_with_latex(
    ui: &mut Ui,
    ctx: &Context,
    latex_cache: &mut LatexCache,
    markdown_cache: &mut egui_commonmark::CommonMarkCache,
    content: &str,
) {
    if !content.contains("$$") && !content.contains("\\[") {
        // Fast path: no display LaTeX math present
        egui_commonmark::CommonMarkViewer::new().show(ui, markdown_cache, content);
        return;
    }

    let segments = split_latex_segments(content);
    for seg in segments {
        match seg {
            Segment::Markdown(md) => {
                if !md.trim().is_empty() {
                    egui_commonmark::CommonMarkViewer::new().show(ui, markdown_cache, md);
                }
            }
            Segment::LatexDisplay(expr) => {
                if let Some(tex) = latex_cache.get_or_render(ctx, expr, true) {
                    ui.add_space(4.0);
                    Frame::none()
                        .fill(Color32::from_rgb(22, 24, 28))
                        .rounding(Rounding::same(8.0))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(45, 48, 55)))
                        .inner_margin(Margin::symmetric(16.0, 10.0))
                        .show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.add(Image::new(&tex));
                            });
                        });
                    ui.add_space(4.0);
                } else {
                    // Fallback to formatted code block if LaTeX parsing fails
                    ui.add_space(2.0);
                    Frame::none()
                        .fill(Color32::from_rgb(25, 27, 32))
                        .rounding(Rounding::same(6.0))
                        .inner_margin(Margin::symmetric(10.0, 6.0))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(format!("$$ {} $$", expr.trim()))
                                    .monospace()
                                    .color(Color32::from_rgb(180, 210, 255)),
                            );
                        });
                    ui.add_space(2.0);
                }
            }
        }
    }
}
