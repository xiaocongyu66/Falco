//! TJS Parser — parses tokens into an AST (Abstract Syntax Tree).
//!
//! Recursive descent parser supporting:
//!   - Expressions: literals, identifiers, binary/unary ops, calls, member access
//!   - Statements: var/let/const, if/else, for, while, return, break, continue
//!   - Functions: declarations, expressions, arrow functions
//!   - Objects: { key: value, ... }
//!   - Arrays: [ a, b, c ]

use crate::tjs::lexer::Token;

#[derive(Debug, Clone)]
pub enum Expr {
    Number(f64),
    /// BigInt literal — stored as a string to preserve precision.
    BigInt(String),
    String(String),
    Boolean(bool),
    Null,
    Undefined,
    Identifier(String),
    /// Private field reference: this.#x or #x (within a class).
    PrivateIdentifier(String),
    Binary(Box<Expr>, String, Box<Expr>),
    Unary(String, Box<Expr>),
    Update(String, Box<Expr>, bool),      // op, expr, is_prefix
    Assign(Box<Expr>, String, Box<Expr>), // target, op, value
    Call(Box<Expr>, Vec<Expr>),
    /// Optional call: f?.() — returns undefined if callee is null/undefined.
    OptionalCall(Box<Expr>, Vec<Expr>),
    Member(Box<Expr>, Box<Expr>, bool), // object, property, is_computed (a.b or a[b])
    /// Optional chaining: a?.b, a?.[b], a?.()
    OptionalMember(Box<Expr>, Box<Expr>, bool),
    /// Spread/rest element: ...x (in arrays, calls, destructuring).
    Spread(Box<Expr>),
    Function(Vec<String>, Vec<Stmt>), // params, body
    /// Generator function: function* name() { yield ... }
    GeneratorFunction(Vec<String>, Vec<Stmt>),
    /// Async function: async function name() { await ... }
    AsyncFunction(Vec<String>, Vec<Stmt>),
    /// Async generator: async function* name() { ... }
    AsyncGeneratorFunction(Vec<String>, Vec<Stmt>),
    Arrow(Vec<String>, Box<Stmt>), // params, body (expression or block)
    Object(Vec<(String, Expr)>),   // key-value pairs
    Array(Vec<Expr>),
    This,
    Super,
    New(Box<Expr>, Vec<Expr>),
    Conditional(Box<Expr>, Box<Expr>, Box<Expr>), // test, consequent, alternate
    Sequence(Vec<Expr>),
    Template(Vec<Expr>), // parts (strings + expressions interleaved)
    /// Class expression: const C = class { ... }
    Class(ClassDef),
    /// `yield expr` inside a generator.
    Yield(Option<Box<Expr>>),
    /// `await expr` inside an async function.
    Await(Box<Expr>),
    /// `new.target` meta-property.
    NewTarget,
}

/// A class definition — shared between Class expression and Class statement.
#[derive(Debug, Clone)]
pub struct ClassDef {
    pub name: Option<String>,
    /// Optional extends clause.
    pub extends: Option<Box<Expr>>,
    /// Field declarations: name, initializer.
    pub fields: Vec<ClassField>,
    /// Methods.
    pub methods: Vec<ClassMethod>,
}

#[derive(Debug, Clone)]
pub struct ClassField {
    /// Field name. May be a private name (starts with `#`).
    pub name: String,
    pub is_private: bool,
    /// Initializer expression, or None for uninitialized.
    pub initializer: Option<Expr>,
    /// Whether this is a static field (`static x = ...`).
    pub is_static: bool,
}

#[derive(Debug, Clone)]
pub struct ClassMethod {
    /// Method name. May be private.
    pub name: String,
    pub is_private: bool,
    pub params: Vec<String>,
    pub body: Vec<Stmt>,
    pub is_static: bool,
    /// Whether this is a generator method (`*name()`).
    pub is_generator: bool,
    /// Whether this is an async method (`async name()`).
    pub is_async: bool,
    /// Method kind: normal, constructor, getter, setter.
    pub kind: MethodKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodKind {
    Normal,
    Constructor,
    Get,
    Set,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Expression(Expr),
    Var(String, Expr), // name, initializer
    Let(String, Expr),
    Const(String, Expr),
    Function(String, Vec<String>, Vec<Stmt>), // name, params, body
    /// Class declaration: class Foo extends Bar { ... }
    Class(ClassDef),
    If(Expr, Vec<Stmt>, Option<Vec<Stmt>>), // condition, then, else
    For(Option<Box<Stmt>>, Option<Expr>, Option<Expr>, Vec<Stmt>), // init, test, update, body
    /// for...of / for...in: kind, var_decl, iterable, body.
    ForInOf(ForInOfKind, String, Expr, Vec<Stmt>),
    While(Expr, Vec<Stmt>),
    DoWhile(Vec<Stmt>, Expr),
    Return(Option<Expr>),
    Break,
    Continue,
    Block(Vec<Stmt>),
    Throw(Expr),
    TryCatch(Vec<Stmt>, Option<String>, Option<Vec<Stmt>>, Option<Vec<Stmt>>), // try, catch param, catch body, finally body
    /// `async function` declaration.
    AsyncFunction(String, Vec<String>, Vec<Stmt>),
    /// `function*` (generator) declaration.
    GeneratorFunction(String, Vec<String>, Vec<Stmt>),
    /// `async function*` (async generator) declaration.
    AsyncGeneratorFunction(String, Vec<String>, Vec<Stmt>),
    /// switch (expr) { case v: ... break; default: ... }
    Switch(Expr, Vec<SwitchCase>),
    Empty,
}

/// A case in a switch statement.
#[derive(Debug, Clone)]
pub struct SwitchCase {
    /// The test expression. None = `default` case.
    pub test: Option<Expr>,
    /// The body statements.
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForInOfKind {
    /// for (x in obj) — iterates over keys.
    ForIn,
    /// for (x of iterable) — iterates over values via Symbol.iterator.
    ForOf,
    /// for await (x of asyncIterable) — async iteration.
    ForAwaitOf,
}

pub fn parse(tokens: &[Token]) -> Result<Vec<Stmt>, String> {
    let mut p = Parser { tokens, pos: 0 };
    let mut stmts = Vec::new();
    while !p.is_eof() {
        stmts.push(p.statement()?);
    }
    Ok(stmts)
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn is_eof(&self) -> bool {
        matches!(self.peek(), Token::EOF)
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn peek_at(&self, offset: usize) -> &Token {
        let idx = (self.pos + offset).min(self.tokens.len() - 1);
        &self.tokens[idx]
    }

    fn advance(&mut self) -> Token {
        let t = self.tokens[self.pos].clone();
        if !matches!(t, Token::EOF) {
            self.pos += 1;
        }
        t
    }

    fn expect(&mut self, expected: &Token) -> Result<(), String> {
        if self.peek() == expected {
            self.pos += 1;
            Ok(())
        } else {
            Err(format!("Expected {:?}, got {:?}", expected, self.peek()))
        }
    }

    fn match_punct(&mut self, c: char) -> bool {
        if matches!(self.peek(), Token::Punct(p) if *p == c) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn match_op(&mut self, op: &str) -> bool {
        if matches!(self.peek(), Token::Operator(o) if o == op) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn match_keyword(&mut self, kw: &str) -> bool {
        if matches!(self.peek(), Token::Keyword(k) if k == kw) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn statement(&mut self) -> Result<Stmt, String> {
        match self.peek().clone() {
            Token::Keyword(k) => match k.as_str() {
                "var" => self.var_decl("var"),
                "let" => self.var_decl("let"),
                "const" => self.var_decl("const"),
                "function" => self.function_decl(),
                "class" => {
                    self.advance(); // skip 'class'
                    let class_def = self.parse_class_body()?;
                    Ok(Stmt::Class(class_def))
                }
                "if" => self.if_stmt(),
                "for" => self.for_stmt(),
                "while" => self.while_stmt(),
                "do" => self.do_while_stmt(),
                "async" => {
                    // async function declaration, OR async function*
                    if matches!(self.peek_at(1), Token::Keyword(k) if k == "function") {
                        self.advance(); // skip 'async'
                        return self.function_decl_async();
                    }
                    self.expr_stmt()
                }
                "return" => {
                    self.advance();
                    if matches!(
                        self.peek(),
                        Token::Punct(';') | Token::Punct('}') | Token::EOF
                    ) {
                        self.match_punct(';');
                        Ok(Stmt::Return(None))
                    } else {
                        let e = self.expression()?;
                        self.match_punct(';');
                        Ok(Stmt::Return(Some(e)))
                    }
                }
                "break" => {
                    self.advance();
                    self.match_punct(';');
                    Ok(Stmt::Break)
                }
                "continue" => {
                    self.advance();
                    self.match_punct(';');
                    Ok(Stmt::Continue)
                }
                "throw" => {
                    self.advance();
                    let e = self.expression()?;
                    self.match_punct(';');
                    Ok(Stmt::Throw(e))
                }
                "try" => self.try_stmt(),
                "switch" => self.switch_stmt(),
                "import" => self.import_stmt(),
                "export" => self.export_stmt(),
                _ => self.expr_stmt(),
            },
            Token::Punct('{') => {
                self.advance();
                let mut stmts = Vec::new();
                while !matches!(self.peek(), Token::Punct('}') | Token::EOF) {
                    stmts.push(self.statement()?);
                }
                self.match_punct('}');
                Ok(Stmt::Block(stmts))
            }
            Token::Punct(';') => {
                self.advance();
                Ok(Stmt::Empty)
            }
            _ => self.expr_stmt(),
        }
    }

    fn expr_stmt(&mut self) -> Result<Stmt, String> {
        let e = self.expression()?;
        self.match_punct(';');
        Ok(Stmt::Expression(e))
    }

    fn var_decl(&mut self, kind: &str) -> Result<Stmt, String> {
        self.advance(); // skip var/let/const
                        // Check for destructuring: { ... } or [ ... ]
        match self.peek() {
            Token::Punct('{') => return self.parse_destructuring_object(kind),
            Token::Punct('[') => return self.parse_destructuring_array(kind),
            _ => {}
        }
        let name = match self.advance() {
            Token::Identifier(s) => s,
            other => {
                return Err(format!(
                    "Expected identifier after {}, got {:?}",
                    kind, other
                ))
            }
        };
        let init = if self.match_op("=") {
            self.expression()?
        } else {
            Expr::Undefined
        };
        // Check for multiple declarations: var a = 1, b = 2
        if self.match_punct(',') {
            // We have multiple declarations. Parse the rest and create a Block.
            let mut stmts = vec![match kind {
                "var" => Stmt::Var(name, init),
                "let" => Stmt::Let(name, init),
                "const" => Stmt::Const(name, init),
                _ => unreachable!(),
            }];
            loop {
                let name2 = match self.advance() {
                    Token::Identifier(s) => s,
                    other => {
                        return Err(format!(
                            "Expected identifier in multi-decl, got {:?}",
                            other
                        ))
                    }
                };
                let init2 = if self.match_op("=") {
                    self.expression()?
                } else {
                    Expr::Undefined
                };
                stmts.push(match kind {
                    "var" => Stmt::Var(name2, init2),
                    "let" => Stmt::Let(name2, init2),
                    "const" => Stmt::Const(name2, init2),
                    _ => unreachable!(),
                });
                if !self.match_punct(',') {
                    break;
                }
            }
            self.match_punct(';');
            return Ok(Stmt::Block(stmts));
        }
        self.match_punct(';');
        match kind {
            "var" => Ok(Stmt::Var(name, init)),
            "let" => Ok(Stmt::Let(name, init)),
            "const" => Ok(Stmt::Const(name, init)),
            _ => unreachable!(),
        }
    }

    /// Parse object destructuring: var {a, b: c, d = 1} = obj
    fn parse_destructuring_object(&mut self, kind: &str) -> Result<Stmt, String> {
        self.advance(); // skip '{'
        let mut fields: Vec<(String, Option<String>, Option<Expr>)> = Vec::new();
        // (property_name, local_variable_name, default_value)
        while !matches!(self.peek(), Token::Punct('}')) {
            let prop_name = match self.advance() {
                Token::Identifier(s) => s,
                Token::String(s) => s,
                Token::Keyword(s) => s,
                other => {
                    return Err(format!(
                        "Expected property name in destructuring, got {:?}",
                        other
                    ))
                }
            };
            let local_name = if self.match_punct(':') {
                match self.advance() {
                    Token::Identifier(s) => s,
                    other => return Err(format!("Expected local name after ':', got {:?}", other)),
                }
            } else {
                prop_name.clone()
            };
            let default = if self.match_op("=") {
                Some(self.assignment()?)
            } else {
                None
            };
            fields.push((prop_name, Some(local_name), default));
            if !self.match_punct(',') {
                break;
            }
        }
        self.match_punct('}');
        self.match_op("="); // expect '='
        let init_expr = self.expression()?;
        self.match_punct(';');

        // Generate: var _temp = init_expr; var a = _temp.a; var b = _temp.b;
        let temp_name = "__destruct_temp";
        let mut stmts = Vec::new();
        stmts.push(match kind {
            "var" => Stmt::Var(temp_name.to_string(), init_expr),
            "let" => Stmt::Let(temp_name.to_string(), init_expr),
            "const" => Stmt::Const(temp_name.to_string(), init_expr),
            _ => unreachable!(),
        });
        for (prop, local, default) in fields {
            let local = local.unwrap_or_else(|| prop.clone());
            let access = Expr::Member(
                Box::new(Expr::Identifier(temp_name.to_string())),
                Box::new(Expr::String(prop)),
                false,
            );
            let val = if let Some(def) = default {
                // _temp.prop !== undefined ? _temp.prop : default
                Expr::Conditional(
                    Box::new(Expr::Binary(
                        Box::new(access.clone()),
                        "!==".to_string(),
                        Box::new(Expr::Undefined),
                    )),
                    Box::new(access),
                    Box::new(def),
                )
            } else {
                access
            };
            stmts.push(match kind {
                "var" => Stmt::Var(local, val),
                "let" => Stmt::Let(local, val),
                "const" => Stmt::Const(local, val),
                _ => unreachable!(),
            });
        }
        Ok(Stmt::Block(stmts))
    }

    /// Parse array destructuring: var [a, b, ...rest] = arr
    fn parse_destructuring_array(&mut self, kind: &str) -> Result<Stmt, String> {
        self.advance(); // skip '['
        let mut fields: Vec<(String, bool)> = Vec::new();
        // (local_name, is_rest)
        while !matches!(self.peek(), Token::Punct(']')) {
            if self.match_op("...") {
                let name = match self.advance() {
                    Token::Identifier(s) => s,
                    other => return Err(format!("Expected name after ..., got {:?}", other)),
                };
                fields.push((name, true));
                break; // rest must be last
            }
            let name = match self.advance() {
                Token::Identifier(s) => s,
                other => {
                    return Err(format!(
                        "Expected name in array destructuring, got {:?}",
                        other
                    ))
                }
            };
            fields.push((name, false));
            if !self.match_punct(',') {
                break;
            }
        }
        self.match_punct(']');
        self.match_op("="); // expect '='
        let init_expr = self.expression()?;
        self.match_punct(';');

        // Generate: var _temp = init_expr; var a = _temp[0]; var b = _temp[1];
        let temp_name = "__destruct_arr";
        let mut stmts = Vec::new();
        stmts.push(match kind {
            "var" => Stmt::Var(temp_name.to_string(), init_expr),
            "let" => Stmt::Let(temp_name.to_string(), init_expr),
            "const" => Stmt::Const(temp_name.to_string(), init_expr),
            _ => unreachable!(),
        });
        for (i, (name, is_rest)) in fields.iter().enumerate() {
            if *is_rest {
                // var rest = _temp.slice(i)
                let val = Expr::Call(
                    Box::new(Expr::Member(
                        Box::new(Expr::Identifier(temp_name.to_string())),
                        Box::new(Expr::String("slice".to_string())),
                        false,
                    )),
                    vec![Expr::Number(i as f64)],
                );
                stmts.push(match kind {
                    "var" => Stmt::Var(name.clone(), val),
                    "let" => Stmt::Let(name.clone(), val),
                    "const" => Stmt::Const(name.clone(), val),
                    _ => unreachable!(),
                });
            } else {
                let val = Expr::Member(
                    Box::new(Expr::Identifier(temp_name.to_string())),
                    Box::new(Expr::Number(i as f64)),
                    true,
                );
                stmts.push(match kind {
                    "var" => Stmt::Var(name.clone(), val),
                    "let" => Stmt::Let(name.clone(), val),
                    "const" => Stmt::Const(name.clone(), val),
                    _ => unreachable!(),
                });
            }
        }
        Ok(Stmt::Block(stmts))
    }

    fn function_decl(&mut self) -> Result<Stmt, String> {
        self.advance(); // skip 'function'
                        // Check for generator: function*
        let is_generator = self.match_op("*");
        let name = match self.advance() {
            Token::Identifier(s) => s,
            other => return Err(format!("Expected function name, got {:?}", other)),
        };
        let params = self.parse_params()?;
        let body = self.parse_block()?;
        if is_generator {
            Ok(Stmt::GeneratorFunction(name, params, body))
        } else {
            Ok(Stmt::Function(name, params, body))
        }
    }

    fn function_decl_async(&mut self) -> Result<Stmt, String> {
        self.advance(); // skip 'function'
        let is_generator = self.match_op("*");
        let name = match self.advance() {
            Token::Identifier(s) => s,
            other => return Err(format!("Expected function name, got {:?}", other)),
        };
        let params = self.parse_params()?;
        let body = self.parse_block()?;
        if is_generator {
            Ok(Stmt::AsyncGeneratorFunction(name, params, body))
        } else {
            Ok(Stmt::AsyncFunction(name, params, body))
        }
    }

    /// Parse a class body: `class Name [extends Base] { members }`.
    /// Assumes the `class` keyword has been consumed.
    fn parse_class_body(&mut self) -> Result<ClassDef, String> {
        let name = match self.peek().clone() {
            Token::Identifier(s) => {
                self.advance();
                Some(s)
            }
            _ => None,
        };
        // Optional extends clause.
        let extends = if self.match_keyword("extends") {
            Some(Box::new(self.left_hand_side()?))
        } else {
            None
        };
        self.match_punct('{');
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        while !matches!(self.peek(), Token::Punct('}') | Token::EOF) {
            // Parse class member.
            self.parse_class_member(&mut fields, &mut methods)?;
        }
        self.match_punct('}');
        Ok(ClassDef {
            name,
            extends,
            fields,
            methods,
        })
    }

    fn parse_class_member(
        &mut self,
        fields: &mut Vec<ClassField>,
        methods: &mut Vec<ClassMethod>,
    ) -> Result<(), String> {
        // Optional 'static'.
        let is_static = self.match_keyword("static");

        // Optional 'async' or '*' for methods.
        let is_async = self.match_keyword("async");
        let is_generator = self.match_op("*");

        // Method kind: get / set / constructor / normal.
        let kind = if !is_async && !is_generator {
            if self.match_keyword("get") {
                MethodKind::Get
            } else if self.match_keyword("set") {
                MethodKind::Set
            } else {
                MethodKind::Normal
            }
        } else {
            MethodKind::Normal
        };

        // Now parse the member name. Can be identifier, string, number, or
        // private identifier (#x). 'constructor' as a name implies MethodKind::Constructor.
        let (name, is_private) = match self.peek().clone() {
            Token::Identifier(s) => {
                self.advance();
                (s, false)
            }
            Token::PrivateIdentifier(s) => {
                self.advance();
                (s, true)
            }
            Token::String(s) => {
                self.advance();
                (s, false)
            }
            Token::Number(n) => {
                self.advance();
                (n.to_string(), false)
            }
            Token::Keyword(k) => {
                // Allow keywords as member names: class { if() {} }
                self.advance();
                (k, false)
            }
            Token::Punct('[') => {
                // Computed member name: [expr]
                self.advance();
                let _e = self.expression()?;
                self.match_punct(']');
                // For simplicity, we don't support computed names yet —
                // store as empty string.
                (String::new(), false)
            }
            other => return Err(format!("Expected class member name, got {:?}", other)),
        };

        // Now check if this is a method (followed by `(`) or a field.
        if matches!(self.peek(), Token::Punct('(')) {
            // It's a method.
            let params = self.parse_params()?;
            let body = self.parse_block()?;
            let final_kind = if name == "constructor" && !is_static {
                MethodKind::Constructor
            } else {
                kind
            };
            methods.push(ClassMethod {
                name,
                is_private,
                params,
                body,
                is_static,
                is_generator,
                is_async,
                kind: final_kind,
            });
        } else {
            // It's a field. Optional initializer.
            let initializer = if self.match_op("=") {
                Some(self.expression()?)
            } else {
                None
            };
            self.match_punct(';');
            fields.push(ClassField {
                name,
                is_private,
                initializer,
                is_static,
            });
        }
        Ok(())
    }

    /// Parse a template literal after the first string part.
    /// The lexer produces: String(part1) Punct('`') expr_tokens Punct('~') String(part2) ...
    /// We convert this to: part1 + String(expr) + part2 + String(expr) + ...
    fn parse_template_literal(&mut self, first_part: Expr) -> Result<Expr, String> {
        let mut result = first_part;
        loop {
            if matches!(self.peek(), Token::Punct('`')) {
                // Template expression start.
                self.advance(); // skip Punct('`')
                                // Parse the expression.
                let expr = self.assignment()?;
                // Expect Punct('~') — template expression end.
                if !matches!(self.peek(), Token::Punct('~')) {
                    return Err("Expected '~' after template expression".to_string());
                }
                self.advance(); // skip Punct('~')
                                // Concatenate: result + String(expr)
                                // We use a Call to String(expr) to convert to string.
                let to_string_call =
                    Expr::Call(Box::new(Expr::Identifier("String".to_string())), vec![expr]);
                result = Expr::Binary(Box::new(result), "+".to_string(), Box::new(to_string_call));
                // Now check if there's another string part (the text after `}`).
                if let Token::String(s) = self.peek().clone() {
                    self.advance();
                    if s.is_empty() {
                        // Empty string part — nothing to concatenate.
                    } else {
                        result = Expr::Binary(
                            Box::new(result),
                            "+".to_string(),
                            Box::new(Expr::String(s)),
                        );
                    }
                    // Check for another ${...}
                    if matches!(self.peek(), Token::Punct('`')) {
                        continue;
                    }
                    break;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        Ok(result)
    }

    /// Parse a left-hand-side expression (for `extends` clause).
    fn left_hand_side(&mut self) -> Result<Expr, String> {
        self.call_member()
    }

    /// Parse the callee in a `new` expression. This is like `call_member`
    /// but does NOT consume parenthesised argument lists — those belong to
    /// the `new` itself.
    fn new_callee(&mut self) -> Result<Expr, String> {
        let mut expr = self.primary()?;
        loop {
            if self.match_punct('.') {
                let prop = match self.advance() {
                    Token::Identifier(s) => Expr::String(s),
                    Token::PrivateIdentifier(s) => Expr::String(format!("#{}", s)),
                    Token::Keyword(k) => Expr::String(k),
                    other => {
                        return Err(format!("Expected property name after '.', got {:?}", other))
                    }
                };
                expr = Expr::Member(Box::new(expr), Box::new(prop), false);
            } else if self.match_op("?.") {
                // new with optional chaining — rare, but handle it.
                if matches!(self.peek(), Token::Identifier(_) | Token::Keyword(_)) {
                    let prop = match self.advance() {
                        Token::Identifier(s) => Expr::String(s),
                        Token::Keyword(k) => Expr::String(k),
                        other => {
                            return Err(format!(
                                "Expected property name after '?.', got {:?}",
                                other
                            ))
                        }
                    };
                    expr = Expr::OptionalMember(Box::new(expr), Box::new(prop), false);
                }
            } else if self.match_punct('[') {
                let prop = self.expression()?;
                self.match_punct(']');
                expr = Expr::Member(Box::new(expr), Box::new(prop), true);
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_params(&mut self) -> Result<Vec<String>, String> {
        self.expect(&Token::Punct('('))?;
        let mut params = Vec::new();
        while !matches!(self.peek(), Token::Punct(')')) {
            if let Token::Identifier(s) = self.advance() {
                params.push(s);
            }
            if !self.match_punct(',') {
                break;
            }
        }
        self.match_punct(')');
        Ok(params)
    }

    fn parse_block(&mut self) -> Result<Vec<Stmt>, String> {
        self.match_punct('{');
        let mut stmts = Vec::new();
        while !matches!(self.peek(), Token::Punct('}') | Token::EOF) {
            stmts.push(self.statement()?);
        }
        self.match_punct('}');
        Ok(stmts)
    }

    fn if_stmt(&mut self) -> Result<Stmt, String> {
        self.advance(); // skip 'if'
        self.match_punct('(');
        let cond = self.expression()?;
        self.match_punct(')');
        let then = if matches!(self.peek(), Token::Punct('{')) {
            self.parse_block()?
        } else {
            vec![self.statement()?]
        };
        let els = if self.match_keyword("else") {
            if matches!(self.peek(), Token::Keyword(k) if k == "if") {
                Some(vec![self.if_stmt()?])
            } else if matches!(self.peek(), Token::Punct('{')) {
                Some(self.parse_block()?)
            } else {
                Some(vec![self.statement()?])
            }
        } else {
            None
        };
        Ok(Stmt::If(cond, then, els))
    }

    fn for_stmt(&mut self) -> Result<Stmt, String> {
        self.advance(); // skip 'for'
                        // Optional 'await' for `for await...of`.
        let for_await = self.match_keyword("await");
        self.match_punct('(');

        // Parse the init part. This can be:
        //   - `;` (no init, traditional for)
        //   - `var/let/const x` followed by `in`/`of` (for-in/for-of)
        //   - `var/let/const x = expr; ...` (traditional for with var decl)
        //   - `expr; ...` (traditional for with expression init)
        //
        // We use a speculative parse: try for-in/for-of first; if it fails,
        // fall back to traditional for.
        if matches!(self.peek(), Token::Punct(';')) {
            // Traditional for with no init.
            return self.for_stmt_traditional(None);
        }

        // Save position for backtracking.
        let saved_pos = self.pos;

        // Try for-in / for-of.
        let is_var_decl =
            matches!(self.peek(), Token::Keyword(k) if k == "var" || k == "let" || k == "const");
        if is_var_decl {
            self.advance(); // skip var/let/const
            let name = match self.advance() {
                Token::Identifier(s) => s,
                _ => {
                    // Restore and try traditional.
                    self.pos = saved_pos;
                    let init = Some(Box::new(self.var_decl("var")?));
                    return self.for_stmt_traditional_after_init(init);
                }
            };
            // Check for `in` or `of`.
            if self.match_keyword("in") {
                let iterable = self.expression()?;
                // Forcefully consume the closing ')'.
                while !matches!(self.peek(), Token::Punct(')') | Token::EOF) {
                    self.advance();
                }
                self.advance(); // consume ')'
                let body = if matches!(self.peek(), Token::Punct('{')) {
                    self.parse_block()?
                } else {
                    vec![self.statement()?]
                };
                return Ok(Stmt::ForInOf(ForInOfKind::ForIn, name, iterable, body));
            }
            if self.match_keyword("of") {
                let iterable = self.expression()?;
                // Forcefully consume the closing ')'.
                while !matches!(self.peek(), Token::Punct(')') | Token::EOF) {
                    self.advance();
                }
                self.advance(); // consume ')'
                let body = if matches!(self.peek(), Token::Punct('{')) {
                    self.parse_block()?
                } else {
                    vec![self.statement()?]
                };
                let kind = if for_await {
                    ForInOfKind::ForAwaitOf
                } else {
                    ForInOfKind::ForOf
                };
                return Ok(Stmt::ForInOf(kind, name, iterable, body));
            }
            // Not for-in/of — restore and parse as traditional for with var decl.
            self.pos = saved_pos;
            let init = Some(Box::new(self.var_decl("var")?));
            return self.for_stmt_traditional_after_init(init);
        }

        // Not a var decl — try to parse as for-in/for-of with existing variable.
        // The key insight: `for (x in obj)` and `for (x of iterable)` have
        // a simple identifier followed by `in` or `of`. We detect this
        // BEFORE calling expression() so that `in` isn't consumed as a
        // binary operator.
        if matches!(self.peek(), Token::Identifier(_)) {
            // Check if the token AFTER the identifier is `in` or `of`.
            if matches!(self.peek_at(1), Token::Keyword(k) if k == "in" || k == "of") {
                let name = if let Token::Identifier(s) = self.advance() {
                    s
                } else {
                    unreachable!()
                };
                let is_in = self.match_keyword("in");
                let is_of = if !is_in {
                    self.match_keyword("of")
                } else {
                    false
                };
                if is_in || is_of {
                    let iterable = self.expression()?;
                    while !matches!(self.peek(), Token::Punct(')') | Token::EOF) {
                        self.advance();
                    }
                    self.advance(); // consume ')'
                    let body = if matches!(self.peek(), Token::Punct('{')) {
                        self.parse_block()?
                    } else {
                        vec![self.statement()?]
                    };
                    let kind = if is_in {
                        ForInOfKind::ForIn
                    } else if for_await {
                        ForInOfKind::ForAwaitOf
                    } else {
                        ForInOfKind::ForOf
                    };
                    return Ok(Stmt::ForInOf(kind, name, iterable, body));
                }
            }
        }

        // Not for-in/for-of — parse as traditional for with expression init.
        let init_expr = self.expression()?;
        if self.match_keyword("in") {
            let iterable = self.expression()?;
            while !matches!(self.peek(), Token::Punct(')') | Token::EOF) {
                self.advance();
            }
            self.advance(); // consume ')'
            let body = if matches!(self.peek(), Token::Punct('{')) {
                self.parse_block()?
            } else {
                vec![self.statement()?]
            };
            let name = if let Expr::Identifier(s) = &init_expr {
                s.clone()
            } else {
                String::new()
            };
            return Ok(Stmt::ForInOf(ForInOfKind::ForIn, name, iterable, body));
        }
        if self.match_keyword("of") {
            let iterable = self.expression()?;
            while !matches!(self.peek(), Token::Punct(')') | Token::EOF) {
                self.advance();
            }
            self.advance(); // consume ')'
            let body = if matches!(self.peek(), Token::Punct('{')) {
                self.parse_block()?
            } else {
                vec![self.statement()?]
            };
            let name = if let Expr::Identifier(s) = &init_expr {
                s.clone()
            } else {
                String::new()
            };
            let kind = if for_await {
                ForInOfKind::ForAwaitOf
            } else {
                ForInOfKind::ForOf
            };
            return Ok(Stmt::ForInOf(kind, name, iterable, body));
        }
        // Traditional for with expression init.
        // We've already consumed the init expression — wrap it as a statement.
        let init_stmt = Stmt::Expression(init_expr);
        let init = Some(Box::new(init_stmt));
        self.for_stmt_traditional_after_init(init)
    }

    /// Continue parsing a traditional for-loop after the init statement has
    /// been consumed. We're positioned right after the init expression
    /// (and the `;` separator may or may not have been consumed).
    fn for_stmt_traditional_after_init(&mut self, init: Option<Box<Stmt>>) -> Result<Stmt, String> {
        // Consume the `;` after init.
        self.match_punct(';');
        let test = if matches!(self.peek(), Token::Punct(';')) {
            None
        } else {
            Some(self.expression()?)
        };
        self.match_punct(';');
        let update = if matches!(self.peek(), Token::Punct(')')) {
            None
        } else {
            Some(self.expression()?)
        };
        self.match_punct(')');
        let body = if matches!(self.peek(), Token::Punct('{')) {
            self.parse_block()?
        } else {
            vec![self.statement()?]
        };
        Ok(Stmt::For(init, test, update, body))
    }

    fn for_stmt_traditional(&mut self, init: Option<Box<Stmt>>) -> Result<Stmt, String> {
        // We're positioned after `for (` and have consumed `;` if init was None.
        self.match_punct(';');
        let test = if matches!(self.peek(), Token::Punct(';')) {
            None
        } else {
            Some(self.expression()?)
        };
        self.match_punct(';');
        let update = if matches!(self.peek(), Token::Punct(')')) {
            None
        } else {
            Some(self.expression()?)
        };
        self.match_punct(')');
        let body = if matches!(self.peek(), Token::Punct('{')) {
            self.parse_block()?
        } else {
            vec![self.statement()?]
        };
        Ok(Stmt::For(init, test, update, body))
    }

    fn while_stmt(&mut self) -> Result<Stmt, String> {
        self.advance(); // skip 'while'
        self.match_punct('(');
        let cond = self.expression()?;
        self.match_punct(')');
        let body = if matches!(self.peek(), Token::Punct('{')) {
            self.parse_block()?
        } else {
            vec![self.statement()?]
        };
        Ok(Stmt::While(cond, body))
    }

    fn do_while_stmt(&mut self) -> Result<Stmt, String> {
        self.advance(); // skip 'do'
        let body = self.parse_block()?;
        self.match_keyword("while");
        self.match_punct('(');
        let cond = self.expression()?;
        self.match_punct(')');
        self.match_punct(';');
        Ok(Stmt::DoWhile(body, cond))
    }

    fn try_stmt(&mut self) -> Result<Stmt, String> {
        self.advance(); // skip 'try'
        let try_body = self.parse_block()?;
        let (catch_param, catch_body) = if self.match_keyword("catch") {
            self.match_punct('(');
            let param = match self.advance() {
                Token::Identifier(s) => Some(s),
                _ => None,
            };
            self.match_punct(')');
            let body = self.parse_block()?;
            (param, Some(body))
        } else {
            (None, None)
        };
        // Parse 'finally' block if present.
        let finally_body = if self.match_keyword("finally") {
            Some(self.parse_block()?)
        } else {
            None
        };
        Ok(Stmt::TryCatch(try_body, catch_param, catch_body, finally_body))
    }

    fn switch_stmt(&mut self) -> Result<Stmt, String> {
        self.advance(); // skip 'switch'
        self.match_punct('(');
        let expr = self.expression()?;
        self.match_punct(')');
        self.match_punct('{');

        let mut cases: Vec<SwitchCase> = Vec::new();
        while !matches!(self.peek(), Token::Punct('}') | Token::EOF) {
            if self.match_keyword("case") {
                let test = self.expression()?;
                self.match_punct(':');
                let mut body = Vec::new();
                while !matches!(
                    self.peek(),
                    Token::Keyword(k) if k == "case" || k == "default"
                ) && !matches!(self.peek(), Token::Punct('}') | Token::EOF)
                {
                    body.push(self.statement()?);
                }
                cases.push(SwitchCase { test: Some(test), body });
            } else if self.match_keyword("default") {
                self.match_punct(':');
                let mut body = Vec::new();
                while !matches!(
                    self.peek(),
                    Token::Keyword(k) if k == "case" || k == "default"
                ) && !matches!(self.peek(), Token::Punct('}') | Token::EOF)
                {
                    body.push(self.statement()?);
                }
                cases.push(SwitchCase { test: None, body });
            } else {
                self.advance(); // skip unexpected token
            }
        }
        self.match_punct('}');
        Ok(Stmt::Switch(expr, cases))
    }

    /// Parse an `import` statement.
    ///
    /// Supports:
    ///   import { foo, bar } from "module";
    ///   import defaultExport from "module";
    ///   import * as name from "module";
    ///   import "module";  (side-effect only)
    fn import_stmt(&mut self) -> Result<Stmt, String> {
        self.advance(); // skip 'import'

        // Check for side-effect-only import: import "module";
        if matches!(self.peek(), Token::String(_)) {
            let path = if let Token::String(s) = self.advance() {
                s
            } else {
                unreachable!()
            };
            self.match_punct(';');
            // Generate: __falco_require("path");
            return Ok(Stmt::Expression(Expr::Call(
                Box::new(Expr::Identifier("__falco_require".to_string())),
                vec![Expr::String(path)],
            )));
        }

        // Parse import specifiers.
        let mut default_name: Option<String> = None;
        let mut named_imports: Vec<String> = Vec::new();
        let mut namespace_name: Option<String> = None;

        // Default import: import defaultExport from ...
        if matches!(self.peek(), Token::Identifier(_)) {
            if let Token::Identifier(s) = self.advance() {
                default_name = Some(s);
            }
            // Check for comma after default: import default, { named } from ...
            if matches!(self.peek(), Token::Punct(',')) {
                self.advance();
            }
        }

        // Namespace import: import * as name from ...
        if self.peek() == &Token::Operator("*".to_string()) {
            self.advance();
            // 'as' is an identifier, not a keyword.
            match self.peek() {
                Token::Identifier(s) if s == "as" => { self.advance(); }
                Token::Keyword(s) if s == "as" => { self.advance(); }
                _ => return Err("expected 'as' after '*' in import".to_string()),
            }
            if let Token::Identifier(s) = self.advance() {
                namespace_name = Some(s);
            } else {
                return Err("expected identifier after 'as'".to_string());
            }
        }

        // Named imports: import { foo, bar } from ...
        if matches!(self.peek(), Token::Punct('{')) {
            self.advance();
            while !matches!(self.peek(), Token::Punct('}')) {
                if let Token::Identifier(s) = self.advance() {
                    named_imports.push(s);
                } else {
                    return Err("expected identifier in import list".to_string());
                }
                if matches!(self.peek(), Token::Punct(',')) {
                    self.advance();
                }
            }
            self.match_punct('}');
        }

        // Expect 'from' — it's an identifier, not a keyword.
        match self.peek() {
            Token::Identifier(s) if s == "from" => { self.advance(); }
            Token::Keyword(s) if s == "from" => { self.advance(); }
            _ => return Err("expected 'from' in import statement".to_string()),
        }

        // Parse module path.
        let path = if let Token::String(s) = self.advance() {
            s
        } else {
            return Err("expected string after 'from'".to_string());
        };
        self.match_punct(';');

        // Generate code: var __mod_N = __falco_require("path");
        // Then extract each named import from the module namespace.
        let mod_var = format!("__mod_{}", path.len());
        let mut stmts: Vec<Stmt> = Vec::new();

        // var __mod_N = __falco_require("path");
        stmts.push(Stmt::Var(
            mod_var.clone(),
            Expr::Call(
                Box::new(Expr::Identifier("__falco_require".to_string())),
                vec![Expr::String(path)],
            ),
        ));

        // Default import: var defaultExport = __mod_N.default || __mod_N;
        if let Some(name) = default_name {
            stmts.push(Stmt::Var(
                name,
                Expr::Binary(
                    Box::new(Expr::Member(
                        Box::new(Expr::Identifier(mod_var.clone())),
                        Box::new(Expr::String("default".to_string())),
                        false,
                    )),
                    "||".to_string(),
                    Box::new(Expr::Identifier(mod_var.clone())),
                ),
            ));
        }

        // Namespace import: var name = __mod_N;
        if let Some(name) = namespace_name {
            stmts.push(Stmt::Var(name, Expr::Identifier(mod_var.clone())));
        }

        // Named imports: var foo = __mod_N.foo;
        for name in &named_imports {
            stmts.push(Stmt::Var(
                name.clone(),
                Expr::Member(
                    Box::new(Expr::Identifier(mod_var.clone())),
                    Box::new(Expr::String(name.clone())),
                    false,
                ),
            ));
        }

        Ok(Stmt::Block(stmts))
    }

    /// Parse an `export` statement.
    ///
    /// Supports:
    ///   export const foo = 42;
    ///   export function foo() {}
    ///   export default expression;
    ///   export { foo, bar };
    fn export_stmt(&mut self) -> Result<Stmt, String> {
        self.advance(); // skip 'export'

        // export default ...
        if self.match_keyword("default") {
            let expr = self.assignment()?;
            self.match_punct(';');
            // Generate: __falco_exports.default = expr;
            return Ok(Stmt::Expression(Expr::Assign(
                Box::new(Expr::Member(
                    Box::new(Expr::Identifier("__falco_exports".to_string())),
                    Box::new(Expr::String("default".to_string())),
                    false,
                )),
                "=".to_string(),
                Box::new(expr),
            )));
        }

        // export const/let/var ...
        if matches!(self.peek(), Token::Keyword(k) if k == "const" || k == "let" || k == "var") {
            let kw = if let Token::Keyword(k) = self.advance() {
                k
            } else {
                unreachable!()
            };
            let name = if let Token::Identifier(s) = self.peek().clone() {
                self.advance();
                s
            } else {
                return Err("expected identifier after export const/let/var".to_string());
            };
            self.match_op("=");
            let init = self.assignment()?;
            self.match_punct(';');

            let mut stmts: Vec<Stmt> = Vec::new();
            // Declare the variable.
            let decl = match kw.as_str() {
                "const" => Stmt::Const(name.clone(), init.clone()),
                "let" => Stmt::Let(name.clone(), init.clone()),
                _ => Stmt::Var(name.clone(), init.clone()),
            };
            stmts.push(decl);
            // __falco_exports.name = name;
            stmts.push(Stmt::Expression(Expr::Assign(
                Box::new(Expr::Member(
                    Box::new(Expr::Identifier("__falco_exports".to_string())),
                    Box::new(Expr::String(name)),
                    false,
                )),
                "=".to_string(),
                Box::new(Expr::Identifier("__falco_exports".to_string())), // placeholder
            )));
            return Ok(Stmt::Block(stmts));
        }

        // export function ...
        if matches!(self.peek(), Token::Keyword(k) if k == "function") {
            self.advance(); // skip 'function'
            let name = if let Token::Identifier(s) = self.advance() {
                s
            } else {
                return Err("expected function name after export function".to_string());
            };
            let params = self.parse_params()?;
            let body = self.parse_block()?;
            self.match_punct(';');

            let mut stmts: Vec<Stmt> = Vec::new();
            stmts.push(Stmt::Function(name.clone(), params, body));
            // __falco_exports.name = name;
            stmts.push(Stmt::Expression(Expr::Assign(
                Box::new(Expr::Member(
                    Box::new(Expr::Identifier("__falco_exports".to_string())),
                    Box::new(Expr::String(name)),
                    false,
                )),
                "=".to_string(),
                Box::new(Expr::Identifier("__falco_exports".to_string())),
            )));
            return Ok(Stmt::Block(stmts));
        }

        // export { foo, bar };
        if matches!(self.peek(), Token::Punct('{')) {
            self.advance();
            let mut names: Vec<String> = Vec::new();
            while !matches!(self.peek(), Token::Punct('}')) {
                if let Token::Identifier(s) = self.advance() {
                    names.push(s);
                } else {
                    return Err("expected identifier in export list".to_string());
                }
                if matches!(self.peek(), Token::Punct(',')) {
                    self.advance();
                }
            }
            self.match_punct('}');
            // Optional 'from "module"' for re-exports.
            if self.match_keyword("from") {
                if let Token::String(_) = self.peek() {
                    self.advance(); // skip module path (re-export, simplified)
                }
            }
            self.match_punct(';');

            let mut stmts: Vec<Stmt> = Vec::new();
            for name in &names {
                // __falco_exports.name = name;
                stmts.push(Stmt::Expression(Expr::Assign(
                    Box::new(Expr::Member(
                        Box::new(Expr::Identifier("__falco_exports".to_string())),
                        Box::new(Expr::String(name.clone())),
                        false,
                    )),
                    "=".to_string(),
                    Box::new(Expr::Identifier(name.clone())),
                )));
            }
            return Ok(Stmt::Block(stmts));
        }

        // Fallback: treat as expression statement.
        self.expr_stmt()
    }

    // Expression parsing (Pratt parser — precedence-based).

    fn expression(&mut self) -> Result<Expr, String> {
        self.assignment()
    }

    fn assignment(&mut self) -> Result<Expr, String> {
        let left = self.conditional()?;
        if matches!(self.peek(), Token::Operator(o) if o == "=" || o == "+=" || o == "-=" || o == "*=" || o == "/=" || o == "%=" || o == "**=" || o == "<<=" || o == ">>=" || o == ">>>=" || o == "&=" || o == "|=" || o == "^=" || o == "&&=" || o == "||=" || o == "??=")
        {
            let op = if let Token::Operator(o) = self.advance() {
                o
            } else {
                unreachable!()
            };
            let right = self.assignment()?;
            Ok(Expr::Assign(Box::new(left), op, Box::new(right)))
        } else {
            Ok(left)
        }
    }

    fn conditional(&mut self) -> Result<Expr, String> {
        let test = self.logical_or()?;
        // Ternary: test ? consequent : alternate
        // The lexer tokenizes '?' as Operator("?"), not Punct('?').
        if matches!(self.peek(), Token::Operator(o) if o == "?") {
            self.advance();
            let consequent = self.assignment()?;
            // Expect ':' — lexer tokenizes as Punct(':').
            self.match_punct(':');
            let alternate = self.assignment()?;
            Ok(Expr::Conditional(
                Box::new(test),
                Box::new(consequent),
                Box::new(alternate),
            ))
        } else {
            Ok(test)
        }
    }

    fn logical_or(&mut self) -> Result<Expr, String> {
        let mut left = self.logical_and()?;
        while matches!(self.peek(), Token::Operator(o) if o == "||" || o == "??") {
            let op = if let Token::Operator(o) = self.advance() {
                o
            } else {
                unreachable!()
            };
            let right = self.logical_and()?;
            left = Expr::Binary(Box::new(left), op, Box::new(right));
        }
        Ok(left)
    }

    fn logical_and(&mut self) -> Result<Expr, String> {
        let mut left = self.bitwise_or()?;
        while matches!(self.peek(), Token::Operator(o) if o == "&&") {
            let op = if let Token::Operator(o) = self.advance() {
                o
            } else {
                unreachable!()
            };
            let right = self.bitwise_or()?;
            left = Expr::Binary(Box::new(left), op, Box::new(right));
        }
        Ok(left)
    }

    fn bitwise_or(&mut self) -> Result<Expr, String> {
        let mut left = self.equality()?;
        while matches!(self.peek(), Token::Operator(o) if o == "|") {
            self.advance();
            let right = self.equality()?;
            left = Expr::Binary(Box::new(left), "|".to_string(), Box::new(right));
        }
        Ok(left)
    }

    fn equality(&mut self) -> Result<Expr, String> {
        let mut left = self.comparison()?;
        while matches!(self.peek(), Token::Operator(o) if o == "==" || o == "!=" || o == "===" || o == "!==")
        {
            let op = if let Token::Operator(o) = self.advance() {
                o
            } else {
                unreachable!()
            };
            let right = self.comparison()?;
            left = Expr::Binary(Box::new(left), op, Box::new(right));
        }
        Ok(left)
    }

    fn comparison(&mut self) -> Result<Expr, String> {
        let mut left = self.additive()?;
        loop {
            // Relational operators: <, >, <=, >=, instanceof, in
            if matches!(self.peek(), Token::Operator(o) if o == "<" || o == ">" || o == "<=" || o == ">=")
            {
                let op = if let Token::Operator(o) = self.advance() {
                    o
                } else {
                    unreachable!()
                };
                let right = self.additive()?;
                left = Expr::Binary(Box::new(left), op, Box::new(right));
            } else if matches!(self.peek(), Token::Keyword(k) if k == "instanceof") {
                self.advance();
                let right = self.additive()?;
                left = Expr::Binary(Box::new(left), "instanceof".to_string(), Box::new(right));
            } else if matches!(self.peek(), Token::Keyword(k) if k == "in") {
                self.advance();
                let right = self.additive()?;
                left = Expr::Binary(Box::new(left), "in".to_string(), Box::new(right));
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn additive(&mut self) -> Result<Expr, String> {
        let mut left = self.multiplicative()?;
        while matches!(self.peek(), Token::Operator(o) if o == "+" || o == "-") {
            let op = if let Token::Operator(o) = self.advance() {
                o
            } else {
                unreachable!()
            };
            let right = self.multiplicative()?;
            left = Expr::Binary(Box::new(left), op, Box::new(right));
        }
        Ok(left)
    }

    fn multiplicative(&mut self) -> Result<Expr, String> {
        let mut left = self.exponentiation()?;
        while matches!(self.peek(), Token::Operator(o) if o == "*" || o == "/" || o == "%") {
            let op = if let Token::Operator(o) = self.advance() {
                o
            } else {
                unreachable!()
            };
            let right = self.exponentiation()?;
            left = Expr::Binary(Box::new(left), op, Box::new(right));
        }
        Ok(left)
    }

    /// Exponentiation: `base ** exp` (right-associative, higher than *).
    fn exponentiation(&mut self) -> Result<Expr, String> {
        let left = self.unary()?;
        if matches!(self.peek(), Token::Operator(o) if o == "**") {
            self.advance();
            let right = self.exponentiation()?; // right-associative
            return Ok(Expr::Binary(Box::new(left), "**".to_string(), Box::new(right)));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Expr, String> {
        if matches!(self.peek(), Token::Operator(o) if o == "!" || o == "-" || o == "+" || o == "~")
        {
            let op = if let Token::Operator(o) = self.advance() {
                o
            } else {
                unreachable!()
            };
            let expr = self.unary()?;
            return Ok(Expr::Unary(op, Box::new(expr)));
        }
        if matches!(self.peek(), Token::Keyword(k) if k == "typeof") {
            self.advance();
            let expr = self.unary()?;
            return Ok(Expr::Unary("typeof".to_string(), Box::new(expr)));
        }
        if matches!(self.peek(), Token::Keyword(k) if k == "delete") {
            self.advance();
            let expr = self.unary()?;
            return Ok(Expr::Unary("delete".to_string(), Box::new(expr)));
        }
        if matches!(self.peek(), Token::Keyword(k) if k == "void") {
            self.advance();
            let expr = self.unary()?;
            return Ok(Expr::Unary("void".to_string(), Box::new(expr)));
        }
        // Prefix ++ / --
        if matches!(self.peek(), Token::Operator(o) if o == "++" || o == "--") {
            let op = if let Token::Operator(o) = self.advance() {
                o
            } else {
                unreachable!()
            };
            let expr = self.unary()?;
            return Ok(Expr::Update(op, Box::new(expr), true));
        }
        self.postfix()
    }

    fn postfix(&mut self) -> Result<Expr, String> {
        let mut expr = self.call_member()?;
        if matches!(self.peek(), Token::Operator(o) if o == "++" || o == "--") {
            let op = if let Token::Operator(o) = self.advance() {
                o
            } else {
                unreachable!()
            };
            expr = Expr::Update(op, Box::new(expr), false);
        }
        Ok(expr)
    }

    fn call_member(&mut self) -> Result<Expr, String> {
        let mut expr = self.primary()?;

        loop {
            // Member access: a.b
            if self.match_punct('.') {
                let prop = match self.advance() {
                    Token::Identifier(s) => Expr::String(s),
                    Token::PrivateIdentifier(s) => {
                        // this.#x — store private name as a String with # prefix.
                        Expr::String(format!("#{}", s))
                    }
                    Token::Keyword(k) => Expr::String(k), // e.g. obj.length, obj.constructor
                    other => {
                        return Err(format!("Expected property name after '.', got {:?}", other))
                    }
                };
                expr = Expr::Member(Box::new(expr), Box::new(prop), false);
            }
            // Optional member access: a?.b  (note: ?.' already includes the dot)
            else if self.match_op("?.") {
                // After ?. we can have: identifier (a?.b), [b] (a?.[b]), or (args) (a?.())
                if matches!(
                    self.peek(),
                    Token::Identifier(_) | Token::Keyword(_) | Token::PrivateIdentifier(_)
                ) {
                    let prop = match self.advance() {
                        Token::Identifier(s) => Expr::String(s),
                        Token::PrivateIdentifier(s) => Expr::String(format!("#{}", s)),
                        Token::Keyword(k) => Expr::String(k),
                        other => {
                            return Err(format!(
                                "Expected property name after '?.', got {:?}",
                                other
                            ))
                        }
                    };
                    expr = Expr::OptionalMember(Box::new(expr), Box::new(prop), false);
                } else if self.match_punct('[') {
                    let prop = self.expression()?;
                    self.match_punct(']');
                    expr = Expr::OptionalMember(Box::new(expr), Box::new(prop), true);
                } else if matches!(self.peek(), Token::Punct('(')) {
                    let args = self.parse_args()?;
                    expr = Expr::OptionalCall(Box::new(expr), args);
                } else {
                    return Err("Expected identifier, '[', or '(' after '?.'".to_string());
                }
            }
            // Computed member: a[b]
            else if self.match_punct('[') {
                let prop = self.expression()?;
                self.match_punct(']');
                expr = Expr::Member(Box::new(expr), Box::new(prop), true);
            }
            // Function call: f(args)
            else if matches!(self.peek(), Token::Punct('(')) {
                let args = self.parse_args()?;
                expr = Expr::Call(Box::new(expr), args);
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn parse_args(&mut self) -> Result<Vec<Expr>, String> {
        self.match_punct('(');
        let mut args = Vec::new();
        while !matches!(self.peek(), Token::Punct(')')) {
            // Spread in call: f(...args)
            if self.match_op("...") {
                let e = self.assignment()?;
                args.push(Expr::Spread(Box::new(e)));
            } else {
                args.push(self.assignment()?);
            }
            if !self.match_punct(',') {
                break;
            }
        }
        self.match_punct(')');
        Ok(args)
    }

    fn primary(&mut self) -> Result<Expr, String> {
        match self.peek().clone() {
            Token::Number(n) => {
                self.advance();
                Ok(Expr::Number(n))
            }
            Token::BigInt(s) => {
                self.advance();
                Ok(Expr::BigInt(s))
            }
            Token::String(s) => {
                self.advance();
                // Check for regex literal: stored as \x01regex\x02pattern|flags
                if s.starts_with("\x01regex\x02") {
                    let inner = &s["\x01regex\x02".len()..];
                    let parts: Vec<&str> = inner.splitn(2, '|').collect();
                    let pattern = parts[0].to_string();
                    let flags = parts.get(1).unwrap_or(&"").to_string();
                    // Generate: new RegExp("pattern", "flags")
                    return Ok(Expr::New(
                        Box::new(Expr::Identifier("RegExp".to_string())),
                        vec![Expr::String(pattern), Expr::String(flags)],
                    ));
                }
                // Check for template literal: String + Punct('`') means ${expr}
                if matches!(self.peek(), Token::Punct('`')) {
                    return self.parse_template_literal(Expr::String(s));
                }
                Ok(Expr::String(s))
            }
            Token::Boolean(b) => {
                self.advance();
                Ok(Expr::Boolean(b))
            }
            Token::Null => {
                self.advance();
                Ok(Expr::Null)
            }
            Token::Undefined => {
                self.advance();
                Ok(Expr::Undefined)
            }
            Token::Identifier(s) => {
                self.advance();
                // Arrow function: (params) => ... or x => ...
                if self.match_op("=>") {
                    let body = if matches!(self.peek(), Token::Punct('{')) {
                        let block = self.parse_block()?;
                        Stmt::Block(block)
                    } else {
                        let e = self.assignment()?;
                        Stmt::Return(Some(e))
                    };
                    return Ok(Expr::Arrow(vec![s], Box::new(body)));
                }
                Ok(Expr::Identifier(s))
            }
            Token::PrivateIdentifier(s) => {
                // Reference to a private field (e.g. #x in a class method).
                self.advance();
                Ok(Expr::PrivateIdentifier(s))
            }
            Token::Keyword(k) if k == "function" => {
                self.advance();
                let is_generator = self.match_op("*");
                // Optional function name (named function expression).
                // e.g. `function foo() { ... }` or `(function serverContract() { ... })()`
                let _name = if let Token::Identifier(s) = self.peek().clone() {
                    self.advance();
                    Some(s)
                } else {
                    None
                };
                let params = self.parse_params()?;
                let body = self.parse_block()?;
                if is_generator {
                    Ok(Expr::GeneratorFunction(params, body))
                } else {
                    Ok(Expr::Function(params, body))
                }
            }
            Token::Keyword(k) if k == "async" => {
                // async function / async function* / async arrow
                self.advance();
                if matches!(self.peek(), Token::Keyword(k) if k == "function") {
                    self.advance(); // skip 'function'
                    let is_generator = self.match_op("*");
                    let params = self.parse_params()?;
                    let body = self.parse_block()?;
                    if is_generator {
                        Ok(Expr::AsyncGeneratorFunction(params, body))
                    } else {
                        Ok(Expr::AsyncFunction(params, body))
                    }
                } else {
                    // async x => ... or async (x, y) => ...
                    // Treat as regular arrow — async semantics aren't fully supported.
                    let params = if matches!(self.peek(), Token::Punct('(')) {
                        // (a, b) => ...
                        let saved = self.pos;
                        let mut ps = Vec::new();
                        let mut is_arrow = false;
                        self.advance(); // (
                        while !matches!(self.peek(), Token::Punct(')')) {
                            if let Token::Identifier(s) = self.advance() {
                                ps.push(s);
                            }
                            if !self.match_punct(',') {
                                break;
                            }
                        }
                        self.match_punct(')');
                        if self.match_op("=>") {
                            is_arrow = true;
                        }
                        if !is_arrow {
                            self.pos = saved;
                        }
                        if is_arrow {
                            ps
                        } else {
                            Vec::new()
                        }
                    } else {
                        // x => ...
                        let name = match self.advance() {
                            Token::Identifier(s) => s,
                            other => {
                                return Err(format!(
                                    "Expected identifier after 'async', got {:?}",
                                    other
                                ))
                            }
                        };
                        if !self.match_op("=>") {
                            return Err("Expected '=>' after async identifier".to_string());
                        }
                        vec![name]
                    };
                    let body = if matches!(self.peek(), Token::Punct('{')) {
                        Stmt::Block(self.parse_block()?)
                    } else {
                        let e = self.assignment()?;
                        Stmt::Return(Some(e))
                    };
                    Ok(Expr::Arrow(params, Box::new(body)))
                }
            }
            Token::Keyword(k) if k == "class" => {
                self.advance(); // skip 'class'
                let class_def = self.parse_class_body()?;
                Ok(Expr::Class(class_def))
            }
            Token::Keyword(k) if k == "new" => {
                self.advance();
                // new.target
                if self.match_punct('.') {
                    if let Token::Identifier(s) = self.peek().clone() {
                        if s == "target" {
                            self.advance();
                            return Ok(Expr::NewTarget);
                        }
                    }
                }
                // Parse the callee WITHOUT consuming the call's parentheses.
                // We use a restricted member-access parser that stops at `(`.
                let callee = self.new_callee()?;
                let args = if matches!(self.peek(), Token::Punct('(')) {
                    self.parse_args()?
                } else {
                    Vec::new()
                };
                Ok(Expr::New(Box::new(callee), args))
            }
            Token::Keyword(k) if k == "this" => {
                self.advance();
                Ok(Expr::This)
            }
            Token::Keyword(k) if k == "super" => {
                self.advance();
                Ok(Expr::Super)
            }
            Token::Keyword(k) if k == "yield" => {
                self.advance();
                // yield (no value) or yield value or yield* value
                if matches!(
                    self.peek(),
                    Token::Punct(';')
                        | Token::Punct(')')
                        | Token::Punct(',')
                        | Token::Punct(']')
                        | Token::Punct('}')
                        | Token::EOF
                ) {
                    return Ok(Expr::Yield(None));
                }
                let _ = self.match_op("*"); // yield* — delegate, ignored for now
                let e = self.assignment()?;
                Ok(Expr::Yield(Some(Box::new(e))))
            }
            Token::Keyword(k) if k == "await" => {
                self.advance();
                let e = self.unary()?;
                Ok(Expr::Await(Box::new(e)))
            }
            Token::Punct('(') => {
                self.advance();
                // Check for arrow function: (a, b) => ...
                let mut params = Vec::new();
                let mut is_arrow = false;
                if !matches!(self.peek(), Token::Punct(')')) {
                    // Try to parse as params (identifiers separated by commas).
                    let saved = self.pos;
                    let mut try_params = true;
                    loop {
                        if let Token::Identifier(s) = self.peek().clone() {
                            params.push(s);
                            self.advance();
                            if self.match_punct(',') {
                                continue;
                            }
                            if matches!(self.peek(), Token::Punct(')')) {
                                self.advance();
                                if self.match_op("=>") {
                                    is_arrow = true;
                                }
                                break;
                            } else {
                                try_params = false;
                                break;
                            }
                        } else {
                            try_params = false;
                            break;
                        }
                    }
                    if !try_params {
                        self.pos = saved;
                        params.clear();
                    }
                } else {
                    self.advance(); // skip ')'
                    if self.match_op("=>") {
                        is_arrow = true;
                    }
                }

                if is_arrow {
                    let body = if matches!(self.peek(), Token::Punct('{')) {
                        Stmt::Block(self.parse_block()?)
                    } else {
                        let e = self.assignment()?;
                        Stmt::Return(Some(e))
                    };
                    return Ok(Expr::Arrow(params, Box::new(body)));
                }

                let expr = self.expression()?;
                self.match_punct(')');
                Ok(expr)
            }
            Token::Punct('[') => {
                self.advance();
                let mut elements = Vec::new();
                while !matches!(self.peek(), Token::Punct(']')) {
                    // Spread in array: [...x, y, ...z]
                    if self.match_op("...") {
                        let e = self.assignment()?;
                        elements.push(Expr::Spread(Box::new(e)));
                    } else {
                        elements.push(self.assignment()?);
                    }
                    if !self.match_punct(',') {
                        break;
                    }
                }
                self.match_punct(']');
                Ok(Expr::Array(elements))
            }
            Token::Punct('{') => {
                self.advance();
                let mut properties = Vec::new();
                while !matches!(self.peek(), Token::Punct('}')) {
                    let key = match self.advance() {
                        Token::Identifier(s) => s,
                        Token::String(s) => s,
                        Token::Keyword(s) => s,
                        Token::Number(n) => n.to_string(),
                        other => return Err(format!("Expected property key, got {:?}", other)),
                    };
                    self.match_punct(':');
                    let value = self.assignment()?;
                    properties.push((key, value));
                    if !self.match_punct(',') {
                        break;
                    }
                }
                self.match_punct('}');
                Ok(Expr::Object(properties))
            }
            Token::Punct('`') => {
                self.advance();
                Ok(Expr::String(String::new()))
            }
            other => Err(format!("Unexpected token in expression: {:?}", other)),
        }
    }
}
