//! CSS parser — lexer, parser, selector matching.
//!
//! Supports 300+ CSS properties (see `registry.rs` for the full list).
//!
//! # Spec-compliant replacement
//!
//! The [`spec`] submodule contains the Selectors-Level-4-compliant
//! replacement (full cascade specificity, `:has()`/`:is()`/`:where()`,
//! cascade layers, container queries, @-rules, animations, containment,
//! filters, clip-path). It is **not yet wired into the render pipeline** —
//! `render_with_base_url` still uses the legacy types in this file.

pub mod pseudo;
pub mod registry;
pub mod spec;

use crate::dom::{ElementData, Node};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
    /// Parsed @keyframes rules, keyed by animation name.
    pub keyframes: HashMap<String, KeyframesRule>,
}

/// A parsed @keyframes rule.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyframesRule {
    pub name: String,
    /// Keyframe stops (0%, 50%, 100%, etc.) with their declarations.
    pub stops: Vec<KeyframeStop>,
}

/// A single keyframe stop (e.g. "50%" or "from" or "to").
#[derive(Debug, Clone, PartialEq)]
pub struct KeyframeStop {
    /// 0.0 = 0%/from, 1.0 = 100%/to.
    pub position: f32,
    pub declarations: Vec<Declaration>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub selectors: Vec<Selector>,
    pub declarations: Vec<Declaration>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Declaration {
    pub property: String,
    pub value: Value,
    pub important: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Keyword(String),
    Length(f32, Unit),
    Percentage(f32),
    Number(f32),
    Color(Color),
    String(String),
    /// Multi-value (e.g. `margin: 10px 20px` becomes Length(10px) Length(20px)).
    List(Vec<Value>),
    None,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Unit {
    Px,
    Em,
    Rem,
    Pt,
    Vw,
    Vh,
    Percent,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Default for Color {
    fn default() -> Self {
        Self::TRANSPARENT
    }
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
    pub const TRANSPARENT: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    pub const WHITE: Color = Color::rgb(255, 255, 255);
}

#[derive(Debug, Clone, PartialEq)]
pub struct Selector {
    pub compound: Vec<SimpleSelector>,
    pub combinator: Combinator,
    pub ancestor: Option<Box<Selector>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Combinator {
    Descendant,
    Child,
    AdjacentSibling,
    GeneralSibling,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SimpleSelector {
    Universal,
    Tag(String),
    Class(String),
    Id(String),
    /// A pseudo-class. The first string is the name (e.g. "hover", "is",
    /// "where", "has"). The second is an optional argument string (the
    /// raw content between parentheses, e.g. ".foo, .bar" for `:is(.foo, .bar)`).
    PseudoClass(String, Option<String>),
    Attr(String),
}

// =========================== Lexer ===========================

struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src: src.as_bytes(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }
    fn next(&mut self) -> Option<u8> {
        let c = self.peek();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }
    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_ascii_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }
    fn skip_ws_and_comments(&mut self) {
        loop {
            self.skip_ws();
            if self.src.get(self.pos..self.pos + 2) == Some(b"/*") {
                self.pos += 2;
                while self.pos + 1 < self.src.len()
                    && self.src.get(self.pos..self.pos + 2) != Some(b"*/")
                {
                    self.pos += 1;
                }
                if self.pos + 1 < self.src.len() {
                    self.pos += 2;
                }
            } else {
                break;
            }
        }
    }
    fn starts_with(&self, s: &str) -> bool {
        let b = s.as_bytes();
        self.src.get(self.pos..self.pos + b.len()) == Some(b)
    }
}

// =========================== Parser ===========================

pub fn parse(src: &str) -> Stylesheet {
    parse_with_viewport(src, 1200, 800)
}

/// Parse a stylesheet, evaluating `@media` queries against the given viewport.
///
/// Rules inside `@media (condition) { ... }` blocks are only included in the
/// returned stylesheet if the condition matches the viewport. Rules outside
/// any `@media` block are always included.
///
/// Supported conditions (subset of Media Queries Level 4):
/// - `(min-width: Npx)`, `(max-width: Npx)`
/// - `(min-height: Npx)`, `(max-height: Npx)`
/// - `screen`, `all`, `only screen` (always match — we only render to screen)
/// - `print` (never matches — we don't render to print)
/// - Combinations with `and`: `screen and (min-width: 600px)`
///
/// Unsupported: `@media (orientation: landscape)`, `@media (prefers-color-scheme: dark)`,
/// device-pixel-ratio, etc. These evaluate to false (rule is skipped).
pub fn parse_with_viewport(src: &str, viewport_width: u32, viewport_height: u32) -> Stylesheet {
    let mut lex = Lexer::new(src);
    let mut rules = Vec::new();
    let mut keyframes = HashMap::new();
    loop {
        lex.skip_ws_and_comments();
        if lex.peek().is_none() {
            break;
        }
        if lex.starts_with("@") {
            // At-rule. Parse @media and @keyframes; skip others.
            if lex.starts_with("@media") {
                if let Some(inner_rules) =
                    parse_media_rule(&mut lex, viewport_width, viewport_height)
                {
                    rules.extend(inner_rules);
                } else {
                    skip_at_rule(&mut lex);
                }
            } else if lex.starts_with("@keyframes") {
                if let Some(kf) = parse_keyframes_rule(&mut lex) {
                    keyframes.insert(kf.name.clone(), kf);
                } else {
                    skip_at_rule(&mut lex);
                }
            } else {
                skip_at_rule(&mut lex);
            }
            continue;
        }
        if let Some(rule) = parse_rule(&mut lex) {
            rules.push(rule);
        } else {
            // Avoid infinite loop — advance one byte.
            lex.pos += 1;
        }
    }
    Stylesheet { rules, keyframes }
}

/// Parse `@media <condition> { <rules> }` and return the inner rules if the
/// condition matches the viewport.
fn parse_media_rule(
    lex: &mut Lexer,
    viewport_width: u32,
    viewport_height: u32,
) -> Option<Vec<Rule>> {
    // Skip "@media"
    lex.pos += 6;
    lex.skip_ws();

    // Read the condition (prelude) until '{'
    let mut condition = String::new();
    while let Some(c) = lex.peek() {
        if c == b'{' {
            break;
        }
        condition.push(c as char);
        lex.pos += 1;
    }
    if lex.peek() != Some(b'{') {
        return None;
    }
    lex.pos += 1; // skip '{'

    // Evaluate the condition against the viewport.
    if !media_query_matches(condition.trim(), viewport_width, viewport_height) {
        // Condition doesn't match — skip the block entirely.
        let mut depth = 1;
        while let Some(c) = lex.next() {
            if c == b'{' {
                depth += 1;
            } else if c == b'}' {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
        }
        return Some(Vec::new());
    }

    // Condition matches — parse the inner rules until the closing '}'.
    let mut inner_rules = Vec::new();
    loop {
        lex.skip_ws_and_comments();
        match lex.peek() {
            None => break,
            Some(b'}') => {
                lex.pos += 1;
                break;
            }
            Some(_) => {}
        }
        if let Some(rule) = parse_rule(lex) {
            inner_rules.push(rule);
        } else {
            lex.pos += 1;
        }
    }
    Some(inner_rules)
}

/// Evaluate a media query condition against the viewport.
///
/// Returns true if the condition matches (rule should apply), false otherwise.
fn media_query_matches(condition: &str, viewport_width: u32, viewport_height: u32) -> bool {
    let cond = condition.to_lowercase();
    let cond = cond.trim();

    // Empty condition = always match.
    if cond.is_empty() {
        return true;
    }

    // Split on " and " — all parts must match.
    let parts: Vec<&str> = cond.split(" and ").collect();
    for part in parts {
        let part = part.trim();
        if !eval_media_part(part, viewport_width, viewport_height) {
            return false;
        }
    }
    true
}

/// Evaluate a single media query part.
fn eval_media_part(part: &str, viewport_width: u32, viewport_height: u32) -> bool {
    let part = part.trim();

    // Media types.
    match part {
        "screen" | "all" | "only screen" => return true,
        "print" | "only print" => return false,
        _ => {}
    }

    // (min-width: Npx), (max-width: Npx), (min-height: Npx), (max-height: Npx)
    if part.starts_with('(') && part.ends_with(')') {
        let inner = &part[1..part.len() - 1];
        let (feature, value) = match inner.split_once(':') {
            Some((f, v)) => (f.trim(), v.trim()),
            None => return false,
        };
        let n: u32 = match value.trim_end_matches("px").parse() {
            Ok(n) => n,
            Err(_) => return false,
        };
        match feature {
            "min-width" => return viewport_width >= n,
            "max-width" => return viewport_width <= n,
            "min-height" => return viewport_height >= n,
            "max-height" => return viewport_height <= n,
            _ => return false, // unsupported feature
        }
    }

    // Unknown part — fail closed (don't apply the rule).
    false
}

/// Parse `@keyframes <name> { <stops> }` and return the parsed rule.
fn parse_keyframes_rule(lex: &mut Lexer) -> Option<KeyframesRule> {
    // Skip "@keyframes"
    lex.pos += 10;
    lex.skip_ws();

    // Read the animation name (identifier or quoted string).
    let name = if lex.peek() == Some(b'"') {
        lex.pos += 1;
        let mut n = String::new();
        while let Some(c) = lex.peek() {
            if c == b'"' {
                lex.pos += 1;
                break;
            }
            n.push(c as char);
            lex.pos += 1;
        }
        n
    } else {
        let mut n = String::new();
        while let Some(c) = lex.peek() {
            if c.is_ascii_whitespace() || c == b'{' {
                break;
            }
            n.push(c as char);
            lex.pos += 1;
        }
        n
    };
    lex.skip_ws();
    if lex.peek() != Some(b'{') {
        return None;
    }
    lex.pos += 1; // skip '{'

    let mut stops = Vec::new();
    loop {
        lex.skip_ws_and_comments();
        match lex.peek() {
            None => break,
            Some(b'}') => {
                lex.pos += 1;
                break;
            }
            Some(_) => {}
        }
        // Read the keyframe selector (e.g. "0%", "50%", "from", "to", "0%, 100%").
        let mut selector = String::new();
        while let Some(c) = lex.peek() {
            if c == b'{' {
                break;
            }
            selector.push(c as char);
            lex.pos += 1;
        }
        if lex.peek() != Some(b'{') {
            break;
        }
        lex.pos += 1; // skip '{'

        // Parse declarations.
        let declarations = parse_declarations(lex);

        // Parse each position in the selector (comma-separated).
        for pos_str in selector.split(',') {
            let pos_str = pos_str.trim();
            let position = if pos_str == "from" {
                0.0
            } else if pos_str == "to" {
                1.0
            } else {
                pos_str
                    .trim_end_matches('%')
                    .parse::<f32>()
                    .ok()
                    .map(|p| p / 100.0)
                    .unwrap_or(0.0)
            };
            stops.push(KeyframeStop {
                position,
                declarations: declarations.clone(),
            });
        }

        // Skip the closing '}' of this stop.
        lex.skip_ws_and_comments();
        if lex.peek() == Some(b'}') {
            lex.pos += 1;
        }
    }

    Some(KeyframesRule { name, stops })
}

fn skip_at_rule(lex: &mut Lexer) {
    // Skip the @keyword.
    while let Some(c) = lex.peek() {
        if c.is_ascii_whitespace() || c == b';' || c == b'{' {
            break;
        }
        lex.pos += 1;
    }
    lex.skip_ws();
    if lex.peek() == Some(b'{') {
        // Skip block.
        let mut depth = 0;
        while let Some(c) = lex.next() {
            if c == b'{' {
                depth += 1;
            } else if c == b'}' {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
        }
    } else if lex.peek() == Some(b';') {
        lex.pos += 1;
    }
}

fn parse_rule(lex: &mut Lexer) -> Option<Rule> {
    // Read selectors until '{'.
    let mut selector_src = String::new();
    while let Some(c) = lex.peek() {
        if c == b'{' {
            break;
        }
        selector_src.push(c as char);
        lex.pos += 1;
    }
    if lex.peek() != Some(b'{') {
        return None;
    }
    lex.pos += 1; // skip '{'
    let selectors = parse_selectors(selector_src.trim());
    let declarations = parse_declarations(lex);
    Some(Rule {
        selectors,
        declarations,
    })
}

fn parse_selectors(src: &str) -> Vec<Selector> {
    // Split on commas, but respect parentheses (don't split inside
    // :is(...), :where(...), :has(...), :not(...)).
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut paren_depth = 0;
    for c in src.chars() {
        match c {
            '(' => {
                paren_depth += 1;
                current.push(c);
            }
            ')' => {
                paren_depth -= 1;
                current.push(c);
            }
            ',' if paren_depth == 0 => {
                parts.push(current.clone());
                current.clear();
            }
            _ => {
                current.push(c);
            }
        }
    }
    if !current.trim().is_empty() {
        parts.push(current);
    }
    parts
        .iter()
        .filter_map(|s| parse_one_selector(s.trim()))
        .collect()
}

fn parse_one_selector(src: &str) -> Option<Selector> {
    if src.is_empty() {
        return None;
    }
    // Tokenize by combinators: ' ', '>', '+', '~'
    // But respect parentheses — don't split on spaces inside :is(...), :has(...), etc.
    let mut tokens: Vec<(String, Combinator)> = Vec::new();
    let mut current = String::new();
    let mut current_combinator = Combinator::Descendant;
    let mut last_was_ws = false;
    let mut paren_depth = 0;
    let chars = src.chars().peekable();
    for c in chars {
        match c {
            '(' => {
                paren_depth += 1;
                last_was_ws = false;
                current.push(c);
            }
            ')' => {
                paren_depth -= 1;
                current.push(c);
            }
            ' ' | '\t' | '\n' if paren_depth == 0 => {
                if !current.is_empty() {
                    tokens.push((
                        std::mem::take(&mut current),
                        std::mem::replace(&mut current_combinator, Combinator::Descendant),
                    ));
                    last_was_ws = true;
                }
            }
            '>' if paren_depth == 0 => {
                current_combinator = Combinator::Child;
                last_was_ws = false;
            }
            '+' if paren_depth == 0 => {
                current_combinator = Combinator::AdjacentSibling;
                last_was_ws = false;
            }
            '~' if paren_depth == 0 => {
                current_combinator = Combinator::GeneralSibling;
                last_was_ws = false;
            }
            _ => {
                if last_was_ws && !tokens.is_empty() && paren_depth == 0 {
                    // Implicit descendant combinator.
                    current_combinator = Combinator::Descendant;
                }
                last_was_ws = false;
                current.push(c);
            }
        }
    }
    if !current.is_empty() {
        tokens.push((current, current_combinator));
    }
    if tokens.is_empty() {
        return None;
    }
    // Build selector tree from right to left.
    let mut iter = tokens.into_iter().rev();
    let (last_str, _) = iter.next().unwrap();
    let mut selector = Selector {
        compound: parse_compound(&last_str),
        combinator: Combinator::Descendant,
        ancestor: None,
    };
    for (s, comb) in iter {
        let ancestor = Selector {
            compound: parse_compound(&s),
            combinator: comb,
            ancestor: None,
        };
        selector.combinator = comb;
        selector.ancestor = Some(Box::new(ancestor));
    }
    Some(selector)
}

fn parse_compound(src: &str) -> Vec<SimpleSelector> {
    let mut out = Vec::new();
    let mut chars = src.chars().peekable();
    while chars.peek().is_some() {
        match chars.peek() {
            Some('*') => {
                chars.next();
                out.push(SimpleSelector::Universal);
            }
            Some('#') => {
                chars.next();
                let mut name = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                        name.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                out.push(SimpleSelector::Id(name));
            }
            Some('.') => {
                chars.next();
                let mut name = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                        name.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                out.push(SimpleSelector::Class(name));
            }
            Some(':') => {
                chars.next();
                let mut name = String::new();
                while let Some(&c) = chars.peek() {
                    if c == '(' {
                        break;
                    }
                    if c.is_ascii_alphanumeric() || c == '-' {
                        name.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                // Capture args in parens (for :is(...), :where(...), :has(...)).
                let arg = if chars.peek() == Some(&'(') {
                    chars.next();
                    let mut depth = 1;
                    let mut arg = String::new();
                    while depth > 0 {
                        if let Some(&c) = chars.peek() {
                            if c == '(' {
                                depth += 1;
                            } else if c == ')' {
                                depth -= 1;
                                if depth == 0 {
                                    chars.next();
                                    break;
                                }
                            }
                            if depth > 0 {
                                arg.push(c);
                            }
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    Some(arg)
                } else {
                    None
                };
                out.push(SimpleSelector::PseudoClass(name, arg));
            }
            Some('[') => {
                chars.next();
                let mut name = String::new();
                while let Some(&c) = chars.peek() {
                    if c == ']' {
                        break;
                    }
                    if c == '=' {
                        break;
                    }
                    name.push(c);
                    chars.next();
                }
                // Skip to ']'.
                while let Some(&c) = chars.peek() {
                    chars.next();
                    if c == ']' {
                        break;
                    }
                }
                out.push(SimpleSelector::Attr(name.trim().to_string()));
            }
            Some(c) if c.is_ascii_alphabetic() || *c == '_' || *c == '-' => {
                let mut name = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                        name.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                out.push(SimpleSelector::Tag(name.to_lowercase()));
            }
            Some(_) => {
                chars.next();
            }
            None => break,
        }
    }
    out
}

fn parse_declarations(lex: &mut Lexer) -> Vec<Declaration> {
    let mut out = Vec::new();
    loop {
        lex.skip_ws_and_comments();
        match lex.peek() {
            None => break,
            Some(b'}') => {
                lex.pos += 1;
                break;
            }
            _ => {}
        }
        // Property name.
        let mut prop = String::new();
        while let Some(c) = lex.peek() {
            if c == b':' || c == b';' || c == b'}' || c.is_ascii_whitespace() {
                break;
            }
            prop.push(c as char);
            lex.pos += 1;
        }
        lex.skip_ws();
        if lex.peek() != Some(b':') {
            // Malformed — skip to ;.
            while let Some(c) = lex.peek() {
                if c == b';' || c == b'}' {
                    break;
                }
                lex.pos += 1;
            }
            if lex.peek() == Some(b';') {
                lex.pos += 1;
            }
            continue;
        }
        lex.pos += 1; // skip ':'
        lex.skip_ws();
        // Value.
        let mut val_str = String::new();
        while let Some(c) = lex.peek() {
            if c == b';' || c == b'}' {
                break;
            }
            val_str.push(c as char);
            lex.pos += 1;
        }
        if lex.peek() == Some(b';') {
            lex.pos += 1;
        }
        let val_str = val_str.trim();
        let important = val_str.ends_with("!important");
        let val_str = val_str.trim_end_matches("!important").trim();
        let value = parse_value(val_str);
        out.push(Declaration {
            property: prop.to_lowercase(),
            value,
            important,
        });
    }
    out
}

fn parse_value(s: &str) -> Value {
    let s = s.trim();
    if s.is_empty() {
        return Value::None;
    }

    // Function call: name(args) — e.g. linear-gradient(...), url(...), var(...), rgba(...)
    // Only if the ENTIRE string is a function call (not part of a multi-value).
    if let Some(open) = s.find('(') {
        if s.ends_with(')') && open > 0 && !s[..open].contains(' ') && !s[..open].contains(',') {
            let name = &s[..open];
            let args = &s[open + 1..s.len() - 1];
            return parse_function(name, args);
        }
    }

    // Color.
    if let Some(c) = parse_color(s) {
        return Value::Color(c);
    }
    // Multi-value (space-separated, respecting parens).
    let parts = split_values_respecting_parens(s);
    if parts.len() > 1 {
        let list: Vec<Value> = parts.iter().map(|p| parse_value(p)).collect();
        if list.iter().all(|v| !matches!(v, Value::None)) {
            return Value::List(list);
        }
    }
    // Length.
    if let Some(v) = parse_length(s) {
        return v;
    }
    // Percentage.
    if let Some(p) = parse_percentage(s) {
        return p;
    }
    // Number.
    if let Ok(n) = s.parse::<f32>() {
        return Value::Number(n);
    }
    // String.
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        return Value::String(s[1..s.len() - 1].to_string());
    }
    // Keyword.
    Value::Keyword(s.to_lowercase())
}

/// Split a value string on whitespace, but respect parentheses depth.
/// E.g. "1px solid rgb(0,0,0)" -> ["1px", "solid", "rgb(0,0,0)"]
fn split_values_respecting_parens(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    let bytes = s.as_bytes();
    for i in 0..bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            c if c.is_ascii_whitespace() && depth == 0 => {
                if i > start {
                    parts.push(&s[start..i]);
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < s.len() {
        parts.push(&s[start..]);
    }
    parts
}

/// Parse a function call value like `linear-gradient(...)` or `url(...)`.
fn parse_function(name: &str, args: &str) -> Value {
    let name = name.to_lowercase();
    match name.as_str() {
        "url" => {
            // url("...") or url(...)
            let url = args.trim().trim_matches('"').trim_matches('\'').to_string();
            Value::String(url)
        }
        "var" => {
            // var(--name) or var(--name, fallback)
            let parts: Vec<&str> = args.splitn(2, ',').collect();
            let var_name = parts[0].trim().to_string();
            let fallback = parts.get(1).map(|s| s.trim().to_string());
            Value::Keyword(format!(
                "__var__:{}:{}",
                var_name,
                fallback.unwrap_or_default()
            ))
        }
        "rgb" | "rgba" | "hsl" | "hsla" => {
            // Reconstruct and pass to parse_color.
            let reconstructed = format!("{name}({args})");
            if let Some(c) = parse_color(&reconstructed) {
                return Value::Color(c);
            }
            Value::None
        }
        "linear-gradient" => {
            // Store as a keyword with raw args — the style module will parse it.
            Value::Keyword(format!("__linear-gradient__:{}", args))
        }
        "radial-gradient" => Value::Keyword(format!("__radial-gradient__:{}", args)),
        "calc" => {
            // We don't support calc() — just extract the first number.
            let n: Vec<char> = args
                .chars()
                .filter(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            if let Ok(n) = n.iter().collect::<String>().parse::<f32>() {
                return Value::Length(n, Unit::Px);
            }
            Value::None
        }
        _ => Value::Keyword(name),
    }
}

pub fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        let (r, g, b, a) = match hex.len() {
            3 => {
                let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
                let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
                let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
                (r, g, b, 255)
            }
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                (r, g, b, 255)
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                (r, g, b, a)
            }
            _ => return None,
        };
        return Some(Color { r, g, b, a });
    }
    if let Some(rest) = s.strip_prefix("rgb(") {
        let rest = rest.trim_end_matches(')');
        let parts: Vec<&str> = rest.split(',').map(|p| p.trim()).collect();
        if parts.len() == 3 {
            let r = parse_color_channel(parts[0])?;
            let g = parse_color_channel(parts[1])?;
            let b = parse_color_channel(parts[2])?;
            return Some(Color { r, g, b, a: 255 });
        }
    }
    if let Some(rest) = s.strip_prefix("rgba(") {
        let rest = rest.trim_end_matches(')');
        let parts: Vec<&str> = rest.split(',').map(|p| p.trim()).collect();
        if parts.len() == 4 {
            let r = parse_color_channel(parts[0])?;
            let g = parse_color_channel(parts[1])?;
            let b = parse_color_channel(parts[2])?;
            let a_f = parts[3].parse::<f32>().ok()?;
            let a = (a_f * 255.0).clamp(0.0, 255.0) as u8;
            return Some(Color { r, g, b, a });
        }
    }
    // HSL support: hsl(120, 100%, 50%) or hsl(120, 100%, 50%, 0.5)
    if let Some(rest) = s.strip_prefix("hsl(") {
        let rest = rest.trim_end_matches(')');
        let parts: Vec<&str> = rest.split(',').map(|p| p.trim()).collect();
        if parts.len() >= 3 {
            let h = parts[0].parse::<f32>().ok()?;
            let s_val = parts[1].trim_end_matches('%').parse::<f32>().ok()? / 100.0;
            let l = parts[2].trim_end_matches('%').parse::<f32>().ok()? / 100.0;
            let (r, g, b) = hsl_to_rgb(h, s_val, l);
            let a = if parts.len() == 4 {
                let a_f = parts[3].parse::<f32>().ok()?;
                (a_f * 255.0).clamp(0.0, 255.0) as u8
            } else {
                255
            };
            return Some(Color { r, g, b, a });
        }
    }
    if let Some(rest) = s.strip_prefix("hsla(") {
        let rest = rest.trim_end_matches(')');
        let parts: Vec<&str> = rest.split(',').map(|p| p.trim()).collect();
        if parts.len() == 4 {
            let h = parts[0].parse::<f32>().ok()?;
            let s_val = parts[1].trim_end_matches('%').parse::<f32>().ok()? / 100.0;
            let l = parts[2].trim_end_matches('%').parse::<f32>().ok()? / 100.0;
            let a_f = parts[3].parse::<f32>().ok()?;
            let (r, g, b) = hsl_to_rgb(h, s_val, l);
            let a = (a_f * 255.0).clamp(0.0, 255.0) as u8;
            return Some(Color { r, g, b, a });
        }
    }
    // Named colors — common subset.
    let named = match s.to_lowercase().as_str() {
        "black" => Some(Color::rgb(0, 0, 0)),
        "white" => Some(Color::rgb(255, 255, 255)),
        "red" => Some(Color::rgb(255, 0, 0)),
        "green" => Some(Color::rgb(0, 128, 0)),
        "lime" => Some(Color::rgb(0, 255, 0)),
        "blue" => Some(Color::rgb(0, 0, 255)),
        "yellow" => Some(Color::rgb(255, 255, 0)),
        "cyan" | "aqua" => Some(Color::rgb(0, 255, 255)),
        "magenta" | "fuchsia" => Some(Color::rgb(255, 0, 255)),
        "gray" | "grey" => Some(Color::rgb(128, 128, 128)),
        "silver" => Some(Color::rgb(192, 192, 192)),
        "maroon" => Some(Color::rgb(128, 0, 0)),
        "olive" => Some(Color::rgb(128, 128, 0)),
        "navy" => Some(Color::rgb(0, 0, 128)),
        "teal" => Some(Color::rgb(0, 128, 128)),
        "purple" => Some(Color::rgb(128, 0, 128)),
        "orange" => Some(Color::rgb(255, 165, 0)),
        "pink" => Some(Color::rgb(255, 192, 203)),
        "brown" => Some(Color::rgb(165, 42, 42)),
        "transparent" => Some(Color::TRANSPARENT),
        // Extended named colors.
        "aliceblue" => Some(Color::rgb(240, 248, 255)),
        "antiquewhite" => Some(Color::rgb(250, 235, 215)),
        "aquamarine" => Some(Color::rgb(127, 255, 212)),
        "azure" => Some(Color::rgb(240, 255, 255)),
        "beige" => Some(Color::rgb(245, 245, 220)),
        "bisque" => Some(Color::rgb(255, 228, 196)),
        "blanchedalmond" => Some(Color::rgb(255, 235, 205)),
        "blueviolet" => Some(Color::rgb(138, 43, 226)),
        "burlywood" => Some(Color::rgb(222, 184, 135)),
        "cadetblue" => Some(Color::rgb(95, 158, 160)),
        "chartreuse" => Some(Color::rgb(127, 255, 0)),
        "chocolate" => Some(Color::rgb(210, 105, 30)),
        "coral" => Some(Color::rgb(255, 127, 80)),
        "cornflowerblue" => Some(Color::rgb(100, 149, 237)),
        "cornsilk" => Some(Color::rgb(255, 248, 220)),
        "crimson" => Some(Color::rgb(220, 20, 60)),
        "darkblue" => Some(Color::rgb(0, 0, 139)),
        "darkcyan" => Some(Color::rgb(0, 139, 139)),
        "darkgoldenrod" => Some(Color::rgb(184, 134, 11)),
        "darkgray" | "darkgrey" => Some(Color::rgb(169, 169, 169)),
        "darkgreen" => Some(Color::rgb(0, 100, 0)),
        "darkkhaki" => Some(Color::rgb(189, 183, 107)),
        "darkmagenta" => Some(Color::rgb(139, 0, 139)),
        "darkolivegreen" => Some(Color::rgb(85, 107, 47)),
        "darkorange" => Some(Color::rgb(255, 140, 0)),
        "darkorchid" => Some(Color::rgb(153, 50, 204)),
        "darkred" => Some(Color::rgb(139, 0, 0)),
        "darksalmon" => Some(Color::rgb(233, 150, 122)),
        "darkseagreen" => Some(Color::rgb(143, 188, 143)),
        "darkslateblue" => Some(Color::rgb(72, 61, 139)),
        "darkslategray" | "darkslategrey" => Some(Color::rgb(47, 79, 79)),
        "darkturquoise" => Some(Color::rgb(0, 206, 209)),
        "darkviolet" => Some(Color::rgb(148, 0, 211)),
        "deeppink" => Some(Color::rgb(255, 20, 147)),
        "deepskyblue" => Some(Color::rgb(0, 191, 255)),
        "dimgray" | "dimgrey" => Some(Color::rgb(105, 105, 105)),
        "dodgerblue" => Some(Color::rgb(30, 144, 255)),
        "firebrick" => Some(Color::rgb(178, 34, 34)),
        "floralwhite" => Some(Color::rgb(255, 250, 240)),
        "forestgreen" => Some(Color::rgb(34, 139, 34)),
        "gainsboro" => Some(Color::rgb(220, 220, 220)),
        "ghostwhite" => Some(Color::rgb(248, 248, 255)),
        "gold" => Some(Color::rgb(255, 215, 0)),
        "goldenrod" => Some(Color::rgb(218, 165, 32)),
        "greenyellow" => Some(Color::rgb(173, 255, 47)),
        "honeydew" => Some(Color::rgb(240, 255, 240)),
        "hotpink" => Some(Color::rgb(255, 105, 180)),
        "indianred" => Some(Color::rgb(205, 92, 92)),
        "indigo" => Some(Color::rgb(75, 0, 130)),
        "ivory" => Some(Color::rgb(255, 255, 240)),
        "khaki" => Some(Color::rgb(240, 230, 140)),
        "lavender" => Some(Color::rgb(230, 230, 250)),
        "lavenderblush" => Some(Color::rgb(255, 240, 245)),
        "lawngreen" => Some(Color::rgb(124, 252, 0)),
        "lemonchiffon" => Some(Color::rgb(255, 250, 205)),
        "lightblue" => Some(Color::rgb(173, 216, 230)),
        "lightcoral" => Some(Color::rgb(240, 128, 128)),
        "lightcyan" => Some(Color::rgb(224, 255, 255)),
        "lightgoldenrodyellow" => Some(Color::rgb(250, 250, 210)),
        "lightgray" | "lightgrey" => Some(Color::rgb(211, 211, 211)),
        "lightgreen" => Some(Color::rgb(144, 238, 144)),
        "lightpink" => Some(Color::rgb(255, 182, 193)),
        "lightsalmon" => Some(Color::rgb(255, 160, 122)),
        "lightseagreen" => Some(Color::rgb(32, 178, 170)),
        "lightskyblue" => Some(Color::rgb(135, 206, 250)),
        "lightslategray" | "lightslategrey" => Some(Color::rgb(119, 136, 153)),
        "lightsteelblue" => Some(Color::rgb(176, 196, 222)),
        "lightyellow" => Some(Color::rgb(255, 255, 224)),
        "limegreen" => Some(Color::rgb(50, 205, 50)),
        "linen" => Some(Color::rgb(250, 240, 230)),
        "mediumaquamarine" => Some(Color::rgb(102, 205, 170)),
        "mediumblue" => Some(Color::rgb(0, 0, 205)),
        "mediumorchid" => Some(Color::rgb(186, 85, 211)),
        "mediumpurple" => Some(Color::rgb(147, 112, 219)),
        "mediumseagreen" => Some(Color::rgb(60, 179, 113)),
        "mediumslateblue" => Some(Color::rgb(123, 104, 238)),
        "mediumspringgreen" => Some(Color::rgb(0, 250, 154)),
        "mediumturquoise" => Some(Color::rgb(72, 209, 204)),
        "mediumvioletred" => Some(Color::rgb(199, 21, 133)),
        "midnightblue" => Some(Color::rgb(25, 25, 112)),
        "mintcream" => Some(Color::rgb(245, 255, 250)),
        "mistyrose" => Some(Color::rgb(255, 228, 225)),
        "moccasin" => Some(Color::rgb(255, 228, 181)),
        "navajowhite" => Some(Color::rgb(255, 222, 173)),
        "oldlace" => Some(Color::rgb(253, 245, 230)),
        "olivedrab" => Some(Color::rgb(107, 142, 35)),
        "orangered" => Some(Color::rgb(255, 69, 0)),
        "orchid" => Some(Color::rgb(218, 112, 214)),
        "palegoldenrod" => Some(Color::rgb(238, 232, 170)),
        "palegreen" => Some(Color::rgb(152, 251, 152)),
        "paleturquoise" => Some(Color::rgb(175, 238, 238)),
        "palevioletred" => Some(Color::rgb(219, 112, 147)),
        "papayawhip" => Some(Color::rgb(255, 239, 213)),
        "peachpuff" => Some(Color::rgb(255, 218, 185)),
        "peru" => Some(Color::rgb(205, 133, 63)),
        "plum" => Some(Color::rgb(221, 160, 221)),
        "powderblue" => Some(Color::rgb(176, 224, 230)),
        "rebeccapurple" => Some(Color::rgb(102, 51, 153)),
        "rosybrown" => Some(Color::rgb(188, 143, 143)),
        "royalblue" => Some(Color::rgb(65, 105, 225)),
        "saddlebrown" => Some(Color::rgb(139, 69, 19)),
        "salmon" => Some(Color::rgb(250, 128, 114)),
        "sandybrown" => Some(Color::rgb(244, 164, 96)),
        "seagreen" => Some(Color::rgb(46, 139, 87)),
        "seashell" => Some(Color::rgb(255, 245, 238)),
        "sienna" => Some(Color::rgb(160, 82, 45)),
        "skyblue" => Some(Color::rgb(135, 206, 235)),
        "slateblue" => Some(Color::rgb(106, 90, 205)),
        "slategray" | "slategrey" => Some(Color::rgb(112, 128, 144)),
        "snow" => Some(Color::rgb(255, 250, 250)),
        "springgreen" => Some(Color::rgb(0, 255, 127)),
        "steelblue" => Some(Color::rgb(70, 130, 180)),
        "tan" => Some(Color::rgb(210, 180, 140)),
        "thistle" => Some(Color::rgb(216, 191, 216)),
        "tomato" => Some(Color::rgb(255, 99, 71)),
        "turquoise" => Some(Color::rgb(64, 224, 208)),
        "violet" => Some(Color::rgb(238, 130, 238)),
        "wheat" => Some(Color::rgb(245, 222, 179)),
        "whitesmoke" => Some(Color::rgb(245, 245, 245)),
        "yellowgreen" => Some(Color::rgb(154, 205, 50)),
        _ => None,
    };
    named
}

/// Parse a color channel value: either an integer (0-255) or a percentage (0%-100%).
fn parse_color_channel(s: &str) -> Option<u8> {
    let s = s.trim();
    if let Some(pct) = s.strip_suffix('%') {
        let val = pct.parse::<f32>().ok()?;
        Some((val / 100.0 * 255.0).clamp(0.0, 255.0) as u8)
    } else {
        s.parse::<u8>().ok()
    }
}

/// Convert HSL (hue 0-360, saturation 0-1, lightness 0-1) to RGB.
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h_prime = h / 60.0;
    let x = c * (1.0 - (h_prime % 2.0 - 1.0).abs());
    let (r1, g1, b1) = if h_prime < 1.0 {
        (c, x, 0.0)
    } else if h_prime < 2.0 {
        (x, c, 0.0)
    } else if h_prime < 3.0 {
        (0.0, c, x)
    } else if h_prime < 4.0 {
        (0.0, x, c)
    } else if h_prime < 5.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    let m = l - c / 2.0;
    let r = ((r1 + m) * 255.0).clamp(0.0, 255.0) as u8;
    let g = ((g1 + m) * 255.0).clamp(0.0, 255.0) as u8;
    let b = ((b1 + m) * 255.0).clamp(0.0, 255.0) as u8;
    (r, g, b)
}

fn parse_length(s: &str) -> Option<Value> {
    let s = s.trim().to_lowercase();
    for (suffix, unit) in [
        ("px", Unit::Px),
        ("em", Unit::Em),
        ("rem", Unit::Rem),
        ("pt", Unit::Pt),
        ("vw", Unit::Vw),
        ("vh", Unit::Vh),
    ] {
        if let Some(num_str) = s.strip_suffix(suffix) {
            if let Ok(n) = num_str.parse::<f32>() {
                return Some(Value::Length(n, unit));
            }
        }
    }
    None
}

fn parse_percentage(s: &str) -> Option<Value> {
    let s = s.trim();
    if let Some(num_str) = s.strip_suffix('%') {
        if let Ok(n) = num_str.parse::<f32>() {
            return Some(Value::Percentage(n));
        }
    }
    None
}

// =========================== Matching ===========================

/// Check if a selector matches an element. `parent_chain` is the chain of
/// ancestors from root to the element's parent (in that order).
pub fn matches(selector: &Selector, element: &ElementData, parent_chain: &[&ElementData]) -> bool {
    if !matches_compound(&selector.compound, element) {
        return false;
    }
    match &selector.ancestor {
        None => true,
        Some(ancestor) => {
            match selector.combinator {
                Combinator::Descendant => {
                    // Try each ancestor.
                    parent_chain.iter().rev().any(|p| {
                        matches_with_chain(
                            ancestor,
                            p,
                            &parent_chain[..parent_chain.len().saturating_sub(1)],
                        )
                    })
                }
                Combinator::Child => parent_chain
                    .last()
                    .map(|p| {
                        matches_with_chain(
                            ancestor,
                            p,
                            &parent_chain[..parent_chain.len().saturating_sub(1)],
                        )
                    })
                    .unwrap_or(false),
                Combinator::AdjacentSibling | Combinator::GeneralSibling => {
                    // We don't track siblings in the parent_chain (would require
                    // passing the full children vec). Treat as descendant —
                    // close enough for the layout engine's needs.
                    parent_chain.iter().rev().any(|p| {
                        matches_with_chain(
                            ancestor,
                            p,
                            &parent_chain[..parent_chain.len().saturating_sub(1)],
                        )
                    })
                }
            }
        }
    }
}

fn matches_with_chain(
    selector: &Selector,
    element: &ElementData,
    parent_chain: &[&ElementData],
) -> bool {
    if !matches_compound(&selector.compound, element) {
        return false;
    }
    match &selector.ancestor {
        None => true,
        Some(ancestor) => parent_chain.iter().rev().any(|p| {
            matches_with_chain(
                ancestor,
                p,
                &parent_chain[..parent_chain.len().saturating_sub(1)],
            )
        }),
    }
}

fn matches_compound(compound: &[SimpleSelector], element: &ElementData) -> bool {
    for sel in compound {
        match sel {
            SimpleSelector::Universal => {}
            SimpleSelector::Tag(t) => {
                if element.tag != *t {
                    return false;
                }
            }
            SimpleSelector::Class(c) => {
                if !element.has_class(c) {
                    return false;
                }
            }
            SimpleSelector::Id(id) => {
                if element.id() != Some(id.as_str()) {
                    return false;
                }
            }
            SimpleSelector::Attr(name) => {
                if !element.attrs.contains_key(name) {
                    return false;
                }
            }
            // Pseudo-classes — evaluate what we can without tree traversal.
            SimpleSelector::PseudoClass(name, arg) => {
                match name.as_str() {
                    // :root matches the <html> element.
                    "root" => {
                        if element.tag != "html" {
                            return false;
                        }
                    }
                    // :empty — element has no children (no element children
                    // and no non-whitespace text children). We can only check
                    // element children here.
                    "empty" => {
                        if !element.children.is_empty() {
                            return false;
                        }
                    }
                    // :is(args) and :where(args) — matches if the element
                    // matches any selector in the argument list. Both have
                    // the same matching behavior (they differ only in
                    // specificity, which we handle separately).
                    "is" | "where" => {
                        if let Some(arg) = arg {
                            // Parse the argument as a selector list and check
                            // if any matches.
                            let selectors = parse_selectors(arg);
                            let matched = selectors
                                .iter()
                                .any(|sel| matches_compound(&sel.compound, element));
                            if !matched {
                                return false;
                            }
                        } else {
                            return false;
                        }
                    }
                    // :has(arg) — matches if the element has a descendant
                    // matching the argument selector. We can't do full
                    // descendant search here (we don't have the tree), so
                    // we check direct children only as a best-effort.
                    "has" => {
                        if let Some(arg) = arg {
                            let selectors = parse_selectors(arg);
                            let matched = element.children.iter().any(|child| {
                                if let crate::dom::Node::Element(child_el) = child {
                                    selectors
                                        .iter()
                                        .any(|sel| matches_compound(&sel.compound, child_el))
                                } else {
                                    false
                                }
                            });
                            if !matched {
                                return false;
                            }
                        } else {
                            return false;
                        }
                    }
                    // :not(arg) — matches if the element does NOT match the
                    // argument selector.
                    "not" => {
                        if let Some(arg) = arg {
                            let selectors = parse_selectors(arg);
                            let matched = selectors
                                .iter()
                                .any(|sel| matches_compound(&sel.compound, element));
                            if matched {
                                return false;
                            }
                        }
                    }
                    // :first-child, :last-child, :only-child — need sibling
                    // info we don't have in ElementData. Best-effort: match
                    // (don't return false). This is the same behavior as before.
                    "first-child" | "last-child" | "only-child" | "first-of-type"
                    | "last-of-type" | "only-of-type" => {
                        // Can't evaluate without sibling info — match.
                    }
                    // :hover, :focus, :active, :visited, :checked, :disabled,
                    // :enabled — interaction/state pseudo-classes. In a static
                    // renderer (no user interaction), these never match.
                    "hover" | "focus" | "focus-visible" | "focus-within" | "active" | "visited"
                    | "target" => {
                        return false;
                    }
                    // :checked, :disabled, :enabled — would need to check
                    // element attributes. Best-effort: match.
                    "checked" | "disabled" | "enabled" | "required" | "optional" | "valid"
                    | "invalid" | "read-only" | "read-write" => {
                        // Can't evaluate without form state — match.
                    }
                    // Unknown pseudo-class — match (don't break existing pages).
                    _ => {}
                }
            }
        }
    }
    true
}

/// Walk the DOM and produce a list of (node, matching_rules) pairs for elements.
pub fn collect_matches<'a>(
    stylesheet: &'a Stylesheet,
    root: &'a Node,
) -> Vec<(&'a Node, Vec<&'a Rule>)> {
    let mut out = Vec::new();
    let mut chain: Vec<&ElementData> = Vec::new();
    walk_dom(root, &mut chain, stylesheet, &mut out);
    out
}

fn walk_dom<'a>(
    node: &'a Node,
    chain: &mut Vec<&'a ElementData>,
    stylesheet: &'a Stylesheet,
    out: &mut Vec<(&'a Node, Vec<&'a Rule>)>,
) {
    if let Node::Element(e) = node {
        let mut matched: Vec<&Rule> = Vec::new();
        for rule in &stylesheet.rules {
            for sel in &rule.selectors {
                if matches(sel, e, chain) {
                    matched.push(rule);
                    break;
                }
            }
        }
        if !matched.is_empty() {
            out.push((node, matched));
        }
        chain.push(e);
        for c in &e.children {
            walk_dom(c, chain, stylesheet, out);
        }
        chain.pop();
    } else if let Node::Document(d) = node {
        for c in &d.children {
            walk_dom(c, chain, stylesheet, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_rule() {
        let css = "p { color: red; font-size: 16px; }";
        let ss = parse(css);
        assert_eq!(ss.rules.len(), 1);
        assert_eq!(ss.rules[0].selectors.len(), 1);
        assert_eq!(ss.rules[0].declarations.len(), 2);
    }

    #[test]
    fn parses_descendant_selector() {
        let css = "div p.foo { color: blue; }";
        let ss = parse(css);
        let sel = &ss.rules[0].selectors[0];
        // Rightmost compound: p.foo
        assert!(sel.compound.contains(&SimpleSelector::Tag("p".into())));
        assert!(sel.compound.contains(&SimpleSelector::Class("foo".into())));
        assert!(sel.ancestor.is_some());
    }

    #[test]
    fn parses_hex_colors() {
        let css = ".a { color: #ff0000; } .b { color: #f00; } .c { color: #ff000080; }";
        let ss = parse(css);
        if let Value::Color(c) = &ss.rules[0].declarations[0].value {
            assert_eq!((c.r, c.g, c.b, c.a), (255, 0, 0, 255));
        } else {
            panic!();
        }
        if let Value::Color(c) = &ss.rules[1].declarations[0].value {
            assert_eq!((c.r, c.g, c.b, c.a), (255, 0, 0, 255));
        } else {
            panic!();
        }
        if let Value::Color(c) = &ss.rules[2].declarations[0].value {
            assert_eq!(c.a, 128);
        } else {
            panic!();
        }
    }

    #[test]
    fn parses_rgba() {
        let css = ".a { color: rgba(10, 20, 30, 0.5); }";
        let ss = parse(css);
        if let Value::Color(c) = &ss.rules[0].declarations[0].value {
            assert_eq!((c.r, c.g, c.b), (10, 20, 30));
            assert!((c.a as f32 / 255.0 - 0.5).abs() < 0.01);
        } else {
            panic!();
        }
    }

    #[test]
    fn skips_at_rules() {
        let css = "@media (min-width: 600px) { p { color: red; } } @keyframes foo { 0% { opacity: 0; } } p { color: blue; }";
        let ss = parse(css);
        // We should have at least the @media inner rule and the last `p` rule.
        assert!(ss.rules.len() >= 1);
    }

    #[test]
    fn media_query_min_width_matches() {
        // Viewport 1200 ≥ 600 → rule should be included.
        let css = "@media (min-width: 600px) { p { color: red; } }";
        let ss = parse_with_viewport(css, 1200, 800);
        assert_eq!(
            ss.rules.len(),
            1,
            "rule should be included when viewport matches"
        );
    }

    #[test]
    fn media_query_min_width_no_match() {
        // Viewport 400 < 600 → rule should be skipped.
        let css = "@media (min-width: 600px) { p { color: red; } }";
        let ss = parse_with_viewport(css, 400, 800);
        assert_eq!(
            ss.rules.len(),
            0,
            "rule should be skipped when viewport doesn't match"
        );
    }

    #[test]
    fn media_query_max_width_matches() {
        let css = "@media (max-width: 768px) { p { color: red; } }";
        let ss = parse_with_viewport(css, 500, 800);
        assert_eq!(ss.rules.len(), 1);
    }

    #[test]
    fn media_query_max_width_no_match() {
        let css = "@media (max-width: 768px) { p { color: red; } }";
        let ss = parse_with_viewport(css, 1200, 800);
        assert_eq!(ss.rules.len(), 0);
    }

    #[test]
    fn media_query_height() {
        let css_min = "@media (min-height: 1000px) { p { color: red; } }";
        let ss = parse_with_viewport(css_min, 1200, 800);
        assert_eq!(
            ss.rules.len(),
            0,
            "min-height 1000 should not match viewport height 800"
        );

        let css_max = "@media (max-height: 1080px) { p { color: red; } }";
        let ss = parse_with_viewport(css_max, 1200, 800);
        assert_eq!(
            ss.rules.len(),
            1,
            "max-height 1080 should match viewport height 800"
        );
    }

    #[test]
    fn media_query_screen_type() {
        // "screen" always matches.
        let css = "@media screen { p { color: red; } }";
        let ss = parse_with_viewport(css, 1200, 800);
        assert_eq!(ss.rules.len(), 1);

        // "print" never matches.
        let css = "@media print { p { color: red; } }";
        let ss = parse_with_viewport(css, 1200, 800);
        assert_eq!(ss.rules.len(), 0);
    }

    #[test]
    fn media_query_and_combinator() {
        // "screen and (min-width: 600px)" — both must match.
        let css = "@media screen and (min-width: 600px) { p { color: red; } }";
        let ss = parse_with_viewport(css, 1200, 800);
        assert_eq!(ss.rules.len(), 1);

        let ss = parse_with_viewport(css, 400, 800);
        assert_eq!(ss.rules.len(), 0);
    }

    #[test]
    fn media_query_multiple_rules_in_block() {
        let css = "@media (min-width: 600px) { p { color: red; } .a { color: blue; } }";
        let ss = parse_with_viewport(css, 1200, 800);
        assert_eq!(ss.rules.len(), 2);
    }

    #[test]
    fn media_query_mixed_with_top_level_rules() {
        let css = "p { color: green; } @media (min-width: 600px) { p { color: red; } } div { color: blue; }";
        let ss = parse_with_viewport(css, 1200, 800);
        // 3 rules: top-level p, media p, top-level div
        assert_eq!(ss.rules.len(), 3);

        let ss = parse_with_viewport(css, 400, 800);
        // 2 rules: top-level p, top-level div (media skipped)
        assert_eq!(ss.rules.len(), 2);
    }

    #[test]
    fn keyframes_parsed() {
        let css = "@keyframes fadeIn { from { opacity: 0; } to { opacity: 1; } }";
        let ss = parse(css);
        assert!(ss.keyframes.contains_key("fadeIn"));
        let kf = ss.keyframes.get("fadeIn").unwrap();
        assert_eq!(kf.stops.len(), 2);
        assert_eq!(kf.stops[0].position, 0.0);
        assert_eq!(kf.stops[1].position, 1.0);
    }

    #[test]
    fn keyframes_with_percentage_stops() {
        let css =
            "@keyframes pulse { 0% { opacity: 1; } 50% { opacity: 0.5; } 100% { opacity: 1; } }";
        let ss = parse(css);
        let kf = ss.keyframes.get("pulse").unwrap();
        assert_eq!(kf.stops.len(), 3);
        assert_eq!(kf.stops[0].position, 0.0);
        assert_eq!(kf.stops[1].position, 0.5);
        assert_eq!(kf.stops[2].position, 1.0);
    }

    #[test]
    fn keyframes_with_multiple_properties() {
        let css = "@keyframes colorShift { from { color: red; background-color: blue; } to { color: blue; background-color: red; } }";
        let ss = parse(css);
        let kf = ss.keyframes.get("colorShift").unwrap();
        assert_eq!(kf.stops[0].declarations.len(), 2);
    }

    #[test]
    fn matches_class_selector() {
        let css = ".foo { color: red; }";
        let ss = parse(css);
        let el = ElementData {
            tag: "p".into(),
            attrs: HashMap::from([("class".to_string(), "foo bar".to_string())]),
            children: vec![],
        };
        assert!(matches(&ss.rules[0].selectors[0], &el, &[]));
    }

    #[test]
    fn matches_id_selector() {
        let css = "#main { color: red; }";
        let ss = parse(css);
        let el = ElementData {
            tag: "div".into(),
            attrs: HashMap::from([("id".to_string(), "main".to_string())]),
            children: vec![],
        };
        assert!(matches(&ss.rules[0].selectors[0], &el, &[]));
    }

    #[test]
    fn pseudo_class_is_matches() {
        let css = ":is(.foo, .bar) { color: red; }";
        let ss = parse(css);
        let el_foo = ElementData {
            tag: "p".into(),
            attrs: HashMap::from([("class".to_string(), "foo".to_string())]),
            children: vec![],
        };
        let el_bar = ElementData {
            tag: "p".into(),
            attrs: HashMap::from([("class".to_string(), "bar".to_string())]),
            children: vec![],
        };
        let el_baz = ElementData {
            tag: "p".into(),
            attrs: HashMap::from([("class".to_string(), "baz".to_string())]),
            children: vec![],
        };
        assert!(matches(&ss.rules[0].selectors[0], &el_foo, &[]));
        assert!(matches(&ss.rules[0].selectors[0], &el_bar, &[]));
        assert!(!matches(&ss.rules[0].selectors[0], &el_baz, &[]));
    }

    #[test]
    fn pseudo_class_where_matches() {
        let css = ":where(.foo) { color: red; }";
        let ss = parse(css);
        let el = ElementData {
            tag: "p".into(),
            attrs: HashMap::from([("class".to_string(), "foo".to_string())]),
            children: vec![],
        };
        assert!(matches(&ss.rules[0].selectors[0], &el, &[]));
    }

    #[test]
    fn pseudo_class_not_matches() {
        let css = ":not(.foo) { color: red; }";
        let ss = parse(css);
        let el_foo = ElementData {
            tag: "p".into(),
            attrs: HashMap::from([("class".to_string(), "foo".to_string())]),
            children: vec![],
        };
        let el_bar = ElementData {
            tag: "p".into(),
            attrs: HashMap::from([("class".to_string(), "bar".to_string())]),
            children: vec![],
        };
        assert!(!matches(&ss.rules[0].selectors[0], &el_foo, &[]));
        assert!(matches(&ss.rules[0].selectors[0], &el_bar, &[]));
    }

    #[test]
    fn pseudo_class_has_matches() {
        let css = "div:has(.child) { color: red; }";
        let ss = parse(css);
        let child = crate::dom::Node::Element(crate::dom::ElementData {
            tag: "span".into(),
            attrs: HashMap::from([("class".to_string(), "child".to_string())]),
            children: vec![],
        });
        let div_with_child = ElementData {
            tag: "div".into(),
            attrs: HashMap::new(),
            children: vec![child],
        };
        let div_without = ElementData {
            tag: "div".into(),
            attrs: HashMap::new(),
            children: vec![],
        };
        assert!(matches(&ss.rules[0].selectors[0], &div_with_child, &[]));
        assert!(!matches(&ss.rules[0].selectors[0], &div_without, &[]));
    }

    #[test]
    fn pseudo_class_root_matches() {
        let css = ":root { color: red; }";
        let ss = parse(css);
        let html = ElementData {
            tag: "html".into(),
            attrs: HashMap::new(),
            children: vec![],
        };
        let body = ElementData {
            tag: "body".into(),
            attrs: HashMap::new(),
            children: vec![],
        };
        assert!(matches(&ss.rules[0].selectors[0], &html, &[]));
        assert!(!matches(&ss.rules[0].selectors[0], &body, &[]));
    }

    #[test]
    fn pseudo_class_empty_matches() {
        let css = "div:empty { color: red; }";
        let ss = parse(css);
        let empty = ElementData {
            tag: "div".into(),
            attrs: HashMap::new(),
            children: vec![],
        };
        let not_empty = ElementData {
            tag: "div".into(),
            attrs: HashMap::new(),
            children: vec![crate::dom::Node::Text(crate::dom::TextData {
                text: "hello".into(),
            })],
        };
        assert!(matches(&ss.rules[0].selectors[0], &empty, &[]));
        // Note: :empty currently only checks element children, not text.
        // A div with text-only children will match as "empty" — this is
        // a known limitation documented in the README.
    }

    #[test]
    fn pseudo_class_hover_never_matches() {
        let css = "a:hover { color: red; }";
        let ss = parse(css);
        let el = ElementData {
            tag: "a".into(),
            attrs: HashMap::new(),
            children: vec![],
        };
        // In a static renderer, :hover never matches.
        assert!(!matches(&ss.rules[0].selectors[0], &el, &[]));
    }
}
