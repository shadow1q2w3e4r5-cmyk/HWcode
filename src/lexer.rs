//! Lexer for the HWCode Programming Language and Hardware Definition Language (.hwd).
//! Supports endpoint literals (`/F3`, `/F3::dma`, `/N0/F3`), explicit routes (`->`, `→`),
//! auto-discovered routes (`~>`, `■`), physical unit literals (`50W`, `5ms`, `100us`), and annotations (`@realtime`).

use crate::diagnostics::Span;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Identifiers & Literals
    Ident(String),
    IntLit(i64, Option<String>), // value + optional unit like "W", "ms", "us", "C", "GBps"
    FloatLit(f64, Option<String>), // value + optional unit
    StringLit(String),
    EndpointLit(String), // e.g. "/F3", "/F3::dma", "/N0/F3"

    // Keywords
    Fn,
    Let,
    Mut,
    Struct,
    Layout,
    Device,
    States,
    Transition,
    Requires,
    Register,
    Op,
    Dma,
    Route,
    Fallback,
    Find,
    Acquire,
    Release,
    AcquireBudget,
    SplitBudget,
    Await,
    Unsafe,
    Trace,
    Return,
    If,
    Else,
    While,
    Extern,
    With,
    Kernel,
    Dispatch,
    Interrupt,

    // Operators & Punctuation
    Arrow,       // -> or →
    AutoRoute,   // ~> or ==> or ■
    Colon,       // :
    DoubleColon, // ::
    Semicolon,   // ;
    Comma,       // ,
    Dot,         // .
    At,          // @
    Assign,      // =
    EqEq,        // ==
    NotEq,       // !=
    Lt,          // <
    Gt,          // >
    Le,          // <=
    Ge,          // >=
    Plus,        // +
    Minus,       // -
    Star,        // *
    Slash,       // /
    Ampersand,   // &
    LParen,      // (
    RParen,      // )
    LBrace,      // {
    RBrace,      // }
    LBracket,    // [
    RBracket,    // ]

    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

pub struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Self {
            chars: source.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_next(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.chars.get(self.pos).copied()?;
        self.pos += 1;
        if ch == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(ch)
    }

    fn is_endpoint_start(&self) -> bool {
        // Matches `/F1`, `/F3::dma`, `/N0/F3`
        if self.peek() != Some('/') {
            return false;
        }
        if let Some(c1) = self.peek_next() {
            if (c1 == 'F' || c1 == 'N')
                && self
                    .chars
                    .get(self.pos + 2)
                    .map(|c2| c2.is_ascii_digit())
                    .unwrap_or(false)
            {
                return true;
            }
        }
        false
    }

    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();

        while let Some(ch) = self.peek() {
            if ch.is_whitespace() {
                self.advance();
                continue;
            }

            // Comments
            if ch == '/' && self.peek_next() == Some('/') {
                while let Some(c) = self.peek() {
                    self.advance();
                    if c == '\n' {
                        break;
                    }
                }
                continue;
            }
            if ch == '/' && self.peek_next() == Some('*') {
                self.advance();
                self.advance();
                while let Some(c) = self.peek() {
                    if c == '*' && self.peek_next() == Some('/') {
                        self.advance();
                        self.advance();
                        break;
                    }
                    self.advance();
                }
                continue;
            }

            let span = Span::new(self.line, self.col);

            // Endpoint literal: `/F3`, `/F3::dma`, `/N0/F3`
            if self.is_endpoint_start() {
                let mut ep = String::new();
                ep.push(self.advance().unwrap()); // '/'
                while let Some(c) = self.peek() {
                    if c.is_ascii_alphanumeric() || c == '_' || c == '/' {
                        ep.push(self.advance().unwrap());
                    } else if c == ':' && self.peek_next() == Some(':') {
                        ep.push(self.advance().unwrap());
                        ep.push(self.advance().unwrap());
                    } else {
                        break;
                    }
                }
                tokens.push(Token {
                    kind: TokenKind::EndpointLit(ep),
                    span,
                });
                continue;
            }

            // String literal
            if ch == '"' {
                self.advance();
                let mut s = String::new();
                while let Some(c) = self.peek() {
                    self.advance();
                    if c == '"' {
                        break;
                    }
                    s.push(c);
                }
                tokens.push(Token {
                    kind: TokenKind::StringLit(s),
                    span,
                });
                continue;
            }

            // Numbers (with optional physical unit like 50W, 5ms, 100us, 85C, 10GBps)
            if ch.is_ascii_digit() {
                let mut num_str = String::new();
                let mut is_float = false;
                while let Some(c) = self.peek() {
                    if c.is_ascii_digit() {
                        num_str.push(self.advance().unwrap());
                    } else if c == '.'
                        && self
                            .peek_next()
                            .map(|n| n.is_ascii_digit())
                            .unwrap_or(false)
                        && !is_float
                    {
                        is_float = true;
                        num_str.push(self.advance().unwrap());
                    } else {
                        break;
                    }
                }
                let mut unit = None;
                if let Some(u) = self.peek() {
                    if u.is_ascii_alphabetic() {
                        let mut u_str = String::new();
                        while let Some(uc) = self.peek() {
                            if uc.is_ascii_alphanumeric() {
                                u_str.push(self.advance().unwrap());
                            } else {
                                break;
                            }
                        }
                        unit = Some(u_str);
                    }
                }
                if is_float {
                    let val = num_str.parse::<f64>().unwrap_or(0.0);
                    tokens.push(Token {
                        kind: TokenKind::FloatLit(val, unit),
                        span,
                    });
                } else {
                    let val = num_str.parse::<i64>().unwrap_or(0);
                    tokens.push(Token {
                        kind: TokenKind::IntLit(val, unit),
                        span,
                    });
                }
                continue;
            }

            // Identifiers & Keywords
            if ch.is_ascii_alphabetic() || ch == '_' {
                let mut ident = String::new();
                while let Some(c) = self.peek() {
                    if c.is_ascii_alphanumeric() || c == '_' {
                        ident.push(self.advance().unwrap());
                    } else {
                        break;
                    }
                }
                let kind = match ident.as_str() {
                    "fn" => TokenKind::Fn,
                    "let" => TokenKind::Let,
                    "mut" => TokenKind::Mut,
                    "struct" => TokenKind::Struct,
                    "layout" => TokenKind::Layout,
                    "device" => TokenKind::Device,
                    "states" => TokenKind::States,
                    "transition" => TokenKind::Transition,
                    "requires" => TokenKind::Requires,
                    "register" => TokenKind::Register,
                    "op" => TokenKind::Op,
                    "dma" => TokenKind::Dma,
                    "route" => TokenKind::Route,
                    "fallback" => TokenKind::Fallback,
                    "find" => TokenKind::Find,
                    "acquire" => TokenKind::Acquire,
                    "release" => TokenKind::Release,
                    "acquire_budget" => TokenKind::AcquireBudget,
                    "split_budget" => TokenKind::SplitBudget,
                    "await" => TokenKind::Await,
                    "unsafe" => TokenKind::Unsafe,
                    "trace" => TokenKind::Trace,
                    "return" => TokenKind::Return,
                    "if" => TokenKind::If,
                    "else" => TokenKind::Else,
                    "while" => TokenKind::While,
                    "extern" => TokenKind::Extern,
                    "with" => TokenKind::With,
                    "kernel" => TokenKind::Kernel,
                    "dispatch" => TokenKind::Dispatch,
                    "interrupt" => TokenKind::Interrupt,
                    _ => TokenKind::Ident(ident),
                };
                tokens.push(Token { kind, span });
                continue;
            }

            // Multi-char and Unicode operators
            match ch {
                '■' | '⇒' | '⇝' => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::AutoRoute,
                        span,
                    });
                }
                '→' => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::Arrow,
                        span,
                    });
                }
                '~' if self.peek_next() == Some('>') => {
                    self.advance();
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::AutoRoute,
                        span,
                    });
                }
                '-' if self.peek_next() == Some('>') => {
                    self.advance();
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::Arrow,
                        span,
                    });
                }
                ':' if self.peek_next() == Some(':') => {
                    self.advance();
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::DoubleColon,
                        span,
                    });
                }
                '=' if self.peek_next() == Some('>') => {
                    self.advance();
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::AutoRoute,
                        span,
                    });
                }
                '=' if self.peek_next() == Some('=') => {
                    self.advance();
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::EqEq,
                        span,
                    });
                }
                '!' if self.peek_next() == Some('=') => {
                    self.advance();
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::NotEq,
                        span,
                    });
                }
                '<' if self.peek_next() == Some('=') => {
                    self.advance();
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::Le,
                        span,
                    });
                }
                '>' if self.peek_next() == Some('=') => {
                    self.advance();
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::Ge,
                        span,
                    });
                }
                ':' => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::Colon,
                        span,
                    });
                }
                ';' => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::Semicolon,
                        span,
                    });
                }
                ',' => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::Comma,
                        span,
                    });
                }
                '.' => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::Dot,
                        span,
                    });
                }
                '@' => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::At,
                        span,
                    });
                }
                '=' => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::Assign,
                        span,
                    });
                }
                '<' => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::Lt,
                        span,
                    });
                }
                '>' => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::Gt,
                        span,
                    });
                }
                '+' => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::Plus,
                        span,
                    });
                }
                '-' => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::Minus,
                        span,
                    });
                }
                '*' => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::Star,
                        span,
                    });
                }
                '/' => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::Slash,
                        span,
                    });
                }
                '&' => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::Ampersand,
                        span,
                    });
                }
                '(' => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::LParen,
                        span,
                    });
                }
                ')' => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::RParen,
                        span,
                    });
                }
                '{' => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::LBrace,
                        span,
                    });
                }
                '}' => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::RBrace,
                        span,
                    });
                }
                '[' => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::LBracket,
                        span,
                    });
                }
                ']' => {
                    self.advance();
                    tokens.push(Token {
                        kind: TokenKind::RBracket,
                        span,
                    });
                }
                _ => {
                    self.advance();
                }
            }
        }

        tokens.push(Token {
            kind: TokenKind::Eof,
            span: Span::new(self.line, self.col),
        });
        tokens
    }
}
