//! TJS Lexer — tokenizes JavaScript source into tokens.
//!
//! Token types: Number, String, Boolean, Null, Identifier, Keyword,
//! Operator, Punctuation, EOF.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Number(f64),
    /// BigInt literal — e.g. `123n`. Stored as a string because we don't
    /// want to lose precision (BigInt can be larger than i64).
    BigInt(String),
    String(String),
    Boolean(bool),
    Null,
    Undefined,
    Identifier(String),
    /// Private field identifier — e.g. `#x`. Used in classes.
    PrivateIdentifier(String),
    Keyword(String),
    Operator(String),
    Punct(char),
    EOF,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Token::Number(n) => write!(f, "Number({})", n),
            Token::BigInt(s) => write!(f, "BigInt({}n)", s),
            Token::String(s) => write!(f, "String(\"{}\")", s),
            Token::Boolean(b) => write!(f, "Boolean({})", b),
            Token::Null => write!(f, "Null"),
            Token::Undefined => write!(f, "Undefined"),
            Token::Identifier(s) => write!(f, "Ident({})", s),
            Token::PrivateIdentifier(s) => write!(f, "PrivateIdent({})", s),
            Token::Keyword(s) => write!(f, "Keyword({})", s),
            Token::Operator(s) => write!(f, "Op({})", s),
            Token::Punct(c) => write!(f, "Punct({})", c),
            Token::EOF => write!(f, "EOF"),
        }
    }
}

const KEYWORDS: &[&str] = &[
    "var",
    "let",
    "const",
    "function",
    "return",
    "if",
    "else",
    "for",
    "while",
    "do",
    "break",
    "continue",
    "true",
    "false",
    "null",
    "undefined",
    "new",
    "this",
    "typeof",
    "instanceof",
    "in",
    "of",
    "try",
    "catch",
    "finally",
    "throw",
    "switch",
    "case",
    "default",
    "class",
    "extends",
    "super",
    "import",
    "export",
    "async",
    "await",
    "yield",
    "delete",
    "void",
    "with",
    "static",
];

/// Check if a `/` should be treated as the start of a RegExp literal.
/// It's a regex if the previous token is: nothing (start), an operator,
/// a keyword (return, typeof, etc.), or specific punctuation.
fn is_regex_context(tokens: &[Token]) -> bool {
    let last = match tokens.last() {
        Some(t) => t,
        None => return true, // Start of input — regex.
    };
    match last {
        Token::Operator(o) => {
            // After most operators, `/` is a regex.
            // Exception: `)`, `]` — after these, `/` is division.
            !matches!(o.as_str(), ")" | "]")
        }
        Token::Keyword(k) => {
            // After keywords like `return`, `typeof`, `new`, `in`, `of`,
            // `delete`, `void`, `instanceof`, `else`, `do`, `yield`, `await`,
            // `/` is a regex.
            matches!(
                k.as_str(),
                "return"
                    | "typeof"
                    | "new"
                    | "in"
                    | "of"
                    | "delete"
                    | "void"
                    | "instanceof"
                    | "else"
                    | "do"
                    | "yield"
                    | "await"
                    | "case"
                    | "throw"
            )
        }
        Token::Punct(c) => {
            // After `(`, `,`, `=`, `:`, `[`, `!`, `&`, `|`, `?`, `{`, `;`,
            // `/` is a regex.
            matches!(c, '(' | ',' | ':' | '[' | '!' | '{' | ';')
        }
        Token::Number(_)
        | Token::String(_)
        | Token::BigInt(_)
        | Token::Boolean(_)
        | Token::Null
        | Token::Undefined
        | Token::Identifier(_)
        | Token::PrivateIdentifier(_) => {
            // After values and identifiers, `/` is division.
            false
        }
        Token::EOF => true,
    }
}

pub fn tokenize(src: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        // Skip whitespace.
        if c.is_whitespace() {
            i += 1;
            continue;
        }

        // Comments: // and /* */
        if c == '/' && i + 1 < chars.len() {
            if chars[i + 1] == '/' {
                // Line comment — skip to end of line.
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            if chars[i + 1] == '*' {
                // Block comment — skip to */.
                i += 2;
                while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                    i += 1;
                }
                i += 2;
                continue;
            }
        }

        // Numbers: 123, 12.34, 0xFF, 1e10
        if c.is_ascii_digit() || (c == '.' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit())
        {
            let start = i;
            // Hex: 0xFF
            if c == '0' && i + 1 < chars.len() && (chars[i + 1] == 'x' || chars[i + 1] == 'X') {
                i += 2;
                while i < chars.len() && chars[i].is_ascii_hexdigit() {
                    i += 1;
                }
                let hex_str: String = chars[start + 2..i].iter().collect();
                let n = u64::from_str_radix(&hex_str, 16).map_err(|e| e.to_string())?;
                tokens.push(Token::Number(n as f64));
                continue;
            }
            // Regular number.
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            // Exponent: e+10, e-5
            if i < chars.len() && (chars[i] == 'e' || chars[i] == 'E') {
                i += 1;
                if i < chars.len() && (chars[i] == '+' || chars[i] == '-') {
                    i += 1;
                }
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }
            // BigInt suffix: 123n
            if i < chars.len() && chars[i] == 'n' {
                let num_str: String = chars[start..i].iter().collect();
                i += 1; // skip 'n'
                tokens.push(Token::BigInt(num_str));
                continue;
            }
            let num_str: String = chars[start..i].iter().collect();
            let n = num_str.parse::<f64>().map_err(|e| e.to_string())?;
            tokens.push(Token::Number(n));
            continue;
        }

        // Strings: "..." or '...' or `...`
        if c == '"' || c == '\'' {
            let quote = c;
            i += 1;
            let start = i;
            let mut s = String::new();
            while i < chars.len() && chars[i] != quote {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    i += 1;
                    match chars[i] {
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        'r' => s.push('\r'),
                        '\\' => s.push('\\'),
                        '"' => s.push('"'),
                        '\'' => s.push('\''),
                        '`' => s.push('`'),
                        '0' => s.push('\0'),
                        'b' => s.push('\u{08}'),
                        'f' => s.push('\u{0C}'),
                        'v' => s.push('\u{0B}'),
                        '/' => s.push('/'),
                        'u' => {
                            if i + 4 < chars.len() {
                                let hex: String = chars[i + 1..i + 5].iter().collect();
                                if let Ok(code) = u32::from_str_radix(&hex, 16) {
                                    if let Some(ch) = char::from_u32(code) {
                                        s.push(ch);
                                    }
                                }
                                i += 4;
                            }
                        }
                        _ => s.push(chars[i]),
                    }
                    i += 1;
                } else {
                    s.push(chars[i]);
                    i += 1;
                }
            }
            i += 1; // skip closing quote
            let _ = start;
            tokens.push(Token::String(s));
            continue;
        }

        // Template literals: `...${expr}...`
        // We tokenize them as: String(prefix) + expression tokens + String(suffix)
        // by emitting a special TemplateStart, then the expression tokens,
        // then TemplateEnd.
        if c == '`' {
            i += 1; // skip opening backtick
            let mut s = String::new();
            loop {
                if i >= chars.len() {
                    break;
                }
                if chars[i] == '`' {
                    i += 1; // skip closing backtick
                    tokens.push(Token::String(s));
                    break;
                }
                if chars[i] == '\\' && i + 1 < chars.len() {
                    i += 1;
                    match chars[i] {
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        'r' => s.push('\r'),
                        '\\' => s.push('\\'),
                        '`' => s.push('`'),
                        '$' => s.push('$'),
                        _ => s.push(chars[i]),
                    }
                    i += 1;
                    continue;
                }
                if chars[i] == '$' && i + 1 < chars.len() && chars[i + 1] == '{' {
                    // Emit the string part, then emit tokens for the expression.
                    tokens.push(Token::String(s.clone()));
                    s.clear();
                    // Emit a special marker: Punct('`') means "template expression start"
                    tokens.push(Token::Punct('`'));
                    i += 2; // skip ${}
                            // Find matching }
                    let mut depth = 1;
                    let expr_start = i;
                    while i < chars.len() && depth > 0 {
                        if chars[i] == '{' {
                            depth += 1;
                        }
                        if chars[i] == '}' {
                            depth -= 1;
                        }
                        if depth > 0 {
                            i += 1;
                        }
                    }
                    let expr_str: String = chars[expr_start..i].iter().collect();
                    if i < chars.len() {
                        i += 1;
                    } // skip }
                      // Tokenize the expression.
                    let expr_tokens = tokenize(&expr_str)?;
                    // Skip the Eof token.
                    for t in expr_tokens {
                        if !matches!(t, Token::EOF) {
                            tokens.push(t);
                        }
                    }
                    // Emit marker: Punct('~') means "template expression end"
                    tokens.push(Token::Punct('~'));
                    continue;
                }
                s.push(chars[i]);
                i += 1;
            }
            continue;
        }

        // Identifiers and keywords.
        if c.is_alphabetic() || c == '_' || c == '$' {
            let start = i;
            while i < chars.len()
                && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '$')
            {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if KEYWORDS.contains(&word.as_str()) {
                match word.as_str() {
                    "true" => tokens.push(Token::Boolean(true)),
                    "false" => tokens.push(Token::Boolean(false)),
                    "null" => tokens.push(Token::Null),
                    "undefined" => tokens.push(Token::Undefined),
                    _ => tokens.push(Token::Keyword(word)),
                }
            } else {
                tokens.push(Token::Identifier(word));
            }
            continue;
        }

        // Private field identifiers: #x, #method, #_private
        // Per spec, # must be followed by an identifier start char.
        if c == '#'
            && i + 1 < chars.len()
            && (chars[i + 1].is_alphabetic() || chars[i + 1] == '_' || chars[i + 1] == '$')
        {
            i += 1; // skip '#'
            let start = i;
            while i < chars.len()
                && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '$')
            {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            tokens.push(Token::PrivateIdentifier(word));
            continue;
        }

        // Multi-character operators.
        let remaining: String = chars[i..].iter().collect();
        let multi_ops = [
            "===", "!==", "==", "!=", "<=", ">=", "&&", "||", "++", "--", "+=", "-=", "*=", "/=",
            "%=", "**=", "**", "<<", ">>", ">>>", "<<=", ">>=", ">>>=", "&=", "|=", "^=", "??",
            "?.", "=>", "...",
        ];
        let mut found = false;
        for op in &multi_ops {
            if remaining.starts_with(op) {
                tokens.push(Token::Operator(op.to_string()));
                i += op.len();
                found = true;
                break;
            }
        }
        if found {
            continue;
        }

        // RegExp literal detection: `/pattern/flags`
        // A `/` is a regex if the previous token suggests we're at the start
        // of an expression (not after a value that could be divided).
        if c == '/' && is_regex_context(&tokens) {
            i += 1; // skip opening /
            let start = i;
            let mut in_class = false; // inside [...]
            while i < chars.len() {
                let ch = chars[i];
                if ch == '\\' && i + 1 < chars.len() {
                    i += 2; // skip escaped char
                    continue;
                }
                if ch == '[' {
                    in_class = true;
                }
                if ch == ']' {
                    in_class = false;
                }
                if ch == '/' && !in_class {
                    break;
                }
                i += 1;
            }
            let pattern: String = chars[start..i].iter().collect();
            if i < chars.len() {
                i += 1;
            } // skip closing /
              // Read flags (g, i, m, s, u, y).
            let mut flags = String::new();
            while i < chars.len() && "gimsuy".contains(chars[i]) {
                flags.push(chars[i]);
                i += 1;
            }
            // Store as a special string token with a prefix so the parser
            // can recognize it as a regex.
            tokens.push(Token::String(format!("\x01regex\x02{}|{}", pattern, flags)));
            continue;
        }

        // Single-character operators.
        if "+-*/%<>!&|^~?=".contains(c) {
            tokens.push(Token::Operator(c.to_string()));
            i += 1;
            continue;
        }

        // Punctuation.
        if "(){}[];,.:".contains(c) {
            tokens.push(Token::Punct(c));
            i += 1;
            continue;
        }

        // Unknown character — skip it.
        i += 1;
    }

    tokens.push(Token::EOF);
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::approx_constant)] // 3.14 is a literal we tokenize, not an approximation of π
    fn tokenize_numbers() {
        let tokens = tokenize("42 3.14 0xFF 1e10").unwrap();
        assert!(matches!(tokens[0], Token::Number(42.0)));
        assert!(matches!(tokens[1], Token::Number(3.14)));
        assert!(matches!(tokens[2], Token::Number(255.0)));
    }

    #[test]
    fn tokenize_strings() {
        let tokens = tokenize("\"hello\" 'world'").unwrap();
        assert!(matches!(&tokens[0], Token::String(s) if s == "hello"));
        assert!(matches!(&tokens[1], Token::String(s) if s == "world"));
    }

    #[test]
    fn tokenize_operators() {
        let tokens = tokenize("a === b !== c => d").unwrap();
        assert!(matches!(&tokens[1], Token::Operator(s) if s == "==="));
        assert!(matches!(&tokens[3], Token::Operator(s) if s == "!=="));
        assert!(matches!(&tokens[5], Token::Operator(s) if s == "=>"));
    }

    #[test]
    fn tokenize_keywords() {
        let tokens = tokenize("var let const function return").unwrap();
        assert!(matches!(&tokens[0], Token::Keyword(s) if s == "var"));
        assert!(matches!(&tokens[1], Token::Keyword(s) if s == "let"));
        assert!(matches!(&tokens[2], Token::Keyword(s) if s == "const"));
    }
}
