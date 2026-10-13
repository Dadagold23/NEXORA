use crate::diagnostic::Diagnostic;
use crate::source::{SourceFile, Span};

#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    Identifier(String),
    Integer(i64),
    Float(f64),
    String(String),
    Character(char),
    StringTemplate(Vec<TokenStringPart>),
    Let,
    Const,
    Mut,
    Ref,
    Borrow,
    Unsafe,
    Fn,
    Async,
    Await,
    Spawn,
    Actor,
    Channel,
    If,
    Else,
    Match,
    When,
    Case,
    For,
    While,
    Loop,
    Break,
    Continue,
    Return,
    Yield,
    Try,
    Catch,
    Finally,
    Throw,
    Module,
    Import,
    Export,
    From,
    As,
    Struct,
    Class,
    Interface,
    Trait,
    Enum,
    Impl,
    Type,
    Use,
    Where,
    True,
    False,
    Null,
    New,
    In,
    Is,
    And,
    Or,
    Not,
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    Comma,
    Colon,
    Semicolon,
    Dot,
    NullSafeDot,
    Question,
    Range,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Equal,
    PlusEqual,
    MinusEqual,
    StarEqual,
    SlashEqual,
    PercentEqual,
    AndEqual,
    OrEqual,
    EqualEqual,
    Bang,
    BangEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    ShiftLeft,
    ShiftRight,
    AndAnd,
    OrOr,
    Ampersand,
    Pipe,
    Caret,
    Arrow,
    FatArrow,
    Eof,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TokenStringPart {
    Text(String),
    Expression(Vec<Token>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

pub fn lex(source: &SourceFile) -> Result<Vec<Token>, Diagnostic> {
    Lexer::new(source).lex()
}

struct Lexer<'a> {
    source: &'a SourceFile,
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a SourceFile) -> Self {
        Self {
            source,
            bytes: source.text.as_bytes(),
            offset: 0,
        }
    }

    fn lex(mut self) -> Result<Vec<Token>, Diagnostic> {
        let mut tokens = Vec::new();
        while self.offset < self.bytes.len() {
            let byte = self.bytes[self.offset];
            if byte.is_ascii_whitespace() {
                self.offset += 1;
                continue;
            }
            if byte == b'/' && self.peek(1) == Some(b'/') {
                self.skip_line_comment();
                continue;
            }
            if byte == b'/' && self.peek(1) == Some(b'*') {
                self.skip_block_comment()?;
                continue;
            }

            let start = self.offset;
            let kind = if is_identifier_start(byte) {
                self.identifier()
            } else if byte.is_ascii_digit() {
                self.number()?
            } else if matches!(byte, b'"' | b'\'' | b'`') {
                self.string()?
            } else {
                self.offset += 1;
                match byte {
                    b'(' => TokenKind::LeftParen,
                    b')' => TokenKind::RightParen,
                    b'{' => TokenKind::LeftBrace,
                    b'}' => TokenKind::RightBrace,
                    b'[' => TokenKind::LeftBracket,
                    b']' => TokenKind::RightBracket,
                    b',' => TokenKind::Comma,
                    b':' => TokenKind::Colon,
                    b';' => TokenKind::Semicolon,
                    b'.' if self.take(b'.') => TokenKind::Range,
                    b'.' => TokenKind::Dot,
                    b'?' if self.take(b'.') => TokenKind::NullSafeDot,
                    b'?' => TokenKind::Question,
                    b'+' if self.take(b'=') => TokenKind::PlusEqual,
                    b'+' => TokenKind::Plus,
                    b'-' if self.take(b'>') => TokenKind::Arrow,
                    b'-' if self.take(b'=') => TokenKind::MinusEqual,
                    b'-' => TokenKind::Minus,
                    b'*' if self.take(b'=') => TokenKind::StarEqual,
                    b'*' => TokenKind::Star,
                    b'/' if self.take(b'=') => TokenKind::SlashEqual,
                    b'/' => TokenKind::Slash,
                    b'%' if self.take(b'=') => TokenKind::PercentEqual,
                    b'%' => TokenKind::Percent,
                    b'=' if self.take(b'=') => TokenKind::EqualEqual,
                    b'=' if self.take(b'>') => TokenKind::FatArrow,
                    b'=' => TokenKind::Equal,
                    b'!' if self.take(b'=') => TokenKind::BangEqual,
                    b'!' => TokenKind::Bang,
                    b'<' if self.take(b'<') => TokenKind::ShiftLeft,
                    b'<' if self.take(b'=') => TokenKind::LessEqual,
                    b'<' => TokenKind::Less,
                    b'>' if self.take(b'>') => TokenKind::ShiftRight,
                    b'>' if self.take(b'=') => TokenKind::GreaterEqual,
                    b'>' => TokenKind::Greater,
                    b'&' => {
                        if self.take(b'&') {
                            if self.take(b'=') {
                                TokenKind::AndEqual
                            } else {
                                TokenKind::AndAnd
                            }
                        } else {
                            TokenKind::Ampersand
                        }
                    }
                    b'|' => {
                        if self.take(b'|') {
                            if self.take(b'=') {
                                TokenKind::OrEqual
                            } else {
                                TokenKind::OrOr
                            }
                        } else {
                            TokenKind::Pipe
                        }
                    }
                    b'^' => TokenKind::Caret,
                    _ => {
                        return Err(Diagnostic::error(
                            "NXL0001",
                            format!("unexpected character '{}'", byte as char),
                            Span::new(start, self.offset),
                        ))
                    }
                }
            };
            tokens.push(Token {
                kind,
                span: Span::new(start, self.offset),
            });
        }
        tokens.push(Token {
            kind: TokenKind::Eof,
            span: Span::new(self.offset, self.offset),
        });
        Ok(tokens)
    }

    fn skip_line_comment(&mut self) {
        self.offset += 2;
        while self.offset < self.bytes.len() && self.bytes[self.offset] != b'\n' {
            self.offset += 1;
        }
    }

    fn skip_block_comment(&mut self) -> Result<(), Diagnostic> {
        let start = self.offset;
        self.offset += 2;
        while self.offset + 1 < self.bytes.len() {
            if self.bytes[self.offset] == b'*' && self.bytes[self.offset + 1] == b'/' {
                self.offset += 2;
                return Ok(());
            }
            self.offset += 1;
        }
        self.offset = self.bytes.len();
        Err(Diagnostic::error(
            "NXL0005",
            "unterminated block comment",
            Span::new(start, self.offset),
        ))
    }

    fn identifier(&mut self) -> TokenKind {
        let start = self.offset;
        self.offset += 1;
        while self.offset < self.bytes.len() && is_identifier_continue(self.bytes[self.offset]) {
            self.offset += 1;
        }
        match &self.source.text[start..self.offset] {
            "let" => TokenKind::Let,
            "const" => TokenKind::Const,
            "mut" => TokenKind::Mut,
            "ref" => TokenKind::Ref,
            "borrow" => TokenKind::Borrow,
            "unsafe" => TokenKind::Unsafe,
            "fn" => TokenKind::Fn,
            "async" => TokenKind::Async,
            "await" => TokenKind::Await,
            "spawn" => TokenKind::Spawn,
            "actor" => TokenKind::Actor,
            "channel" => TokenKind::Channel,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "match" => TokenKind::Match,
            "when" => TokenKind::When,
            "case" => TokenKind::Case,
            "for" => TokenKind::For,
            "while" => TokenKind::While,
            "loop" => TokenKind::Loop,
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
            "return" => TokenKind::Return,
            "yield" => TokenKind::Yield,
            "try" => TokenKind::Try,
            "catch" => TokenKind::Catch,
            "finally" => TokenKind::Finally,
            "throw" => TokenKind::Throw,
            "module" => TokenKind::Module,
            "import" => TokenKind::Import,
            "export" => TokenKind::Export,
            "from" => TokenKind::From,
            "as" => TokenKind::As,
            "struct" => TokenKind::Struct,
            "class" => TokenKind::Class,
            "interface" => TokenKind::Interface,
            "trait" => TokenKind::Trait,
            "enum" => TokenKind::Enum,
            "impl" => TokenKind::Impl,
            "type" => TokenKind::Type,
            "use" => TokenKind::Use,
            "where" => TokenKind::Where,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "null" => TokenKind::Null,
            "new" => TokenKind::New,
            "in" => TokenKind::In,
            "is" => TokenKind::Is,
            "and" => TokenKind::And,
            "or" => TokenKind::Or,
            "not" => TokenKind::Not,
            name => TokenKind::Identifier(name.to_owned()),
        }
    }

    fn number(&mut self) -> Result<TokenKind, Diagnostic> {
        let start = self.offset;
        if self.bytes[self.offset] == b'0'
            && matches!(self.peek(1), Some(b'x' | b'X' | b'b' | b'B'))
        {
            let radix = if matches!(self.peek(1), Some(b'x' | b'X')) {
                16
            } else {
                2
            };
            self.offset += 2;
            let digits_start = self.offset;
            while self.offset < self.bytes.len()
                && (self.bytes[self.offset].is_ascii_hexdigit() || self.bytes[self.offset] == b'_')
            {
                self.offset += 1;
            }
            let digits = self.source.text[digits_start..self.offset].replace('_', "");
            if digits.is_empty() {
                return Err(Diagnostic::error(
                    "NXL0002",
                    "numeric base prefix must be followed by digits",
                    Span::new(start, self.offset),
                ));
            }
            let value = i64::from_str_radix(&digits, radix).map_err(|_| {
                Diagnostic::error(
                    "NXL0002",
                    "integer literal is outside the supported range",
                    Span::new(start, self.offset),
                )
            })?;
            return Ok(TokenKind::Integer(value));
        }
        while self.offset < self.bytes.len()
            && (self.bytes[self.offset].is_ascii_digit() || self.bytes[self.offset] == b'_')
        {
            self.offset += 1;
        }
        let is_float = self.bytes.get(self.offset) == Some(&b'.')
            && self
                .bytes
                .get(self.offset + 1)
                .is_some_and(u8::is_ascii_digit);
        if is_float {
            self.offset += 1;
            while self.offset < self.bytes.len()
                && (self.bytes[self.offset].is_ascii_digit() || self.bytes[self.offset] == b'_')
            {
                self.offset += 1;
            }
        }
        let text = self.source.text[start..self.offset].replace('_', "");
        if is_float {
            text.parse::<f64>().map(TokenKind::Float).map_err(|_| {
                Diagnostic::error(
                    "NXL0002",
                    "invalid floating-point literal",
                    Span::new(start, self.offset),
                )
            })
        } else {
            text.parse::<i64>().map(TokenKind::Integer).map_err(|_| {
                Diagnostic::error(
                    "NXL0002",
                    "integer literal is outside the supported range",
                    Span::new(start, self.offset),
                )
            })
        }
    }

    fn string(&mut self) -> Result<TokenKind, Diagnostic> {
        let start = self.offset;
        let quote = self.bytes[self.offset];
        let raw = quote == b'`';
        self.offset += 1;
        let mut value = String::new();
        let mut parts = Vec::new();
        while self.offset < self.bytes.len() {
            if !raw && self.peek(0) == Some(b'$') && self.peek(1) == Some(b'{') {
                if !value.is_empty() {
                    parts.push(TokenStringPart::Text(std::mem::take(&mut value)));
                }
                let (expression, expression_span) = self.interpolation()?;
                if expression.trim().is_empty() {
                    return Err(Diagnostic::error(
                        "NXL0007",
                        "string interpolation requires an expression",
                        expression_span,
                    ));
                }
                let nested_source = SourceFile::new(self.source.name.clone(), expression);
                let mut nested_tokens = Lexer::new(&nested_source).lex().map_err(|mut error| {
                    error.span.start += expression_span.start;
                    error.span.end += expression_span.start;
                    error
                })?;
                shift_token_spans(&mut nested_tokens, expression_span.start);
                parts.push(TokenStringPart::Expression(nested_tokens));
                continue;
            }
            let byte = self.bytes[self.offset];
            self.offset += 1;
            if byte == quote {
                if parts.is_empty() {
                    if quote == b'\'' && value.chars().count() == 1 {
                        return Ok(TokenKind::Character(value.chars().next().unwrap()));
                    }
                    return Ok(TokenKind::String(value));
                }
                if !value.is_empty() {
                    parts.push(TokenStringPart::Text(value));
                }
                return Ok(TokenKind::StringTemplate(parts));
            }
            if byte == b'\\' && !raw {
                let Some(escaped) = self.bytes.get(self.offset).copied() else {
                    break;
                };
                self.offset += 1;
                value.push(match escaped {
                    b'n' => '\n',
                    b'r' => '\r',
                    b't' => '\t',
                    b'\\' => '\\',
                    b'"' => '"',
                    b'\'' => '\'',
                    b'$' => '$',
                    _ => {
                        return Err(Diagnostic::error(
                            "NXL0003",
                            "unsupported string escape",
                            Span::new(self.offset - 2, self.offset),
                        ))
                    }
                });
            } else if byte.is_ascii() {
                value.push(byte as char);
            } else {
                let start_char = self.offset - 1;
                let ch = self.source.text[start_char..]
                    .chars()
                    .next()
                    .expect("offset points to a valid character boundary");
                value.push(ch);
                self.offset = start_char + ch.len_utf8();
            }
        }
        Err(Diagnostic::error(
            "NXL0004",
            "unterminated string literal",
            Span::new(start, self.offset),
        ))
    }

    fn interpolation(&mut self) -> Result<(String, Span), Diagnostic> {
        let start = self.offset;
        self.offset += 2;
        let expression_start = self.offset;
        let mut depth = 1usize;
        let mut quote = None;
        while self.offset < self.bytes.len() {
            let byte = self.bytes[self.offset];
            if let Some(active_quote) = quote {
                self.offset += 1;
                if byte == b'\\' && active_quote != b'`' {
                    self.offset = (self.offset + 1).min(self.bytes.len());
                } else if byte == active_quote {
                    quote = None;
                }
                continue;
            }
            if byte == b'/' && self.peek(1) == Some(b'/') {
                self.skip_line_comment();
                continue;
            }
            if byte == b'/' && self.peek(1) == Some(b'*') {
                self.skip_block_comment()?;
                continue;
            }
            if matches!(byte, b'"' | b'\'' | b'`') {
                quote = Some(byte);
                self.offset += 1;
                continue;
            }
            match byte {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        let end = self.offset;
                        self.offset += 1;
                        return Ok((
                            self.source.text[expression_start..end].to_owned(),
                            Span::new(expression_start, end),
                        ));
                    }
                }
                _ => {}
            }
            self.offset += 1;
        }
        Err(Diagnostic::error(
            "NXL0006",
            "unterminated string interpolation",
            Span::new(start, self.offset),
        ))
    }

    fn peek(&self, distance: usize) -> Option<u8> {
        self.bytes.get(self.offset + distance).copied()
    }

    fn take(&mut self, expected: u8) -> bool {
        if self.bytes.get(self.offset) == Some(&expected) {
            self.offset += 1;
            true
        } else {
            false
        }
    }
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_identifier_continue(byte: u8) -> bool {
    is_identifier_start(byte) || byte.is_ascii_digit()
}

fn shift_token_spans(tokens: &mut [Token], offset: usize) {
    for token in tokens {
        token.span.start += offset;
        token.span.end += offset;
        if let TokenKind::StringTemplate(parts) = &mut token.kind {
            for part in parts {
                if let TokenStringPart::Expression(nested) = part {
                    shift_token_spans(nested, offset);
                }
            }
        }
    }
}
