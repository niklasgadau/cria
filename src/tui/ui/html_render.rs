// Render Vikunja HTML (descriptions, comments) as styled terminal text

use html2text::render::RichAnnotation;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// Convert HTML to styled lines wrapped at `width`. Plain text is passed through.
pub fn html_to_styled_lines(html: &str, width: usize) -> Vec<Line<'static>> {
    if !html.contains('<') {
        return html.lines().map(|l| Line::from(l.to_string())).collect();
    }
    let tagged = match html2text::from_read_rich(html.as_bytes(), width.max(20)) {
        Ok(lines) => lines,
        Err(_) => return vec![Line::from(html.to_string())],
    };

    tagged
        .iter()
        .map(|line| {
            let parts: Vec<(String, Vec<RichAnnotation>)> = line
                .tagged_strings()
                // Strikeout comes with combining U+0336; the terminal modifier replaces it
                .map(|ts| (ts.s.replace('\u{336}', ""), ts.tag.clone()))
                .collect();
            styled_line(parts)
        })
        .collect()
}

fn is_preformat(tags: &[RichAnnotation]) -> bool {
    tags.iter().any(|t| matches!(t, RichAnnotation::Preformat(_)))
}

fn styled_line(mut parts: Vec<(String, Vec<RichAnnotation>)>) -> Line<'static> {
    if parts.is_empty() {
        return Line::from("");
    }

    // Code block: gutter + muted style for the whole line
    if parts.iter().any(|(_, tags)| is_preformat(tags)) {
        let mut spans = vec![Span::styled("│ ", Style::default().fg(Color::DarkGray))];
        spans.extend(parts.into_iter().map(|(s, _)| Span::styled(s, code_block_style())));
        return Line::from(spans);
    }

    // Heading: html2text prefixes "# ", "## ", ...
    let first = &parts[0].0;
    let hashes = first.chars().take_while(|c| *c == '#').count();
    if hashes > 0 && first[hashes..].starts_with(' ') {
        let level = hashes;
        parts[0].0 = first[hashes + 1..].to_string();
        let style = heading_style(level);
        let mut spans = vec![Span::styled("▍", style)];
        spans.extend(
            parts
                .into_iter()
                .map(|(s, tags)| Span::styled(s, inline_style(&tags).patch(style))),
        );
        return Line::from(spans);
    }

    // List bullets and quotes: rewrite the leading markers
    let mut prefix_spans = Vec::new();
    {
        let first = &mut parts[0].0;
        let indent = first.len() - first.trim_start_matches(' ').len();
        let rest = &first[indent..];
        if let Some(text) = rest.strip_prefix("* ") {
            prefix_spans.push(Span::raw(" ".repeat(indent)));
            prefix_spans.push(Span::styled("• ", Style::default().fg(Color::Blue)));
            *first = text.to_string();
        } else if rest.starts_with("> ") {
            let depth = rest.matches("> ").count();
            prefix_spans.push(Span::raw(" ".repeat(indent)));
            prefix_spans.push(Span::styled("▎ ".repeat(depth), Style::default().fg(Color::DarkGray)));
            *first = rest.replacen("> ", "", depth);
        }
    }

    let mut spans = prefix_spans;
    spans.extend(
        parts
            .into_iter()
            .filter(|(s, _)| !s.is_empty())
            .map(|(s, tags)| Span::styled(s, inline_style(&tags))),
    );
    Line::from(spans)
}

fn heading_style(level: usize) -> Style {
    let color = match level {
        1 => Color::Blue,
        2 => Color::Green,
        3 => Color::Cyan,
        _ => Color::Magenta,
    };
    let style = Style::default().fg(color).add_modifier(Modifier::BOLD);
    if level == 1 {
        style.add_modifier(Modifier::UNDERLINED)
    } else {
        style
    }
}

fn code_block_style() -> Style {
    Style::default().fg(Color::DarkGray)
}

fn inline_style(tags: &[RichAnnotation]) -> Style {
    let mut style = Style::default();
    for tag in tags {
        style = match tag {
            RichAnnotation::Strong => style.add_modifier(Modifier::BOLD),
            RichAnnotation::Emphasis => style.add_modifier(Modifier::ITALIC),
            RichAnnotation::Strikeout => style.add_modifier(Modifier::CROSSED_OUT),
            RichAnnotation::Code => style.fg(Color::Magenta),
            RichAnnotation::Link(_) => style.fg(Color::Blue).add_modifier(Modifier::UNDERLINED),
            RichAnnotation::Image(_) => style.fg(Color::Cyan).add_modifier(Modifier::ITALIC),
            _ => style,
        };
    }
    style
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(line: &Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn plain_text_passes_through() {
        let lines = html_to_styled_lines("one\ntwo", 80);
        assert_eq!(lines.iter().map(text).collect::<Vec<_>>(), vec!["one", "two"]);
    }

    #[test]
    fn heading_drops_hashes() {
        let lines = html_to_styled_lines("<h2>Notes</h2><p>x</p>", 80);
        assert_eq!(text(&lines[0]), "▍Notes");
        assert!(lines[0].spans[1].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn bullets_and_links() {
        let lines = html_to_styled_lines(
            r#"<ul><li>see <a href="https://x.y">docs</a> and <code>vk</code></li></ul>"#,
            80,
        );
        assert_eq!(text(&lines[0]), "• see docs and vk");
        let link = lines[0].spans.iter().find(|s| s.content == "docs").unwrap();
        assert!(link.style.add_modifier.contains(Modifier::UNDERLINED));
        let code = lines[0].spans.iter().find(|s| s.content == "vk").unwrap();
        assert_eq!(code.style.fg, Some(Color::Magenta));
    }

    #[test]
    fn bold_italic_strike() {
        let lines = html_to_styled_lines("<p><strong>b</strong> <em>i</em> <s>s</s></p>", 80);
        let get = |c: &str| lines[0].spans.iter().find(|s| s.content == c).unwrap().style;
        assert!(get("b").add_modifier.contains(Modifier::BOLD));
        assert!(get("i").add_modifier.contains(Modifier::ITALIC));
        assert!(get("s").add_modifier.contains(Modifier::CROSSED_OUT));
    }

    #[test]
    fn code_block_gets_gutter() {
        let lines = html_to_styled_lines("<pre><code># comment\nls -la</code></pre>", 80);
        assert_eq!(text(&lines[0]), "│ # comment");
        assert_eq!(text(&lines[1]), "│ ls -la");
    }
}
