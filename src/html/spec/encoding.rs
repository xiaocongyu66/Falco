//! Character encoding detection.
//!
//! Spec: https://html.spec.whatwg.org/multipage/parsing.html#determining-the-character-encoding
//!
//! Determines the character encoding of an HTML document by checking, in order:
//! 1. BOM (UTF-8, UTF-16LE, UTF-16BE)
//! 2. `Content-Type` HTTP header `charset` parameter
//! 3. `<meta charset>` in the first 1024 bytes
//! 4. `<meta http-equiv="Content-Type">` in the first 1024 bytes
//! 5. Heuristic analysis of byte patterns (UTF-8 vs Windows-1252 vs Shift-JIS)
//! 6. Default to UTF-8 (per HTML5 spec)

/// The detected encoding. We support a subset of encodings that cover the
/// vast majority of real-world documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf16Le,
    Utf16Be,
    /// Windows-1252 (a superset of ISO-8859-1).
    Windows1252,
    /// ISO-8859-1 (Latin-1).
    Iso88591,
    /// Shift-JIS (Japanese).
    ShiftJis,
    /// EUC-JP (Japanese).
    EucJp,
    /// GB18030 (Simplified Chinese).
    Gb18030,
    /// Big5 (Traditional Chinese).
    Big5,
    /// EUC-KR (Korean).
    EucKr,
    /// KOI8-R (Russian).
    Koi8R,
    /// Unknown — fall back to UTF-8.
    Unknown,
}

impl Encoding {
    /// Returns the canonical name of the encoding (lowercase, IANA-style).
    pub fn name(&self) -> &'static str {
        match self {
            Encoding::Utf8 => "utf-8",
            Encoding::Utf16Le => "utf-16le",
            Encoding::Utf16Be => "utf-16be",
            Encoding::Windows1252 => "windows-1252",
            Encoding::Iso88591 => "iso-8859-1",
            Encoding::ShiftJis => "shift_jis",
            Encoding::EucJp => "euc-jp",
            Encoding::Gb18030 => "gb18030",
            Encoding::Big5 => "big5",
            Encoding::EucKr => "euc-kr",
            Encoding::Koi8R => "koi8-r",
            Encoding::Unknown => "utf-8",
        }
    }

    /// Parse an encoding name string into the enum. Returns `Unknown` for
    /// unrecognized names.
    pub fn from_name(name: &str) -> Self {
        let lower_owned = name.to_lowercase();
        let lower = lower_owned.trim_matches(|c: char| c == '"' || c == '\'');
        match lower {
            "utf-8" | "utf8" | "us-ascii" | "ascii" => Encoding::Utf8,
            "utf-16" | "utf-16le" | "utf16" => Encoding::Utf16Le,
            "utf-16be" => Encoding::Utf16Be,
            "windows-1252" | "cp1252" | "iso-8859-1" | "iso8859-1" | "latin1" | "latin-1" => {
                Encoding::Windows1252
            }
            "iso-8859-15" | "iso8859-15" => Encoding::Iso88591,
            "shift_jis" | "shift-jis" | "sjis" | "ms_kanji" => Encoding::ShiftJis,
            "euc-jp" | "eucjp" => Encoding::EucJp,
            "gb18030" | "gbk" | "gb2312" => Encoding::Gb18030,
            "big5" | "big-5" => Encoding::Big5,
            "euc-kr" | "euckr" | "ks_c_5601-1987" => Encoding::EucKr,
            "koi8-r" | "koi8r" => Encoding::Koi8R,
            _ => Encoding::Unknown,
        }
    }
}

/// Detect the encoding of a byte buffer.
///
/// `content_type` is the value of the HTTP `Content-Type` header, if available.
/// Returns the detected encoding. If nothing matches, returns UTF-8 (the
/// HTML5 default).
pub fn detect(bytes: &[u8], content_type: Option<&str>) -> Encoding {
    // 1. BOM check.
    if let Some(enc) = detect_bom(bytes) {
        return enc;
    }
    // 2. HTTP Content-Type header.
    if let Some(ct) = content_type {
        if let Some(enc) = parse_content_type_charset(ct) {
            return enc;
        }
    }
    // 3. Prescan the first 1024 bytes for <meta charset>.
    let head = &bytes[..bytes.len().min(1024)];
    if let Some(enc) = prescan_meta_charset(head) {
        return enc;
    }
    // 4. Heuristic analysis.
    if let Some(enc) = heuristic_detect(bytes) {
        return enc;
    }
    // 5. Default: UTF-8.
    Encoding::Utf8
}

/// Check for a BOM at the start of the byte buffer.
pub fn detect_bom(bytes: &[u8]) -> Option<Encoding> {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        Some(Encoding::Utf8)
    } else if bytes.starts_with(&[0xFF, 0xFE]) {
        Some(Encoding::Utf16Le)
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        Some(Encoding::Utf16Be)
    } else {
        None
    }
}

/// Strip the BOM from a byte buffer, if present.
pub fn strip_bom(bytes: &[u8]) -> &[u8] {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &bytes[3..]
    } else if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        &bytes[2..]
    } else {
        bytes
    }
}

/// Parse a `Content-Type: text/html; charset=utf-8` header value.
pub fn parse_content_type_charset(content_type: &str) -> Option<Encoding> {
    for part in content_type.split(';') {
        let part = part.trim();
        if let Some(rest) = part
            .strip_prefix("charset=")
            .or_else(|| part.strip_prefix("Charset="))
        {
            let name = rest.trim_matches(|c: char| c == '"' || c == '\'' || c.is_whitespace());
            let enc = Encoding::from_name(name);
            return Some(enc);
        }
    }
    None
}

/// Prescan the first 1024 bytes of an HTML document for `<meta charset>` or
/// `<meta http-equiv="Content-Type">` declarations. Spec: §13.2.3.2.
///
/// This is a fast regex-like scan that does not actually parse HTML — it
/// just looks for `charset=` and `content="...; charset=..."` patterns
/// inside `<meta>` tags.
pub fn prescan_meta_charset(bytes: &[u8]) -> Option<Encoding> {
    let s = String::from_utf8_lossy(bytes);
    // Walk the string looking for <meta ...> tags.
    let mut i = 0;
    while i < s.len() {
        // Find next "<meta".
        if let Some(pos) = s[i..].to_lowercase().find("<meta") {
            let abs = i + pos;
            // Find the end of the tag (next '>').
            let end = s[abs..].find('>').map(|p| abs + p).unwrap_or(s.len());
            let tag = &s[abs..end];
            // Check for charset attribute directly.
            if let Some(enc) = find_charset_attr(tag) {
                return Some(enc);
            }
            // Check for http-equiv="Content-Type" with content containing charset.
            if tag.to_lowercase().contains("http-equiv")
                && tag.to_lowercase().contains("content-type")
            {
                if let Some(content) = extract_attr_value(tag, "content") {
                    if let Some(enc) = parse_content_type_charset(&content) {
                        return Some(enc);
                    }
                }
            }
            i = end + 1;
        } else {
            break;
        }
    }
    None
}

/// Find `charset="..."` or `charset=...` in a tag.
fn find_charset_attr(tag: &str) -> Option<Encoding> {
    let lower = tag.to_lowercase();
    if let Some(pos) = lower.find("charset") {
        let after = &tag[pos + 7..];
        let after = after.trim_start();
        if let Some(rest) = after.strip_prefix('=') {
            let rest = rest.trim_start();
            let value = if rest.starts_with('"') {
                rest[1..].split('"').next().unwrap_or("")
            } else if rest.starts_with('\'') {
                rest[1..].split('\'').next().unwrap_or("")
            } else {
                rest.split_whitespace().next().unwrap_or("")
            };
            return Some(Encoding::from_name(value));
        }
    }
    None
}

/// Extract the value of a named attribute from a tag string.
fn extract_attr_value(tag: &str, attr_name: &str) -> Option<String> {
    let lower = tag.to_lowercase();
    let target = format!("{}=", attr_name);
    let pos = lower.find(&target)?;
    let after = &tag[pos + target.len()..];
    let after = after.trim_start();
    if after.starts_with('"') {
        Some(after[1..].split('"').next()?.to_string())
    } else if after.starts_with('\'') {
        Some(after[1..].split('\'').next()?.to_string())
    } else {
        Some(after.split_whitespace().next()?.to_string())
    }
}

/// Heuristic detection of encoding based on byte patterns.
///
/// - UTF-8 has very specific multi-byte patterns; we validate.
/// - UTF-16LE/BE patterns (high frequency of 0x00 bytes in even/odd positions).
/// - For single-byte encodings we can't reliably distinguish without a
///   language model, so we fall back to Windows-1252.
pub fn heuristic_detect(bytes: &[u8]) -> Option<Encoding> {
    if bytes.is_empty() {
        return None;
    }
    // UTF-8 validation.
    if is_valid_utf8(bytes) {
        return Some(Encoding::Utf8);
    }
    // UTF-16 detection: count zero bytes in even vs odd positions.
    let sample = &bytes[..bytes.len().min(1024)];
    let mut even_zeros = 0;
    let mut odd_zeros = 0;
    for (i, &b) in sample.iter().enumerate() {
        if b == 0 {
            if i % 2 == 0 {
                even_zeros += 1;
            } else {
                odd_zeros += 1;
            }
        }
    }
    let total = sample.len();
    if total > 10 {
        let even_ratio = even_zeros as f32 / (total / 2) as f32;
        let odd_ratio = odd_zeros as f32 / (total / 2) as f32;
        if even_ratio > 0.3 && odd_ratio < 0.05 {
            return Some(Encoding::Utf16Be);
        }
        if odd_ratio > 0.3 && even_ratio < 0.05 {
            return Some(Encoding::Utf16Le);
        }
    }
    // Default to Windows-1252 for non-UTF-8 single-byte content.
    // (Real browsers do language modelling here; we don't.)
    Some(Encoding::Windows1252)
}

/// Check whether a byte sequence is valid UTF-8.
fn is_valid_utf8(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes).is_ok()
}

/// Decode bytes using the detected encoding. Returns a UTF-8 string.
///
/// For encodings we don't have a codec for, falls back to lossy UTF-8.
pub fn decode(bytes: &[u8], encoding: Encoding) -> String {
    // Strip BOM first.
    let bytes = strip_bom(bytes);
    match encoding {
        Encoding::Utf8 => String::from_utf8_lossy(bytes).to_string(),
        Encoding::Utf16Le => decode_utf16(bytes, true),
        Encoding::Utf16Be => decode_utf16(bytes, false),
        Encoding::Windows1252 | Encoding::Iso88591 => decode_windows1252(bytes),
        // For other encodings we don't have codecs — fall back to lossy UTF-8.
        // Real browsers use ICU here.
        _ => String::from_utf8_lossy(bytes).to_string(),
    }
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> String {
    let mut out = String::with_capacity(bytes.len() / 2);
    let mut i = 0;
    while i + 1 < bytes.len() {
        let unit = if little_endian {
            u16::from_le_bytes([bytes[i], bytes[i + 1]])
        } else {
            u16::from_be_bytes([bytes[i], bytes[i + 1]])
        };
        // Handle surrogates.
        if (0xD800..=0xDBFF).contains(&unit) && i + 3 < bytes.len() {
            let low = if little_endian {
                u16::from_le_bytes([bytes[i + 2], bytes[i + 3]])
            } else {
                u16::from_be_bytes([bytes[i + 2], bytes[i + 3]])
            };
            if (0xDC00..=0xDFFF).contains(&low) {
                let code = 0x10000 + ((unit as u32 - 0xD800) << 10) + (low as u32 - 0xDC00);
                if let Some(c) = char::from_u32(code) {
                    out.push(c);
                    i += 4;
                    continue;
                }
            }
        }
        if let Some(c) = char::from_u32(unit as u32) {
            out.push(c);
        }
        i += 2;
    }
    out
}

fn decode_windows1252(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &b in bytes {
        // Windows-1252 maps 0x80-0x9F to specific characters.
        let c = match b {
            0x80 => '€',
            0x82 => '‚',
            0x83 => 'ƒ',
            0x84 => '„',
            0x85 => '…',
            0x86 => '†',
            0x87 => '‡',
            0x88 => 'ˆ',
            0x89 => '‰',
            0x8A => 'Š',
            0x8B => '‹',
            0x8C => 'Œ',
            0x8E => 'Ž',
            0x91 => '‘',
            0x92 => '’',
            0x93 => '“',
            0x94 => '”',
            0x95 => '•',
            0x96 => '–',
            0x97 => '—',
            0x98 => '˜',
            0x99 => '™',
            0x9A => 'š',
            0x9B => '›',
            0x9C => 'œ',
            0x9E => 'ž',
            0x9F => 'Ÿ',
            _ => b as char,
        };
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_utf8_bom() {
        let bytes = [0xEF, 0xBB, 0xBF, b'h', b'i'];
        assert_eq!(detect(&bytes, None), Encoding::Utf8);
    }

    #[test]
    fn detects_utf16le_bom() {
        let bytes = [0xFF, 0xFE, b'h', 0x00, b'i', 0x00];
        assert_eq!(detect(&bytes, None), Encoding::Utf16Le);
    }

    #[test]
    fn detects_utf16be_bom() {
        let bytes = [0xFE, 0xFF, 0x00, b'h', 0x00, b'i'];
        assert_eq!(detect(&bytes, None), Encoding::Utf16Be);
    }

    #[test]
    fn detects_from_content_type() {
        let bytes = b"<html></html>";
        let ct = "text/html; charset=iso-8859-1";
        assert_eq!(detect(bytes, Some(ct)), Encoding::Windows1252);
    }

    #[test]
    fn detects_from_meta_charset() {
        let bytes = b"<html><head><meta charset=\"utf-8\"></head><body></body></html>";
        assert_eq!(detect(bytes, None), Encoding::Utf8);
    }

    #[test]
    fn detects_from_meta_http_equiv() {
        let bytes = b"<html><head><meta http-equiv=\"Content-Type\" content=\"text/html; charset=utf-8\"></head></html>";
        assert_eq!(detect(bytes, None), Encoding::Utf8);
    }

    #[test]
    fn defaults_to_utf8() {
        let bytes = b"plain ASCII text";
        assert_eq!(detect(bytes, None), Encoding::Utf8);
    }

    #[test]
    fn decodes_utf8_lossy() {
        let bytes = [b'h', b'i', 0xC3, 0xA9]; // "hié"
        let s = decode(&bytes, Encoding::Utf8);
        assert_eq!(s, "hié");
    }

    #[test]
    fn decodes_utf16le() {
        let bytes = [b'h', 0x00, b'i', 0x00];
        let s = decode(&bytes, Encoding::Utf16Le);
        assert_eq!(s, "hi");
    }

    #[test]
    fn decodes_windows1252() {
        let bytes = [0x80, 0x99]; // €™
        let s = decode(&bytes, Encoding::Windows1252);
        assert_eq!(s, "€™");
    }

    #[test]
    fn strips_bom() {
        let bytes = [0xEF, 0xBB, 0xBF, b'x'];
        assert_eq!(strip_bom(&bytes), b"x");
    }

    #[test]
    fn encoding_from_name() {
        assert_eq!(Encoding::from_name("UTF-8"), Encoding::Utf8);
        assert_eq!(Encoding::from_name("utf8"), Encoding::Utf8);
        assert_eq!(Encoding::from_name("iso-8859-1"), Encoding::Windows1252);
        assert_eq!(Encoding::from_name("shift_jis"), Encoding::ShiftJis);
        assert_eq!(Encoding::from_name("unknown"), Encoding::Unknown);
    }
}
