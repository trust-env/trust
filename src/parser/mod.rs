pub mod adapter;
pub mod app;
pub mod ast;
mod config;
mod driver;
mod import;
mod infra;
mod plugin;
mod resource;
mod schedule;
mod type_decl;
mod use_case;

pub use ast::*;

use crate::lexer::{Spanned, Token};

pub struct Parser {
    tokens: Vec<Spanned<Token>>,
    current: usize,
    /// Original source text, as chars (matching `Span`'s char-offset
    /// convention), used only to slice out a plugin construct's raw body
    /// text faithfully. Empty when `with_source` was never called — this
    /// is fine as long as no plugin keywords are registered either, since
    /// `try_plugin_construct` can then never match anything.
    source_chars: Vec<char>,
    /// Keyword strings registered by loaded `--plugin`s. An unrecognized
    /// bareword already lexes as `Token::Identifier`, so this is the only
    /// way the parser can tell "this identifier is a plugin construct"
    /// apart from an ordinary identifier.
    plugin_keywords: Vec<String>,
    /// The furthest token index ever read — where a parse error is
    /// reported, since alternatives backtrack before failing.
    furthest: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Spanned<Token>>) -> Self {
        Parser {
            tokens,
            current: 0,
            source_chars: Vec::new(),
            plugin_keywords: Vec::new(),
            furthest: 0,
        }
    }

    pub fn with_source(mut self, source: &str) -> Self {
        self.source_chars = source.chars().collect();
        self
    }

    pub fn with_plugin_keywords(mut self, keywords: Vec<String>) -> Self {
        self.plugin_keywords = keywords;
        self
    }

    fn next_token(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.current);
        if self.current > self.furthest {
            self.furthest = self.current;
        }
        self.current += 1;
        token.map(|spanned| spanned.value.clone())
    }

    fn reset(&mut self, position: usize) {
        self.current = position;
    }

    fn revert(&mut self, offset: usize) {
        if self.current >= offset {
            self.current -= offset;
        } else {
            self.current = 0;
        }
    }
}

impl Parser {
    /// Char offset of the furthest token read, for error locations.
    pub fn error_offset(&self) -> usize {
        self.tokens
            .get(self.furthest.min(self.tokens.len().saturating_sub(1)))
            .map(|t| t.span.start)
            .unwrap_or(0)
    }
}
