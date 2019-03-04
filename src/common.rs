//! Functions common for all target languages.

use std::collections::hash_map::{Entry, HashMap};
use syntax::ast;
use syntax::print::pprust;
use Error;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum FilterMode {
    Blacklist,
    Whitelist,
}

/// Outputs several files as a result of an AST transformation.
pub type Outputs = HashMap<String, String>;

/// Target language support
pub trait Lang {
    /// Convert a Rust constant (`pub const NAME: Type = value;`) into a target
    /// language constant.
    fn parse_const(
        &mut self,
        _item: &syn::ItemConst,
        _module: &[String],
        _outputs: &mut Outputs,
    ) -> Result<(), Error> {
        Ok(())
    }

    /// Convert `pub type A = B;` into `typedef B A;`.
    fn parse_ty(
        &mut self,
        _item: &syn::ItemType,
        _module: &[String],
        _outputs: &mut Outputs,
    ) -> Result<(), Error> {
        Ok(())
    }

    /// Convert a Rust enum into a target language enum.
    fn parse_enum(
        &mut self,
        _item: &syn::ItemEnum,
        _module: &[String],
        _outputs: &mut Outputs,
    ) -> Result<(), Error> {
        Ok(())
    }

    /// Convert a Rust struct into a target language struct.
    fn parse_struct(
        &mut self,
        _item: &syn::ItemStruct,
        _module: &[String],
        _outputs: &mut Outputs,
    ) -> Result<(), Error> {
        Ok(())
    }

    /// Convert a Rust function declaration into a target language function declaration.
    fn parse_fn(
        &mut self,
        _item: &syn::ItemFn,
        _module: &[String],
        _outputs: &mut Outputs,
    ) -> Result<(), Error> {
        Ok(())
    }

    /// Add extra and custom code after the code generation part is done.
    fn finalise_output(&mut self, _outputs: &mut Outputs) -> Result<(), Error> {
        Ok(())
    }
}

/// Append or create new output file
pub fn append_output(text: String, file: &str, o: &mut Outputs) {
    match o.entry(file.to_string()) {
        Entry::Occupied(o) => o.into_mut().push_str(&text),
        Entry::Vacant(v) => {
            let _ = v.insert(text);
        }
    }
}

/// Check the attribute is `#[no_mangle]`.
pub fn check_no_mangle(attr: &syn::Attribute) -> bool {
    if attr.tts.to_string() == "no_mangle" {
        return true;
    } else {
        return false;
    }
}

/// Check the function argument is `user_data: *mut c_void`
pub fn is_user_data_arg(arg: &ast::Arg) -> bool {
    pprust::pat_to_string(&*arg.pat) == "user_data"
        && pprust::ty_to_string(&*arg.ty) == "*mut c_void"
}

/// Check the function argument is `result: *const FfiResult`
pub fn is_result_arg(arg: &ast::Arg) -> bool {
    pprust::pat_to_string(&*arg.pat) == "result"
        && pprust::ty_to_string(&*arg.ty) == "*const FfiResult"
}

/// Check the function argument is a length argument for a *const u8 pointer
pub fn is_ptr_len_arg(ty: &ast::Ty, arg_name: &str) -> bool {
    pprust::ty_to_string(ty) == "usize"
        && (arg_name.ends_with("_len") || arg_name == "len" || arg_name == "size")
}

/// Detect array ptrs and skip the length args - e.g. for a case of
/// `ptr: *const u8, ptr_len: usize` we're going to skip the `len` part.
pub fn is_array_arg(arg: &ast::Arg, next_arg: Option<&ast::Arg>) -> bool {
    if let ast::TyKind::Ptr(..) = arg.ty.node {
        !is_result_arg(arg)
            && next_arg
                .map(|arg| is_ptr_len_arg(&*arg.ty, &pprust::pat_to_string(&*arg.pat)))
                .unwrap_or(false)
    } else {
        false
    }
}

// TODO: Maybe it would be wise to use syntax::attr here.
/// Loop through a list of attributes.
///
/// Check that at least one attribute matches some criteria (usually `#[repr(C)]` or `#[no_mangle]`)
/// and optionally retrieve a String from it (usually a docstring).
pub fn parse_attr<C, R>(attrs: &[syn::Attribute], check: C, retrieve: R) -> (bool, String)
where
    C: Fn(&syn::Attribute) -> bool,
    R: Fn(&syn::Attribute) -> Option<String>,
{
    let mut check_passed = false;
    let mut retrieved_str = String::new();
    for attr in attrs {
        // Don't want to accidently set it to false after it's been set to true.
        if !check_passed {
            check_passed = check(attr);
        }
        // If this attribute has any strings to retrieve, retrieve them.
        if let Some(string) = retrieve(attr) {
            retrieved_str.push_str(&string);
        }
    }

    (check_passed, retrieved_str)
}

/// Check the attribute is #[repr(C)].
pub fn check_repr_c(attr: &syn::Attribute) -> bool {
    match attr.parse_meta() {
        syn::Meta::List(ref word) if attr.path == "repr" => {
            match word.nested.first() {
                Some(word) => {
                    match word.into_value() {
                        // Return true only if attribute is #[repr(C)].
                        syn::NestedMeta::Meta(ref item) if item.name() == "C" => true,
                        _ => false,
                    }
                }
                _ => false,
            }
        }
        _ => false,
    }
}

/// If the attribute is  a docstring, indent it the required amount and return it.
pub fn retrieve_docstring(attr: &syn::Attribute, prepend: &str) -> Option<String> {
    match attr.parse_meta() {
        syn::Meta::NameValue(ref val) if attr.path == "doc" => {
            match val.lit {
                // Docstring attributes omit the trailing newline.
                syn::Lit::Str(ref docs) => Some(format!("{}{}\n", prepend, docs)),
                _ => unreachable!("docs must be literal strings"),
            }
        }
        _ => None,
    }
}

/// Returns whether the calling convention of the function is compatible with
/// C (i.e. `extern "C"`).
pub fn is_extern(abi: syn::Abi) -> bool {
    match *abi.name.unwrap().value().as_str() {
        // If it doesn't have a C ABI it can't be called from C.
        &"C" | &"Cdecl" | &"Stdcall" | &"Fastcall" | &"System" => true,
        _ => false,
    }
}
