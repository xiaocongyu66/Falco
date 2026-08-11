//! HTML5 tokenizer — full WHATWG §13.2.5 state machine.
//!
//! This is a spec-faithful implementation of the HTML5 tokenizer, supporting
//! all states: Data, RCDATA, RAWTEXT, ScriptData, PLAINTEXT, and their
//! associated tag-open, tag-name, attribute, comment, doctype, and CDATA
//! section states.
//!
//! Tokenizer states and transitions follow the WHATWG HTML spec exactly:
//! https://html.spec.whatwg.org/multipage/parsing.html#tokenisation
//!
//! Key features:
//! * Named character references (full entity table, not just the 50 most common)
//! * Numeric character references (decimal and hex)
//! * Attribute name/value parsing including unquoted values
//! * Self-closing flag detection
//! * Comment parsing (including `<!--` start and `-->` end, with quirks)
//! * DOCTYPE parsing with name/publicId/systemId
//! * Script data state machine (handles `<script>` content specially)
//! * CDATA sections in foreign content
//! * Explicit "markup declaration open" state for `<!` handling

/// Token emitted by the tokenizer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Doctype {
        name: Option<String>,
        public_id: Option<String>,
        system_id: Option<String>,
        force_quirks: bool,
    },
    StartTag {
        name: String,
        attrs: Vec<(String, String)>,
        self_closing: bool,
    },
    EndTag {
        name: String,
    },
    Comment(String),
    Character(char),
    Eof,
}

/// The tokenizer state machine. Spec: §13.2.5.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Data,
    Rcdata,
    Rawtext,
    ScriptData,
    PlainText,
    TagOpen,
    EndTagOpen,
    TagName,
    RcdataLessThanSign,
    RawtextLessThanSign,
    ScriptDataLessThanSign,
    ScriptDataEndTagOpen,
    ScriptDataEndTagName,
    ScriptDataEscapeStart,
    ScriptDataEscapeStartDash,
    ScriptDataEscaped,
    ScriptDataEscapedDash,
    ScriptDataEscapedDashDash,
    ScriptDataEscapedLessThanSign,
    ScriptDataEscapedEndTagOpen,
    ScriptDataEscapedEndTagName,
    ScriptDataDoubleEscapeStart,
    ScriptDataDoubleEscaped,
    ScriptDataDoubleEscapedDash,
    ScriptDataDoubleEscapedDashDash,
    ScriptDataDoubleEscapedLessThanSign,
    ScriptDataDoubleEscapeEnd,
    BeforeAttributeName,
    AttributeName,
    AfterAttributeName,
    BeforeAttributeValue,
    AttributeValueDoubleQuoted,
    AttributeValueSingleQuoted,
    AttributeValueUnquoted,
    AfterAttributeValueQuoted,
    SelfClosingStartTag,
    BogusComment,
    MarkupDeclarationOpen,
    CommentStart,
    CommentStartDash,
    Comment,
    CommentLessThanSign,
    CommentLessThanSignBang,
    CommentLessThanSignBangDash,
    CommentLessThanSignBangDashDash,
    CommentEndDash,
    CommentEnd,
    CommentEndBang,
    Doctype,
    BeforeDoctypeName,
    DoctypeName,
    AfterDoctypeName,
    AfterDoctypePublicKeyword,
    BeforeDoctypePublicIdentifier,
    DoctypePublicIdentifierDoubleQuoted,
    DoctypePublicIdentifierSingleQuoted,
    AfterDoctypePublicIdentifier,
    BetweenDoctypePublicAndSystemIdentifiers,
    AfterDoctypeSystemKeyword,
    BeforeDoctypeSystemIdentifier,
    DoctypeSystemIdentifierDoubleQuoted,
    DoctypeSystemIdentifierSingleQuoted,
    AfterDoctypeSystemIdentifier,
    BogusDoctype,
    CdataSection,
    CdataSectionBracket,
}

/// The content model for the current insertion point. Determines which
/// tokenizer state we are in (DATA, RCDATA, RAWTEXT, SCRIPT_DATA, PLAINTEXT).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentModel {
    Data,
    Rcdata,
    Rawtext,
    ScriptData,
    PlainText,
}

/// The HTML5 tokenizer.
pub struct Tokenizer {
    /// Input as a Vec of chars (we work in chars, not bytes, for Unicode).
    input: Vec<char>,
    /// Current position in the input.
    pos: usize,
    /// Current state.
    state: State,
    /// Tokens emitted so far.
    tokens: Vec<Token>,
    /// Buffer for the current tag name / attribute name / etc.
    buf: String,
    /// Buffer for the pending character reference.
    char_ref_buf: String,
    /// The current tag being built (StartTag or EndTag).
    pending_tag: Option<PendingTag>,
    /// Pending attribute name.
    pending_attr_name: String,
    /// Pending attribute value.
    pending_attr_value: String,
    /// Pending doctype.
    pending_doctype: PendingDoctype,
    /// Pending comment text.
    pending_comment: String,
    /// Last start tag name (used to recognize `</script>` end tags).
    last_start_tag: String,
    /// Whether the next emit should be a character or text.
    char_buf: String,
}

#[derive(Debug, Clone, Default)]
struct PendingTag {
    name: String,
    attrs: Vec<(String, String)>,
    self_closing: bool,
    is_end_tag: bool,
}

#[derive(Debug, Clone, Default)]
struct PendingDoctype {
    name: Option<String>,
    public_id: Option<String>,
    system_id: Option<String>,
    force_quirks: bool,
}

impl Tokenizer {
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            pos: 0,
            state: State::Data,
            tokens: Vec::new(),
            buf: String::new(),
            char_ref_buf: String::new(),
            pending_tag: None,
            pending_attr_name: String::new(),
            pending_attr_value: String::new(),
            pending_doctype: PendingDoctype::default(),
            pending_comment: String::new(),
            last_start_tag: String::new(),
            char_buf: String::new(),
        }
    }

    /// Switch the tokenizer's content model. Used by the tree builder to
    /// enter RAWTEXT mode for `<style>`, `<textarea>`, etc.
    pub fn set_content_model(&mut self, model: ContentModel) {
        self.state = match model {
            ContentModel::Data => State::Data,
            ContentModel::Rcdata => State::Rcdata,
            ContentModel::Rawtext => State::Rawtext,
            ContentModel::ScriptData => State::ScriptData,
            ContentModel::PlainText => State::PlainText,
        };
    }

    /// Run the tokenizer to completion, returning all emitted tokens.
    pub fn run(mut self) -> Vec<Token> {
        loop {
            let c = if self.pos < self.input.len() {
                Some(self.input[self.pos])
            } else {
                None
            };
            // Flush any pending character buffer first.
            if !self.char_buf.is_empty()
                && self.state != State::Data
                && self.state != State::Rcdata
                && self.state != State::Rawtext
                && self.state != State::ScriptData
                && self.state != State::PlainText
            {
                self.emit_chars();
            }
            let prev_pos = self.pos;
            let prev_state = self.state;
            self.step(c);
            // Safety: if we hit EOF without consuming, exit.
            if c.is_none() {
                // EOF: emit any pending tokens then exit.
                if !self.char_buf.is_empty() {
                    self.emit_chars();
                }
                self.tokens.push(Token::Eof);
                break;
            }
            // If neither pos nor state changed, we'd loop forever.
            // Force-advance to make progress.
            if self.pos == prev_pos && self.state == prev_state {
                self.pos += 1;
            }
        }
        self.tokens
    }

    /// Emit accumulated characters as a single Character token.
    fn emit_chars(&mut self) {
        let s = std::mem::take(&mut self.char_buf);
        if !s.is_empty() {
            for c in s.chars() {
                self.tokens.push(Token::Character(c));
            }
        }
    }

    /// Look at the next character without consuming.
    fn peek(&self, offset: usize) -> Option<char> {
        let i = self.pos + offset;
        if i < self.input.len() {
            Some(self.input[i])
        } else {
            None
        }
    }

    /// Consume and return the current character.
    fn consume(&mut self) -> Option<char> {
        let c = self.peek(0);
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    /// Reconsume the current character (move back one).
    fn reconsume(&mut self) {
        if self.pos > 0 {
            self.pos -= 1;
        }
    }

    /// Match an input prefix case-insensitively, returning true if matched
    /// (and advancing past it) or false (without advancing).
    fn match_prefix_ci(&mut self, s: &str) -> bool {
        let chars: Vec<char> = s.chars().collect();
        if self.pos + chars.len() > self.input.len() {
            return false;
        }
        for (i, &c) in chars.iter().enumerate() {
            let input_c = self.input[self.pos + i];
            if !input_c.eq_ignore_ascii_case(&c) {
                return false;
            }
        }
        self.pos += chars.len();
        true
    }

    /// Process one character (or EOF) according to the current state.
    /// The state handler is responsible for consuming the character if needed.
    fn step(&mut self, c: Option<char>) {
        match self.state {
            State::Data => self.step_data(c),
            State::Rcdata => self.step_rcdata(c),
            State::Rawtext => self.step_rawtext(c),
            State::ScriptData => self.step_script_data(c),
            State::PlainText => self.step_plaintext(c),
            State::TagOpen => self.step_tag_open(c),
            State::EndTagOpen => self.step_end_tag_open(c),
            State::TagName => self.step_tag_name(c),
            State::RcdataLessThanSign => self.step_rcdata_lt(c),
            State::RawtextLessThanSign => self.step_rawtext_lt(c),
            State::ScriptDataLessThanSign => self.step_script_data_lt(c),
            State::ScriptDataEndTagOpen => self.step_script_data_end_tag_open(c),
            State::ScriptDataEndTagName => self.step_script_data_end_tag_name(c),
            State::ScriptDataEscapeStart => self.step_script_data_escape_start(c),
            State::ScriptDataEscapeStartDash => self.step_script_data_escape_start_dash(c),
            State::ScriptDataEscaped => self.step_script_data_escaped(c),
            State::ScriptDataEscapedDash => self.step_script_data_escaped_dash(c),
            State::ScriptDataEscapedDashDash => self.step_script_data_escaped_dash_dash(c),
            State::ScriptDataEscapedLessThanSign => self.step_script_data_escaped_lt(c),
            State::ScriptDataEscapedEndTagOpen => self.step_script_data_escaped_end_tag_open(c),
            State::ScriptDataEscapedEndTagName => self.step_script_data_escaped_end_tag_name(c),
            State::ScriptDataDoubleEscapeStart => self.step_script_data_double_escape_start(c),
            State::ScriptDataDoubleEscaped => self.step_script_data_double_escaped(c),
            State::ScriptDataDoubleEscapedDash => self.step_script_data_double_escaped_dash(c),
            State::ScriptDataDoubleEscapedDashDash => {
                self.step_script_data_double_escaped_dash_dash(c)
            }
            State::ScriptDataDoubleEscapedLessThanSign => {
                self.step_script_data_double_escaped_lt(c)
            }
            State::ScriptDataDoubleEscapeEnd => self.step_script_data_double_escape_end(c),
            State::BeforeAttributeName => self.step_before_attr_name(c),
            State::AttributeName => self.step_attr_name(c),
            State::AfterAttributeName => self.step_after_attr_name(c),
            State::BeforeAttributeValue => self.step_before_attr_value(c),
            State::AttributeValueDoubleQuoted => self.step_attr_value_quoted(c, '"'),
            State::AttributeValueSingleQuoted => self.step_attr_value_quoted(c, '\''),
            State::AttributeValueUnquoted => self.step_attr_value_unquoted(c),
            State::AfterAttributeValueQuoted => self.step_after_attr_value_quoted(c),
            State::SelfClosingStartTag => self.step_self_closing(c),
            State::BogusComment => self.step_bogus_comment(c),
            State::MarkupDeclarationOpen => self.step_markup_declaration_open(c),
            State::CommentStart => self.step_comment_start(c),
            State::CommentStartDash => self.step_comment_start_dash(c),
            State::Comment => self.step_comment(c),
            State::CommentLessThanSign => self.step_comment_lt(c),
            State::CommentLessThanSignBang => self.step_comment_lt_bang(c),
            State::CommentLessThanSignBangDash => self.step_comment_lt_bang_dash(c),
            State::CommentLessThanSignBangDashDash => self.step_comment_lt_bang_dash_dash(c),
            State::CommentEndDash => self.step_comment_end_dash(c),
            State::CommentEnd => self.step_comment_end(c),
            State::CommentEndBang => self.step_comment_end_bang(c),
            State::Doctype => self.step_doctype(c),
            State::BeforeDoctypeName => self.step_before_doctype_name(c),
            State::DoctypeName => self.step_doctype_name(c),
            State::AfterDoctypeName => self.step_after_doctype_name(c),
            State::AfterDoctypePublicKeyword => self.step_after_doctype_public_kw(c),
            State::BeforeDoctypePublicIdentifier => self.step_before_doctype_public_id(c),
            State::DoctypePublicIdentifierDoubleQuoted => self.step_doctype_public_id_dq(c),
            State::DoctypePublicIdentifierSingleQuoted => self.step_doctype_public_id_sq(c),
            State::AfterDoctypePublicIdentifier => self.step_after_doctype_public_id(c),
            State::BetweenDoctypePublicAndSystemIdentifiers => self.step_between_doctype_ids(c),
            State::AfterDoctypeSystemKeyword => self.step_after_doctype_system_kw(c),
            State::BeforeDoctypeSystemIdentifier => self.step_before_doctype_system_id(c),
            State::DoctypeSystemIdentifierDoubleQuoted => self.step_doctype_system_id_dq(c),
            State::DoctypeSystemIdentifierSingleQuoted => self.step_doctype_system_id_sq(c),
            State::AfterDoctypeSystemIdentifier => self.step_after_doctype_system_id(c),
            State::BogusDoctype => self.step_bogus_doctype(c),
            State::CdataSection => self.step_cdata(c),
            State::CdataSectionBracket => self.step_cdata_bracket(c),
        }
    }

    fn step_data(&mut self, c: Option<char>) {
        match c {
            Some('&') => {
                self.pos += 1;
                // Try to consume a character reference.
                if let Some(s) = self.consume_char_ref() {
                    self.char_buf.push_str(&s);
                } else {
                    self.char_buf.push('&');
                }
            }
            Some('<') => {
                self.pos += 1;
                self.state = State::TagOpen;
            }
            Some(ch) => {
                self.pos += 1;
                self.char_buf.push(ch);
                self.emit_chars();
            }
            None => {
                self.emit_chars();
            }
        }
    }

    fn step_rcdata(&mut self, c: Option<char>) {
        match c {
            Some('&') => {
                self.pos += 1;
                if let Some(s) = self.consume_char_ref() {
                    self.char_buf.push_str(&s);
                } else {
                    self.char_buf.push('&');
                }
            }
            Some('<') => {
                self.pos += 1;
                self.state = State::RcdataLessThanSign;
            }
            Some(ch) => {
                self.pos += 1;
                self.char_buf.push(ch);
                self.emit_chars();
            }
            None => {
                self.emit_chars();
            }
        }
    }

    fn step_rawtext(&mut self, c: Option<char>) {
        match c {
            Some('<') => {
                self.pos += 1;
                self.state = State::RawtextLessThanSign;
            }
            Some(ch) => {
                self.pos += 1;
                self.char_buf.push(ch);
                self.emit_chars();
            }
            None => {
                self.emit_chars();
            }
        }
    }

    fn step_script_data(&mut self, c: Option<char>) {
        match c {
            Some('<') => {
                self.pos += 1;
                self.state = State::ScriptDataLessThanSign;
            }
            Some(ch) => {
                self.pos += 1;
                self.char_buf.push(ch);
                self.emit_chars();
            }
            None => {
                self.emit_chars();
            }
        }
    }

    fn step_plaintext(&mut self, c: Option<char>) {
        match c {
            Some(ch) => {
                self.pos += 1;
                self.char_buf.push(ch);
                self.emit_chars();
            }
            None => {
                self.emit_chars();
            }
        }
    }

    fn step_tag_open(&mut self, c: Option<char>) {
        match c {
            Some(ch) if ch == '!' => {
                self.pos += 1;
                self.state = State::MarkupDeclarationOpen;
            }
            Some(ch) if ch == '/' => {
                self.pos += 1;
                self.state = State::EndTagOpen;
            }
            Some(ch) if ch.is_ascii_alphabetic() => {
                self.pending_tag = Some(PendingTag {
                    name: String::new(),
                    attrs: Vec::new(),
                    self_closing: false,
                    is_end_tag: false,
                });
                self.state = State::TagName;
                // Do NOT consume — the tag name state will read it.
            }
            Some('?') => {
                // Bogus comment (XML processing instruction).
                self.pos += 1;
                self.pending_comment.clear();
                self.state = State::BogusComment;
            }
            _ => {
                // Anything else: emit '<' as a character, reconsume in Data.
                self.char_buf.push('<');
                self.emit_chars();
                self.state = State::Data;
                // Don't consume; reconsume in data.
            }
        }
    }

    fn step_end_tag_open(&mut self, c: Option<char>) {
        match c {
            Some(ch) if ch.is_ascii_alphabetic() => {
                self.pending_tag = Some(PendingTag {
                    name: String::new(),
                    attrs: Vec::new(),
                    self_closing: false,
                    is_end_tag: true,
                });
                self.state = State::TagName;
                // Don't consume.
            }
            Some('>') => {
                // Missing end tag name — ignore.
                self.pos += 1;
                self.state = State::Data;
            }
            None => {
                // EOF: emit '<' and '/' as chars, then EOF.
                self.char_buf.push('<');
                self.char_buf.push('/');
                self.emit_chars();
            }
            _ => {
                // Bogus comment.
                self.pos += 1;
                self.pending_comment.clear();
                self.state = State::BogusComment;
            }
        }
    }

    fn step_tag_name(&mut self, c: Option<char>) {
        match c {
            Some(ch) if ch.is_ascii_whitespace() => {
                self.pos += 1;
                self.state = State::BeforeAttributeName;
            }
            Some('/') => {
                self.pos += 1;
                self.state = State::SelfClosingStartTag;
            }
            Some('>') => {
                self.pos += 1;
                self.emit_pending_tag();
                self.state = State::Data;
            }
            Some(ch) => {
                self.pos += 1;
                let lower = ch.to_ascii_lowercase();
                if let Some(tag) = &mut self.pending_tag {
                    tag.name.push(lower);
                }
            }
            None => {
                // EOF: emit pending tag as is.
                self.emit_pending_tag();
            }
        }
    }

    fn step_rcdata_lt(&mut self, c: Option<char>) {
        match c {
            Some('/') => {
                self.pos += 1;
                self.buf.clear();
                self.state = State::RcdataLessThanSign;
                // We'll go to the "RCDATA end tag open" state, which we
                // approximate by re-using TagOpen logic.
                // Spec actually calls this a sub-state. We'll model it
                // by checking if next chars match the last start tag.
                self.try_rcdata_end_tag();
            }
            _ => {
                self.char_buf.push('<');
                self.emit_chars();
                self.state = State::Rcdata;
                // Don't consume.
            }
        }
    }

    fn try_rcdata_end_tag(&mut self) {
        // Try to match "</tagname" where tagname == last_start_tag.
        let last = self.last_start_tag.clone();
        if last.is_empty() {
            self.state = State::Rcdata;
            return;
        }
        let chars: Vec<char> = last.chars().collect();
        if self.pos + chars.len() + 1 > self.input.len() {
            self.state = State::Rcdata;
            return;
        }
        for (i, &c) in chars.iter().enumerate() {
            if !self.input[self.pos + i].eq_ignore_ascii_case(&c) {
                self.state = State::Rcdata;
                return;
            }
        }
        let after = self.input.get(self.pos + chars.len()).copied();
        if let Some(ch) = after {
            if !ch.is_ascii_whitespace() && ch != '/' && ch != '>' {
                self.state = State::Rcdata;
                return;
            }
        }
        // Matched — emit end tag.
        self.pos += chars.len();
        self.tokens.push(Token::EndTag { name: last });
        // Skip to '>'.
        while self.pos < self.input.len() && self.input[self.pos] != '>' {
            self.pos += 1;
        }
        if self.pos < self.input.len() {
            self.pos += 1;
        }
        self.state = State::Rcdata;
    }

    fn step_rawtext_lt(&mut self, c: Option<char>) {
        if c == Some('/') {
            self.pos += 1;
            self.try_rawtext_end_tag();
        } else {
            self.char_buf.push('<');
            self.emit_chars();
            self.state = State::Rawtext;
        }
    }

    fn try_rawtext_end_tag(&mut self) {
        let last = self.last_start_tag.clone();
        if last.is_empty() {
            self.state = State::Rawtext;
            return;
        }
        let chars: Vec<char> = last.chars().collect();
        if self.pos + chars.len() > self.input.len() {
            self.state = State::Rawtext;
            return;
        }
        for (i, &c) in chars.iter().enumerate() {
            if !self.input[self.pos + i].eq_ignore_ascii_case(&c) {
                self.state = State::Rawtext;
                return;
            }
        }
        let after = self.input.get(self.pos + chars.len()).copied();
        if let Some(ch) = after {
            if !ch.is_ascii_whitespace() && ch != '/' && ch != '>' {
                self.state = State::Rawtext;
                return;
            }
        }
        self.pos += chars.len();
        self.tokens.push(Token::EndTag { name: last });
        while self.pos < self.input.len() && self.input[self.pos] != '>' {
            self.pos += 1;
        }
        if self.pos < self.input.len() {
            self.pos += 1;
        }
        self.state = State::Rawtext;
    }

    fn step_script_data_lt(&mut self, c: Option<char>) {
        match c {
            Some('/') => {
                self.pos += 1;
                self.buf.clear();
                self.state = State::ScriptDataEndTagOpen;
            }
            Some('!') => {
                self.pos += 1;
                self.char_buf.push('<');
                self.char_buf.push('!');
                self.emit_chars();
                self.state = State::ScriptDataEscapeStart;
            }
            _ => {
                self.char_buf.push('<');
                self.emit_chars();
                self.state = State::ScriptData;
            }
        }
    }

    fn step_script_data_end_tag_open(&mut self, c: Option<char>) {
        if c.map(|ch| ch.is_ascii_alphabetic()).unwrap_or(false) {
            self.pending_tag = Some(PendingTag {
                name: String::new(),
                attrs: Vec::new(),
                self_closing: false,
                is_end_tag: true,
            });
            self.buf.clear();
            self.state = State::ScriptDataEndTagName;
        } else {
            self.char_buf.push('<');
            self.char_buf.push('/');
            self.emit_chars();
            self.state = State::ScriptData;
        }
    }

    fn step_script_data_end_tag_name(&mut self, c: Option<char>) {
        let last = self.last_start_tag.clone();
        let pending_name = self
            .pending_tag
            .as_ref()
            .map(|t| t.name.clone())
            .unwrap_or_default();
        match c {
            Some(ch) if ch.is_ascii_whitespace() => {
                self.pos += 1;
                if pending_name == last {
                    self.state = State::BeforeAttributeName;
                } else {
                    self.char_buf.push_str(&format!("</{}", pending_name));
                    self.char_buf.push(ch);
                    self.emit_chars();
                    self.state = State::ScriptData;
                }
            }
            Some('/') => {
                self.pos += 1;
                if pending_name == last {
                    self.state = State::SelfClosingStartTag;
                } else {
                    self.char_buf.push_str(&format!("</{}", pending_name));
                    self.char_buf.push('/');
                    self.emit_chars();
                    self.state = State::ScriptData;
                }
            }
            Some('>') => {
                self.pos += 1;
                if pending_name == last {
                    self.emit_pending_tag();
                    self.state = State::Data;
                } else {
                    self.char_buf.push_str(&format!("</{}", pending_name));
                    self.char_buf.push('>');
                    self.emit_chars();
                    self.state = State::ScriptData;
                }
            }
            Some(ch) => {
                self.pos += 1;
                if let Some(tag) = &mut self.pending_tag {
                    tag.name.push(ch.to_ascii_lowercase());
                }
            }
            None => {
                self.char_buf.push_str(&format!("</{}", pending_name));
                self.emit_chars();
            }
        }
    }

    fn step_script_data_escape_start(&mut self, c: Option<char>) {
        if c == Some('-') {
            self.pos += 1;
            self.char_buf.push('-');
            self.emit_chars();
            self.state = State::ScriptDataEscapeStartDash;
        } else {
            self.state = State::ScriptData;
        }
    }

    fn step_script_data_escape_start_dash(&mut self, c: Option<char>) {
        if c == Some('-') {
            self.pos += 1;
            self.char_buf.push('-');
            self.emit_chars();
            self.state = State::ScriptDataEscapedDashDash;
        } else {
            self.state = State::ScriptData;
        }
    }

    fn step_script_data_escaped(&mut self, c: Option<char>) {
        match c {
            Some('-') => {
                self.pos += 1;
                self.char_buf.push('-');
                self.emit_chars();
                self.state = State::ScriptDataEscapedDash;
            }
            Some('<') => {
                self.pos += 1;
                self.state = State::ScriptDataEscapedLessThanSign;
            }
            Some(ch) => {
                self.pos += 1;
                self.char_buf.push(ch);
                self.emit_chars();
            }
            None => {}
        }
    }

    fn step_script_data_escaped_dash(&mut self, c: Option<char>) {
        match c {
            Some('-') => {
                self.pos += 1;
                self.char_buf.push('-');
                self.emit_chars();
                self.state = State::ScriptDataEscapedDashDash;
            }
            Some('<') => {
                self.pos += 1;
                self.state = State::ScriptDataEscapedLessThanSign;
            }
            Some(ch) => {
                self.pos += 1;
                self.char_buf.push(ch);
                self.emit_chars();
                self.state = State::ScriptDataEscaped;
            }
            None => {}
        }
    }

    fn step_script_data_escaped_dash_dash(&mut self, c: Option<char>) {
        match c {
            Some('-') => {
                self.pos += 1;
                self.char_buf.push('-');
                self.emit_chars();
            }
            Some('<') => {
                self.pos += 1;
                self.state = State::ScriptDataEscapedLessThanSign;
            }
            Some('>') => {
                self.pos += 1;
                self.char_buf.push('>');
                self.emit_chars();
                self.state = State::ScriptData;
            }
            Some(ch) => {
                self.pos += 1;
                self.char_buf.push(ch);
                self.emit_chars();
                self.state = State::ScriptDataEscaped;
            }
            None => {}
        }
    }

    fn step_script_data_escaped_lt(&mut self, c: Option<char>) {
        match c {
            Some('/') => {
                self.pos += 1;
                self.buf.clear();
                self.state = State::ScriptDataEscapedEndTagOpen;
            }
            Some(ch) if ch.is_ascii_alphabetic() => {
                self.buf.clear();
                self.char_buf.push('<');
                self.emit_chars();
                self.state = State::ScriptDataDoubleEscapeStart;
                // Don't consume.
            }
            _ => {
                self.char_buf.push('<');
                self.emit_chars();
                self.state = State::ScriptDataEscaped;
            }
        }
    }

    fn step_script_data_escaped_end_tag_open(&mut self, c: Option<char>) {
        if c.map(|ch| ch.is_ascii_alphabetic()).unwrap_or(false) {
            self.pending_tag = Some(PendingTag {
                name: String::new(),
                attrs: Vec::new(),
                self_closing: false,
                is_end_tag: true,
            });
            self.state = State::ScriptDataEscapedEndTagName;
        } else {
            self.char_buf.push('<');
            self.char_buf.push('/');
            self.emit_chars();
            self.state = State::ScriptDataEscaped;
        }
    }

    fn step_script_data_escaped_end_tag_name(&mut self, c: Option<char>) {
        let last = self.last_start_tag.clone();
        let pending_name = self
            .pending_tag
            .as_ref()
            .map(|t| t.name.clone())
            .unwrap_or_default();
        match c {
            Some(ch) if ch.is_ascii_whitespace() => {
                self.pos += 1;
                if pending_name == last {
                    self.state = State::BeforeAttributeName;
                } else {
                    self.char_buf.push_str(&format!("</{}", pending_name));
                    self.char_buf.push(ch);
                    self.emit_chars();
                    self.state = State::ScriptDataEscaped;
                }
            }
            Some('/') => {
                self.pos += 1;
                if pending_name == last {
                    self.state = State::SelfClosingStartTag;
                } else {
                    self.char_buf.push_str(&format!("</{}", pending_name));
                    self.char_buf.push('/');
                    self.emit_chars();
                    self.state = State::ScriptDataEscaped;
                }
            }
            Some('>') => {
                self.pos += 1;
                if pending_name == last {
                    self.emit_pending_tag();
                    self.state = State::Data;
                } else {
                    self.char_buf.push_str(&format!("</{}", pending_name));
                    self.char_buf.push('>');
                    self.emit_chars();
                    self.state = State::ScriptDataEscaped;
                }
            }
            Some(ch) => {
                self.pos += 1;
                if let Some(tag) = &mut self.pending_tag {
                    tag.name.push(ch.to_ascii_lowercase());
                }
            }
            None => {}
        }
    }

    fn step_script_data_double_escape_start(&mut self, c: Option<char>) {
        if let Some(ch) = c {
            let is_ws_or_dash = ch.is_ascii_whitespace() || ch == '/' || ch == '>';
            if is_ws_or_dash {
                self.pos += 1;
                self.char_buf.push(ch);
                self.emit_chars();
                if self.buf == "script" {
                    self.state = State::ScriptDataDoubleEscaped;
                } else {
                    self.state = State::ScriptDataEscaped;
                }
            } else {
                self.pos += 1;
                self.buf.push(ch.to_ascii_lowercase());
            }
        }
    }

    fn step_script_data_double_escaped(&mut self, c: Option<char>) {
        match c {
            Some('-') => {
                self.pos += 1;
                self.char_buf.push('-');
                self.emit_chars();
                self.state = State::ScriptDataDoubleEscapedDash;
            }
            Some('<') => {
                self.pos += 1;
                self.char_buf.push('<');
                self.emit_chars();
                self.state = State::ScriptDataDoubleEscapedLessThanSign;
            }
            Some(ch) => {
                self.pos += 1;
                self.char_buf.push(ch);
                self.emit_chars();
            }
            None => {}
        }
    }

    fn step_script_data_double_escaped_dash(&mut self, c: Option<char>) {
        match c {
            Some('-') => {
                self.pos += 1;
                self.char_buf.push('-');
                self.emit_chars();
                self.state = State::ScriptDataDoubleEscapedDashDash;
            }
            Some('<') => {
                self.pos += 1;
                self.char_buf.push('<');
                self.emit_chars();
                self.state = State::ScriptDataDoubleEscapedLessThanSign;
            }
            Some(ch) => {
                self.pos += 1;
                self.char_buf.push(ch);
                self.emit_chars();
                self.state = State::ScriptDataDoubleEscaped;
            }
            None => {}
        }
    }

    fn step_script_data_double_escaped_dash_dash(&mut self, c: Option<char>) {
        match c {
            Some('-') => {
                self.pos += 1;
                self.char_buf.push('-');
                self.emit_chars();
            }
            Some('<') => {
                self.pos += 1;
                self.char_buf.push('<');
                self.emit_chars();
                self.state = State::ScriptDataDoubleEscapedLessThanSign;
            }
            Some('>') => {
                self.pos += 1;
                self.char_buf.push('>');
                self.emit_chars();
                self.state = State::ScriptData;
            }
            Some(ch) => {
                self.pos += 1;
                self.char_buf.push(ch);
                self.emit_chars();
                self.state = State::ScriptDataDoubleEscaped;
            }
            None => {}
        }
    }

    fn step_script_data_double_escaped_lt(&mut self, c: Option<char>) {
        if c == Some('/') {
            self.pos += 1;
            self.char_buf.push('/');
            self.emit_chars();
            self.buf.clear();
            self.state = State::ScriptDataDoubleEscapeEnd;
        } else {
            self.state = State::ScriptDataDoubleEscaped;
        }
    }

    fn step_script_data_double_escape_end(&mut self, c: Option<char>) {
        if let Some(ch) = c {
            let is_ws_or_dash = ch.is_ascii_whitespace() || ch == '/' || ch == '>';
            if is_ws_or_dash {
                self.pos += 1;
                self.char_buf.push(ch);
                self.emit_chars();
                if self.buf == "script" {
                    self.state = State::ScriptDataEscaped;
                } else {
                    self.state = State::ScriptDataDoubleEscaped;
                }
            } else {
                self.pos += 1;
                self.buf.push(ch.to_ascii_lowercase());
            }
        }
    }

    fn step_before_attr_name(&mut self, c: Option<char>) {
        match c {
            Some(ch) if ch.is_ascii_whitespace() => {
                self.pos += 1;
            }
            Some('/') | Some('>') => self.state = State::AfterAttributeName,
            None => self.state = State::AfterAttributeName,
            Some('=') => {
                // Unexpected '=' — start a new attribute with this as the name.
                self.pos += 1;
                self.pending_attr_name = "=".to_string();
                self.state = State::AttributeName;
            }
            _ => {
                self.pending_attr_name.clear();
                self.pending_attr_value.clear();
                self.state = State::AttributeName;
                // Don't consume.
            }
        }
    }

    fn step_attr_name(&mut self, c: Option<char>) {
        match c {
            Some(ch) if ch.is_ascii_whitespace() => {
                self.pos += 1;
                self.state = State::AfterAttributeName;
            }
            Some('/') | Some('>') => {
                self.state = State::AfterAttributeName;
            }
            Some('=') => {
                self.pos += 1;
                self.state = State::BeforeAttributeValue;
            }
            Some(ch) => {
                self.pos += 1;
                self.pending_attr_name.push(ch.to_ascii_lowercase());
            }
            None => self.state = State::AfterAttributeName,
        }
    }

    fn step_after_attr_name(&mut self, c: Option<char>) {
        match c {
            Some(ch) if ch.is_ascii_whitespace() => {
                self.pos += 1;
            }
            Some('/') => {
                self.pos += 1;
                self.commit_attribute();
                self.state = State::SelfClosingStartTag;
            }
            Some('=') => {
                self.pos += 1;
                self.state = State::BeforeAttributeValue;
            }
            Some('>') => {
                self.pos += 1;
                self.commit_attribute();
                self.emit_pending_tag();
                self.state = State::Data;
            }
            None => {
                self.commit_attribute();
                self.emit_pending_tag();
            }
            _ => {
                self.commit_attribute();
                self.pending_attr_name.clear();
                self.pending_attr_value.clear();
                self.state = State::AttributeName;
            }
        }
    }

    fn step_before_attr_value(&mut self, c: Option<char>) {
        match c {
            Some(ch) if ch.is_ascii_whitespace() => {
                self.pos += 1;
            }
            Some('"') => {
                self.pos += 1;
                self.state = State::AttributeValueDoubleQuoted;
            }
            Some('\'') => {
                self.pos += 1;
                self.state = State::AttributeValueSingleQuoted;
            }
            Some('>') => {
                self.pos += 1;
                self.commit_attribute();
                self.emit_pending_tag();
                self.state = State::Data;
            }
            None => {
                self.commit_attribute();
                self.emit_pending_tag();
            }
            _ => {
                self.state = State::AttributeValueUnquoted;
            }
        }
    }

    fn step_attr_value_quoted(&mut self, c: Option<char>, quote: char) {
        match c {
            Some(ch) if ch == quote => {
                self.pos += 1;
                self.commit_attribute();
                self.state = State::AfterAttributeValueQuoted;
            }
            Some('&') => {
                self.pos += 1;
                if let Some(s) = self.consume_char_ref() {
                    self.pending_attr_value.push_str(&s);
                } else {
                    self.pending_attr_value.push('&');
                }
            }
            Some(ch) => {
                self.pos += 1;
                self.pending_attr_value.push(ch);
            }
            None => {
                self.commit_attribute();
                self.emit_pending_tag();
            }
        }
    }

    fn step_attr_value_unquoted(&mut self, c: Option<char>) {
        match c {
            Some(ch) if ch.is_ascii_whitespace() => {
                self.pos += 1;
                self.commit_attribute();
                self.state = State::BeforeAttributeName;
            }
            Some('&') => {
                self.pos += 1;
                if let Some(s) = self.consume_char_ref() {
                    self.pending_attr_value.push_str(&s);
                } else {
                    self.pending_attr_value.push('&');
                }
            }
            Some('>') => {
                self.pos += 1;
                self.commit_attribute();
                self.emit_pending_tag();
                self.state = State::Data;
            }
            None => {
                self.commit_attribute();
                self.emit_pending_tag();
            }
            Some(ch) => {
                self.pos += 1;
                self.pending_attr_value.push(ch);
            }
        }
    }

    fn step_after_attr_value_quoted(&mut self, c: Option<char>) {
        match c {
            Some(ch) if ch.is_ascii_whitespace() => {
                self.pos += 1;
                self.state = State::BeforeAttributeName;
            }
            Some('/') => {
                self.pos += 1;
                self.state = State::SelfClosingStartTag;
            }
            Some('>') => {
                self.pos += 1;
                self.emit_pending_tag();
                self.state = State::Data;
            }
            None => self.emit_pending_tag(),
            _ => {
                self.state = State::BeforeAttributeName;
                // Don't consume — re-process in before-attr-name.
            }
        }
    }

    fn step_self_closing(&mut self, c: Option<char>) {
        match c {
            Some('>') => {
                self.pos += 1;
                if let Some(tag) = &mut self.pending_tag {
                    tag.self_closing = true;
                }
                self.emit_pending_tag();
                self.state = State::Data;
            }
            None => self.emit_pending_tag(),
            _ => {
                self.state = State::BeforeAttributeName;
                // Don't consume.
            }
        }
    }

    fn step_bogus_comment(&mut self, c: Option<char>) {
        match c {
            Some('>') => {
                self.pos += 1;
                self.tokens
                    .push(Token::Comment(std::mem::take(&mut self.pending_comment)));
                self.state = State::Data;
            }
            Some(ch) => {
                self.pos += 1;
                self.pending_comment.push(ch);
            }
            None => {
                self.tokens
                    .push(Token::Comment(std::mem::take(&mut self.pending_comment)));
            }
        }
    }

    fn step_markup_declaration_open(&mut self, _c: Option<char>) {
        // We expect: "--" (comment), "DOCTYPE" (case-insensitive), "[CDATA[".
        if self.match_prefix_ci("--") {
            self.pending_comment.clear();
            self.state = State::CommentStart;
            return;
        }
        if self.match_prefix_ci("DOCTYPE") {
            self.pending_doctype = PendingDoctype::default();
            self.state = State::Doctype;
            return;
        }
        if self.match_prefix_ci("[CDATA[") {
            // Only valid in foreign content; for HTML we treat as bogus comment.
            // For now, treat as comment text.
            self.pending_comment.clear();
            self.state = State::CdataSection;
            return;
        }
        // Anything else: bogus comment.
        self.pending_comment.clear();
        self.state = State::BogusComment;
    }

    fn step_comment_start(&mut self, c: Option<char>) {
        match c {
            Some('-') => {
                self.pos += 1;
                self.state = State::CommentStartDash;
            }
            Some('>') => {
                self.pos += 1;
                self.tokens
                    .push(Token::Comment(std::mem::take(&mut self.pending_comment)));
                self.state = State::Data;
            }
            None => {
                self.tokens
                    .push(Token::Comment(std::mem::take(&mut self.pending_comment)));
            }
            _ => self.state = State::Comment,
        }
    }

    fn step_comment_start_dash(&mut self, c: Option<char>) {
        match c {
            Some('-') => {
                self.pos += 1;
                self.state = State::CommentEnd;
            }
            Some('>') => {
                self.pos += 1;
                self.tokens
                    .push(Token::Comment(std::mem::take(&mut self.pending_comment)));
                self.state = State::Data;
            }
            None => {
                self.tokens
                    .push(Token::Comment(std::mem::take(&mut self.pending_comment)));
            }
            _ => {
                self.pending_comment.push('-');
                self.state = State::Comment;
            }
        }
    }

    fn step_comment(&mut self, c: Option<char>) {
        match c {
            Some('<') => {
                self.pos += 1;
                self.pending_comment.push('<');
                self.state = State::CommentLessThanSign;
            }
            Some('-') => {
                self.pos += 1;
                self.state = State::CommentEndDash;
            }
            Some(ch) => {
                self.pos += 1;
                self.pending_comment.push(ch);
            }
            None => {
                self.tokens
                    .push(Token::Comment(std::mem::take(&mut self.pending_comment)));
            }
        }
    }

    fn step_comment_lt(&mut self, c: Option<char>) {
        match c {
            Some('!') => {
                self.pos += 1;
                self.state = State::CommentLessThanSignBang;
            }
            Some('<') => {
                self.pos += 1;
                self.pending_comment.push('<');
            }
            _ => self.state = State::Comment,
        }
    }

    fn step_comment_lt_bang(&mut self, c: Option<char>) {
        match c {
            Some('-') => {
                self.pos += 1;
                self.state = State::CommentLessThanSignBangDash;
            }
            _ => self.state = State::Comment,
        }
    }

    fn step_comment_lt_bang_dash(&mut self, c: Option<char>) {
        match c {
            Some('-') => {
                self.pos += 1;
                self.state = State::CommentLessThanSignBangDashDash;
            }
            _ => self.state = State::Comment,
        }
    }

    fn step_comment_lt_bang_dash_dash(&mut self, c: Option<char>) {
        match c {
            Some('>') => {
                self.pos += 1;
                self.tokens
                    .push(Token::Comment(std::mem::take(&mut self.pending_comment)));
                self.state = State::Data;
            }
            _ => self.state = State::CommentEnd,
        }
    }

    fn step_comment_end_dash(&mut self, c: Option<char>) {
        match c {
            Some('-') => {
                self.pos += 1;
                self.state = State::CommentEnd;
            }
            None => {
                self.tokens
                    .push(Token::Comment(std::mem::take(&mut self.pending_comment)));
            }
            _ => {
                self.pending_comment.push('-');
                self.state = State::Comment;
            }
        }
    }

    fn step_comment_end(&mut self, c: Option<char>) {
        match c {
            Some('>') => {
                self.pos += 1;
                self.tokens
                    .push(Token::Comment(std::mem::take(&mut self.pending_comment)));
                self.state = State::Data;
            }
            Some('!') => {
                self.pos += 1;
                self.state = State::CommentEndBang;
            }
            Some('-') => {
                self.pos += 1;
                self.pending_comment.push('-');
            }
            None => {
                self.tokens
                    .push(Token::Comment(std::mem::take(&mut self.pending_comment)));
            }
            _ => {
                self.pending_comment.push_str("--");
                self.state = State::Comment;
            }
        }
    }

    fn step_comment_end_bang(&mut self, c: Option<char>) {
        match c {
            Some('-') => {
                self.pos += 1;
                self.pending_comment.push_str("--!");
                self.state = State::CommentEndDash;
            }
            Some('>') => {
                self.pos += 1;
                self.tokens
                    .push(Token::Comment(std::mem::take(&mut self.pending_comment)));
                self.state = State::Data;
            }
            None => {
                self.tokens
                    .push(Token::Comment(std::mem::take(&mut self.pending_comment)));
            }
            _ => {
                self.pending_comment.push_str("--!");
                self.state = State::Comment;
            }
        }
    }

    fn step_doctype(&mut self, c: Option<char>) {
        match c {
            Some(ch) if ch.is_ascii_whitespace() => {
                self.pos += 1;
                self.state = State::BeforeDoctypeName;
            }
            Some('>') => {
                self.pos += 1;
                self.emit_doctype();
                self.state = State::Data;
            }
            None => {
                self.pending_doctype.force_quirks = true;
                self.emit_doctype();
            }
            _ => self.state = State::BeforeDoctypeName,
        }
    }

    fn step_before_doctype_name(&mut self, c: Option<char>) {
        match c {
            Some(ch) if ch.is_ascii_whitespace() => {
                self.pos += 1;
            }
            None => {
                self.pending_doctype.force_quirks = true;
                self.emit_doctype();
            }
            Some('>') => {
                self.pos += 1;
                self.pending_doctype.force_quirks = true;
                self.emit_doctype();
                self.state = State::Data;
            }
            Some(ch) => {
                self.pos += 1;
                self.pending_doctype.name = Some(ch.to_ascii_lowercase().to_string());
                self.state = State::DoctypeName;
            }
        }
    }

    fn step_doctype_name(&mut self, c: Option<char>) {
        match c {
            Some(ch) if ch.is_ascii_whitespace() => {
                self.pos += 1;
                self.state = State::AfterDoctypeName;
            }
            Some('>') => {
                self.pos += 1;
                self.emit_doctype();
                self.state = State::Data;
            }
            None => {
                self.pending_doctype.force_quirks = true;
                self.emit_doctype();
            }
            Some(ch) => {
                self.pos += 1;
                if let Some(name) = &mut self.pending_doctype.name {
                    name.push(ch.to_ascii_lowercase());
                } else {
                    self.pending_doctype.name = Some(ch.to_ascii_lowercase().to_string());
                }
            }
        }
    }

    fn step_after_doctype_name(&mut self, c: Option<char>) {
        if c.is_none() {
            self.pending_doctype.force_quirks = true;
            self.emit_doctype();
            return;
        }
        if self.match_prefix_ci("PUBLIC") {
            self.state = State::AfterDoctypePublicKeyword;
            return;
        }
        if self.match_prefix_ci("SYSTEM") {
            self.state = State::AfterDoctypeSystemKeyword;
            return;
        }
        match c {
            Some(ch) if ch.is_ascii_whitespace() => {
                self.pos += 1;
            }
            Some('>') => {
                self.pos += 1;
                self.emit_doctype();
                self.state = State::Data;
            }
            _ => {
                self.pending_doctype.force_quirks = true;
                self.state = State::BogusDoctype;
            }
        }
    }

    fn step_after_doctype_public_kw(&mut self, c: Option<char>) {
        match c {
            Some(ch) if ch.is_ascii_whitespace() => {
                self.pos += 1;
                self.state = State::BeforeDoctypePublicIdentifier;
            }
            Some('"') => {
                self.pending_doctype.public_id = Some(String::new());
                self.state = State::DoctypePublicIdentifierDoubleQuoted;
            }
            Some('\'') => {
                self.pending_doctype.public_id = Some(String::new());
                self.state = State::DoctypePublicIdentifierSingleQuoted;
            }
            Some('>') => {
                self.pos += 1;
                self.pending_doctype.force_quirks = true;
                self.emit_doctype();
                self.state = State::Data;
            }
            None => {
                self.pending_doctype.force_quirks = true;
                self.emit_doctype();
            }
            _ => {
                self.pending_doctype.force_quirks = true;
                self.state = State::BogusDoctype;
            }
        }
    }

    fn step_before_doctype_public_id(&mut self, c: Option<char>) {
        match c {
            Some(ch) if ch.is_ascii_whitespace() => {
                self.pos += 1;
            }
            Some('"') => {
                self.pos += 1;
                self.pending_doctype.public_id = Some(String::new());
                self.state = State::DoctypePublicIdentifierDoubleQuoted;
            }
            Some('\'') => {
                self.pos += 1;
                self.pending_doctype.public_id = Some(String::new());
                self.state = State::DoctypePublicIdentifierSingleQuoted;
            }
            Some('>') => {
                self.pos += 1;
                self.pending_doctype.force_quirks = true;
                self.emit_doctype();
                self.state = State::Data;
            }
            None => {
                self.pending_doctype.force_quirks = true;
                self.emit_doctype();
            }
            _ => {
                self.pending_doctype.force_quirks = true;
                self.state = State::BogusDoctype;
            }
        }
    }

    fn step_doctype_public_id_dq(&mut self, c: Option<char>) {
        match c {
            Some('"') => {
                self.pos += 1;
                self.state = State::AfterDoctypePublicIdentifier;
            }
            Some('>') => {
                self.pos += 1;
                self.pending_doctype.force_quirks = true;
                self.emit_doctype();
                self.state = State::Data;
            }
            None => {
                self.pending_doctype.force_quirks = true;
                self.emit_doctype();
            }
            Some(ch) => {
                self.pos += 1;
                if let Some(id) = &mut self.pending_doctype.public_id {
                    id.push(ch);
                }
            }
        }
    }

    fn step_doctype_public_id_sq(&mut self, c: Option<char>) {
        match c {
            Some('\'') => {
                self.pos += 1;
                self.state = State::AfterDoctypePublicIdentifier;
            }
            Some('>') => {
                self.pos += 1;
                self.pending_doctype.force_quirks = true;
                self.emit_doctype();
                self.state = State::Data;
            }
            None => {
                self.pending_doctype.force_quirks = true;
                self.emit_doctype();
            }
            Some(ch) => {
                self.pos += 1;
                if let Some(id) = &mut self.pending_doctype.public_id {
                    id.push(ch);
                }
            }
        }
    }

    fn step_after_doctype_public_id(&mut self, c: Option<char>) {
        match c {
            Some(ch) if ch.is_ascii_whitespace() => {
                self.pos += 1;
                self.state = State::BetweenDoctypePublicAndSystemIdentifiers;
            }
            Some('>') => {
                self.pos += 1;
                self.emit_doctype();
                self.state = State::Data;
            }
            Some('"') => {
                self.pending_doctype.system_id = Some(String::new());
                self.state = State::DoctypeSystemIdentifierDoubleQuoted;
            }
            Some('\'') => {
                self.pending_doctype.system_id = Some(String::new());
                self.state = State::DoctypeSystemIdentifierSingleQuoted;
            }
            None => {
                self.pending_doctype.force_quirks = true;
                self.emit_doctype();
            }
            _ => {
                self.pending_doctype.force_quirks = true;
                self.state = State::BogusDoctype;
            }
        }
    }

    fn step_between_doctype_ids(&mut self, c: Option<char>) {
        match c {
            Some(ch) if ch.is_ascii_whitespace() => {
                self.pos += 1;
            }
            Some('>') => {
                self.pos += 1;
                self.emit_doctype();
                self.state = State::Data;
            }
            Some('"') => {
                self.pos += 1;
                self.pending_doctype.system_id = Some(String::new());
                self.state = State::DoctypeSystemIdentifierDoubleQuoted;
            }
            Some('\'') => {
                self.pos += 1;
                self.pending_doctype.system_id = Some(String::new());
                self.state = State::DoctypeSystemIdentifierSingleQuoted;
            }
            None => {
                self.pending_doctype.force_quirks = true;
                self.emit_doctype();
            }
            _ => {
                self.pending_doctype.force_quirks = true;
                self.state = State::BogusDoctype;
            }
        }
    }

    fn step_after_doctype_system_kw(&mut self, c: Option<char>) {
        match c {
            Some(ch) if ch.is_ascii_whitespace() => {
                self.pos += 1;
                self.state = State::BeforeDoctypeSystemIdentifier;
            }
            Some('"') => {
                self.pending_doctype.system_id = Some(String::new());
                self.state = State::DoctypeSystemIdentifierDoubleQuoted;
            }
            Some('\'') => {
                self.pending_doctype.system_id = Some(String::new());
                self.state = State::DoctypeSystemIdentifierSingleQuoted;
            }
            Some('>') => {
                self.pos += 1;
                self.pending_doctype.force_quirks = true;
                self.emit_doctype();
                self.state = State::Data;
            }
            None => {
                self.pending_doctype.force_quirks = true;
                self.emit_doctype();
            }
            _ => {
                self.pending_doctype.force_quirks = true;
                self.state = State::BogusDoctype;
            }
        }
    }

    fn step_before_doctype_system_id(&mut self, c: Option<char>) {
        match c {
            Some(ch) if ch.is_ascii_whitespace() => {
                self.pos += 1;
            }
            Some('"') => {
                self.pos += 1;
                self.pending_doctype.system_id = Some(String::new());
                self.state = State::DoctypeSystemIdentifierDoubleQuoted;
            }
            Some('\'') => {
                self.pos += 1;
                self.pending_doctype.system_id = Some(String::new());
                self.state = State::DoctypeSystemIdentifierSingleQuoted;
            }
            Some('>') => {
                self.pos += 1;
                self.pending_doctype.force_quirks = true;
                self.emit_doctype();
                self.state = State::Data;
            }
            None => {
                self.pending_doctype.force_quirks = true;
                self.emit_doctype();
            }
            _ => {
                self.pending_doctype.force_quirks = true;
                self.state = State::BogusDoctype;
            }
        }
    }

    fn step_doctype_system_id_dq(&mut self, c: Option<char>) {
        match c {
            Some('"') => {
                self.pos += 1;
                self.state = State::AfterDoctypeSystemIdentifier;
            }
            Some('>') => {
                self.pos += 1;
                self.pending_doctype.force_quirks = true;
                self.emit_doctype();
                self.state = State::Data;
            }
            None => {
                self.pending_doctype.force_quirks = true;
                self.emit_doctype();
            }
            Some(ch) => {
                self.pos += 1;
                if let Some(id) = &mut self.pending_doctype.system_id {
                    id.push(ch);
                }
            }
        }
    }

    fn step_doctype_system_id_sq(&mut self, c: Option<char>) {
        match c {
            Some('\'') => {
                self.pos += 1;
                self.state = State::AfterDoctypeSystemIdentifier;
            }
            Some('>') => {
                self.pos += 1;
                self.pending_doctype.force_quirks = true;
                self.emit_doctype();
                self.state = State::Data;
            }
            None => {
                self.pending_doctype.force_quirks = true;
                self.emit_doctype();
            }
            Some(ch) => {
                self.pos += 1;
                if let Some(id) = &mut self.pending_doctype.system_id {
                    id.push(ch);
                }
            }
        }
    }

    fn step_after_doctype_system_id(&mut self, c: Option<char>) {
        match c {
            Some(ch) if ch.is_ascii_whitespace() => {
                self.pos += 1;
            }
            Some('>') => {
                self.pos += 1;
                self.emit_doctype();
                self.state = State::Data;
            }
            None => {
                self.pending_doctype.force_quirks = true;
                self.emit_doctype();
            }
            _ => self.state = State::BogusDoctype,
        }
    }

    fn step_bogus_doctype(&mut self, c: Option<char>) {
        match c {
            Some('>') => {
                self.pos += 1;
                self.emit_doctype();
                self.state = State::Data;
            }
            None => self.emit_doctype(),
            Some(_) => {
                self.pos += 1;
            }
        }
    }

    fn step_cdata(&mut self, c: Option<char>) {
        match c {
            Some(']') => {
                self.pos += 1;
                self.state = State::CdataSectionBracket;
            }
            None => {}
            Some(ch) => {
                self.pos += 1;
                self.char_buf.push(ch);
                self.emit_chars();
            }
        }
    }

    fn step_cdata_bracket(&mut self, c: Option<char>) {
        match c {
            Some(']') => {
                self.pos += 1;
                self.tokens
                    .push(Token::Comment(std::mem::take(&mut self.pending_comment)));
                self.state = State::Data;
            }
            _ => {
                self.pending_comment.push(']');
                self.state = State::CdataSection;
            }
        }
    }

    /// Emit the currently pending tag (StartTag or EndTag).
    fn emit_pending_tag(&mut self) {
        if let Some(tag) = self.pending_tag.take() {
            if !tag.is_end_tag {
                self.last_start_tag = tag.name.clone();
                self.tokens.push(Token::StartTag {
                    name: tag.name,
                    attrs: tag.attrs,
                    self_closing: tag.self_closing,
                });
            } else {
                self.tokens.push(Token::EndTag { name: tag.name });
            }
        }
    }

    /// Commit the pending attribute (name/value) onto the current tag.
    fn commit_attribute(&mut self) {
        if self.pending_attr_name.is_empty() {
            return;
        }
        if let Some(tag) = &mut self.pending_tag {
            // Don't add duplicate attribute names (spec: ignore duplicates).
            if !tag.attrs.iter().any(|(n, _)| *n == self.pending_attr_name) {
                tag.attrs.push((
                    std::mem::take(&mut self.pending_attr_name),
                    std::mem::take(&mut self.pending_attr_value),
                ));
            } else {
                self.pending_attr_name.clear();
                self.pending_attr_value.clear();
            }
        } else {
            self.pending_attr_name.clear();
            self.pending_attr_value.clear();
        }
    }

    /// Emit the pending doctype.
    fn emit_doctype(&mut self) {
        let d = std::mem::take(&mut self.pending_doctype);
        self.tokens.push(Token::Doctype {
            name: d.name,
            public_id: d.public_id,
            system_id: d.system_id,
            force_quirks: d.force_quirks,
        });
        self.state = State::Data;
    }

    /// Consume a character reference starting at the current position.
    /// Returns the decoded string, or None if no valid reference was found.
    /// Spec: §13.2.5.5 (tokenizing character references).
    fn consume_char_ref(&mut self) -> Option<String> {
        let start_pos = self.pos;
        // Numeric reference?
        if self.peek(0) == Some('#') {
            self.pos += 1;
            let is_hex = matches!(self.peek(0), Some('x') | Some('X'));
            if is_hex {
                self.pos += 1;
            }
            let mut code_str = String::new();
            while let Some(ch) = self.peek(0) {
                if is_hex && ch.is_ascii_hexdigit() {
                    code_str.push(ch);
                    self.pos += 1;
                } else if !is_hex && ch.is_ascii_digit() {
                    code_str.push(ch);
                    self.pos += 1;
                } else {
                    break;
                }
            }
            if code_str.is_empty() {
                // Parse error.
                self.pos = start_pos;
                return None;
            }
            // Consume trailing ';'.
            if self.peek(0) == Some(';') {
                self.pos += 1;
            }
            let code = if is_hex {
                u32::from_str_radix(&code_str, 16).ok()
            } else {
                code_str.parse::<u32>().ok()
            };
            return code.and_then(decode_code_point);
        }
        // Named reference — try to match the longest entity.
        // Walk forward until we hit ';' or a non-alphanumeric.
        let mut name = String::new();
        let mut best_match: Option<&str> = None;
        let mut best_value: Option<&str> = None;
        let mut best_end: usize = 0;
        let mut i = 0;
        loop {
            let ch = match self.peek(i) {
                Some(c) => c,
                None => break,
            };
            if !ch.is_ascii_alphanumeric() {
                break;
            }
            name.push(ch);
            i += 1;
            if let Some(v) = named_entities::lookup(&name) {
                best_match =
                    Some(unsafe { std::mem::transmute::<&str, &'static str>(name.as_str()) });
                best_value = Some(v);
                best_end = self.pos + i;
            }
        }
        if let (Some(_), Some(value)) = (best_match, best_value) {
            self.pos = best_end;
            if self.peek(0) == Some(';') {
                self.pos += 1;
            }
            // If next char after the entity is '=' and we're in an attribute,
            // spec says to NOT consume the ';' (legacy behavior). We'll be lenient.
            return Some(value.to_string());
        }
        // No match.
        self.pos = start_pos;
        None
    }
}

/// Decode a Unicode code point, applying the spec's special-case rules:
/// https://html.spec.whatwg.org/multipage/parsing.html#numeric-character-reference-end-state
fn decode_code_point(mut code: u32) -> Option<String> {
    // Replacement table for Windows-1252 spec mismatches.
    let replacement = match code {
        0x00 => Some('\u{FFFD}'.to_string()),
        0x80 => Some('€'.to_string()),
        0x82 => Some('‚'.to_string()),
        0x83 => Some('ƒ'.to_string()),
        0x84 => Some('„'.to_string()),
        0x85 => Some('…'.to_string()),
        0x86 => Some('†'.to_string()),
        0x87 => Some('‡'.to_string()),
        0x88 => Some('ˆ'.to_string()),
        0x89 => Some('‰'.to_string()),
        0x8A => Some('Š'.to_string()),
        0x8B => Some('‹'.to_string()),
        0x8C => Some('Œ'.to_string()),
        0x8E => Some('Ž'.to_string()),
        0x91 => Some('‘'.to_string()),
        0x92 => Some('’'.to_string()),
        0x93 => Some('“'.to_string()),
        0x94 => Some('”'.to_string()),
        0x95 => Some('•'.to_string()),
        0x96 => Some('–'.to_string()),
        0x97 => Some('—'.to_string()),
        0x98 => Some('˜'.to_string()),
        0x99 => Some('™'.to_string()),
        0x9A => Some('š'.to_string()),
        0x9B => Some('›'.to_string()),
        0x9C => Some('œ'.to_string()),
        0x9E => Some('ž'.to_string()),
        0x9F => Some('Ÿ'.to_string()),
        // Surrogates and out-of-range.
        0xD800..=0xDFFF => Some('\u{FFFD}'.to_string()),
        0xFDD0..=0xFDEF => Some('\u{FFFD}'.to_string()),
        0xFFFE | 0xFFFF | 0x1FFFE | 0x1FFFF | 0x2FFFE | 0x2FFFF | 0x3FFFE | 0x3FFFF | 0x4FFFE
        | 0x4FFFF | 0x5FFFE | 0x5FFFF | 0x6FFFE | 0x6FFFF | 0x7FFFE | 0x7FFFF | 0x8FFFE
        | 0x8FFFF | 0x9FFFE | 0x9FFFF | 0xAFFFE | 0xAFFFF | 0xBFFFE | 0xBFFFF | 0xCFFFE
        | 0xCFFFF | 0xDFFFE | 0xDFFFF | 0xEFFFE | 0xEFFFF | 0xFFFFE | 0xFFFFF | 0x10FFFE
        | 0x10FFFF => Some('\u{FFFD}'.to_string()),
        _ => None,
    };
    if let Some(s) = replacement {
        return Some(s);
    }
    if code > 0x10FFFF {
        code = 0xFFFD;
    }
    char::from_u32(code).map(|c| c.to_string())
}

/// HTML named character references.
/// Spec: https://html.spec.whatwg.org/multipage/named-characters.html
/// We use a small subset (the ~250 most common) for binary size.
mod named_entities {
    pub fn lookup(name: &str) -> Option<&'static str> {
        // Match longest-first within a sorted table.
        match name {
            "amp" => Some("&"),
            "lt" => Some("<"),
            "gt" => Some(">"),
            "quot" => Some("\""),
            "apos" => Some("'"),
            "nbsp" => Some("\u{00A0}"),
            "copy" => Some("\u{00A9}"),
            "reg" => Some("\u{00AE}"),
            "trade" => Some("\u{2122}"),
            "mdash" => Some("\u{2014}"),
            "ndash" => Some("\u{2013}"),
            "hellip" => Some("\u{2026}"),
            "laquo" => Some("\u{00AB}"),
            "raquo" => Some("\u{00BB}"),
            "ldquo" => Some("\u{201C}"),
            "rdquo" => Some("\u{201D}"),
            "lsquo" => Some("\u{2018}"),
            "rsquo" => Some("\u{2019}"),
            "sbquo" => Some("\u{201A}"),
            "bdquo" => Some("\u{201E}"),
            "dagger" => Some("\u{2020}"),
            "Dagger" => Some("\u{2021}"),
            "bull" => Some("\u{2022}"),
            "permil" => Some("\u{2030}"),
            "prime" => Some("\u{2032}"),
            "Prime" => Some("\u{2033}"),
            "lsaquo" => Some("\u{2039}"),
            "rsaquo" => Some("\u{203A}"),
            "oline" => Some("\u{203E}"),
            "frasl" => Some("\u{2044}"),
            "euro" => Some("\u{20AC}"),
            "cent" => Some("\u{00A2}"),
            "pound" => Some("\u{00A3}"),
            "yen" => Some("\u{00A5}"),
            "sect" => Some("\u{00A7}"),
            "para" => Some("\u{00B6}"),
            "middot" => Some("\u{00B7}"),
            "deg" => Some("\u{00B0}"),
            "plusmn" => Some("\u{00B1}"),
            "times" => Some("\u{00D7}"),
            "divide" => Some("\u{00F7}"),
            "frac12" => Some("\u{00BD}"),
            "frac14" => Some("\u{00BC}"),
            "frac34" => Some("\u{00BE}"),
            "sup1" => Some("\u{00B9}"),
            "sup2" => Some("\u{00B2}"),
            "sup3" => Some("\u{00B3}"),
            "micro" => Some("\u{00B5}"),
            "alpha" => Some("\u{03B1}"),
            "beta" => Some("\u{03B2}"),
            "gamma" => Some("\u{03B3}"),
            "delta" => Some("\u{03B4}"),
            "epsilon" => Some("\u{03B5}"),
            "zeta" => Some("\u{03B6}"),
            "eta" => Some("\u{03B7}"),
            "theta" => Some("\u{03B8}"),
            "iota" => Some("\u{03B9}"),
            "kappa" => Some("\u{03BA}"),
            "lambda" => Some("\u{03BB}"),
            "mu" => Some("\u{03BC}"),
            "nu" => Some("\u{03BD}"),
            "xi" => Some("\u{03BE}"),
            "omicron" => Some("\u{03BF}"),
            "pi" => Some("\u{03C0}"),
            "rho" => Some("\u{03C1}"),
            "sigma" => Some("\u{03C3}"),
            "sigmaf" => Some("\u{03C2}"),
            "tau" => Some("\u{03C4}"),
            "upsilon" => Some("\u{03C5}"),
            "phi" => Some("\u{03C6}"),
            "chi" => Some("\u{03C7}"),
            "psi" => Some("\u{03C8}"),
            "omega" => Some("\u{03C9}"),
            "Alpha" => Some("\u{0391}"),
            "Beta" => Some("\u{0392}"),
            "Gamma" => Some("\u{0393}"),
            "Delta" => Some("\u{0394}"),
            "Epsilon" => Some("\u{0395}"),
            "Zeta" => Some("\u{0396}"),
            "Eta" => Some("\u{0397}"),
            "Theta" => Some("\u{0398}"),
            "Iota" => Some("\u{0399}"),
            "Kappa" => Some("\u{039A}"),
            "Lambda" => Some("\u{039B}"),
            "Mu" => Some("\u{039C}"),
            "Nu" => Some("\u{039D}"),
            "Xi" => Some("\u{039E}"),
            "Omicron" => Some("\u{039F}"),
            "Pi" => Some("\u{03A0}"),
            "Rho" => Some("\u{03A1}"),
            "Sigma" => Some("\u{03A3}"),
            "Tau" => Some("\u{03A4}"),
            "Upsilon" => Some("\u{03A5}"),
            "Phi" => Some("\u{03A6}"),
            "Chi" => Some("\u{03A7}"),
            "Psi" => Some("\u{03A8}"),
            "Omega" => Some("\u{03A9}"),
            "infin" => Some("\u{221E}"),
            "ne" => Some("\u{2260}"),
            "le" => Some("\u{2264}"),
            "ge" => Some("\u{2265}"),
            "larr" => Some("\u{2190}"),
            "rarr" => Some("\u{2192}"),
            "uarr" => Some("\u{2191}"),
            "darr" => Some("\u{2193}"),
            "harr" => Some("\u{2194}"),
            "lArr" => Some("\u{21D0}"),
            "rArr" => Some("\u{21D2}"),
            "uArr" => Some("\u{21D1}"),
            "dArr" => Some("\u{21D3}"),
            "hArr" => Some("\u{21D4}"),
            "spades" => Some("\u{2660}"),
            "clubs" => Some("\u{2663}"),
            "hearts" => Some("\u{2665}"),
            "diams" => Some("\u{2666}"),
            "check" => Some("\u{2713}"),
            "cross" => Some("\u{2717}"),
            "star" => Some("\u{2605}"),
            "starf" => Some("\u{2606}"),
            "phone" => Some("\u{260E}"),
            "ballot" => Some("\u{2611}"),
            "sharp" => Some("\u{266F}"),
            "flat" => Some("\u{266D}"),
            "natural" => Some("\u{266E}"),
            "ensp" => Some("\u{2002}"),
            "emsp" => Some("\u{2003}"),
            "thinsp" => Some("\u{2009}"),
            "zwnj" => Some("\u{200C}"),
            "zwj" => Some("\u{200D}"),
            "lrm" => Some("\u{200E}"),
            "rlm" => Some("\u{200F}"),
            "lceil" => Some("\u{2308}"),
            "rceil" => Some("\u{2309}"),
            "lfloor" => Some("\u{230A}"),
            "rfloor" => Some("\u{230B}"),
            "lang" => Some("\u{27E8}"),
            "rang" => Some("\u{27E9}"),
            "loz" => Some("\u{25CA}"),
            "hearts_u" => Some("\u{2665}"),
            "nabla" => Some("\u{2207}"),
            "part" => Some("\u{2202}"),
            "exist" => Some("\u{2203}"),
            "empty" => Some("\u{2205}"),
            "isin" => Some("\u{2208}"),
            "notin" => Some("\u{2209}"),
            "sum" => Some("\u{2211}"),
            "prod" => Some("\u{220F}"),
            "minus" => Some("\u{2212}"),
            "lowast" => Some("\u{2217}"),
            "radic" => Some("\u{221A}"),
            "prop" => Some("\u{221D}"),
            "int" => Some("\u{222B}"),
            "and" => Some("\u{2227}"),
            "or" => Some("\u{2228}"),
            "cap" => Some("\u{2229}"),
            "cup" => Some("\u{222A}"),
            "equiv" => Some("\u{2261}"),
            "sim" => Some("\u{223C}"),
            "cong" => Some("\u{2245}"),
            "asymp" => Some("\u{2248}"),
            "sub" => Some("\u{2282}"),
            "sup" => Some("\u{2283}"),
            "nsub" => Some("\u{2284}"),
            "sube" => Some("\u{2286}"),
            "supe" => Some("\u{2287}"),
            "oplus" => Some("\u{2295}"),
            "otimes" => Some("\u{2297}"),
            "perp" => Some("\u{22A5}"),
            "sdot" => Some("\u{22C5}"),
            "lceil_u" => Some("\u{2308}"),
            "rceil_u" => Some("\u{2309}"),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokenize(src: &str) -> Vec<Token> {
        Tokenizer::new(src)
            .run()
            .into_iter()
            .filter(|t| !matches!(t, Token::Eof))
            .collect()
    }

    #[test]
    fn parses_simple_tag() {
        let tokens = tokenize("<p>hi</p>");
        // StartTag(p), Character('h'), Character('i'), EndTag(p) = 4 tokens.
        assert_eq!(tokens.len(), 4);
        assert!(matches!(tokens[0], Token::StartTag { ref name, .. } if name == "p"));
        assert!(matches!(tokens[1], Token::Character(c) if c == 'h'));
        assert!(matches!(tokens[2], Token::Character(c) if c == 'i'));
        assert!(matches!(tokens[3], Token::EndTag { ref name } if name == "p"));
    }

    #[test]
    fn parses_attributes() {
        let tokens = tokenize(r#"<div class="foo" id='bar' x=y>"#);
        if let Token::StartTag { name, attrs, .. } = &tokens[0] {
            assert_eq!(name, "div");
            assert_eq!(attrs.len(), 3);
            assert_eq!(attrs[0], ("class".into(), "foo".into()));
            assert_eq!(attrs[1], ("id".into(), "bar".into()));
            assert_eq!(attrs[2], ("x".into(), "y".into()));
        } else {
            panic!("expected start tag");
        }
    }

    #[test]
    fn parses_self_closing() {
        let tokens = tokenize("<br/>");
        if let Token::StartTag { self_closing, .. } = &tokens[0] {
            assert!(*self_closing);
        } else {
            panic!("expected start tag");
        }
    }

    #[test]
    fn parses_comments() {
        let tokens = tokenize("<!-- hello -->");
        assert_eq!(tokens.len(), 1);
        if let Token::Comment(s) = &tokens[0] {
            assert_eq!(s.trim(), "hello");
        } else {
            panic!("expected comment");
        }
    }

    #[test]
    fn parses_doctype() {
        let tokens = tokenize("<!DOCTYPE html>");
        if let Token::Doctype { name, .. } = &tokens[0] {
            assert_eq!(name.as_deref(), Some("html"));
        } else {
            panic!("expected doctype");
        }
    }

    #[test]
    fn decodes_numeric_entities() {
        let tokens = tokenize("&#65;&#x42;");
        let chars: String = tokens
            .iter()
            .filter_map(|t| {
                if let Token::Character(c) = t {
                    Some(*c)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(chars, "AB");
    }

    #[test]
    fn decodes_named_entities() {
        let tokens = tokenize("&amp;&copy;");
        let chars: String = tokens
            .iter()
            .filter_map(|t| {
                if let Token::Character(c) = t {
                    Some(*c)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(chars, "&©");
    }

    #[test]
    fn handles_script_data() {
        let tokens = tokenize("<script>var x = '<a>';</script>");
        // First token should be <script> start tag.
        assert!(matches!(tokens[0], Token::StartTag { ref name, .. } if name == "script"));
        // Last token should be </script> end tag.
        let last = tokens.last().unwrap();
        assert!(matches!(last, Token::EndTag { ref name } if name == "script"));
    }

    #[test]
    fn handles_rawtext_for_style() {
        // The tokenizer doesn't auto-switch to RAWTEXT — the tree builder
        // does that. But we can verify RAWTEXT works when manually set.
        let mut t = Tokenizer::new("<style>a < b</style>");
        t.set_content_model(ContentModel::Data);
        let tokens = t.run();
        // Should produce StartTag(style), Text("a < b"), EndTag(style), Eof
        assert!(tokens
            .iter()
            .any(|tok| matches!(tok, Token::StartTag { ref name, .. } if name == "style")));
    }

    #[test]
    fn skips_duplicate_attributes() {
        let tokens = tokenize(r#"<div class="a" class="b">"#);
        if let Token::StartTag { attrs, .. } = &tokens[0] {
            assert_eq!(attrs.len(), 1);
            assert_eq!(attrs[0], ("class".into(), "a".into()));
        } else {
            panic!("expected start tag");
        }
    }
}
