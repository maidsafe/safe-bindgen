use crate::common::{Lang, Outputs};
use crate::{parse, Error};
use colored::*;
use std::collections::HashMap;
use std::fmt::Write;
use unwrap::unwrap;

macro_rules! compile {
    ($lang:expr, $rust:tt) => {
        unwrap!(try_compile!($lang, $rust))
    };
}

macro_rules! try_compile {
    ($lang:expr, $rust:tt) => {{
        let rust_src = stringify!($rust);
        let rust_src = rust_src[1..rust_src.len() - 1].to_string();
        use crate::test_utils;
        test_utils::try_compile($lang, rust_src)
    }};
}

// This is like `assert_eq`, but produces more readable output for multiline
// strings.
macro_rules! assert_multiline_eq {
    ($left:expr, $right:expr) => {{
        use crate::test_utils::format_diff;
        use colored::*;

        let left = $left;
        let right = $right;

        if left != right {
            panic!(
                "assertion failed: `({} == {})`\n```\n{}```\n",
                "left".red(),
                "right".green(),
                format_diff(&left, &right)
            );
        }
    }};
}

/// Convert source string into an AST type
pub fn ty(source: &str) -> syn::Type {
    let item: Result<syn::Type, _> = syn::parse_str(source);
    unwrap!(item)
}

pub fn try_compile(
    mut lang: impl Lang,
    rust_src: String,
) -> Result<HashMap<String, String>, Vec<Error>> {
    let ast: syn::File = unwrap!(syn::parse_str(&rust_src));
    let mut outputs = Outputs::default();
    parse::parse_file(&mut lang, &ast, &[Default::default()], &mut outputs)?;
    lang.finalise_output(&mut outputs)?;

    Ok(outputs)
}

pub fn format_diff(left: &str, right: &str) -> String {
    let mut output = String::new();

    for res in diff::lines(left, right) {
        match res {
            diff::Result::Left(line) => unwrap!(writeln!(output, "{}{}", "-".red(), line.red())),
            diff::Result::Right(line) => {
                unwrap!(writeln!(output, "{}{}", "+".green(), line.green()))
            }
            diff::Result::Both(line, _) => unwrap!(writeln!(output, " {}", line.white())),
        };
    }

    output
}

pub fn fetch<'a>(outputs: &'a HashMap<String, String>, name: &str) -> &'a str {
    outputs.get(name).map(String::as_str).unwrap_or("")
}
