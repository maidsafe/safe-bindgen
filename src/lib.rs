//! `safe_bindgen` is a library based on moz-cheddar for converting Rust source files
//! into Java and C# bindings.
//!
//! It is built specifically for the SAFE Client Libs project.

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/maidsafe/QA/master/Images/maidsafe_logo.png",
    html_favicon_url = "http://maidsafe.net/img/favicon.ico",
    test(attr(forbid(warnings)))
)]
// For explanation of lint checks, run `rustc -W help` or see
// https://github.com/maidsafe/QA/blob/master/Documentation/Rust%20Lint%20Checks.md
#![forbid(
    exceeding_bitshifts,
    mutable_transmutes,
    no_mangle_const_items,
    unknown_crate_types
)]
#![deny(
    deprecated,
    improper_ctypes,
    non_shorthand_field_patterns,
    overflowing_literals,
    plugin_as_library,
    stable_features,
    unconditional_recursion,
    unknown_lints,
    unsafe_code,
    unused_allocation,
    unused_attributes,
    unused_comparisons,
    unused_features,
    unused_parens,
    while_true
)]
#![warn(
    trivial_casts,
    trivial_numeric_casts,
    unused_extern_crates,
    unused_import_braces,
    unused_qualifications
)]
#![allow(
    box_pointers,
    missing_copy_implementations,
    missing_debug_implementations,
    variant_size_differences
)]
// FIXME: add documentation and deny `missing_documentation`
#![allow(missing_docs)]
#![cfg_attr(not(feature = "with-syntex"), feature(rustc_private))]
#![recursion_limit = "128"]

#[cfg(not(feature = "with-syntex"))]
extern crate rustc_errors as errors;
#[cfg(not(feature = "with-syntex"))]
extern crate syntax;
#[cfg(feature = "with-syntex")]
extern crate syntex_errors as errors;
#[cfg(feature = "with-syntex")]
extern crate syntex_syntax as syntax;
extern crate toml;
//#[macro_use]
extern crate quote;
//extern crate jni;
extern crate petgraph;
extern crate rustfmt;

extern crate syn;

#[cfg(test)]
extern crate colored;
#[cfg(test)]
extern crate diff;
#[cfg(test)]
#[macro_use]
extern crate indoc;
#[macro_use]
extern crate unwrap;

pub use common::FilterMode;
use common::{Lang, Outputs};
pub use csharp::LangCSharp;
pub use errors::Level;
//pub use java::LangJava;
pub use lang_c::LangC;
use std::collections::HashMap;
use std::fmt::Display;
use std::fs;
use std::fs::File;
use std::io::Error as IoError;
use std::io::{Read, Write};
use std::path::{self, Component, Path, PathBuf};
//use syn::export::Span as synspan;
use syntax::codemap::{FilePathMapping, Span};

#[cfg(test)]
#[macro_use]
mod test_utils;
mod common;
mod csharp;
//mod java;
mod lang_c;
mod output;
mod parse;
//mod struct_field;

/// Describes an error encountered by the compiler.
///
/// These can be printed nicely using the `Cheddar::print_err` method.
#[derive(Debug)]
pub struct Error {
    pub level: Level,
    span: Option<Span>,
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
}

impl Display for Error {
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
            _ => unreachable!(),
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

impl Error {
    /// Use a ParseSess to print the error in the correct format.
    #[allow(unused_must_use)]
    fn print(&self, sess: &syntax::parse::ParseSess) {
        // TODO: there must be some way to reduce the amount of code here.
        // Throw away the results (with { ...; }) since they are handled elsewhere.
        if let Some(span) = self.span {
            match self.level {
                Level::Bug => {
                    sess.span_diagnostic.span_bug(span, &self.message);
                }
                Level::Fatal => {
                    sess.span_diagnostic.span_fatal(span, &self.message);
                }
                Level::Error => {
                    sess.span_diagnostic.span_err(span, &self.message);
                }
                Level::Warning => {
                    sess.span_diagnostic.span_warn(span, &self.message);
                }
                Level::Note => {
                    sess.span_diagnostic
                        .span_note_without_error(span, &self.message);
                }
                Level::Help => {
                    sess.span_diagnostic
                        .struct_dummy()
                        .span_help(span, &self.message);
                }
                _ => unreachable!(),
            };
        } else {
            match self.level {
                Level::Bug => {
                    sess.span_diagnostic.bug(&self.message);
                }
                Level::Fatal => {
                    sess.span_diagnostic.fatal(&self.message);
                }
                Level::Error => {
                    sess.span_diagnostic.err(&self.message);
                }
                Level::Warning => {
                    sess.span_diagnostic.warn(&self.message);
                }
                Level::Note => {
                    sess.span_diagnostic.note_without_error(&self.message);
                }
                Level::Help => {
                    sess.span_diagnostic.struct_dummy().help(&self.message);
                }
                _ => unreachable!(),
            };
        }
    }
}

enum Input {
    File(PathBuf),
    Code { file_name: String, code: String },
}

/// Stores configuration for the bindgen.
///
/// # Examples
///
/// Since construction can only fail if there is an error _while_ reading the cargo manifest it is
/// usually safe to call `.unwrap()` on the result (though `.expect()` is considered better
/// practice).
///
/// ```ignore
/// Bindgen::new().expect("unable to read cargo manifest");
/// ```
///
/// If your project is a valid cargo project or follows the same structure, you can simply place
/// the following in your build script.
///
/// ```ignore
/// Bindgen::new().expect("unable to read cargo manifest")
///     .run_build("path/to/output/file");
/// ```
///
/// If you use a different structure you should use `.source_file("...")` to set the path to the
/// root crate file.
///
/// ```ignore
/// Bindgen::new().expect("unable to read cargo manifest")
///     .source_file("src/root.rs")
///     .run_build("include/my_header.h");
/// ```
pub struct Bindgen {
    /// The root source file of the crate.
    input: Input,
    /// The current parser session.
    ///
    /// Used for printing errors.
    session: syntax::parse::ParseSess,
}

impl Bindgen {
    /// Create a new bindgen instance.
    ///
    /// This can only fail if there are issues reading the cargo manifest. If there is no cargo
    /// manifest available then the source file defaults to `src/lib.rs`.
    pub fn new() -> Result<Self, Error> {
        let source_path = source_file_from_cargo()?;
        let input = Input::File(PathBuf::from(source_path));

        Ok(Bindgen {
            input,
            session: syntax::parse::ParseSess::new(FilePathMapping::empty()),
        })
    }

    /// Set the path to the root source file of the crate.
    ///
    /// This should only be used when not using a `cargo` build system.
    pub fn source_file<T>(&mut self, path: T) -> &mut Self
    where
        PathBuf: From<T>,
    {
        self.input = Input::File(PathBuf::from(path));
        self
    }

    /// Use custom code as input.
    pub fn source_code<S>(&mut self, file_name: S, code: S) -> &mut Self
    where
        S: Into<String>,
    {
        self.input = Input::Code {
            file_name: file_name.into(),
            code: code.into(),
        };
        self
    }

    /// Compile just the code into header declarations.
    ///
    /// This does not add any include-guards, includes, or extern declarations. It is mainly
    /// intended for internal use, but may be of interest to people who wish to embed
    /// moz-cheddar's generated code in another file.
    pub fn compile<L: Lang>(
        &mut self,
        lang: &mut L,
        outputs: &mut Outputs,
        finalise: bool,
    ) -> Result<(), Vec<Error>> {
        match &self.input {
            Input::Code { file_name, code } => {
                self.compile_from_source(lang, outputs, file_name.clone(), code.clone())?;
            }
            Input::File(path) => {
                self.compile_from_path(lang, outputs, path)?;
            }
        }
        if finalise {
            lang.finalise_output(outputs)?;
        }
        Ok(())
    }

    fn compile_from_path<L: Lang>(
        &self,
        lang: &mut L,
        outputs: &mut Outputs,
        path: &PathBuf,
    ) -> Result<(), Vec<Error>> {
        let base_path = unwrap!(path.parent());
        let mod_path = unwrap!(path.to_str()).to_string();

        // Parse the top level mod.
        // Creates AST for the entire file
        let mut file = unwrap!(File::open(path));
        let mut content = String::new();
        unwrap!(file.read_to_string(&mut content));
        let ast = unwrap!(syn::parse_file(&content));
        let mut imported: Vec<Vec<String>> = Vec::new();
        for item in ast.items {
            match &item {
                syn::Item::Use(ref itemuse) => {
                    if parse::imported_mods(itemuse).is_some() {
                        imported.push(parse::imported_mods(itemuse).unwrap());
                    }
                }
                _ => {}
            }
        }
        println!("{:?}", imported);
        for module in imported {
            let mut mod_path = base_path.join(&format!(
                "{}.rs",
                module.join(&path::MAIN_SEPARATOR.to_string())
            ));

            if !mod_path.exists() {
                mod_path = base_path.join(&format!(
                    "{}/mod.rs",
                    module.join(&path::MAIN_SEPARATOR.to_string())
                ));
            }

            println!("Parsing {} ({:?})", module.join("::"), mod_path);

            let mut file = unwrap!(File::open(mod_path));
            let mut content = String::new();
            unwrap!(file.read_to_string(&mut content));
            let ast = unwrap!(syn::parse_file(&content));
            parse::parse_file(lang, &ast, &module, outputs)?;
        }
        Ok(())
    }

    fn compile_from_source<L: Lang>(
        &self,
        lang: &mut L,
        outputs: &mut Outputs,
        file_name: String,
        source: String,
    ) -> Result<(), Vec<Error>> {
        let module = convert_lib_path_to_module(&PathBuf::from(file_name.clone()));

        let _ast: syn::File = unwrap!(syn::parse_str(&source));

        for item in _ast.items {
            match &item {
                syn::Item::Mod(ref item) => {
                    parse::parse_mod(lang, item, &module[..], outputs)?;
                }
                syn::Item::Const(ref item) => {
                    lang.parse_const(item, &module[..], outputs)?;
                }
                syn::Item::Type(ref item) => {
                    lang.parse_ty(item, &module[..], outputs)?;
                }
                syn::Item::Enum(ref item) => {
                    lang.parse_enum(item, &module[..], outputs)?;
                }
                syn::Item::Fn(ref item) => {
                    lang.parse_fn(item, &module[..], outputs)?;
                }
                syn::Item::Struct(ref item) => {
                    lang.parse_struct(item, &module[..], outputs)?;
                }
                _ => {}
            }
        }

        Ok(())
    }

    pub fn compile_or_panic<L: Lang>(
        &mut self,
        lang: &mut L,
        outputs: &mut Outputs,
        finalise: bool,
    ) {
        if let Err(errors) = self.compile(lang, outputs, finalise) {
            for error in &errors {
                self.print_error(error);
            }

            panic!("Failed to compile.");
        }
    }

    /// Writes virtual files to the file system
    pub fn write_outputs<P: AsRef<Path>>(&self, root: P, outputs: &Outputs) -> Result<(), IoError> {
        let root = root.as_ref();

        for (path, contents) in outputs {
            let full_path = root.join(PathBuf::from(path));

            if let Some(parent_dirs) = full_path.parent() {
                fs::create_dir_all(parent_dirs)?;
            }

            let mut f = fs::File::create(full_path)?;
            f.write_all(contents.as_bytes())?;
            f.sync_all()?;
        }

        Ok(())
    }

    pub fn write_outputs_or_panic<P: AsRef<Path>>(&self, root: P, outputs: &Outputs) {
        if let Err(err) = self.write_outputs(root, outputs) {
            self.print_error(&From::from(err));
            panic!("Failed to write output.");
        }
    }

    /// Write the header to a file, panicking on error.
    ///
    /// This is a convenience method for use in build scripts. If errors occur during compilation
    /// they will be printed then the function will panic.
    ///
    /// # Panics
    ///
    /// Panics on any compilation error so that the build script exits and prints output.
    pub fn run_build<P: AsRef<Path>, L: Lang>(&mut self, lang: &mut L, output_dir: P) {
        let mut outputs = HashMap::new();
        self.compile_or_panic(lang, &mut outputs, true);

        self.write_outputs_or_panic(output_dir, &outputs);
    }

    /// Print an error using the ParseSess stored in Cheddar.
    pub fn print_error(&self, error: &Error) {
        error.print(&self.session);
    }
}

/// Convert a path into a top-level module name (e.g. `ffi_utils/src/lib.rs` -> `ffi_libs`)
fn convert_lib_path_to_module<P: AsRef<Path>>(path: &P) -> Vec<String> {
    let mut res = Vec::new();
    let path = path.as_ref();

    for component in path.components() {
        if let Component::Normal(path) = component {
            let path = unwrap!(path.to_str());
            res.push(path.to_string());
        }
    }

    // Cut off the "src/lib.rs" part
    if res[(res.len() - 2)..] == ["src", "lib.rs"] {
        res = res[..(res.len() - 2)].to_vec();
    }

    res
}

/// Extract the path to the root source file from a `Cargo.toml`.
fn source_file_from_cargo() -> Result<String, Error> {
    let cargo_toml = path::Path::new(
        &std::env::var_os("CARGO_MANIFEST_DIR").unwrap_or_else(|| std::ffi::OsString::from("")),
    )
    .join("Cargo.toml");

    // If no `Cargo.toml` assume `src/lib.rs` until told otherwise.
    let default = "src/lib.rs";
    let mut cargo_toml = match std::fs::File::open(&cargo_toml) {
        Ok(value) => value,
        Err(..) => return Ok(default.to_owned()),
    };

    let mut buf = String::new();
    match cargo_toml.read_to_string(&mut buf) {
        Ok(..) => {}
        Err(..) => {
            return Err(Error {
                level: Level::Fatal,
                span: None,
                message: "could not read cargo manifest".into(),
            });
        }
    };

    let table = match (&buf).parse::<toml::Value>() {
        Ok(value) => value,
        Err(..) => {
            return Err(Error {
                level: Level::Fatal,
                span: None,
                message: "could not parse cargo manifest".into(),
            });
        }
    };

    // If not explicitly stated then defaults to `src/lib.rs`.
    Ok(table
        .get("lib")
        .and_then(|t| t.get("path"))
        .and_then(|s| s.as_str())
        .unwrap_or(default)
        .into())
}
