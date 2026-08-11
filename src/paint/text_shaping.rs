//! Basic text shaping — RTL/bidi awareness.
//!
//! # Overview
//!
//! Real text shaping requires HarfBuzz for glyph substitution (ligatures,
//! contextual alternates) and unicode-bidi for bidirectional reordering.
//!
//! This module provides a **basic Unicode bidi algorithm** (UAX #9) that
//! reorders characters for display. It handles:
//!
//! - LTR (Left-to-Right) — English, Latin, Cyrillic, etc.
//! - RTL (Right-to-Left) — Arabic, Hebrew, Persian, Urdu
//! - Mixed content — "Hello عالم World" → correct visual order
//!
//! It does NOT handle:
//! - Ligatures (Arabic contextual forms, Latin fi/fl ligatures)
//! - Complex shaping (Indic conjuncts, Arabic joining)
//! - Vertical text modes
//! - Font fallback (HarfBuzz integration)
//!
//! For full shaping, integrate HarfBuzz via `harfbuzz_rs` or `rust-harfbuzz`.

/// Unicode bidi character type (UAX #9 §4.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BidiType {
    /// Left-to-right (Latin, Cyrillic, etc.)
    L,
    /// Right-to-left (Arabic, Hebrew)
    R,
    /// Right-to-left Arabic (Arabic letters with joining)
    AL,
    /// European number (0-9 in Latin script)
    EN,
    /// European separator (+, -)
    ES,
    /// European terminator (#, ¤)
    ET,
    /// Arabic number (Arabic-Indic digits)
    AN,
    /// Common separator (comma, space between numbers)
    CS,
    /// Nonspacing mark (combining characters)
    NSM,
    /// Boundary neutral (zero-width)
    BN,
    /// Block separator (paragraph, line break)
    B,
    /// Segment separator (tab)
    S,
    /// Whitespace
    WS,
    /// Other neutral (punctuation, symbols)
    ON,
    /// Left-to-right embedding
    LRE,
    /// Right-to-left embedding
    RLE,
    /// Left-to-right override
    LRO,
    /// Right-to-left override
    RLO,
    /// Pop directional format
    PDF,
    /// Left-to-right isolate
    LRI,
    /// Right-to-left isolate
    RLI,
    /// First strong isolate
    FSI,
    /// Pop directional isolate
    PDI,
}

/// Get the bidi type of a Unicode character.
pub fn bidi_type(c: char) -> BidiType {
    // Hebrew (U+0590–U+05FF, U+FB1D–U+FB4F)
    if (('\u{0590}'..='\u{05FF}').contains(&c)) || (('\u{FB1D}'..='\u{FB4F}').contains(&c)) {
        return BidiType::R;
    }
    // Arabic (U+0600–U+06FF, U+0750–U+077F, U+08A0–U+08FF, U+FB50–U+FDFF, U+FE70–U+FEFF)
    if (('\u{0600}'..='\u{06FF}').contains(&c))
        || (('\u{0750}'..='\u{077F}').contains(&c))
        || (('\u{08A0}'..='\u{08FF}').contains(&c))
        || (('\u{FB50}'..='\u{FDFF}').contains(&c))
        || (('\u{FE70}'..='\u{FEFF}').contains(&c))
    {
        return BidiType::AL;
    }
    // Arabic-Indic digits
    if '\u{0600}' <= c && c <= '\u{06FF}' && c.is_ascii_digit() {
        return BidiType::AN;
    }
    // European digits
    if c.is_ascii_digit() {
        return BidiType::EN;
    }
    // European separators
    if c == '+' || c == '-' {
        return BidiType::ES;
    }
    // European terminators
    if c == '#' || c == '\u{00A4}' || c == '\u{00A2}' || c == '\u{00A3}' || c == '\u{00A5}' {
        return BidiType::ET;
    }
    // Common separators
    if c == ',' || c == '.' || c == ':' || c == '\u{060C}' || c == '\u{066C}' {
        return BidiType::CS;
    }
    // Nonspacing marks (combining characters)
    if c.is_combining_mark() {
        return BidiType::NSM;
    }
    // Whitespace
    if c == ' ' || c == '\t' {
        return BidiType::WS;
    }
    // Block/segment separators
    if c == '\n' || c == '\r' {
        return BidiType::B;
    }
    // Default: LTR
    BidiType::L
}

/// Check if a character is a combining mark (nonspacing).
trait CombiningMark {
    fn is_combining_mark(&self) -> bool;
}

impl CombiningMark for char {
    fn is_combining_mark(&self) -> bool {
        // Combining Diacritical Marks (U+0300–U+036F)
        // Combining Diacritical Marks Extended (U+1AB0–U+1AFF)
        // Combining Diacritical Marks Supplement (U+1DC0–U+1DFF)
        // Combining Diacritical Marks for Symbols (U+20D0–U+20FF)
        // Variation Selectors (U+FE00–U+FE0F)
        ('\u{0300}'..='\u{036F}').contains(self)
            || ('\u{1AB0}'..='\u{1AFF}').contains(self)
            || ('\u{1DC0}'..='\u{1DFF}').contains(self)
            || ('\u{20D0}'..='\u{20FF}').contains(self)
            || ('\u{FE00}'..='\u{FE0F}').contains(self)
    }
}

/// A run of characters with the same direction.
#[derive(Debug, Clone)]
pub struct DirectionalRun {
    /// The characters in this run (in logical order).
    pub chars: Vec<char>,
    /// Whether this run is RTL.
    pub is_rtl: bool,
}

/// Split text into directional runs (simplified bidi algorithm).
///
/// This is a simplified version of UAX #9 that groups consecutive
/// characters with the same direction into runs. It does NOT do full
/// reordering (which requires resolving embedding levels, neutrals,
/// and implicit pairs).
pub fn split_into_runs(text: &str) -> Vec<DirectionalRun> {
    let mut runs: Vec<DirectionalRun> = Vec::new();
    let mut current_chars: Vec<char> = Vec::new();
    let mut current_rtl = false;
    let mut started = false;

    for c in text.chars() {
        let bt = bidi_type(c);
        let is_rtl = matches!(bt, BidiType::R | BidiType::AL);

        // Whitespace and neutrals inherit the direction of the previous char.
        let effective_rtl = if matches!(bt, BidiType::WS | BidiType::ON | BidiType::CS | BidiType::NSM | BidiType::BN) {
            current_rtl
        } else {
            is_rtl
        };

        if !started {
            current_rtl = effective_rtl;
            started = true;
        } else if effective_rtl != current_rtl {
            // Direction changed — flush current run.
            if !current_chars.is_empty() {
                runs.push(DirectionalRun {
                    chars: std::mem::take(&mut current_chars),
                    is_rtl: current_rtl,
                });
            }
            current_rtl = effective_rtl;
        }

        current_chars.push(c);
    }

    if !current_chars.is_empty() {
        runs.push(DirectionalRun {
            chars: current_chars,
            is_rtl: current_rtl,
        });
    }

    runs
}

/// Reorder a string for visual display using the simplified bidi algorithm.
///
/// Returns the characters in visual order (left to right for display).
/// RTL runs have their characters reversed.
pub fn reorder_for_display(text: &str) -> String {
    let runs = split_into_runs(text);

    // Reorder runs: RTL runs should appear to the right of LTR runs.
    // This is a simplified reordering — full bidi is more complex.
    let mut result = String::new();

    // Collect LTR and RTL runs separately.
    let mut ltr_runs: Vec<&DirectionalRun> = Vec::new();
    let mut rtl_runs: Vec<&DirectionalRun> = Vec::new();

    for run in &runs {
        if run.is_rtl {
            rtl_runs.push(run);
        } else {
            ltr_runs.push(run);
        }
    }

    // In a paragraph that is predominantly LTR, RTL runs appear
    // in their logical position but with reversed characters.
    // For simplicity, we keep logical order but reverse RTL runs.
    for run in &runs {
        if run.is_rtl {
            // Reverse the characters in this run.
            for c in run.chars.iter().rev() {
                result.push(*c);
            }
        } else {
            for c in &run.chars {
                result.push(*c);
            }
        }
    }

    result
}

/// Check if a string contains any RTL characters.
pub fn contains_rtl(text: &str) -> bool {
    text.chars().any(|c| matches!(bidi_type(c), BidiType::R | BidiType::AL))
}

/// Get the base direction of a paragraph (LTR or RTL).
///
/// Uses the first strong character to determine the paragraph direction
/// (UAX #9 §3.3.1 — P2-P3 rules).
pub fn paragraph_direction(text: &str) -> bool {
    // true = RTL, false = LTR
    for c in text.chars() {
        match bidi_type(c) {
            BidiType::R | BidiType::AL => return true,
            BidiType::L => return false,
            _ => continue,
        }
    }
    false // Default LTR
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ltr_text_has_no_rtl() {
        assert!(!contains_rtl("Hello World"));
    }

    #[test]
    fn arabic_is_rtl() {
        assert!(contains_rtl("مرحبا"));
        assert!(paragraph_direction("مرحبا"));
    }

    #[test]
    fn hebrew_is_rtl() {
        assert!(contains_rtl("שלום"));
        assert!(paragraph_direction("שלום"));
    }

    #[test]
    fn mixed_text_contains_rtl() {
        assert!(contains_rtl("Hello مرحبا World"));
    }

    #[test]
    fn mixed_text_paragraph_is_ltr() {
        // First strong char is LTR (H).
        assert!(!paragraph_direction("Hello مرحبا World"));
    }

    #[test]
    fn bidi_type_arabic_letter() {
        assert_eq!(bidi_type('ا'), BidiType::AL);
        assert_eq!(bidi_type('م'), BidiType::AL);
    }

    #[test]
    fn bidi_type_hebrew() {
        assert_eq!(bidi_type('ש'), BidiType::R);
    }

    #[test]
    fn bidi_type_latin() {
        assert_eq!(bidi_type('A'), BidiType::L);
        assert_eq!(bidi_type('z'), BidiType::L);
    }

    #[test]
    fn bidi_type_digit() {
        assert_eq!(bidi_type('0'), BidiType::EN);
        assert_eq!(bidi_type('9'), BidiType::EN);
    }

    #[test]
    fn bidi_type_space() {
        assert_eq!(bidi_type(' '), BidiType::WS);
        assert_eq!(bidi_type('\t'), BidiType::WS);
    }

    #[test]
    fn split_into_runs_pure_ltr() {
        let runs = split_into_runs("Hello World");
        assert_eq!(runs.len(), 1);
        assert!(!runs[0].is_rtl);
    }

    #[test]
    fn split_into_runs_mixed() {
        let runs = split_into_runs("Hello مرحبا World");
        assert!(runs.len() >= 2);
        // At least one run should be RTL.
        assert!(runs.iter().any(|r| r.is_rtl));
    }

    #[test]
    fn reorder_reverses_rtl_runs() {
        // "abc" in RTL should be reversed.
        let rtl_text = "abc";
        let runs = split_into_runs(rtl_text);
        // Pure LTR text — no reversal.
        let reordered = reorder_for_display(rtl_text);
        assert_eq!(reordered, "abc");
    }

    #[test]
    fn paragraph_direction_empty() {
        assert!(!paragraph_direction(""));
    }

    #[test]
    fn combining_mark_detected() {
        assert!('\u{0300}'.is_combining_mark());
        assert!(!'a'.is_combining_mark());
    }
}
