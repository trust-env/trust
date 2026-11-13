//! Tokens of a `logic { ... }` script.

use super::{Pos, ScriptError};

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Ident(String),
    Int(String),
    Float(String),
    /// A string literal: text pieces and `{expr}` pieces (already lexed).
    Str(Vec<Piece>),
    Sym(&'static str),
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Piece {
    Text(String),
    Code(Vec<Token>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub pos: Pos,
}

/// Longest first, so `==` wins over `=`.
const SYMBOLS: &[&str] = &[
    "==", "!=", "<=", ">=", "=>", "->", "??", "..", "=", "<", ">", "+", "-", "*", "/", "%", "(",
    ")", "[", "]", "{", "}", ",", ":", ".", "&",
];

/// What people coming from other languages type, and what to write instead.
const FOREIGN: &[(&str, &str)] = &[
    ("&&", "write 'and'"),
    ("||", "write 'or'"),
    ("!", "write 'not' (or '!=')"),
    (
        "?",
        "infra errors propagate on their own; '??' gives a default to an Option",
    ),
    ("'", "strings use double quotes"),
];

pub fn lex(chars: &[char], start: Pos) -> Result<Vec<Token>, ScriptError> {
    let mut lexer = Lexer {
        chars,
        i: 0,
        pos: start,
    };
    let mut out = Vec::new();
    loop {
        let token = lexer.next()?;
        let eof = token.tok == Tok::Eof;
        out.push(token);
        if eof {
            return Ok(out);
        }
    }
}

struct Lexer<'a> {
    chars: &'a [char],
    i: usize,
    pos: Pos,
}

impl Lexer<'_> {
    fn peek(&self, ahead: usize) -> Option<char> {
        self.chars.get(self.i + ahead).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek(0)?;
        self.i += 1;
        if c == '\n' {
            self.pos.line += 1;
            self.pos.col = 1;
        } else {
            self.pos.col += 1;
        }
        Some(c)
    }

    fn skip_trivia(&mut self) {
        loop {
            match (self.peek(0), self.peek(1)) {
                (Some(c), _) if c.is_whitespace() || c == ';' => {
                    self.bump();
                }
                (Some('/'), Some('/')) => {
                    while self.peek(0).is_some_and(|c| c != '\n') {
                        self.bump();
                    }
                }
                _ => return,
            }
        }
    }

    fn next(&mut self) -> Result<Token, ScriptError> {
        self.skip_trivia();
        let pos = self.pos;
        let Some(c) = self.peek(0) else {
            return Ok(Token { tok: Tok::Eof, pos });
        };
        if c.is_alphabetic() || c == '_' {
            let mut word = String::new();
            while let Some(c) = self.peek(0).filter(|c| c.is_alphanumeric() || *c == '_') {
                word.push(c);
                self.bump();
            }
            return Ok(Token {
                tok: Tok::Ident(word),
                pos,
            });
        }
        if c.is_ascii_digit() {
            return Ok(Token {
                tok: self.number(),
                pos,
            });
        }
        if c == '"' {
            self.bump();
            return Ok(Token {
                tok: Tok::Str(self.string(pos)?),
                pos,
            });
        }
        // `&&` / `||` before `&` is read as a symbol.
        for (foreign, hint) in &FOREIGN[..2] {
            if foreign
                .chars()
                .enumerate()
                .all(|(k, f)| self.peek(k) == Some(f))
            {
                return Err(ScriptError::new(
                    pos,
                    format!("unexpected '{}': {}", foreign, hint),
                ));
            }
        }
        for symbol in SYMBOLS {
            let len = symbol.chars().count();
            if (0..len).all(|k| self.peek(k) == symbol.chars().nth(k)) {
                for _ in 0..len {
                    self.bump();
                }
                return Ok(Token {
                    tok: Tok::Sym(symbol),
                    pos,
                });
            }
        }
        for (foreign, hint) in FOREIGN {
            if foreign
                .chars()
                .enumerate()
                .all(|(k, f)| self.peek(k) == Some(f))
            {
                return Err(ScriptError::new(
                    pos,
                    format!("unexpected '{}': {}", foreign, hint),
                ));
            }
        }
        Err(ScriptError::new(
            pos,
            format!("unexpected character '{}'", c),
        ))
    }

    /// After the `{` of an interpolation: up to its `}`, stepping over
    /// strings inside it (`"{d.format("dd/MM")}"`). False at the end.
    fn skip_code(&mut self) -> bool {
        let mut depth = 1;
        loop {
            match self.bump() {
                Some('{') => depth += 1,
                Some('}') => {
                    depth -= 1;
                    if depth == 0 {
                        return true;
                    }
                }
                // The string is consumed by the guard either way.
                Some('"') if !self.skip_string() => return false,
                None => return false,
                _ => {}
            }
        }
    }

    /// After a `"` inside an interpolation: up to its closing quote.
    fn skip_string(&mut self) -> bool {
        loop {
            match self.bump() {
                Some('\\') => {
                    self.bump();
                }
                Some('"') => return true,
                Some('{') if self.peek(0) == Some('{') => {
                    self.bump();
                }
                Some('{') if !self.skip_code() => return false,
                None => return false,
                _ => {}
            }
        }
    }

    fn number(&mut self) -> Tok {
        let mut text = String::new();
        let mut float = false;
        while let Some(c) = self.peek(0) {
            if c.is_ascii_digit() {
                text.push(c);
            } else if c == '_' {
            } else if c == '.' && !float && self.peek(1).is_some_and(|d| d.is_ascii_digit()) {
                float = true;
                text.push(c);
            } else {
                break;
            }
            self.bump();
        }
        if float {
            Tok::Float(text)
        } else {
            Tok::Int(text)
        }
    }

    /// After the opening quote. `{expr}` interpolates, `{{`/`}}` are
    /// literal braces.
    fn string(&mut self, start: Pos) -> Result<Vec<Piece>, ScriptError> {
        let mut pieces = Vec::new();
        let mut text = String::new();
        loop {
            let Some(c) = self.bump() else {
                return Err(ScriptError::new(start, "unterminated string".to_string()));
            };
            match c {
                '"' => break,
                '\\' => {
                    let escaped = self.bump();
                    text.push(match escaped {
                        Some('n') => '\n',
                        Some('t') => '\t',
                        Some('r') => '\r',
                        Some('\\') => '\\',
                        Some('"') => '"',
                        other => {
                            return Err(ScriptError::new(
                                self.pos,
                                format!(
                                    "unknown escape '\\{}'",
                                    other.map(String::from).unwrap_or_default()
                                ),
                            ));
                        }
                    });
                }
                '{' if self.peek(0) == Some('{') => {
                    self.bump();
                    text.push('{');
                }
                '}' if self.peek(0) == Some('}') => {
                    self.bump();
                    text.push('}');
                }
                '}' => {
                    return Err(ScriptError::new(
                        self.pos,
                        "a lone '}' in a string: write '}}'".to_string(),
                    ));
                }
                '{' => {
                    if !text.is_empty() {
                        pieces.push(Piece::Text(std::mem::take(&mut text)));
                    }
                    let code_start = self.pos;
                    let begin = self.i;
                    if !self.skip_code() {
                        return Err(ScriptError::new(
                            code_start,
                            "unterminated '{' in a string".to_string(),
                        ));
                    }
                    let inner = &self.chars[begin..self.i - 1];
                    let tokens = lex(inner, code_start)?;
                    if tokens.len() == 1 {
                        return Err(ScriptError::new(
                            code_start,
                            "empty '{}' in a string: write '{{}}'".to_string(),
                        ));
                    }
                    pieces.push(Piece::Code(tokens));
                }
                c => text.push(c),
            }
        }
        if !text.is_empty() || pieces.is_empty() {
            pieces.push(Piece::Text(text));
        }
        Ok(pieces)
    }
}
