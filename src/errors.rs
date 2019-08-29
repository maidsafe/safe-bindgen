use std::fmt;
use std::io::Error as IoError;
use syn::export::Span;

/// Describes an error encountered by the compiler.
///
/// These can be printed nicely using the `Bindgen::print_err` method.
#[derive(Debug)]
pub struct Error {
    pub level: Level,
    pub(crate) span: Option<Span>,
    pub message: String,
}

impl Error {
    pub fn error(message: &str) -> Self {
        Error {
            level: Level::Error,
            span: None,
            message: message.to_string(),
        }
    }

    pub(crate) fn print(&self) {
        // TODO: improve error output, add spans where needed
        println!("{:?}, {}: {}", self.span, self.level, self.message);
    }

    /// Constructs an error for unsupported generics.
    pub fn unsupported_generics_error(name: &str) -> Self {
        Self {
            level: Level::Error,
            span: None, //NONE FOR NOW
            message: format!("bindgen can not handle parameterized {}", name),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.level, self.message)
    }
}

impl std::error::Error for Error {
    fn description(&self) -> &str {
        match self.level {
            Level::Bug => "internal error",
            Level::Fatal | Level::Error => "error",
            Level::Warning => "warning",
            Level::Note => "note",
            Level::Help => "help",
        }
    }
}

impl From<IoError> for Error {
    fn from(e: IoError) -> Self {
        Error {
            level: Level::Fatal,
            span: None,
            message: format!("I/O Error: {}", e),
        }
    }
}

impl From<Error> for Vec<Error> {
    fn from(e: Error) -> Self {
        vec![e]
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Level {
    Bug,
    Fatal,
    Error,
    Warning,
    Note,
    Help,
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Level::Bug => write!(f, "bug"),
            Level::Fatal => write!(f, "fatal"),
            Level::Error => write!(f, "error"),
            Level::Warning => write!(f, "warning"),
            Level::Note => write!(f, "note"),
            Level::Help => write!(f, "help"),
        }
    }
}
