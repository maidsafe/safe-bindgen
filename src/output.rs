use std::fmt::{self, Write};
use std::ops::Deref;

pub struct IndentedWriter {
    inner: String,
    indent: bool,
    indent_level: usize,
    indent_width: usize,
}

impl IndentedWriter {
    pub fn new(width: usize) -> Self {
        IndentedWriter {
            inner: String::new(),
            indent: true,
            indent_level: 0,
            indent_width: width,
        }
    }

    pub fn into_inner(self) -> String {
        self.inner
    }

    pub fn indent(&mut self) {
        self.indent_level += 1;
    }

    pub fn unindent(&mut self) {
        self.indent_level -= 1;
    }

    fn write_indent(&mut self) -> Result<(), fmt::Error> {
        for _ in 0..(self.indent_level * self.indent_width) {
            self.inner.write_char(' ')?;
        }

        Ok(())
    }
}

impl Write for IndentedWriter {
    fn write_str(&mut self, s: &str) -> Result<(), fmt::Error> {
        if s.is_empty() {
            return Ok(());
        }

        let mut lines = s.lines().peekable();
        let mut indent = self.indent;

        while let Some(line) = lines.next() {
            if !line.is_empty() && indent {
                self.write_indent()?;
            }

            self.inner.write_str(line)?;

            if lines.peek().is_some() {
                self.inner.write_char('\n')?;
            }

            indent = true;
        }

        if s.ends_with('\n') {
            self.inner.write_char('\n')?;
            self.indent = true;
        } else {
            self.indent = false;
        }

        Ok(())
    }
}

impl Deref for IndentedWriter {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unwrap::unwrap;

    #[test]
    fn smoke() {
        let mut output = IndentedWriter::new(4);

        unwrap!(writeln!(output, "pub struct Foo {{"));
        output.indent();
        unwrap!(writeln!(output, "bar: Bar,"));
        unwrap!(write!(output, "baz"));
        unwrap!(writeln!(output, ": Baz,"));
        output.unindent();
        unwrap!(writeln!(output, "}}"));

        let expected = "pub struct Foo {\n    bar: Bar,\n    baz: Baz,\n}\n";
        assert_eq!(&*output, expected);
    }

    #[test]
    fn does_not_indent_empty_lines() {
        let mut output = IndentedWriter::new(4);
        output.indent();

        unwrap!(write!(output, "foo\n\n\nbar"));

        let expected = "    foo\n\n\n    bar";
        assert_eq!(&*output, expected);
    }
}
