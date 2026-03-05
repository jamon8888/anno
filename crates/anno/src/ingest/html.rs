//! HTML-to-text conversion utilities.
//!
//! Pure `&str -> String` transforms with no I/O. These are text normalization
//! functions, not format converters: they take an HTML string and return plain
//! text suitable for NER extraction.

use once_cell::sync::Lazy;
use regex::Regex;

/// Matches Wikipedia-style reference markers: [1], [2], [edit], [citation needed], etc.
/// Also matches bare `edit]` fragments (without opening bracket) that survive
/// HTML `<span>` tag processing on some Wikipedia pages.
static WIKI_REF_BRACKET: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\[(\d+|edit|citation needed)\]|\bedit\]").unwrap());

/// Strip HTML tags and decode entities, returning clean text suitable for NER.
///
/// Removes script/style/nav/header/footer/aside content. Also strips
/// Wikipedia/MediaWiki-specific structural elements (TOC, references,
/// citation lists, navigation boxes).
pub fn strip_html_to_text(html: &str) -> String {
    _strip_html_to_text_impl(html)
}

/// Detect whether content looks like HTML (has tags near the start).
pub fn looks_like_html(content: &str) -> bool {
    let end = content.floor_char_boundary(1024);
    let prefix = &content[..end];
    let trimmed = prefix.trim_start();
    trimmed.starts_with("<!")
        || trimmed.starts_with("<html")
        || trimmed.starts_with("<HTML")
        || trimmed.starts_with("<?xml")
        || (trimmed.contains("<head") && trimmed.contains("<body"))
        || (trimmed.starts_with('<') && trimmed.contains("</"))
}

fn _strip_html_to_text_impl(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;
    // Nesting depth for semantic tags whose content should be skipped.
    let mut skip_depth: u32 = 0;
    // Separate counter for wiki/MediaWiki-specific skip sections so their
    // closing tags don't interfere with the semantic skip_tags depth.
    let mut wiki_skip_depth: u32 = 0;
    let mut chars = html.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '<' => {
                in_tag = true;
                // Check for script/style tags
                let mut tag_buffer = String::new();
                tag_buffer.push('<');
                let mut tag_name = String::new();
                let mut in_tag_name = true;

                while let Some(&next_ch) = chars.peek() {
                    if next_ch == '>' {
                        chars.next();
                        tag_buffer.push('>');
                        let tag_lower = tag_name.to_lowercase();
                        if tag_lower == "script" || tag_lower.starts_with("script ") {
                            in_script = true;
                        } else if tag_lower == "/script" || tag_lower.starts_with("/script ") {
                            in_script = false;
                        } else if tag_lower == "style" || tag_lower.starts_with("style ") {
                            in_style = true;
                        } else if tag_lower == "/style" || tag_lower.starts_with("/style ") {
                            in_style = false;
                        }
                        // Semantic HTML tags: skip nav, header, footer,
                        // aside, menu, form, select, noscript content.
                        // Also skip <div> with role="navigation"/role="banner"/
                        // role="contentinfo" (ARIA landmark roles common on
                        // news sites).
                        let skip_tags: &[&str] = &[
                            "head",
                            "nav",
                            "header",
                            "footer",
                            "aside",
                            "menu",
                            "noscript",
                            "form",
                            "select",
                            "figcaption",
                        ];

                        // Skip Wikipedia/MediaWiki-specific structural elements:
                        // - div with id/class containing "toc", "references",
                        //   "reflist", "catlinks", "mw-panel", "mw-navigation",
                        //   "sidebar", "siteSub", "contentSub", "jump-to-nav",
                        //   "external", "see-also", "further-reading", "navbox"
                        // - ol/ul with class "references"
                        // These carry navigation, citation metadata, and
                        // boilerplate that overwhelm NER backends.
                        let tag_lower_full = format!(
                            "{} {}",
                            tag_name.to_lowercase(),
                            tag_buffer[1..].to_lowercase()
                        );
                        let wiki_skip_ids: &[&str] = &[
                            "toc",
                            "references",
                            "reflist",
                            "catlinks",
                            "mw-panel",
                            "mw-navigation",
                            "sidebar",
                            "sitesub",
                            "contentsub",
                            "jump-to-nav",
                            "navbox",
                            "external",
                            "see-also",
                            "further-reading",
                            "mw-head",
                            "mw-page-base",
                            "mw-head-base",
                            "footer",
                            "printfooter",
                        ];
                        let is_wiki_skip = wiki_skip_ids.iter().any(|id| {
                            tag_lower_full.contains(&format!("id=\"{}\"", id))
                                || tag_lower_full.contains(&format!("id=\"{}\"", id))
                                || tag_lower_full.contains(&format!("class=\"{}", id))
                                || tag_lower_full.contains(id)
                                    && (tag_lower_full.contains("class=")
                                        || tag_lower_full.contains("id="))
                        });
                        if is_wiki_skip
                            && matches!(
                                tag_name.to_lowercase().as_str(),
                                "div" | "ol" | "ul" | "table" | "span" | "section"
                            )
                        {
                            // Track wiki-skip nesting separately so closing
                            // tags don't underflow the semantic skip_tags depth.
                            wiki_skip_depth += 1;
                            skip_depth += 1;
                        }
                        // Handle closing tags for wiki-skip containers.
                        if wiki_skip_depth > 0 {
                            let wiki_close_tags: &[&str] =
                                &["div", "ol", "ul", "table", "span", "section"];
                            for &wtag in wiki_close_tags {
                                if tag_lower == format!("/{}", wtag)
                                    || tag_lower.starts_with(&format!("/{} ", wtag))
                                {
                                    wiki_skip_depth = wiki_skip_depth.saturating_sub(1);
                                    skip_depth = skip_depth.saturating_sub(1);
                                }
                            }
                        }
                        for &stag in skip_tags {
                            if tag_lower == stag || tag_lower.starts_with(&format!("{} ", stag)) {
                                skip_depth += 1;
                            } else if tag_lower == format!("/{}", stag)
                                || tag_lower.starts_with(&format!("/{} ", stag))
                            {
                                skip_depth = skip_depth.saturating_sub(1);
                            }
                        }
                        in_tag = false;
                        break;
                    } else if next_ch.is_whitespace() {
                        in_tag_name = false;
                        tag_buffer.push(
                            chars
                                .next()
                                .expect("chars.peek() returned Some, so next() should be Some"),
                        );
                    } else if in_tag_name {
                        tag_name.push(
                            chars
                                .next()
                                .expect("chars.peek() returned Some, so next() should be Some"),
                        );
                    } else {
                        tag_buffer.push(
                            chars
                                .next()
                                .expect("chars.peek() returned Some, so next() should be Some"),
                        );
                    }
                }
                // Don't add script/style/skipped-semantic content
                if !in_script && !in_style && skip_depth == 0 {
                    // Add space after block-level and sectioning elements for readability.
                    // Covers: traditional blocks, table cells (infobox merging fix),
                    // HTML5 semantic elements (section header merging fix).
                    if matches!(
                        tag_name.to_lowercase().as_str(),
                        "p" | "div"
                            | "br"
                            | "li"
                            | "ul"
                            | "ol"
                            | "td"
                            | "th"
                            | "tr"
                            | "dt"
                            | "dd"
                            | "h1"
                            | "h2"
                            | "h3"
                            | "h4"
                            | "h5"
                            | "h6"
                            | "section"
                            | "article"
                            | "header"
                            | "footer"
                            | "aside"
                            | "main"
                            | "blockquote"
                            | "figcaption"
                            | "figure"
                            | "details"
                            | "summary"
                    ) && !text.ends_with(' ')
                        && !text.is_empty()
                    {
                        text.push(' ');
                    }
                }
            }
            '>' if in_tag => {
                in_tag = false;
            }
            _ if in_tag || in_script || in_style || skip_depth > 0 => {
                // Skip content inside tags, scripts, styles, and semantic skip tags
            }
            '&' => {
                // Decode common HTML entities
                let mut entity = String::new();
                entity.push('&');
                let mut found_semicolon = false;
                while let Some(&next_ch) = chars.peek() {
                    entity.push(
                        chars
                            .next()
                            .expect("chars.peek() returned Some, so next() should be Some"),
                    );
                    if next_ch == ';' {
                        found_semicolon = true;
                        break;
                    }
                    if next_ch.is_whitespace() || next_ch == '<' {
                        break;
                    }
                }

                if found_semicolon {
                    let decoded = match entity.as_str() {
                        "&amp;" => "&",
                        "&lt;" => "<",
                        "&gt;" => ">",
                        "&quot;" => "\"",
                        "&apos;" => "'",
                        "&nbsp;" => " ",
                        "&#39;" => "'",
                        "&#8217;" => "'",
                        "&#8220;" => "\"",
                        "&#8221;" => "\"",
                        _ => {
                            // Try numeric entity (decimal &#N; or hex &#xN;)
                            if entity.starts_with("&#") && entity.len() > 2 {
                                let num_str = &entity[2..entity.len() - 1];
                                let parsed = if let Some(hex) = num_str
                                    .strip_prefix('x')
                                    .or_else(|| num_str.strip_prefix('X'))
                                {
                                    u32::from_str_radix(hex, 16).ok()
                                } else {
                                    num_str.parse::<u32>().ok()
                                };
                                if let Some(ch) = parsed.and_then(char::from_u32) {
                                    text.push(ch);
                                    continue;
                                }
                            }
                            // Unknown entity, keep as-is
                            text.push_str(&entity);
                            continue;
                        }
                    };
                    text.push_str(decoded);
                } else {
                    // Not a valid entity, keep as-is
                    text.push('&');
                    text.push_str(&entity[1..]);
                }
            }
            ch if !in_tag && !in_script && !in_style && skip_depth == 0 => {
                text.push(ch);
            }
            _ => {}
        }
    }

    // Clean up whitespace.
    //
    // HTML whitespace semantics are "collapsed": runs of whitespace render as a single space
    // (outside of <pre>, which we don't handle here). If we preserve raw newlines/indentation
    // from the HTML source, we end up with spans whose (start,end) point into `doc.text`,
    // but whose extracted `surface` has spaces (many NER backends reconstruct surfaces by
    // joining tokens with spaces). That mismatch creates a lot of validation noise on real
    // pages and makes debug output harder to trust.
    //
    // So: collapse ALL whitespace to single spaces and trim.
    let mut cleaned = String::with_capacity(text.len());
    let mut last_was_space = true; // avoid leading spaces
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !last_was_space {
                cleaned.push(' ');
                last_was_space = true;
            }
        } else {
            cleaned.push(ch);
            last_was_space = false;
        }
    }
    // Strip Wikipedia-style reference markers ([1], [edit], etc.) that
    // bleed into entity spans when extracting from wiki pages.
    let cleaned = WIKI_REF_BRACKET.replace_all(cleaned.trim(), "");
    cleaned.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wiki_reference_brackets_stripped() {
        let html = r#"<html><body>
            <p>Albert Einstein[1] was a physicist.[2] He developed
            the theory of relativity.[3][4] See also[edit] quantum mechanics.</p>
        </body></html>"#;
        let text = strip_html_to_text(html);
        assert!(!text.contains("[1]"), "reference [1] should be stripped, got: {}", text);
        assert!(!text.contains("[2]"), "reference [2] should be stripped, got: {}", text);
        assert!(!text.contains("[edit]"), "[edit] should be stripped, got: {}", text);
        assert!(text.contains("Albert Einstein"), "entity text should survive, got: {}", text);
        assert!(text.contains("quantum mechanics"), "content should survive, got: {}", text);
    }

    #[test]
    fn wiki_citation_needed_stripped() {
        let text = strip_html_to_text("<p>Some claim[citation needed] is here.</p>");
        assert!(!text.contains("[citation needed]"), "should strip [citation needed], got: {}", text);
        assert!(text.contains("Some claim"));
    }

    #[test]
    fn collapses_whitespace() {
        let html = r#"
            <html>
              <head><title>t</title></head>
              <body>
                <h1>Hello
                    world</h1>
                <p>Line1<br>Line2</p>
                <div>Tabbed	text</div>
              </body>
            </html>
        "#;
        let text = strip_html_to_text(html);
        assert!(text.contains("Hello world"));
        assert!(text.contains("Line1 Line2"));
        assert!(text.contains("Tabbed text"));
        assert!(!text.contains('\n'));
        assert!(!text.contains('\t'));
        assert!(!text.contains("  "));
    }

    #[test]
    fn multilingual_preserved() {
        let html = r#"<html><body>
            <p>&#x4E60;&#x8FD1;&#x5E73;&#x5728;&#x5317;&#x4EAC;</p>
            <p>Путин встретился с Си Цзиньпином в Москве.</p>
            <p>प्रधान मंत्री शर्मा आज आए।</p>
        </body></html>"#;
        let text = strip_html_to_text(html);
        assert!(text.contains("Путин встретился с Си Цзиньпином в Москве."));
        assert!(text.contains("प्रधान मंत्री शर्मा आज आए।"));
    }

    #[test]
    fn hex_entity_decoded() {
        let text = strip_html_to_text("<p>It&#x27;s a test</p>");
        assert!(text.contains("It's"), "&#x27; should decode to apostrophe, got: {}", text);
    }

    #[test]
    fn hex_entity_uppercase_x() {
        let text = strip_html_to_text("<p>It&#X27;s a test</p>");
        assert!(text.contains("It's"), "&#X27; should decode to apostrophe, got: {}", text);
    }

    #[test]
    fn decimal_entity_decoded() {
        let text = strip_html_to_text("<p>It&#39;s a test</p>");
        assert!(text.contains("It's"), "&#39; should decode to apostrophe, got: {}", text);
    }

    #[test]
    fn named_entity_decoded() {
        let text = strip_html_to_text("<p>A &amp; B &lt; C</p>");
        assert!(text.contains("A & B"), "should decode &amp;, got: {}", text);
        assert!(text.contains("< C"), "should decode &lt;, got: {}", text);
    }

    // =========================================================================
    // Semantic HTML tag filtering
    // =========================================================================

    #[test]
    fn nav_content_stripped() {
        let html = r#"<html><body>
            <nav><a href="/">Home</a><a href="/about">About</a></nav>
            <main><p>Main content here.</p></main>
        </body></html>"#;
        let text = strip_html_to_text(html);
        assert!(text.contains("Main content here"));
        assert!(!text.contains("Home"), "nav content should be stripped");
        assert!(!text.contains("About"), "nav content should be stripped");
    }

    #[test]
    fn footer_content_stripped() {
        let html = r#"<html><body>
            <article><p>Article body.</p></article>
            <footer><p>Copyright 2024 Example Corp.</p></footer>
        </body></html>"#;
        let text = strip_html_to_text(html);
        assert!(text.contains("Article body"));
        assert!(!text.contains("Copyright"), "footer content should be stripped");
    }

    #[test]
    fn header_content_stripped() {
        let html = r#"<html><body>
            <header><h1>Site Title</h1><nav>Menu</nav></header>
            <main><p>Page content.</p></main>
            <footer><p>Footer text</p></footer>
        </body></html>"#;
        let text = strip_html_to_text(html);
        assert!(text.contains("Page content"));
        assert!(!text.contains("Site Title"), "header content should be stripped");
        assert!(!text.contains("Footer text"), "footer content should be stripped");
    }

    #[test]
    fn aside_content_stripped() {
        let html = r#"<html><body>
            <main><p>Main text.</p></main>
            <aside><p>Sidebar widget.</p></aside>
        </body></html>"#;
        let text = strip_html_to_text(html);
        assert!(text.contains("Main text"));
        assert!(!text.contains("Sidebar widget"), "aside content should be stripped");
    }

    #[test]
    fn article_content_preserved() {
        let html = r#"<html><body>
            <article>
                <h2>Article Title</h2>
                <p>First paragraph.</p>
                <p>Second paragraph.</p>
            </article>
        </body></html>"#;
        let text = strip_html_to_text(html);
        assert!(text.contains("Article Title"));
        assert!(text.contains("First paragraph"));
        assert!(text.contains("Second paragraph"));
    }

    #[test]
    fn nested_semantic_tags() {
        let html = r#"<html><body>
            <header>
                <nav><ul><li>Link1</li></ul></nav>
                <p>Header text</p>
            </header>
            <main><p>Real content.</p></main>
        </body></html>"#;
        let text = strip_html_to_text(html);
        assert!(text.contains("Real content"));
        assert!(!text.contains("Link1"), "nested nav inside header should be stripped");
        assert!(!text.contains("Header text"), "header content should be stripped");
    }

    #[test]
    fn noscript_stripped() {
        let html = r#"<html><body>
            <noscript><p>Enable JavaScript to view this page.</p></noscript>
            <main><p>App content.</p></main>
        </body></html>"#;
        let text = strip_html_to_text(html);
        assert!(text.contains("App content"));
        assert!(!text.contains("Enable JavaScript"), "noscript content should be stripped");
    }

    // =========================================================================
    // looks_like_html
    // =========================================================================

    #[test]
    fn detects_html() {
        assert!(looks_like_html("<!DOCTYPE html><html><head>"));
        assert!(looks_like_html("<html><head><body>"));
        assert!(looks_like_html("  \n<!DOCTYPE html>\n<html>"));
        assert!(looks_like_html("<?xml version=\"1.0\"?><html>"));
    }

    #[test]
    fn rejects_non_html() {
        assert!(!looks_like_html("Tim Cook announced new products today."));
        assert!(!looks_like_html("The patient has no history of diabetes."));
        assert!(!looks_like_html("# Markdown heading\n\nSome text."));
    }
}
