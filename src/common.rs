//! Functions common for all target languages.

use crate::Error;
use std::collections::hash_map::{Entry, HashMap};
use syn::export::ToTokens;
use unwrap::unwrap;

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
        item: &syn::ItemConst,
        module: &[String],
        outputs: &mut Outputs,
    ) -> Result<(), Error>;

    /// Convert `pub type A = B;` into a target language type definition.
    fn parse_ty(
        &mut self,
        item: &syn::ItemType,
        module: &[String],
        outputs: &mut Outputs,
    ) -> Result<(), Error>;

    /// Convert a Rust enum into a target language enum.
    fn parse_enum(
        &mut self,
        item: &syn::ItemEnum,
        module: &[String],
        outputs: &mut Outputs,
    ) -> Result<(), Error>;

    /// Convert a Rust struct into a target language struct.
    fn parse_struct(
        &mut self,
        item: &syn::ItemStruct,
        module: &[String],
        outputs: &mut Outputs,
    ) -> Result<(), Error>;

    /// Convert a Rust function declaration into a target language function declaration.
    fn parse_fn(
        &mut self,
        item: &syn::ItemFn,
        module: &[String],
        outputs: &mut Outputs,
    ) -> Result<(), Error>;

    /// Add extra and custom code after the code generation part is done.
    fn finalise_output(&mut self, _outputs: &mut Outputs) -> Result<(), Error>;
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
    attr.path.clone().into_token_stream().to_string() == "no_mangle"
}

pub fn transform_fnarg_to_argcap(fnarg: &syn::FnArg) -> Option<&syn::ArgCaptured> {
    if let syn::FnArg::Captured(ref argcap) = fnarg {
        Some(argcap)
    } else {
        None
    }
}

pub fn transform_fnarg_to_argcap_option(fnarg: Option<&syn::FnArg>) -> Option<&syn::ArgCaptured> {
    if let Some(fnarg) = fnarg {
        if let syn::FnArg::Captured(ref argcap) = fnarg {
            Some(argcap)
        } else {
            None
        }
    } else {
        None
    }
}

pub fn take_out_pat(argcappat: &syn::Pat) -> Option<&syn::PatIdent> {
    if let syn::Pat::Ident(ref pat) = argcappat {
        Some(pat)
    } else {
        None
    }
}

pub fn take_out_ident_from_type(typ: &syn::Type) -> Option<String> {
    match typ {
        syn::Type::Ptr(ref ptr) => {
            let idt = take_out_ident_from_type(&*ptr.elem).unwrap();
            Some(idt)
        }
        syn::Type::Path(path) => Some(path.to_owned().path.into_token_stream().to_string()),
        _ => None,
    }
}

/// Check the function argument is `user_data: *mut c_void`
pub fn is_user_data_arg(arg: &syn::ArgCaptured) -> bool {
    match arg.pat {
        syn::Pat::Ident(ref pat) if pat.ident == "user_data" => {}
        _ => return false,
    }
    matches!(arg.ty, syn::Type::Ptr(ref pat) if pat.into_token_stream().to_string() == "* mut c_void")
}

pub fn is_user_data_arg_barefn(arg: &syn::BareFnArg) -> bool {
    if unwrap!(arg.to_owned().name)
        .0
        .into_token_stream()
        .to_string()
        != "user_data"
    {
        return false;
    }
    matches!(arg.ty, syn::Type::Ptr(ref pat) if pat.into_token_stream().to_string() == "* mut c_void")
}

/// Check the function argument is `result: *const FfiResult`
pub fn is_result_arg(arg: &syn::ArgCaptured) -> bool {
    match arg.pat {
        syn::Pat::Ident(ref pat) if pat.ident == "result" => (),
        _ => return false,
    }
    match arg.ty {
        syn::Type::Ptr(ref ptr) if ptr.into_token_stream().to_string() == "*const FfiResult" => {
            true
        }
        _ => false,
    }
}

pub fn is_result_arg_barefn(arg: &syn::BareFnArg) -> bool {
    let arg_name = if let Some((syn::BareFnArgName::Named(ref arg_name), _)) = arg.name {
        arg_name.to_string()
    } else {
        return false;
    };
    arg_name == "result" && arg.ty.clone().into_token_stream().to_string() == "* const FfiResult"
}

/// Check the function argument is a length argument for a *const u8 pointer
pub fn is_ptr_len_arg(ty: &syn::Type, arg_name: &str) -> bool {
    &*ty.to_owned().into_token_stream().to_string().as_str() == "usize"
        && (arg_name.ends_with("_len") || arg_name == "len" || arg_name == "size")
}

/// Detect array ptrs and skip the length args - e.g. for a case of
/// `ptr: *const u8, ptr_len: usize` we're going to skip the `len` part.
pub fn is_array_arg(arg: &syn::ArgCaptured, next_arg: Option<&syn::ArgCaptured>) -> bool {
    if let syn::Type::Ptr(ref _typeptr) = arg.ty {
        !is_result_arg(&arg)
            && next_arg
                .map(|arg| {
                    is_ptr_len_arg(
                        &arg.ty,
                        &unwrap!(take_out_pat(&arg.pat))
                            .ident
                            .clone()
                            .into_token_stream()
                            .to_string(),
                    )
                })
                .unwrap_or(false)
    } else {
        false
    }
}

pub fn is_array_arg_barefn(arg: &syn::BareFnArg, next_arg: Option<&syn::BareFnArg>) -> bool {
    if let syn::Type::Ptr(ref _typeptr) = arg.ty {
        !is_result_arg_barefn(&arg)
            && next_arg
                .map(|arg| {
                    is_ptr_len_arg(
                        &arg.ty,
                        &arg.clone()
                            .name
                            .map(|(name, _)| name.into_token_stream().to_string())
                            .unwrap_or_else(|| "".to_string()),
                    )
                })
                .unwrap_or(false)
    } else {
        false
    }
}

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
    match unwrap!(attr.parse_meta()) {
        syn::Meta::List(ref word)
            if attr
                .to_owned()
                .path
                .into_token_stream()
                .to_string()
                .as_str()
                == "repr" =>
        {
            match word.nested.first() {
                Some(word) => {
                    matches!(word.into_value(), syn::NestedMeta::Meta(ref item) if item.name() == "C")
                }
                _ => false,
            }
        }
        _ => false,
    }
}

/// If the attribute is a docstring, indent it the required amount and return it.
pub fn retrieve_docstring(attr: &syn::Attribute, prepend: &str) -> Option<String> {
    match unwrap!(attr.parse_meta()) {
        syn::Meta::NameValue(ref val)
            if attr
                .to_owned()
                .path
                .into_token_stream()
                .to_string()
                .as_str()
                == "doc" =>
        {
            match val.lit {
                // Docstring attributes omit the trailing newline.
                syn::Lit::Str(ref docs) => Some(format!("///{}{}", prepend, docs.value().as_str())),
                _ => unreachable!("docs must be literal strings"),
            }
        }
        _ => None,
    }
}

/// Returns whether the calling convention of the function is compatible with C (i.e. `extern "C"`).
pub fn is_extern(abi: syn::Abi) -> bool {
    matches!(
        unwrap!(abi.name).value().as_str(),
        "C" | "Cdecl" | "Stdcall" | "Fastcall" | "System"
    )
}

/// Extracts the int literal from the expression, if it exists.
pub fn extract_int_literal(lit: &syn::ExprLit) -> Option<i64> {
    if let syn::Lit::Int(val) = &lit.lit {
        Some(val.value() as i64)
    } else {
        None
    }
}

/// Extracts the enum variant value/discriminant, if it exists.
pub fn extract_enum_variant_value(variant: &syn::Variant) -> Option<i64> {
    if let Some(ref expr) = variant.discriminant {
        if let syn::Expr::Lit(ref lit) = expr.1 {
            return extract_int_literal(lit);
        }
    }

    None
}
