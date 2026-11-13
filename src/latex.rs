use std::collections::HashMap;
use egui::{Color32, ColorImage, Context, Image, RichText, TextureHandle, TextureOptions, Ui};
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

    /// Renders a display LaTeX math expression to an egui TextureHandle (cached).
    pub fn get_or_render(&mut self, ctx: &Context, expr: &str, is_display: bool) -> Option<TextureHandle> {
        let clean_expr = expr.trim();
        if clean_expr.is_empty() {
            return None;
        }

        let key = format!("{}:{}", if is_display { "disp" } else { "inline" }, clean_expr);
        if let Some(tex) = self.textures.get(&key) {
            return Some(tex.clone());
        }

        let font = self.font.as_ref()?;
        let mut opts = PngOptions::new();
        opts.display = is_display;
        opts.dpi = Dim::from_i64(144);
        opts.color = Color::rgb(230, 240, 255);
        opts.font_size_pt = if is_display { Dim::from_i64(15) } else { Dim::from_i64(13) };

        match latex_to_png(clean_expr, font, &opts) {
            Ok(png_bytes) => {
                if let Ok(img) = image::load_from_memory(&png_bytes) {
                    let rgba = img.into_rgba8();
                    let size = [rgba.width() as usize, rgba.height() as usize];
                    let color_img = ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                    let tex = ctx.load_texture(
                        format!("latex_{:x}", seahash(clean_expr.as_bytes())),
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

fn seahash(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0x123456789abcdef0;
    for b in bytes {
        h = h.wrapping_mul(31).wrapping_add(*b as u64);
    }
    h
}

fn is_simple_atom(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return false;
    }
    !trimmed.contains(|c: char| {
        c.is_whitespace()
            || c == '+'
            || c == '-'
            || c == '−'
            || c == '*'
            || c == '·'
            || c == '×'
            || c == '/'
            || c == '='
            || c == '<'
            || c == '>'
            || c == '≤'
            || c == '≥'
            || c == ','
            || c == ';'
    })
}

/// Helper to parse matching braces `{...}`
fn parse_single_brace(s: &str) -> Option<(String, usize)> {
    let start_idx = s.find('{')?;
    let bytes = s.as_bytes();
    let mut depth = 0;
    let mut inner_start = 0;

    for (i, &b) in bytes.iter().enumerate().skip(start_idx) {
        if b == b'{' {
            if depth == 0 {
                inner_start = i + 1;
            }
            depth += 1;
        } else if b == b'}' {
            depth -= 1;
            if depth == 0 {
                let inner = s[inner_start..i].to_string();
                return Some((inner, i + 1));
            }
        }
    }
    None
}

/// Helper to parse two consecutive braces `{...}{...}` for `\frac{A}{B}`
fn parse_two_braces(s: &str) -> Option<(String, String, usize)> {
    let (first, first_end) = parse_single_brace(s)?;
    let remainder = &s[first_end..];
    let (second, second_end) = parse_single_brace(remainder)?;
    Some((first, second, first_end + second_end))
}

/// Converts a subscript character or token to Unicode subscript
fn to_subscript_char(ch: char) -> Option<char> {
    match ch {
        '0' => Some('₀'),
        '1' => Some('₁'),
        '2' => Some('₂'),
        '3' => Some('₃'),
        '4' => Some('₄'),
        '5' => Some('₅'),
        '6' => Some('₆'),
        '7' => Some('₇'),
        '8' => Some('₈'),
        '9' => Some('⁹'),
        '+' => Some('₊'),
        '-' => Some('₋'),
        '=' => Some('₌'),
        '(' => Some('₍'),
        ')' => Some('₎'),
        'a' => Some('ₐ'),
        'e' => Some('ₑ'),
        'h' => Some('ₕ'),
        'i' => Some('ᵢ'),
        'j' => Some('ⱼ'),
        'k' => Some('ₖ'),
        'l' => Some('ₗ'),
        'm' => Some('ₘ'),
        'n' => Some('ₙ'),
        'o' => Some('ₒ'),
        'p' => Some('ₚ'),
        'r' => Some('ᵣ'),
        's' => Some('ₛ'),
        't' => Some('ₜ'),
        'u' => Some('ᵤ'),
        'v' => Some('ᵥ'),
        'x' => Some('ₓ'),
        _ => None,
    }
}

/// Converts a superscript character to Unicode superscript
fn to_superscript_char(ch: char) -> Option<char> {
    match ch {
        '0' => Some('⁰'),
        '1' => Some('¹'),
        '2' => Some('²'),
        '3' => Some('³'),
        '4' => Some('⁴'),
        '5' => Some('⁵'),
        '6' => Some('⁶'),
        '7' => Some('⁷'),
        '8' => Some('⁸'),
        '9' => Some('⁹'),
        '+' => Some('⁺'),
        '-' => Some('⁻'),
        '=' => Some('⁼'),
        '(' => Some('⁽'),
        ')' => Some('⁾'),
        'a' => Some('ᵃ'),
        'b' => Some('ᵇ'),
        'c' => Some('ᶜ'),
        'd' => Some('ᵈ'),
        'e' => Some('ᵉ'),
        'f' => Some('ᶠ'),
        'g' => Some('ᵍ'),
        'h' => Some('ʰ'),
        'i' => Some('ⁱ'),
        'j' => Some('ʲ'),
        'k' => Some('ᵏ'),
        'l' => Some('ˡ'),
        'm' => Some('ᵐ'),
        'n' => Some('ⁿ'),
        'o' => Some('ᵒ'),
        'p' => Some('ᵖ'),
        'r' => Some('ʳ'),
        's' => Some('ˢ'),
        't' => Some('ᵗ'),
        'u' => Some('ᵘ'),
        'v' => Some('ᵛ'),
        'w' => Some('ʷ'),
        'x' => Some('ˣ'),
        'y' => Some('ʸ'),
        'z' => Some('ᶻ'),
        _ => None,
    }
}

fn convert_subscripts(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '_' && i + 1 < chars.len() {
            if chars[i + 1] == '{' {
                // Braced subscript _{...}
                let mut j = i + 2;
                let mut inner = String::new();
                while j < chars.len() && chars[j] != '}' {
                    inner.push(chars[j]);
                    j += 1;
                }
                if j < chars.len() && chars[j] == '}' {
                    let mut all_sub = true;
                    let mut sub_buf = String::new();
                    for c in inner.chars() {
                        if let Some(sub) = to_subscript_char(c) {
                            sub_buf.push(sub);
                        } else {
                            all_sub = false;
                            break;
                        }
                    }
                    if all_sub && !sub_buf.is_empty() {
                        out.push_str(&sub_buf);
                    } else {
                        out.push_str(&format!("_({})", inner));
                    }
                    i = j + 1;
                    continue;
                }
            } else if let Some(sub) = to_subscript_char(chars[i + 1]) {
                out.push(sub);
                i += 2;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn convert_superscripts(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '^' && i + 1 < chars.len() {
            if chars[i + 1] == '{' {
                // Braced superscript ^{...}
                let mut j = i + 2;
                let mut inner = String::new();
                while j < chars.len() && chars[j] != '}' {
                    inner.push(chars[j]);
                    j += 1;
                }
                if j < chars.len() && chars[j] == '}' {
                    let mut all_sup = true;
                    let mut sup_buf = String::new();
                    for c in inner.chars() {
                        if let Some(sup) = to_superscript_char(c) {
                            sup_buf.push(sup);
                        } else {
                            all_sup = false;
                            break;
                        }
                    }
                    if all_sup && !sup_buf.is_empty() {
                        out.push_str(&sup_buf);
                    } else {
                        out.push_str(&format!("^({})", inner));
                    }
                    i = j + 1;
                    continue;
                }
            } else if let Some(sup) = to_superscript_char(chars[i + 1]) {
                out.push(sup);
                i += 2;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Converts LaTeX math commands and symbols to clean, readable Unicode math.
pub fn latex_math_to_unicode(latex: &str) -> String {
    let mut s = latex.trim().to_string();

    // Remove \left and \right
    s = s.replace(r"\left", "").replace(r"\right", "");

    // Escaped braces
    s = s.replace(r"\{", "{").replace(r"\}", "}");

    // Spacing macros
    s = s.replace(r"\,", " ")
        .replace(r"\;", " ")
        .replace(r"\quad", "  ")
        .replace(r"\qquad", "   ")
        .replace(r"\!", "")
        .replace(r"\:", " ")
        .replace(r"\ ", " ")
        .replace(r"\space", " ")
        .replace("~", " ");

    // Fractions: \frac{A}{B}
    while let Some(frac_idx) = s.find(r"\frac") {
        if let Some((num, den, end_idx)) = parse_two_braces(&s[frac_idx + 5..]) {
            let num_clean = latex_math_to_unicode(&num);
            let den_clean = latex_math_to_unicode(&den);
            let num_part = if is_simple_atom(&num_clean) {
                num_clean
            } else {
                format!("({})", num_clean)
            };
            let den_part = if is_simple_atom(&den_clean) {
                den_clean
            } else {
                format!("({})", den_clean)
            };
            let rep = format!("{}/{}", num_part, den_part);
            s.replace_range(frac_idx..frac_idx + 5 + end_idx, &rep);
        } else {
            break;
        }
    }

    // Square root: \sqrt{arg}
    while let Some(sqrt_idx) = s.find(r"\sqrt") {
        if let Some((arg, end_idx)) = parse_single_brace(&s[sqrt_idx + 5..]) {
            let arg_clean = latex_math_to_unicode(&arg);
            s.replace_range(sqrt_idx..sqrt_idx + 5 + end_idx, &format!("√({})", arg_clean));
        } else {
            break;
        }
    }

    // Text & formatting macros
    for m in &[r"\text", r"\mathrm", r"\mathbf", r"\mathit", r"\boldsymbol", r"\textbf", r"\operatorname"] {
        while let Some(idx) = s.find(m) {
            let plen = m.len();
            if let Some((arg, end_idx)) = parse_single_brace(&s[idx + plen..]) {
                s.replace_range(idx..idx + plen + end_idx, &arg);
            } else {
                break;
            }
        }
    }

    // Standard Math Functions
    for func in &[
        r"\sin", r"\cos", r"\tan", r"\arcsin", r"\arccos", r"\arctan",
        r"\sinh", r"\cosh", r"\tanh", r"\ln", r"\log", r"\exp",
        r"\det", r"\gcd", r"\deg", r"\lim", r"\max", r"\min", r"\sup", r"\inf",
    ] {
        s = s.replace(func, &func[1..]);
    }

    // Blackboard bold sets
    s = s.replace(r"\mathbb{N}", "ℕ")
        .replace(r"\mathbb{R}", "ℝ")
        .replace(r"\mathbb{Z}", "ℤ")
        .replace(r"\mathbb{Q}", "ℚ")
        .replace(r"\mathbb{C}", "ℂ")
        .replace(r"\mathbb{P}", "ℙ")
        .replace(r"\mathbb{K}", "𝕂")
        .replace(r"\mathbb{H}", "ℍ");

    // Arrows & Logic (LONGER PATTERNS FIRST!)
    s = s.replace(r"\Longleftrightarrow", " ⟺ ")
        .replace(r"\Longrightarrow", " ⟹ ")
        .replace(r"\longleftarrow", " ⟵ ")
        .replace(r"\longrightarrow", " ⟶ ")
        .replace(r"\implies", " ⟹ ")
        .replace(r"\iff", " ⟺ ")
        .replace(r"\rightarrow", " → ")
        .replace(r"\leftarrow", " ← ")
        .replace(r"\Rightarrow", " ⇒ ")
        .replace(r"\Leftarrow", " ⇐ ")
        .replace(r"\Leftrightarrow", " ⇔ ")
        .replace(r"\mapsto", " ↦ ")
        .replace(r"\to", " → ");

    // Sets, Relations & Operators (LONGER PATTERNS FIRST!)
    s = s.replace(r"\subseteq", "⊆")
        .replace(r"\supseteq", "⊇")
        .replace(r"\subset", "⊂")
        .replace(r"\supset", "⊃")
        .replace(r"\notin", " ∉ ")
        .replace(r"\infty", "∞")
        .replace(r"\oint", "∮")
        .replace(r"\int", "∫")
        .replace(r"\in", " ∈ ")
        .replace(r"\leq", "≤")
        .replace(r"\le", "≤")
        .replace(r"\geq", "≥")
        .replace(r"\ge", "≥")
        .replace(r"\neq", "≠")
        .replace(r"\ne", "≠")
        .replace(r"\simeq", "≃")
        .replace(r"\approx", "≈")
        .replace(r"\equiv", "≡")
        .replace(r"\cong", "≅")
        .replace(r"\sim", "∼")
        .replace(r"\ldots", "…")
        .replace(r"\cdots", "…")
        .replace(r"\ddots", "⋱")
        .replace(r"\vdots", "⋮")
        .replace(r"\dots", "…")
        .replace(r"\setminus", "∖")
        .replace(r"\forall", "∀")
        .replace(r"\exists", "∃")
        .replace(r"\nexists", "∄")
        .replace(r"\emptyset", "∅")
        .replace(r"\parallel", "∥")
        .replace(r"\perp", "⊥")
        .replace(r"\mid", " | ")
        .replace(r"\pm", "±")
        .replace(r"\mp", "∓")
        .replace(r"\times", "×")
        .replace(r"\cdot", "·")
        .replace(r"\circ", " ∘ ")
        .replace(r"\comp", " ∘ ")
        .replace(r"\bullet", " • ")
        .replace(r"\star", " ⋆ ")
        .replace(r"\div", "÷")
        .replace(r"\ast", "∗")
        .replace(r"\cup", "∪")
        .replace(r"\cap", "∩")
        .replace(r"\sum", "∑")
        .replace(r"\prod", "∏")
        .replace(r"\partial", "∂")
        .replace(r"\nabla", "∇");

    // Greek letters (variants first)
    s = s.replace(r"\varepsilon", "ε")
        .replace(r"\epsilon", "ε")
        .replace(r"\vartheta", "ϑ")
        .replace(r"\theta", "θ")
        .replace(r"\varrho", "ϱ")
        .replace(r"\rho", "ρ")
        .replace(r"\varsigma", "ς")
        .replace(r"\sigma", "σ")
        .replace(r"\varphi", "ϕ")
        .replace(r"\phi", "φ")
        .replace(r"\varpi", "ϖ")
        .replace(r"\pi", "π")
        .replace(r"\alpha", "α")
        .replace(r"\beta", "β")
        .replace(r"\gamma", "γ")
        .replace(r"\delta", "δ")
        .replace(r"\zeta", "ζ")
        .replace(r"\eta", "η")
        .replace(r"\iota", "ι")
        .replace(r"\kappa", "κ")
        .replace(r"\lambda", "λ")
        .replace(r"\mu", "μ")
        .replace(r"\nu", "ν")
        .replace(r"\xi", "ξ")
        .replace(r"\tau", "τ")
        .replace(r"\upsilon", "υ")
        .replace(r"\chi", "χ")
        .replace(r"\psi", "ψ")
        .replace(r"\omega", "ω")
        .replace(r"\Gamma", "Γ")
        .replace(r"\Delta", "Δ")
        .replace(r"\Theta", "Θ")
        .replace(r"\Lambda", "Λ")
        .replace(r"\Xi", "Ξ")
        .replace(r"\Pi", "Π")
        .replace(r"\Sigma", "Σ")
        .replace(r"\Upsilon", "Υ")
        .replace(r"\Phi", "Φ")
        .replace(r"\Psi", "Ψ")
        .replace(r"\Omega", "Ω");

    // Convert subscripts and superscripts
    s = convert_subscripts(&s);
    s = convert_superscripts(&s);

    // Spacing and punctuation cleanup
    s = s.replace(", ;", ", ")
        .replace(",;", ", ")
        .replace(", ,", ", ")
        .replace(",,", ",");

    // Clean multiple consecutive spaces
    while s.contains("  ") {
        s = s.replace("  ", " ");
    }

    s.trim().to_string()
}

/// Preprocesses text by converting inline math `$ ... $` and `\( ... \)` into formatted Unicode text.
pub fn preprocess_inline_math(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut i = 0;
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();

    while i < len {
        // Check for display math $$ or \[ — preserve as-is for block renderer
        if i + 1 < len && chars[i] == '$' && chars[i + 1] == '$' {
            result.push('$');
            result.push('$');
            i += 2;
            continue;
        }
        if i + 1 < len && chars[i] == '\\' && chars[i + 1] == '[' {
            result.push('\\');
            result.push('[');
            i += 2;
            continue;
        }

        // Check for inline math $...$
        if chars[i] == '$' {
            // Find closing $
            let mut j = i + 1;
            let mut found_end = false;
            while j < len {
                if chars[j] == '$' && (j == 0 || chars[j - 1] != '\\') {
                    found_end = true;
                    break;
                }
                if chars[j] == '\n' {
                    // Inline math does not cross line breaks
                    break;
                }
                j += 1;
            }

            if found_end && j > i + 1 {
                let expr: String = chars[i + 1..j].iter().collect();
                let clean_expr = expr.trim();
                if !clean_expr.is_empty() {
                    let formatted = latex_math_to_unicode(clean_expr);
                    // If it's a single variable like "f", "g", "x", or Greek "α", italicize it in markdown
                    if formatted.chars().count() == 1 && formatted.chars().all(|c| c.is_alphabetic()) {
                        result.push('*');
                        result.push_str(&formatted);
                        result.push('*');
                    } else {
                        result.push_str(&formatted);
                    }
                    i = j + 1;
                    continue;
                }
            }
        }

        // Check for \( ... \)
        if i + 1 < len && chars[i] == '\\' && chars[i + 1] == '(' {
            let mut j = i + 2;
            let mut found_end = false;
            while j + 1 < len {
                if chars[j] == '\\' && chars[j + 1] == ')' {
                    found_end = true;
                    break;
                }
                if chars[j] == '\n' {
                    break;
                }
                j += 1;
            }

            if found_end && j > i + 2 {
                let expr: String = chars[i + 2..j].iter().collect();
                let clean_expr = expr.trim();
                if !clean_expr.is_empty() {
                    let formatted = latex_math_to_unicode(clean_expr);
                    if formatted.chars().count() == 1 && formatted.chars().all(|c| c.is_alphabetic()) {
                        result.push('*');
                        result.push_str(&formatted);
                        result.push('*');
                    } else {
                        result.push_str(&formatted);
                    }
                    i = j + 2;
                    continue;
                }
            }
        }

        result.push(chars[i]);
        i += 1;
    }

    result
}

enum Segment<'a> {
    Markdown(&'a str),
    LatexDisplay(&'a str),
}

fn split_display_math(content: &str) -> Vec<Segment<'_>> {
    let mut segments = Vec::new();
    let mut cur = 0;
    let len = content.len();

    while cur < len {
        let dollar_pos = content[cur..].find("$$");
        let bracket_pos = content[cur..].find("\\[");

        let next_delim = match (dollar_pos, bracket_pos) {
            (Some(d), Some(b)) => {
                if d <= b {
                    Some((d, "$$", "$$"))
                } else {
                    Some((b, "\\[", "\\]"))
                }
            }
            (Some(d), None) => Some((d, "$$", "$$")),
            (None, Some(b)) => Some((b, "\\[", "\\]")),
            (None, None) => None,
        };

        if let Some((rel_start, open_delim, close_delim)) = next_delim {
            let start = cur + rel_start;
            if start > cur {
                segments.push(Segment::Markdown(&content[cur..start]));
            }
            let formula_start = start + open_delim.len();
            if let Some(rel_end) = content[formula_start..].find(close_delim) {
                let end = formula_start + rel_end;
                segments.push(Segment::LatexDisplay(&content[formula_start..end]));
                cur = end + close_delim.len();
            } else {
                segments.push(Segment::Markdown(&content[start..]));
                break;
            }
        } else {
            segments.push(Segment::Markdown(&content[cur..]));
            break;
        }
    }

    segments
}

/// Renders a message content stream with seamless, natural LaTeX math.
///
/// 1. Inline math (`$...$` and `\(...\)`) is converted into clean Unicode math typography directly within sentences.
/// 2. Display equations (`$$...$$` and `\[...\]`) are centered naturally with proportional scaling and NO awkward borders or heavy dark cards.
pub fn render_content_with_latex(
    ui: &mut Ui,
    ctx: &Context,
    latex_cache: &mut LatexCache,
    markdown_cache: &mut egui_commonmark::CommonMarkCache,
    content: &str,
) {
    let has_display = content.contains("$$") || content.contains("\\[");

    if !has_display {
        // Fast path: Preprocess inline math and render as standard markdown
        let processed = preprocess_inline_math(content);
        egui_commonmark::CommonMarkViewer::new().show(ui, markdown_cache, &processed);
        return;
    }

    let segments = split_display_math(content);
    for seg in segments {
        match seg {
            Segment::Markdown(md) => {
                if !md.trim().is_empty() {
                    let processed = preprocess_inline_math(md);
                    egui_commonmark::CommonMarkViewer::new().show(ui, markdown_cache, &processed);
                }
            }
            Segment::LatexDisplay(expr) => {
                let clean_expr = expr.trim();
                if clean_expr.is_empty() {
                    continue;
                }

                ui.add_space(8.0);

                // Try rendering crisp LaTeX formula with proper text-proportional scaling
                if let Some(tex) = latex_cache.get_or_render(ctx, clean_expr, true) {
                    let orig_size = tex.size_vec2();
                    // Proportional scaling: target a natural line height (~26-30pt) instead of gigantic raster
                    let target_h = 28.0_f32;
                    let scale = (target_h / orig_size.y).clamp(0.40, 0.75);
                    let display_size = orig_size * scale;

                    ui.vertical_centered(|ui| {
                        ui.add(Image::new(&tex).fit_to_exact_size(display_size));
                    });
                } else {
                    // Fallback to beautiful centered Unicode mathematical equation
                    let unicode_eq = latex_math_to_unicode(clean_expr);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new(unicode_eq)
                                .size(15.5)
                                .color(Color32::from_rgb(225, 238, 255))
                                .strong(),
                        );
                    });
                }

                ui.add_space(8.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_latex_to_unicode() {
        assert_eq!(
            latex_math_to_unicode(r"\mathbb{N} = \{0, 1, 2, \dots\}"),
            "ℕ = {0, 1, 2, …}"
        );
        assert_eq!(
            latex_math_to_unicode(r"g(1) = \frac{1-1}{2} = 0 \in \mathbb{N}"),
            "g(1) = (1-1)/2 = 0 ∈ ℕ"
        );
        assert_eq!(
            latex_math_to_unicode(r"f : \mathbb{N} \to \mathbb{N}, ; k \mapsto 2k"),
            "f : ℕ → ℕ, k ↦ 2k"
        );
        assert_eq!(
            latex_math_to_unicode(r"k_1, k_2 \in \mathbb{N}"),
            "k₁, k₂ ∈ ℕ"
        );
        assert_eq!(
            latex_math_to_unicode(r"f(k_1) = f(k_2)"),
            "f(k₁) = f(k₂)"
        );
        assert_eq!(
            latex_math_to_unicode(r"2k_1 = 2k_2 \implies k_1 = k_2"),
            "2k₁ = 2k₂ ⟹ k₁ = k₂"
        );
    }

    #[test]
    fn test_preprocess_inline_math() {
        let input = "Voici la résolution (avec la convention usuelle $\\mathbb{N} = \\{0, 1, 2, \\dots\\}$, cohérente avec $g(1) = \\frac{1-1}{2} = 0 \\in \\mathbb{N}$).";
        let out = preprocess_inline_math(input);
        assert!(out.contains("ℕ = {0, 1, 2, …}"));
        assert!(out.contains("g(1) = (1-1)/2 = 0 ∈ ℕ"));

        let single_var = "Injectivité de $f$ et de $g$";
        assert_eq!(preprocess_inline_math(single_var), "Injectivité de *f* et de *g*");

        let comp = "Ce n'est pas parce que $g \\circ f$ est injective que $g$ est injective (puisque $g \\circ f = \\mathrm{id}$ est injective, mais $g$ ne l'est pas).";
        let comp_out = preprocess_inline_math(comp);
        assert!(comp_out.contains("g ∘ f est injective que *g* est injective"));
        assert!(comp_out.contains("g ∘ f = id"));
        assert!(comp_out.contains("mais *g* ne l'est pas"));

        let space_test = "composition $g \\ f$ n'implique";
        let space_out = preprocess_inline_math(space_test);
        assert_eq!(space_out, "composition g f n'implique");
    }

    #[test]
    fn test_split_display_math() {
        let text = "Text before\n\\[\nx^2 + y^2 = z^2\n\\]\nMiddle text\n$$\na = b\n$$\nEnd text";
        let segs = split_display_math(text);
        assert_eq!(segs.len(), 5);
    }
}
