#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn join(self, other: Self) -> Self {
        Self::new(self.start.min(other.start), self.end.max(other.end))
    }
}

#[derive(Clone, Debug)]
pub struct SourceFile {
    pub name: String,
    pub text: String,
}

impl SourceFile {
    /// Build a source file, stripping a leading UTF-8 byte-order mark.
    ///
    /// Windows editors and PowerShell's default `utf8` encoding write a BOM,
    /// and without this the very first character of such a file fails to lex.
    pub fn new(name: impl Into<String>, text: impl Into<String>) -> Self {
        let mut text = text.into();
        if text.starts_with('\u{feff}') {
            text.drain(..'\u{feff}'.len_utf8());
        }
        Self {
            name: name.into(),
            text,
        }
    }

    pub fn line_column(&self, byte_offset: usize) -> (usize, usize) {
        let mut offset = byte_offset.min(self.text.len());
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        let prefix = &self.text[..offset];
        let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
        let column = prefix
            .rsplit('\n')
            .next()
            .map_or(1, |last_line| last_line.chars().count() + 1);
        (line, column)
    }
}
