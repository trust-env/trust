#[derive(Debug, Clone)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone)]
pub struct Spanned<T> {
    pub span: Span,
    pub value: T,
}

impl<T> Spanned<T> {
    pub fn new(start: usize, end: usize, value: T) -> Self {
        Spanned {
            span: Span { start, end },
            value,
        }
    }
}
