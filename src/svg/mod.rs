//! SVG module — parser and renderer for SVG images.
//!
//! Falco can render SVG from:
//! - `<img src="*.svg">` — loaded and rasterized
//! - `data:image/svg+xml;base64,...` — inline data URLs
//! - Inline `<svg>...</svg>` in HTML

pub mod parser;
pub mod renderer;

use crate::paint::Canvas;
use parser::SvgDocument;
use std::collections::HashMap;

/// Parse an SVG XML string into an SvgDocument.
///
/// This function:
/// 1. Extracts the <svg> root element's width, height, viewBox.
/// 2. Parses <defs> content and builds a lookup table of id → element.
/// 3. Parses the remaining children, resolving <use href="#id"> references.
pub fn parse_svg(xml: &str) -> Option<SvgDocument> {
    let lower = xml.to_lowercase();
    if !lower.contains("<svg") {
        return None;
    }

    // Extract svg root attributes.
    let svg_start = lower.find("<svg")?;
    let svg_tag_end = lower[svg_start..].find('>')?;
    let svg_tag = &xml[svg_start..svg_start + svg_tag_end];

    let width = extract_attr_f32(svg_tag, "width").unwrap_or(300.0);
    let height = extract_attr_f32(svg_tag, "height").unwrap_or(150.0);
    let viewBox = extract_attr_str(svg_tag, "viewbox").and_then(|s| {
        let parts: Vec<f32> = s
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter_map(|x| x.parse().ok())
            .collect();
        if parts.len() == 4 {
            Some((parts[0], parts[1], parts[2], parts[3]))
        } else {
            None
        }
    });

    // Extract content between <svg> and </svg>.
    let content_start = svg_start + svg_tag_end + 1;
    let content_end = lower[content_start..].find("</svg>")?;
    let content = &xml[content_start..content_start + content_end];

    // Step 1: Parse <defs> and build a lookup table of id → SvgElement.
    let defs = parse_defs(content);

    // Step 2: Parse children with defs lookup for <use> resolution.
    let children = parse_svg_children(content, &defs);

    Some(SvgDocument {
        width,
        height,
        viewBox,
        children,
    })
}

/// Parse all <defs> blocks in the SVG content and return a map of
/// id → SvgElement. Elements inside <defs> are not rendered directly,
/// but can be referenced by <use href="#id">.
///
/// This function does multiple passes to resolve nested <use> references
/// (e.g. `<use id="A" href="#B"/>` where B is defined later).
fn parse_defs(content: &str) -> HashMap<String, parser::SvgElement> {
    let mut defs_map: HashMap<String, parser::SvgElement> = HashMap::new();

    // Collect all <defs> block contents.
    let lower = content.to_lowercase();
    let mut search_from = 0;
    let mut defs_contents = Vec::new();
    while let Some(defs_start) = lower[search_from..].find("<defs") {
        let abs_start = search_from + defs_start;
        let tag_end = match lower[abs_start..].find('>') {
            Some(i) => abs_start + i + 1,
            None => break,
        };
        let defs_content = match lower[tag_end..].find("</defs>") {
            Some(i) => &content[tag_end..tag_end + i],
            None => break,
        };
        defs_contents.push(defs_content);
        search_from = tag_end + defs_content.len() + 7;
    }

    // Pass 1: Parse all non-<use> elements with ids (shapes, groups, etc.).
    for dc in &defs_contents {
        parse_defs_pass1(dc, &mut defs_map);
    }
    // Also scan elements outside <defs> that have ids.
    parse_defs_pass1(content, &mut defs_map);

    // Pass 2+: Resolve <use> elements. Repeat until no new elements are added.
    loop {
        let before = defs_map.len();
        let defs_snapshot = defs_map.clone();
        for dc in &defs_contents {
            parse_defs_use_pass(dc, &defs_snapshot, &mut defs_map);
        }
        parse_defs_use_pass(content, &defs_snapshot, &mut defs_map);
        if defs_map.len() == before {
            break;
        }
    }

    defs_map
}

/// First pass: parse all non-<use> elements with id attributes.
/// <use> elements are skipped in this pass (they need references resolved first).
fn parse_defs_pass1(content: &str, defs_map: &mut HashMap<String, parser::SvgElement>) {
    let lower = content.to_lowercase();
    let mut pos = 0;

    while pos < content.len() {
        let tag_start = match lower[pos..].find('<') {
            Some(i) => pos + i,
            None => break,
        };
        if content[tag_start..].starts_with("</")
            || content[tag_start..].starts_with("<!--")
            || content[tag_start..].starts_with("<![CDATA[")
        {
            if let Some(end) = lower[tag_start..].find('>') {
                pos = tag_start + end + 1;
            } else {
                break;
            }
            continue;
        }
        if content[tag_start..].to_lowercase().starts_with("<style")
            || content[tag_start..].to_lowercase().starts_with("<metadata")
        {
            let close_tag = if content[tag_start..].to_lowercase().starts_with("<style") {
                "</style>"
            } else {
                "</metadata>"
            };
            if let Some(end) = lower[tag_start..].find(close_tag) {
                pos = tag_start + end + close_tag.len();
            } else {
                pos = tag_start + 1;
            }
            continue;
        }

        let tag_end = match lower[tag_start..].find('>') {
            Some(i) => tag_start + i,
            None => break,
        };
        let tag_content = &content[tag_start + 1..tag_end];
        let tag_name = tag_content
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_lowercase();
        let id = extract_attr_str(tag_content, "id");
        let self_closing = tag_content.ends_with('/');

        // Skip <use> in pass 1 — they'll be resolved in pass 2.
        if tag_name != "use" {
            let attrs = parse_attrs(tag_content);
            let element = parse_one_element(
                tag_name.as_str(),
                tag_content,
                &attrs,
                content,
                tag_end,
                self_closing,
                &HashMap::new(), // No defs yet — <use> inside groups won't resolve, that's ok for pass 1.
            );
            if let Some(elem) = element {
                if let Some(ref id_str) = id {
                    defs_map.entry(id_str.clone()).or_insert(elem);
                }
            }
        }

        // Advance.
        if self_closing {
            pos = tag_end + 1;
        } else {
            let close_tag = format!("</{}>", tag_name);
            if let Some(end) = lower[tag_end + 1..].find(&close_tag.to_lowercase()) {
                pos = tag_end + 1 + end + close_tag.len();
            } else {
                pos = tag_end + 1;
            }
        }
    }
}

/// Second pass: resolve <use> elements using the current defs_map.
/// Each resolved <use> is added to defs_map with its own id (if it has one).
fn parse_defs_use_pass(
    content: &str,
    defs: &HashMap<String, parser::SvgElement>,
    defs_map: &mut HashMap<String, parser::SvgElement>,
) {
    let lower = content.to_lowercase();
    let mut pos = 0;

    while pos < content.len() {
        let tag_start = match lower[pos..].find('<') {
            Some(i) => pos + i,
            None => break,
        };
        if content[tag_start..].starts_with("</")
            || content[tag_start..].starts_with("<!--")
            || content[tag_start..].starts_with("<![CDATA[")
        {
            if let Some(end) = lower[tag_start..].find('>') {
                pos = tag_start + end + 1;
            } else {
                break;
            }
            continue;
        }
        if content[tag_start..].to_lowercase().starts_with("<style")
            || content[tag_start..].to_lowercase().starts_with("<metadata")
        {
            let close_tag = if content[tag_start..].to_lowercase().starts_with("<style") {
                "</style>"
            } else {
                "</metadata>"
            };
            if let Some(end) = lower[tag_start..].find(close_tag) {
                pos = tag_start + end + close_tag.len();
            } else {
                pos = tag_start + 1;
            }
            continue;
        }

        let tag_end = match lower[tag_start..].find('>') {
            Some(i) => tag_start + i,
            None => break,
        };
        let tag_content = &content[tag_start + 1..tag_end];
        let tag_name = tag_content
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_lowercase();
        let id = extract_attr_str(tag_content, "id");
        let self_closing = tag_content.ends_with('/');

        // Only process <use> elements that have an id and haven't been resolved yet.
        if tag_name == "use" {
            if let Some(ref id_str) = id {
                if defs_map.contains_key(id_str) {
                    // Already resolved — skip.
                    pos = tag_end + 1;
                    continue;
                }
                let attrs = parse_attrs(tag_content);
                let element = parse_one_element(
                    "use",
                    tag_content,
                    &attrs,
                    content,
                    tag_end,
                    self_closing,
                    defs, // Use the current defs map for resolution.
                );
                if let Some(elem) = element {
                    defs_map.insert(id_str.clone(), elem);
                }
            }
        }

        // Advance.
        if self_closing {
            pos = tag_end + 1;
        } else {
            let close_tag = format!("</{}>", tag_name);
            if let Some(end) = lower[tag_end + 1..].find(&close_tag.to_lowercase()) {
                pos = tag_end + 1 + end + close_tag.len();
            } else {
                pos = tag_end + 1;
            }
        }
    }
}

fn parse_svg_children(
    content: &str,
    defs: &HashMap<String, parser::SvgElement>,
) -> Vec<parser::SvgElement> {
    let mut children = Vec::new();
    let lower = content.to_lowercase();
    let mut pos = 0;

    while pos < content.len() {
        let tag_start = match lower[pos..].find('<') {
            Some(i) => pos + i,
            None => break,
        };
        // Skip closing tags and comments.
        if content[tag_start..].starts_with("</") || content[tag_start..].starts_with("<!--") {
            if let Some(end) = lower[tag_start..].find('>') {
                pos = tag_start + end + 1;
            } else {
                break;
            }
            continue;
        }

        // Skip <style> and <metadata> blocks.
        if content[tag_start..].to_lowercase().starts_with("<style")
            || content[tag_start..].to_lowercase().starts_with("<metadata")
        {
            let close_tag = if content[tag_start..].to_lowercase().starts_with("<style") {
                "</style>"
            } else {
                "</metadata>"
            };
            if let Some(end) = lower[tag_start..].find(close_tag) {
                pos = tag_start + end + close_tag.len();
            } else {
                pos = tag_start + 1;
            }
            continue;
        }

        // Skip <defs> blocks — already parsed.
        if content[tag_start..].to_lowercase().starts_with("<defs") {
            if let Some(end) = lower[tag_start..].find("</defs>") {
                pos = tag_start + end + 7;
            } else {
                pos = tag_start + 1;
            }
            continue;
        }

        let tag_end = match lower[tag_start..].find('>') {
            Some(i) => tag_start + i,
            None => break,
        };
        let tag_content = &content[tag_start + 1..tag_end];
        let tag_name = tag_content
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_lowercase();

        let attrs = parse_attrs(tag_content);
        let self_closing = tag_content.ends_with('/');

        let element = parse_one_element(
            tag_name.as_str(),
            tag_content,
            &attrs,
            content,
            tag_end,
            self_closing,
            defs,
        );

        if let Some(e) = element {
            children.push(e);
        }

        // Advance position.
        if self_closing {
            pos = tag_end + 1;
        } else {
            let close_tag = format!("</{}>", tag_name);
            if let Some(end) = lower[tag_end + 1..].find(&close_tag.to_lowercase()) {
                pos = tag_end + 1 + end + close_tag.len();
            } else {
                pos = tag_end + 1;
            }
        }
    }

    children
}

/// Parse a single SVG element from its tag content.
/// This is shared between parse_svg_children and parse_defs_elements.
fn parse_one_element(
    tag_name: &str,
    tag_content: &str,
    attrs: &HashMap<String, String>,
    content: &str,
    tag_end: usize,
    self_closing: bool,
    defs: &HashMap<String, parser::SvgElement>,
) -> Option<parser::SvgElement> {
    let lower = content.to_lowercase();
    match tag_name {
        "rect" => {
            let r = parser::SvgRect {
                x: extract_attr_f32(tag_content, "x").unwrap_or(0.0),
                y: extract_attr_f32(tag_content, "y").unwrap_or(0.0),
                width: extract_attr_f32(tag_content, "width").unwrap_or(0.0),
                height: extract_attr_f32(tag_content, "height").unwrap_or(0.0),
                rx: extract_attr_f32(tag_content, "rx").unwrap_or(0.0),
                ry: extract_attr_f32(tag_content, "ry").unwrap_or(0.0),
                style: parser::SvgStyle::from_attrs(attrs),
            };
            Some(parser::SvgElement::Rect(r))
        }
        "circle" => {
            let c = parser::SvgCircle {
                cx: extract_attr_f32(tag_content, "cx").unwrap_or(0.0),
                cy: extract_attr_f32(tag_content, "cy").unwrap_or(0.0),
                r: extract_attr_f32(tag_content, "r").unwrap_or(0.0),
                style: parser::SvgStyle::from_attrs(attrs),
            };
            Some(parser::SvgElement::Circle(c))
        }
        "ellipse" => {
            let e = parser::SvgEllipse {
                cx: extract_attr_f32(tag_content, "cx").unwrap_or(0.0),
                cy: extract_attr_f32(tag_content, "cy").unwrap_or(0.0),
                rx: extract_attr_f32(tag_content, "rx").unwrap_or(0.0),
                ry: extract_attr_f32(tag_content, "ry").unwrap_or(0.0),
                style: parser::SvgStyle::from_attrs(attrs),
            };
            Some(parser::SvgElement::Ellipse(e))
        }
        "line" => {
            let l = parser::SvgLine {
                x1: extract_attr_f32(tag_content, "x1").unwrap_or(0.0),
                y1: extract_attr_f32(tag_content, "y1").unwrap_or(0.0),
                x2: extract_attr_f32(tag_content, "x2").unwrap_or(0.0),
                y2: extract_attr_f32(tag_content, "y2").unwrap_or(0.0),
                style: parser::SvgStyle::from_attrs(attrs),
            };
            Some(parser::SvgElement::Line(l))
        }
        "polyline" => {
            let pts_str = extract_attr_str(tag_content, "points").unwrap_or_default();
            Some(parser::SvgElement::Polyline(parser::SvgPolyline {
                points: parser::parse_points(&pts_str),
                style: parser::SvgStyle::from_attrs(attrs),
            }))
        }
        "polygon" => {
            let pts_str = extract_attr_str(tag_content, "points").unwrap_or_default();
            Some(parser::SvgElement::Polygon(parser::SvgPolygon {
                points: parser::parse_points(&pts_str),
                style: parser::SvgStyle::from_attrs(attrs),
            }))
        }
        "path" => {
            let d = extract_attr_str(tag_content, "d").unwrap_or_default();
            Some(parser::SvgElement::Path(parser::SvgPath {
                commands: parser::parse_path_d(&d),
                style: parser::SvgStyle::from_attrs(attrs),
            }))
        }
        "text" => {
            let text_content = if self_closing {
                String::new()
            } else {
                let close_tag = format!("</{}>", tag_name);
                if let Some(end) = lower[tag_end + 1..].find(&close_tag.to_lowercase()) {
                    content[tag_end + 1..tag_end + 1 + end].trim().to_string()
                } else {
                    String::new()
                }
            };
            Some(parser::SvgElement::Text(parser::SvgText {
                x: extract_attr_f32(tag_content, "x").unwrap_or(0.0),
                y: extract_attr_f32(tag_content, "y").unwrap_or(0.0),
                text: text_content,
                font_size: extract_attr_f32(tag_content, "font-size").unwrap_or(16.0),
                font_weight: extract_attr_str(tag_content, "font-weight")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(400),
                style: parser::SvgStyle::from_attrs(attrs),
            }))
        }
        "g" | "a" => {
            let transform_str = extract_attr_str(tag_content, "transform");
            let transform = transform_str.as_deref().and_then(parser::parse_transform);
            let group_content = if self_closing {
                ""
            } else {
                find_matching_close_tag(content, tag_end + 1, tag_name)
            };
            let group_children = parse_svg_children(group_content, defs);
            Some(parser::SvgElement::Group(parser::SvgGroup {
                transform,
                style: parser::SvgStyle::from_attrs(attrs),
                children: group_children,
            }))
        }
        "symbol" => {
            // <symbol> is like <g> but only rendered through <use>.
            // Store its children in a group.
            let group_content = if self_closing {
                ""
            } else {
                find_matching_close_tag(content, tag_end + 1, "symbol")
            };
            let group_children = parse_svg_children(group_content, defs);
            Some(parser::SvgElement::Group(parser::SvgGroup {
                transform: None,
                style: parser::SvgStyle::from_attrs(attrs),
                children: group_children,
            }))
        }
        "use" => {
            // <use href="#id" x=".." y=".." transform=".." />
            // Resolve the reference: look up the element in defs and
            // wrap it in a group with the use's transform.
            let href = extract_attr_str(tag_content, "href")
                .or_else(|| extract_attr_str(tag_content, "xlink:href"));

            if let Some(href) = href {
                let id = href.trim_start_matches('#');
                if let Some(referenced) = defs.get(id) {
                    // Clone the referenced element and wrap in a group
                    // with the use's x/y offset and transform.
                    let x = extract_attr_f32(tag_content, "x").unwrap_or(0.0);
                    let y = extract_attr_f32(tag_content, "y").unwrap_or(0.0);
                    let transform_str = extract_attr_str(tag_content, "transform");
                    let base_transform = transform_str.as_deref().and_then(parser::parse_transform);

                    // Combine x/y offset with optional transform.
                    let transform = if x != 0.0 || y != 0.0 {
                        if let Some(bt) = base_transform {
                            // Merge: apply x/y translate first, then the transform.
                            Some(parser::SvgTransform {
                                translate: (bt.translate.0 + x, bt.translate.1 + y),
                                scale: bt.scale,
                                rotate: bt.rotate,
                            })
                        } else {
                            Some(parser::SvgTransform {
                                translate: (x, y),
                                scale: (1.0, 1.0),
                                rotate: 0.0,
                            })
                        }
                    } else {
                        base_transform
                    };

                    // Clone the referenced element and wrap it in a group.
                    // The group applies the use's transform (x/y offset +
                    // optional transform attribute).
                    Some(parser::SvgElement::Group(parser::SvgGroup {
                        transform,
                        style: parser::SvgStyle::default(),
                        children: vec![referenced.clone()],
                    }))
                } else {
                    // Reference not found — return empty group.
                    None
                }
            } else {
                None
            }
        }
        "image" => {
            let x = extract_attr_f32(tag_content, "x").unwrap_or(0.0);
            let y = extract_attr_f32(tag_content, "y").unwrap_or(0.0);
            let w = extract_attr_f32(tag_content, "width").unwrap_or(0.0);
            let h = extract_attr_f32(tag_content, "height").unwrap_or(0.0);
            Some(parser::SvgElement::Rect(parser::SvgRect {
                x,
                y,
                width: w,
                height: h,
                rx: 0.0,
                ry: 0.0,
                style: parser::SvgStyle::from_attrs(attrs),
            }))
        }
        // Skip these elements (not rendered).
        "view" | "animate" | "clippath" | "filter" | "feturbulence" | "femergenode" | "stop"
        | "lineargradient" | "radialgradient" | "defs" | "title" | "desc" | "cc" | "dcterms"
        | "metadata" | "rdf" | "style" => None,
        _ => None,
    }
}

/// Find the matching closing tag for a container element, handling nesting.
fn find_matching_close_tag<'a>(content: &'a str, start: usize, tag_name: &str) -> &'a str {
    let lower = content.to_lowercase();
    let open_tag = format!("<{}", tag_name);
    let close_tag = format!("</{}>", tag_name);
    let mut depth = 1;
    let mut pos = start;
    while pos < content.len() && depth > 0 {
        let next_open = lower[pos..].find(&open_tag).map(|i| pos + i);
        let next_close = lower[pos..].find(&close_tag).map(|i| pos + i);
        match (next_open, next_close) {
            (Some(o), Some(c)) if o < c => {
                depth += 1;
                pos = o + open_tag.len();
            }
            (_, Some(c)) => {
                depth -= 1;
                if depth == 0 {
                    return &content[start..c];
                }
                pos = c + close_tag.len();
            }
            (None, None) => break,
            (Some(o), None) => {
                depth += 1;
                pos = o + open_tag.len();
            }
            (None, Some(c)) => {
                depth -= 1;
                if depth == 0 {
                    return &content[start..c];
                }
                pos = c + close_tag.len();
            }
        }
    }
    ""
}

fn parse_attrs(tag_content: &str) -> HashMap<String, String> {
    let mut attrs = HashMap::new();
    let mut chars = tag_content.chars().peekable();
    let mut key = String::new();
    let mut in_value = false;
    let mut quote = '"';
    let mut value = String::new();
    let mut reading_key = false;

    while let Some(c) = chars.next() {
        if !in_value {
            if c == '=' && !key.is_empty() {
                if let Some(&q) = chars.peek() {
                    if q == '"' || q == '\'' {
                        quote = q;
                        chars.next();
                        in_value = true;
                        value.clear();
                    }
                }
            } else if c.is_whitespace() {
                if reading_key && !key.is_empty() {
                    attrs.insert(key.clone(), String::new());
                    key.clear();
                    reading_key = false;
                }
            } else {
                if !reading_key {
                    key.clear();
                    reading_key = true;
                }
                if !key.is_empty() || c != ' ' {
                    key.push(c);
                }
            }
        } else {
            if c == quote {
                attrs.insert(key.trim().to_lowercase(), value.clone());
                key.clear();
                in_value = false;
                reading_key = false;
            } else {
                value.push(c);
            }
        }
    }

    let tag_name = tag_content.split_whitespace().next().unwrap_or("");
    attrs.remove(tag_name);

    attrs
}

fn extract_attr_f32(tag: &str, name: &str) -> Option<f32> {
    extract_attr_str(tag, name)?
        .trim_end_matches("px")
        .parse()
        .ok()
}

fn extract_attr_str(tag: &str, name: &str) -> Option<String> {
    let pattern = format!("{}=\"", name);
    if let Some(start) = tag.to_lowercase().find(&pattern.to_lowercase()) {
        let rest = &tag[start + pattern.len()..];
        if let Some(end) = rest.find('"') {
            return Some(rest[..end].to_string());
        }
    }
    let pattern = format!("{}='", name);
    if let Some(start) = tag.to_lowercase().find(&pattern.to_lowercase()) {
        let rest = &tag[start + pattern.len()..];
        if let Some(end) = rest.find('\'') {
            return Some(rest[..end].to_string());
        }
    }
    None
}

/// Render an SVG string onto a canvas.
pub fn render_svg_to_canvas(
    svg_xml: &str,
    canvas: &mut Canvas,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) -> bool {
    if let Some(doc) = parse_svg(svg_xml) {
        renderer::render_svg(canvas, &doc, x, y, w, h);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_svg() {
        let svg = r##"<svg width="100" height="100" viewBox="0 0 100 100">
            <rect x="10" y="10" width="80" height="80" fill="#ff0000" stroke="#000000" stroke-width="2"/>
            <circle cx="50" cy="50" r="30" fill="#0000ff"/>
        </svg>"##;
        let doc = parse_svg(svg);
        assert!(doc.is_some());
        let doc = doc.unwrap();
        assert_eq!(doc.width, 100.0);
        assert_eq!(doc.height, 100.0);
        assert_eq!(doc.children.len(), 2);
    }

    #[test]
    fn parses_path() {
        let svg =
            r##"<svg width="100" height="100"><path d="M 10 10 L 50 50 Z" fill="red"/></svg>"##;
        let doc = parse_svg(svg).unwrap();
        assert_eq!(doc.children.len(), 1);
    }

    #[test]
    fn parses_group() {
        let svg = r##"<svg width="100" height="100"><g transform="translate(10,20)"><rect width="50" height="50" fill="blue"/></g></svg>"##;
        let doc = parse_svg(svg).unwrap();
        assert_eq!(doc.children.len(), 1);
    }

    #[test]
    fn use_resolves_defs_reference() {
        let svg = r##"<svg width="100" height="100">
            <defs>
                <rect id="myRect" x="0" y="0" width="50" height="50" fill="red"/>
            </defs>
            <use href="#myRect" x="10" y="10"/>
        </svg>"##;
        let doc = parse_svg(svg).unwrap();
        // The <use> should resolve to a group containing the referenced rect.
        assert_eq!(doc.children.len(), 1);
        match &doc.children[0] {
            parser::SvgElement::Group(g) => {
                assert_eq!(g.children.len(), 1);
                assert!(matches!(g.children[0], parser::SvgElement::Rect(_)));
                // Check transform has x=10, y=10.
                assert_eq!(g.transform.unwrap().translate, (10.0, 10.0));
            }
            _ => panic!("expected group from <use>"),
        }
    }

    #[test]
    fn use_with_transform() {
        let svg = r##"<svg width="100" height="100">
            <defs>
                <circle id="myCircle" cx="0" cy="0" r="10" fill="blue"/>
            </defs>
            <use href="#myCircle" transform="translate(50,50)"/>
        </svg>"##;
        let doc = parse_svg(svg).unwrap();
        assert_eq!(doc.children.len(), 1);
        match &doc.children[0] {
            parser::SvgElement::Group(g) => {
                assert_eq!(g.transform.unwrap().translate, (50.0, 50.0));
                assert!(matches!(g.children[0], parser::SvgElement::Circle(_)));
            }
            _ => panic!("expected group from <use>"),
        }
    }
}
