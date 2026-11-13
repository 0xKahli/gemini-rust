use std::fs::File;
use std::io::{Seek, Write};
use std::path::Path;

/// Escapes text for PDF string literals: `\` -> `\\`, `(` -> `\(`, `)` -> `\)`
fn escape_pdf_str(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            // Standard PDF Type 1 fonts only support WinAnsiEncoding / ASCII
            c if c.is_ascii() => out.push(c),
            '✦' => out.push_str("*"),
            '•' => out.push_str("*"),
            '—' | '–' => out.push('-'),
            '“' | '”' => out.push('"'),
            '‘' | '’' => out.push('\''),
            '…' => out.push_str("..."),
            _ => out.push('?'),
        }
    }
    out
}

/// Word-wraps text into lines that fit within max_chars.
fn wrap_text(text: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for raw_line in text.lines() {
        if raw_line.is_empty() {
            lines.push(String::new());
            continue;
        }

        let words: Vec<&str> = raw_line.split_whitespace().collect();
        if words.is_empty() {
            lines.push(String::new());
            continue;
        }

        let mut current_line = String::new();
        for word in words {
            if current_line.is_empty() {
                current_line.push_str(word);
            } else if current_line.len() + 1 + word.len() <= max_chars {
                current_line.push(' ');
                current_line.push_str(word);
            } else {
                lines.push(current_line);
                current_line = word.to_string();
            }
        }
        if !current_line.is_empty() {
            lines.push(current_line);
        }
    }
    lines
}

pub struct PdfExporter;

impl PdfExporter {
    /// Exports the markdown answer to a publication-quality multi-page PDF document.
    pub fn export_to_file(content: &str, path: &Path) -> std::io::Result<()> {
        let page_width = 595.28_f32;  // A4 width
        let page_height = 841.89_f32; // A4 height
        let margin_x = 50.0_f32;
        let margin_top = 50.0_f32;
        let margin_bottom = 50.0_f32;
        let content_width = page_width - (margin_x * 2.0);

        let mut pages_stream: Vec<String> = Vec::new();
        let mut cur_stream = String::new();
        let mut cur_y = page_height - margin_top;

        let start_page = |cur_stream: &mut String, cur_y: &mut f32| {
            cur_stream.clear();
            *cur_y = page_height - margin_top;

            // Header banner
            cur_stream.push_str(&format!(
                "BT /F2 16 Tf 0.1 0.45 0.95 rg {} {} Td (Rusty Gemini) Tj ET\n",
                margin_x, *cur_y
            ));
            cur_stream.push_str(&format!(
                "BT /F1 10 Tf 0.5 0.5 0.55 rg {} {} Td ( - AI Response Document) Tj ET\n",
                margin_x + 115.0, *cur_y
            ));
            *cur_y -= 14.0;

            let date_str = chrono::Local::now().format("%B %d, %Y - %H:%M").to_string();
            cur_stream.push_str(&format!(
                "BT /F1 9 Tf 0.5 0.55 0.6 rg {} {} Td (Exported on: {}) Tj ET\n",
                margin_x, *cur_y, escape_pdf_str(&date_str)
            ));
            *cur_y -= 12.0;

            // Horizontal dividing rule
            cur_stream.push_str(&format!(
                "0.8 0.85 0.9 RG 1 w {} {} m {} {} l S\n",
                margin_x, *cur_y, margin_x + content_width, *cur_y
            ));
            *cur_y -= 22.0;
        };

        start_page(&mut cur_stream, &mut cur_y);

        let mut in_code_block = false;

        for line in content.lines() {
            // Check if page overflow
            if cur_y < margin_bottom + 50.0 {
                pages_stream.push(cur_stream.clone());
                start_page(&mut cur_stream, &mut cur_y);
            }

            let trimmed = line.trim();

            // Code block delimiter
            if trimmed.starts_with("```") {
                in_code_block = !in_code_block;
                cur_y -= 4.0;
                continue;
            }

            if in_code_block {
                // Courier 9pt for code lines
                let wrapped = wrap_text(line, 80);
                for sub_line in wrapped {
                    if cur_y < margin_bottom + 40.0 {
                        pages_stream.push(cur_stream.clone());
                        start_page(&mut cur_stream, &mut cur_y);
                    }
                    let esc = escape_pdf_str(&sub_line);
                    cur_stream.push_str(&format!(
                        "BT /F3 9.5 Tf 0.15 0.15 0.2 rg {} {} Td ({}) Tj ET\n",
                        margin_x + 10.0, cur_y, esc
                    ));
                    cur_y -= 12.5;
                }
            } else if trimmed.starts_with("# ") {
                // Heading 1
                cur_y -= 8.0;
                let text = escape_pdf_str(trimmed.trim_start_matches("# ").trim());
                cur_stream.push_str(&format!(
                    "BT /F2 15 Tf 0.05 0.05 0.1 rg {} {} Td ({}) Tj ET\n",
                    margin_x, cur_y, text
                ));
                cur_y -= 18.0;
            } else if trimmed.starts_with("## ") {
                // Heading 2
                cur_y -= 6.0;
                let text = escape_pdf_str(trimmed.trim_start_matches("## ").trim());
                cur_stream.push_str(&format!(
                    "BT /F2 13 Tf 0.1 0.2 0.4 rg {} {} Td ({}) Tj ET\n",
                    margin_x, cur_y, text
                ));
                cur_y -= 16.0;
            } else if trimmed.starts_with("### ") {
                // Heading 3
                cur_y -= 4.0;
                let text = escape_pdf_str(trimmed.trim_start_matches("### ").trim());
                cur_stream.push_str(&format!(
                    "BT /F2 11.5 Tf 0.15 0.15 0.25 rg {} {} Td ({}) Tj ET\n",
                    margin_x, cur_y, text
                ));
                cur_y -= 14.5;
            } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
                // Bullet points
                let bullet_text = trimmed[2..].trim();
                let wrapped = wrap_text(bullet_text, 74);
                for (i, sub_line) in wrapped.into_iter().enumerate() {
                    if cur_y < margin_bottom + 40.0 {
                        pages_stream.push(cur_stream.clone());
                        start_page(&mut cur_stream, &mut cur_y);
                    }
                    let esc = escape_pdf_str(&sub_line);
                    if i == 0 {
                        cur_stream.push_str(&format!(
                            "BT /F2 10.5 Tf 0.2 0.45 0.9 rg {} {} Td (*) Tj ET\n",
                            margin_x + 5.0, cur_y
                        ));
                        cur_stream.push_str(&format!(
                            "BT /F1 10.5 Tf 0.15 0.15 0.15 rg {} {} Td ({}) Tj ET\n",
                            margin_x + 18.0, cur_y, esc
                        ));
                    } else {
                        cur_stream.push_str(&format!(
                            "BT /F1 10.5 Tf 0.15 0.15 0.15 rg {} {} Td ({}) Tj ET\n",
                            margin_x + 18.0, cur_y, esc
                        ));
                    }
                    cur_y -= 13.5;
                }
            } else if trimmed.is_empty() {
                cur_y -= 7.0;
            } else {
                // Standard paragraph
                let wrapped = wrap_text(line, 76);
                for sub_line in wrapped {
                    if cur_y < margin_bottom + 40.0 {
                        pages_stream.push(cur_stream.clone());
                        start_page(&mut cur_stream, &mut cur_y);
                    }
                    let esc = escape_pdf_str(&sub_line);
                    cur_stream.push_str(&format!(
                        "BT /F1 10.5 Tf 0.12 0.12 0.12 rg {} {} Td ({}) Tj ET\n",
                        margin_x, cur_y, esc
                    ));
                    cur_y -= 14.0;
                }
            }
        }

        pages_stream.push(cur_stream);

        // Add Footers with page numbering
        let total_pages = pages_stream.len();
        for (page_idx, stream) in pages_stream.iter_mut().enumerate() {
            let footer_y = margin_bottom - 15.0;
            // Dividing line above footer
            stream.push_str(&format!(
                "0.85 0.85 0.88 RG 0.5 w {} {} m {} {} l S\n",
                margin_x, footer_y + 15.0, margin_x + content_width, footer_y + 15.0
            ));
            // Footer text
            stream.push_str(&format!(
                "BT /F1 8.5 Tf 0.5 0.5 0.55 rg {} {} Td (Generated with Rusty Gemini - https://rusty-gemini.vercel.app) Tj ET\n",
                margin_x, footer_y
            ));
            let page_str = format!("Page {} of {}", page_idx + 1, total_pages);
            stream.push_str(&format!(
                "BT /F1 8.5 Tf 0.5 0.5 0.55 rg {} {} Td ({}) Tj ET\n",
                margin_x + content_width - 60.0, footer_y, escape_pdf_str(&page_str)
            ));
        }

        // Assemble PDF file structure
        let mut file = File::create(path)?;
        let mut offsets = Vec::new();

        writeln!(file, "%PDF-1.4")?;
        writeln!(file, "%âãÏÓ")?;

        // Object 1: Catalog
        offsets.push(file.stream_position()?);
        writeln!(file, "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj")?;

        // Object 2: Pages Root
        let kids_refs: Vec<String> = (0..total_pages)
            .map(|i| format!("{} 0 R", 3 + (i * 2)))
            .collect();
        offsets.push(file.stream_position()?);
        writeln!(
            file,
            "2 0 obj\n<< /Type /Pages /Kids [{}] /Count {} >>\nendobj",
            kids_refs.join(" "),
            total_pages
        )?;

        // Font objects
        let font_base_obj = 3 + (total_pages * 2);
        // Page objects & Content Streams
        for (i, stream) in pages_stream.iter().enumerate() {
            let page_obj_id = 3 + (i * 2);
            let content_obj_id = page_obj_id + 1;

            // Page Object
            offsets.push(file.stream_position()?);
            writeln!(
                file,
                "{} 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.2} {:.2}] /Contents {} 0 R /Resources << /Font << /F1 {} 0 R /F2 {} 0 R /F3 {} 0 R >> >> >>\nendobj",
                page_obj_id, page_width, page_height, content_obj_id,
                font_base_obj, font_base_obj + 1, font_base_obj + 2
            )?;

            // Content Stream Object
            offsets.push(file.stream_position()?);
            writeln!(
                file,
                "{} 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj",
                content_obj_id,
                stream.len(),
                stream
            )?;
        }

        // Fonts
        // F1: Helvetica
        offsets.push(file.stream_position()?);
        writeln!(
            file,
            "{} 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>\nendobj",
            font_base_obj
        )?;
        // F2: Helvetica-Bold
        offsets.push(file.stream_position()?);
        writeln!(
            file,
            "{} 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>\nendobj",
            font_base_obj + 1
        )?;
        // F3: Courier
        offsets.push(file.stream_position()?);
        writeln!(
            file,
            "{} 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Courier /Encoding /WinAnsiEncoding >>\nendobj",
            font_base_obj + 2
        )?;

        // Cross-Reference Table
        let xref_pos = file.stream_position()?;
        writeln!(file, "xref")?;
        writeln!(file, "0 {}", offsets.len() + 1)?;
        writeln!(file, "0000000000 65535 f ")?;
        for off in &offsets {
            writeln!(file, "{:010} 00000 n ", off)?;
        }

        // Trailer
        writeln!(
            file,
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF",
            offsets.len() + 1,
            xref_pos
        )?;

        Ok(())
    }
}
