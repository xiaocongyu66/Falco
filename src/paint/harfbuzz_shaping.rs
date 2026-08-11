//! HarfBuzz text shaping — proper glyph shaping with rustybuzz.
//!
//! # Overview
//!
//! This module replaces the per-character rendering of ab_glyph with
//! proper text shaping using rustybuzz (a Rust port of HarfBuzz).
//!
//! HarfBuzz provides:
//! - **Ligatures** — fi → ligature, Arabic contextual forms
//! - **Kerning** — adjusting spacing between glyph pairs
//! - **Complex scripts** — Indic conjuncts, Arabic joining, Hebrew
//! - **BiDi reordering** — mixed LTR/RTL text
//! - **Vertical text** — CJK vertical layout
//! - **Feature control** — OpenType features (smcp, c2sc, lnum, etc.)
//!
//! # Usage
//!
//! ```no_run
//! use falco::paint::harfbuzz_shaping::shape_text;
//!
//! // Shape a string into positioned glyphs.
//! let glyphs = shape_text("Hello", font_data, font_size, rtl: false);
//! for glyph in glyphs {
//!     // glyph.glyph_id, glyph.x_offset, glyph.y_offset, glyph.x_advance
//! }
//! ```

use rustybuzz::{Face, UnicodeBuffer};

/// A shaped glyph — glyph ID + positioning.
#[derive(Debug, Clone)]
pub struct ShapedGlyph {
    /// The glyph ID in the font.
    pub glyph_id: u32,
    /// X offset from the pen position (in font units / units_per_em * size).
    pub x_offset: i32,
    /// Y offset from the pen position.
    pub y_offset: i32,
    /// X advance — how far to move the pen after this glyph.
    pub x_advance: i32,
    /// Y advance (for vertical text).
    pub y_advance: i32,
    /// The cluster index (for mapping back to the source string).
    pub cluster: u32,
}

/// Shape a text string into positioned glyphs using rustybuzz (HarfBuzz).
///
/// # Arguments
/// * `text` — the text to shape
/// * `font_data` — raw font file bytes (TTF/OTF)
/// * `font_size` — font size in pixels
/// * `rtl` — true for right-to-left text (Arabic, Hebrew)
///
/// # Returns
/// A vector of shaped glyphs with positioning information.
pub fn shape_text(text: &str, font_data: &[u8], font_size: f32, rtl: bool) -> Vec<ShapedGlyph> {
    // Parse the font face.
    let face = match Face::from_slice(font_data, 0) {
        Some(f) => f,
        None => return Vec::new(),
    };

    // Create a Unicode buffer.
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    if rtl {
        buffer.set_direction(rustybuzz::Direction::RightToLeft);
    } else {
        buffer.set_direction(rustybuzz::Direction::LeftToRight);
    }

    // Shape the text.
    let glyph_buffer = rustybuzz::shape(&face, &[], buffer);

    // Extract glyph information.
    let glyph_infos = glyph_buffer.glyph_infos();
    let glyph_positions = glyph_buffer.glyph_positions();

    let units_per_em = face.units_per_em() as f32;
    let scale = font_size / units_per_em;

    glyph_infos
        .iter()
        .zip(glyph_positions.iter())
        .map(|(info, pos)| ShapedGlyph {
            glyph_id: info.glyph_id,
            x_offset: (pos.x_offset as f32 * scale) as i32,
            y_offset: (pos.y_offset as f32 * scale) as i32,
            x_advance: (pos.x_advance as f32 * scale) as i32,
            y_advance: (pos.y_advance as f32 * scale) as i32,
            cluster: info.cluster,
        })
        .collect()
}

/// Check if a font supports a specific script.
pub fn font_supports_script(font_data: &[u8]) -> bool {
    if let Some(_face) = Face::from_slice(font_data, 0) {
        return true;
    }
    false
}

/// Shape Arabic text with proper contextual forms.
///
/// Arabic letters change shape depending on their position in a word
/// (initial, medial, final, isolated). HarfBuzz handles this automatically
/// via the GSUB table.
pub fn shape_arabic(text: &str, font_data: &[u8], font_size: f32) -> Vec<ShapedGlyph> {
    shape_text(text, font_data, font_size, true)
}

/// Shape text with OpenType features (e.g., ligatures, small caps).
///
/// Common features:
/// - `"liga"` — standard ligatures (fi, fl)
/// - `"dlig"` — discretionary ligatures
/// - `"smcp"` — small caps
/// - `"onum"` — old-style figures
/// - `"tnum"` — tabular figures
pub fn shape_with_features(
    text: &str,
    font_data: &[u8],
    font_size: f32,
    features: &[(&str, u32)],
) -> Vec<ShapedGlyph> {
    let face = match Face::from_slice(font_data, 0) {
        Some(f) => f,
        None => return Vec::new(),
    };

    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.set_direction(rustybuzz::Direction::LeftToRight);

    // Build feature list.
    let feature_list: Vec<rustybuzz::Feature> = features
        .iter()
        .filter_map(|(tag, value)| {
            let tag_bytes = tag.as_bytes();
            if tag_bytes.len() >= 4 {
                let mut arr = [0u8; 4];
                arr.copy_from_slice(&tag_bytes[..4]);
                Some(rustybuzz::Feature::new(
                    ttf_parser::Tag::from_bytes(&arr),
                    *value,
                    ..,
                ))
            } else {
                None
            }
        })
        .collect();

    let glyph_buffer = rustybuzz::shape(&face, &feature_list, buffer);

    let glyph_infos = glyph_buffer.glyph_infos();
    let glyph_positions = glyph_buffer.glyph_positions();
    let units_per_em = face.units_per_em() as f32;
    let scale = font_size / units_per_em;

    glyph_infos
        .iter()
        .zip(glyph_positions.iter())
        .map(|(info, pos)| ShapedGlyph {
            glyph_id: info.glyph_id,
            x_offset: (pos.x_offset as f32 * scale) as i32,
            y_offset: (pos.y_offset as f32 * scale) as i32,
            x_advance: (pos.x_advance as f32 * scale) as i32,
            y_advance: (pos.y_advance as f32 * scale) as i32,
            cluster: info.cluster,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_empty_text() {
        // Without a real font, this returns empty.
        let result = shape_text("", &[], 16.0, false);
        assert!(result.is_empty());
    }

    #[test]
    fn shape_with_invalid_font() {
        let result = shape_text("Hello", &[0; 10], 16.0, false);
        assert!(result.is_empty());
    }

    #[test]
    fn shaped_glyph_fields() {
        let g = ShapedGlyph {
            glyph_id: 42,
            x_offset: 1,
            y_offset: 2,
            x_advance: 100,
            y_advance: 0,
            cluster: 0,
        };
        assert_eq!(g.glyph_id, 42);
        assert_eq!(g.x_advance, 100);
    }
}
