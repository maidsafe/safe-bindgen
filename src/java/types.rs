//! Functions for converting Rust types to Java types.

use common::{
    is_array_arg, is_array_arg_barefn, is_result_arg, is_result_arg_barefn, is_user_data_arg,
    is_user_data_arg_barefn, transform_fnarg_to_argcap,
};
use core::borrow::Borrow;
use java::Context;
use jni::signature::{JavaType, Primitive};
use std::ops::Deref;
use syn::export::ToTokens;
use syn::punctuated::Iter;
use syntax::abi::Abi;
use syntax::print::pprust;
use syntax::{ast, codemap};
use {Error, Level};

fn primitive_type_to_str(ty: Primitive) -> &'static str {
    match ty {
        Primitive::Boolean => "boolean",
        Primitive::Byte => "byte",
        Primitive::Char => "char",
        Primitive::Double => "double",
        Primitive::Float => "float",
        Primitive::Int => "int",
        Primitive::Long => "long",
        Primitive::Short => "short",
        Primitive::Void => "void",
    }
}

/// Converts `JavaType` into Java code
pub fn java_type_to_str(ty: &JavaType) -> Result<String, Error> {
    match *ty {
        JavaType::Primitive(primitive) => Ok(primitive_type_to_str(primitive).to_string()),
        JavaType::Object(ref obj) => match obj.as_str() {
            "java/lang/String" => Ok("String".into()),
            _ => Ok(obj.to_string()),
        },
        JavaType::Array(ref boxed) => Ok(format!("{}[]", java_type_to_str(&*boxed)?)),
        JavaType::Method(..) => Err(Error {
            level: Level::Error,
            span: None,
            message: "Java methods are not supported".into(),
        }),
    }
}

/// Transform a struct name into a Java class name
pub fn struct_to_java_classname<S: AsRef<str>>(s: S) -> String {
    // s.as_ref().to_class_case()
    let mut c = s.as_ref().chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

/// Get the Java interface name for the callback based on its types
pub fn callback_name(inputs: &[syn::BareFnArg], context: &Context) -> Result<String, Error> {
    let mut components = Vec::new();
    let mut inputs = inputs.iter().peekable();

    while let Some(&ref arg) = inputs.next() {
        if is_user_data_arg_barefn(&arg.clone()) {
            // Skip user_data args
            continue;
        }
        if is_result_arg_barefn(&arg) {
            // Make sure that a CB taking a single "result: *const FfiResult" param
            // won't end up being called "CallbackVoid" (but "CallbackResult" instead)
            components.push(From::from("Result"));
            continue;
        }

        let arg_type = &rust_ty_to_java_class_name(&arg.ty, context)?;
        let mut arg_type = struct_to_java_classname(arg_type);

        if is_array_arg_barefn(&arg, inputs.peek().cloned()) {
            &inputs.next();
            arg_type.push_str("ArrayLen");
        }

        components.push(arg_type);
    }

    if components.is_empty() {
        Ok(From::from("CallbackVoid"))
    } else {
        Ok(format!("Callback{}", components.join("")))
    }
}

/// Converts a callback function argument into a Java interface name
fn callback_arg_to_java(
    fn_ty: &syn::TypeBareFn,
    //fn_span: codemap::Span,
    context: &Context,
) -> Result<JavaType, Error> {
    match unwrap!(unwrap!(fn_ty.abi.to_owned()).name).value().as_str() {
        // If it doesn't have a C ABI it can't be called from C.
        "C" | "Cdecl" | "Stdcall" | "Fastcall" | "System" => {}
        _ => {
            return Err(Error {
                level: Level::Error,
                span: None, //NONE FOR NOW
                message: "callbacks that don't have C ABI are not supported".into(),
            });
        }
    }

    if fn_ty.lifetimes.is_some() {
        return Err(Error {
            level: Level::Error,
            span: None, //NONE FOR NOW
            message: "can not handle lifetimes".into(),
        });
    }
    let mut vec = vec![];
    for x in fn_ty.inputs.to_owned() {
        vec.push(x);
    }
    Ok(JavaType::Object(callback_name(vec.as_slice(), context)?))
}

/// Turn a Rust type with an associated name or type into a C type.
pub fn rust_to_java(ty: &syn::Type, context: &Context) -> Result<JavaType, Error> {
    match ty {
        // This is a callback ref taken as a function argument
        syn::Type::BareFn(ref bare_fn) => callback_arg_to_java(bare_fn, context),

        // All other types just have a name associated with them.
        _ => anon_rust_to_java(ty, context, true),
    }
}

/// Turn a Rust type into a part of the Java class name.
/// Handles the size types in a special way because Rust has to distinguish
/// between usize and u64, that's required for JNI bindings to work properly.
fn rust_ty_to_java_class_name(ty: &syn::Type, context: &Context) -> Result<String, Error> {
    match ty {
        syn::Type::Path(ref path) => {
            let primitive_type: String = path.path.segments[0].ident.to_string();
            if primitive_type.as_str() == "usize" || primitive_type == "isize" {
                Ok(From::from("size"))
            } else {
                java_type_to_str(&path_to_java(&path.path, context, false)?)
            }
        }
        _ => java_type_to_str(&anon_rust_to_java(ty, context, false)?),
    }
}

/// Turn a Rust type into a Java type signature.
fn anon_rust_to_java(
    ty: &syn::Type,
    context: &Context,
    use_type_map: bool,
) -> Result<JavaType, Error> {
    match ty {
        // Function pointers should not be in this function.
        syn::Type::BareFn(..) => Err(Error {
            level: Level::Error,
            span: None, //NONE FOR NOW
            message: "C function pointers must have a name or function declaration \
                      associated with them"
                .into(),
        }),

        // Standard pointers.
        syn::Type::Ptr(ref ptr) => {
            let ty_str = ptr
                .to_owned()
                .elem
                .deref()
                .to_owned()
                .into_token_stream()
                .to_string();
            // Detect strings, which are *const c_char or *mut c_char
            if ty_str.as_str() == "* const c_char" || ty_str.as_str() == "* mut c_char" {
                return Ok(JavaType::Object("String".into()));
            }
            anon_rust_to_java(&*ptr.elem, context, use_type_map)
        }

        // Plain old types.
        syn::Type::Path(ref path) => path_to_java(&path.path, context, use_type_map),

        // Possibly void, likely not.
        _ => {
            let new_type = ty.to_owned().into_token_stream().to_string();
            if new_type == "()" {
                Ok(JavaType::Primitive(Primitive::Void))
            } else {
                Err(Error {
                    level: Level::Error,
                    span: None, //NONE FOR NOW
                    message: format!("unknown type `{}`", new_type),
                })
            }
        }
    }
}

/// Convert a Rust path type (`my_mod::MyType`) to a Java type.
///
/// Types hidden behind modules are almost certainly custom types (which wouldn't work) except
/// types in `libc` which we special case.
fn path_to_java(
    path: &syn::Path,
    context: &Context,
    use_type_map: bool,
) -> Result<JavaType, Error> {
    if path.segments.is_empty() {
        return Err(Error {
            level: Level::Bug,
            span: None, //NONE FOR NOW
            message: "invalid type".into(),
        });
    }

    // Types in modules, `my_mod::MyType`.
    if path.segments.len() > 1 {
        let ty = unwrap!(path.segments.last()).into_value();
        //.expect("already checked that there were at least two elements");
        let ty: String = ty.ident.to_owned().to_string();
        let mut module = String::new();
        module.to_owned();
        for segment in path.segments.iter() {
            module.push_str(segment.ident.to_string().as_str());
        }
        match &*module.into_boxed_str() {
            "std::os::raw" | "libc" => {
                Ok(rust_ty_to_java(ty.as_str()).unwrap_or_else(|| JavaType::Object(ty)))
            }
            _ => Err(Error {
                level: Level::Error,
                span: None, //NONE FOR NOW
                message: "can't convert type".into(),
            }),
        }
    } else {
        let ty: String = path.segments[0].ident.to_owned().to_string();
        let mapped = rust_ty_to_java(ty.as_str()).unwrap_or_else(|| {
            if !use_type_map {
                // Unknown type - most likely it's a structure, so convert it into an object
                return JavaType::Object(struct_to_java_classname(ty));
            }
            if let Some(mapping) = context.type_map.get(ty.as_str()) {
                (*mapping).clone()
            } else {
                JavaType::Object(struct_to_java_classname(ty))
            }
        });
        Ok(mapped)
    }
}

/// Convert any string representation of a Rust type into Java.
///
/// This includes user-defined types. We currently trust the user not to use types which we don't
/// know the structure of (like String).
pub fn rust_ty_to_java(ty: &str) -> Option<JavaType> {
    match ty {
        "c_void" | "()" => Some(JavaType::Primitive(Primitive::Void)), // "void",
        "c_bool" | "bool" => Some(JavaType::Primitive(Primitive::Boolean)), // "boolean",
        "c_float" | "f32" => Some(JavaType::Primitive(Primitive::Float)), // "float",
        "c_double" | "f64" => Some(JavaType::Primitive(Primitive::Double)), // "double",
        "c_char" | "c_schar" | "c_uchar" | "u8" | "i8" => {
            Some(JavaType::Primitive(Primitive::Byte))
        } // "byte",
        "c_short" | "c_ushort" | "u16" | "i16" => Some(JavaType::Primitive(Primitive::Short)),
        // "short",
        "c_int" | "c_uint" | "u32" | "i32" => Some(JavaType::Primitive(Primitive::Int)), // "int",
        "c_long" | "c_ulong" | "u64" | "i64" | "usize" | "isize" => {
            Some(JavaType::Primitive(Primitive::Long))
        } // "long",
        _ => None, // unknown type
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jni::signature::{JavaType, Primitive};
    use syntax::ast::*;
    use syntax::codemap::DUMMY_SP;
    use syntax::ptr::P;

    #[test]
    fn test_rust_to_java() {
        let context = Context::default();

        assert_eq!(
            // Check `*const c_char` is correctly converted into `String`
            unwrap!(rust_to_java(
                &Ty {
                    id: DUMMY_NODE_ID,
                    span: DUMMY_SP,
                    node: TyKind::Ptr(MutTy {
                        mutbl: Mutability::Immutable,
                        ty: P(Ty {
                            id: DUMMY_NODE_ID,
                            span: DUMMY_SP,
                            node: TyKind::Path(
                                None,
                                Path::from_ident(DUMMY_SP, Ident::from_str("c_char")),
                            ),
                        }),
                    }),
                },
                &context,
            )),
            JavaType::Object("String".to_string())
        );
    }

    #[test]
    fn java_types_to_string() {
        assert_eq!(
            unwrap!(java_type_to_str(&JavaType::Object("String".to_string()))).as_str(),
            "String"
        );

        assert_eq!(
            unwrap!(java_type_to_str(&JavaType::Object(
                "net.maidsafe.Test".to_string()
            ),))
            .as_str(),
            "net.maidsafe.Test"
        );

        assert_eq!(
            unwrap!(java_type_to_str(&JavaType::Array(Box::new(
                JavaType::Primitive(Primitive::Byte)
            ),)))
            .as_str(),
            "byte[]"
        );
    }
}
