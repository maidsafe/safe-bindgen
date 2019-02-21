//! Functions for converting Rust types to C types.

#[cfg(test)]
mod tests;
mod types;

use self::types::{CPtrType, CType, CTypeNamed};
use common::{
    append_output, check_no_mangle, check_repr_c, parse_attr, retrieve_docstring, Lang, Outputs,
};
use petgraph::{algo, Graph};
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path;
use Error;
use Level;

pub struct LangC {
    lib_name: String,
    decls: BTreeMap<String, String>,
    deps: BTreeMap<String, Vec<String>>,
    custom_code: String,
}

/// Compile the header declarations then add the needed `#include`s.
///
/// Currently includes:
///
/// - `stdint.h`
/// - `stdbool.h`
impl LangC {
    pub fn new() -> Self {
        Self {
            lib_name: "backend".to_owned(),
            decls: BTreeMap::new(),
            deps: BTreeMap::new(),
            custom_code: Default::default(),
        }
    }

    /// Set the name of the native library.
    pub fn set_lib_name<T: Into<String>>(&mut self, name: T) {
        self.lib_name = name.into();
    }

    /// Adds manual C code into the top-level header - can be useful for typedefs,
    /// like e.g. opaque pointers.
    pub fn add_custom_code(&mut self, code: &str) {
        self.custom_code.push_str(code);
    }

    fn add_dependencies(&mut self, module: &[String], cty: &CType) -> Result<(), Error> {
        let deps = cty.dependencies();

        if !deps.is_empty() {
            let header = header_name(module, &self.lib_name)?;

            match self.deps.entry(header) {
                Entry::Occupied(o) => o.into_mut().extend(deps.into_iter()),
                Entry::Vacant(v) => {
                    let _ = v.insert(deps);
                }
            }
        }

        Ok(())
    }

    fn append_to_header(
        &mut self,
        buffer: String,
        module: &[String],
        outputs: &mut Outputs,
    ) -> Result<(), Error> {
        let header = header_name(module, &self.lib_name)?;
        append_output(buffer, &header, outputs);
        Ok(())
    }

    /// Transform a Rust FFI function into a C function decl
    pub fn transform_native_fn(
        &mut self,
        fn_decl: &syn::ItemFn, //
        docs: &str,
        name: &str,
        module: &[String],
        outputs: &mut Outputs,
    ) -> Result<(), Error> {
        // Handle the case when the return type is a function pointer (which requires that the
        // entire declaration is wrapped by the function pointer type) by first creating the name
        // and parameters, then passing that whole thing to `rust_to_c`.

        let mut args = Vec::new();

        // Arguments
        for arg in fn_decl.decl.inputs.iter() {
            let arg_name = arg.pat.ident.to_string();
            let c_ty = rust_to_c(&arg.ty, &arg_name)?;
            self.add_dependencies(module, &c_ty.1)?;
            args.push(c_ty);
        }

        let buf = format!(
            "{}({})",
            name,
            if args.is_empty() {
                String::from("void")
            } else {
                args.into_iter()
                    .map(|cty| format!("{}", cty))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        );

        // Generate return type
        let output_type = &fn_decl.decl.output;
        let full_declaration = match *output_type {
            syn::ReturnType::Type(_, ref ty) => {
                match ty.as_ref() {
                    syn::Type::Never(..) => {
                        return Err(Error {
                            level: Level::Error,
                            span: Some(ty.span), //there is no span for types in syn, hence None
                            message: "panics across a C boundary are naughty!".into(),
                        });
                    }
                    _ => {
                        let c_ty = rust_to_c(&*ty, &buf)?;
                        self.add_dependencies(module, &c_ty.1)?;
                        format!("{}", c_ty)
                    }
                }
            }
            syn::ReturnType::Default => format!("void {}", buf),
        };

        let mut output = String::new();
        output.push_str(docs);
        output.push_str(&full_declaration);
        output.push_str(";\n\n");

        append_output(output, &header_name(module, &self.lib_name)?, outputs);

        Ok(())
    }
}

impl Default for LangC {
    fn default() -> Self {
        Self::new()
    }
}

impl Lang for LangC {
    /// Convert `pub type A = B;` into `typedef B A;`.
    ///
    /// Aborts if A is generic.
    fn parse_ty(
        &mut self,
        item: &syn::ItemType,
        module: &[String],
        outputs: &mut Outputs,
    ) -> Result<(), Error> {
        let (_, docs) = parse_attr(
            &item.attrs[..],
            |_| true,
            |attr| retrieve_docstring(attr, ""),
        );

        let mut buffer = String::new();
        buffer.push_str(&docs);

        let name = item.ident.to_string();
        let new_type = match item {
            syn::ItemEnum => {
                // Can not yet convert generics.
                if item.generics.lt_token.is_some() {
                    return Ok(());
                }
                rust_to_c(&*item.ty.deref(), &name)?
            }
            _ => {
                return Err(Error {
                    level: Level::Bug,
                    span: Some(item.span),
                    message: "`parse_ty` called on wrong `Item_`".into(),
                });
            }
        };

        buffer.push_str(&format!("typedef {};\n\n", new_type));
        self.append_to_header(buffer, module, outputs)?;

        self.decls
            .insert(name.to_string(), header_name(module, &self.lib_name)?);

        Ok(())
    }

    /// Convert a Rust enum into a C enum.
    ///
    /// The Rust enum must be marked with `#[repr(C)]` and must be public otherwise the function
    /// will abort.
    ///
    /// Bindgen will error if the enum if generic or if it contains non-unit variants.
    fn parse_enum(
        &mut self,
        item: &syn::ItemEnum,
        module: &[String],
        outputs: &mut Outputs,
    ) -> Result<(), Error> {
        let (repr_c, docs) = parse_attr(&item.attrs[..], check_repr_c, |attr| {
            retrieve_docstring(attr, "")
        });
        // If it's not #[repr(C)] then it can't be called from C.
        if !repr_c {
            return Ok(());
        }

        let mut buffer = String::new();
        buffer.push_str(&docs);

        let name = item.ident.to_string();
        buffer.push_str(&format!("typedef enum {} {{\n", name));
        if syn::ItemEnum(..) = item {
            if item.generics.lt_token.is_some() {
                return Err(Error {
                    level: Level::Error,
                    span: Some(item.span),
                    message: "bindgen can not handle parameterized `#[repr(C)]` enums".into(),
                });
            }

            for var in item.variants {
                if var == syn::Fields::Unit {
                    return Err(Error {
                        level: Level::Error,
                        span: Some(var.span),
                        message: "bindgen can not handle `#[repr(C)]` enums with non-unit variants"
                            .into(),
                    });
                }

                let (_, docs) = parse_attr(
                    &var.node.attrs,
                    |_| true,
                    |attr| retrieve_docstring(attr, "\t"),
                );
                buffer.push_str(&docs);
                //TODO: Must be verified
                buffer.push_str(&format!("\t{}_{},\n", name, var.ident.to_string()));
            }
        } else {
            return Err(Error {
                level: Level::Bug,
                span: Some(item.span),
                message: "`parse_enum` called on wrong `Item_`".into(),
            });
        }

        buffer.push_str(&format!("}} {};\n\n", name));
        self.append_to_header(buffer, module, outputs)?;

        Ok(())
    }

    /// Convert a Rust struct into a C struct.
    ///
    /// The rust struct must be marked `#[repr(C)]` and must be public otherwise the function will
    /// abort.
    ///
    /// Bindgen will error if the struct is generic or if the struct is a unit or tuple struct.
    fn parse_struct(
        &mut self,
        item: &syn::ItemStruct,
        module: &[String],
        outputs: &mut Outputs,
    ) -> Result<(), Error> {
        let (repr_c, docs) = parse_attr(&item.attrs[..], check_repr_c, |attr| {
            retrieve_docstring(attr, "")
        });
        // If it's not #[repr(C)] then it can't be called from C.
        if !repr_c {
            return Ok(());
        }

        let mut buffer = String::new();
        buffer.push_str(&docs);

        let name = item.ident.to_string();
        buffer.push_str(&format!("typedef struct {}", name));

        if let syn::Item::Struct(ref item1) = item {
            if item1.generics.lt_token.is_some() {
                return Err(Error {
                    level: Level::Error,
                    span: Some(item1.span),
                    message: "bindgen can not handle parameterized `#[repr(C)]` structs".into(),
                });
            }
            buffer.push_str(" {\n");
            for field in item1.fields.iter() {
                let (_, docs) = parse_attr(
                    &field.attrs[..],
                    |_| true,
                    |attr| retrieve_docstring(attr, "\t"),
                );
                buffer.push_str(&docs);

                let name = match field.ident {
                    Some(name) => name.to_string(),
                    None => unreachable!("a tuple struct snuck through"),
                };

                let ty = rust_to_c(&*field.ty, &name)?;
                self.add_dependencies(module, &ty.1)?;
                buffer.push_str(&format!("\t{};\n", ty));
            }
            buffer.push_str("}");
        } else {
            return Err(Error {
                level: Level::Bug,
                span: Some(item.span),
                message: "`parse_struct` called on wrong `Item_`".into(),
            });
        }

        buffer.push_str(&format!(" {};\n\n", name));
        self.append_to_header(buffer, module, outputs)?;

        self.decls
            .insert(name.to_string(), header_name(module, &self.lib_name)?);

        Ok(())
    }

    /// Convert a Rust function declaration into a C function declaration.
    ///
    /// The function declaration must be marked `#[no_mangle]` and have a C ABI otherwise the
    /// function will abort.
    ///
    /// If the declaration is generic or diverges then bindgen will error.
    fn parse_fn(
        &mut self,
        item: &syn::ItemFn,
        module: &[String],
        outputs: &mut Outputs,
    ) -> Result<(), Error> {
        let (no_mangle, docs) = parse_attr(&item.attrs[..], check_no_mangle, |attr| {
            retrieve_docstring(attr, "")
        });
        // If it's not #[no_mangle] then it can't be called from C.
        if !no_mangle {
            return Ok(());
        }

        let name = item.ident.to_string();
        if let syn::Item::Fn(ref item) = item {
            match *item.abi.unwrap().name.unwrap().value().as_str() {
                // If it doesn't have a C ABI it can't be called from C.
                &"C" | &"Cdecl" | &"Stdcall" | &"Fastcall" | &"System" => {}
                _ => return Ok(()),
            }

            if item.decl.generics.lt_token.is_some() {
                return Err(Error {
                    level: Level::Error,
                    span: Some(item.span),
                    message: "bindgen can not handle parameterized extern functions".into(),
                });
            }

            self.transform_native_fn(&*item, &docs, &format!("{}", name), module, outputs)?;

            Ok(())
        } else {
            Err(Error {
                level: Level::Bug,
                span: Some(item.span),
                message: "`parse_fn` called on wrong `Item_`".into(),
            })
        }
    }

    fn finalise_output(&mut self, outputs: &mut Outputs) -> Result<(), Error> {
        let mut depgraph = Graph::<String, String>::new();
        let nodes_map: HashMap<String, _> = outputs
            .keys()
            .map(|m| (m.clone(), depgraph.add_node(m.clone())))
            .collect();
        let node_ids_map: HashMap<_, String> =
            nodes_map.iter().map(|(k, v)| (*v, k.clone())).collect();
        let mut edges = BTreeSet::new();

        // Wrap modules with common includes
        for (header_name, value) in outputs.iter_mut() {
            let code = format!("#include <stdint.h>\n#include <stdbool.h>\n\n{}", value);

            *value = wrap_guard(&wrap_extern(&code), header_name);

            // Building a graph of dependencies
            if let Some(module_deps) = self.deps.get(header_name) {
                for dep in module_deps {
                    if let Some(mod_name) = self.decls.get(dep) {
                        let pred = mod_name.to_string();
                        let succ = header_name.to_string();
                        if pred == succ {
                            continue;
                        }
                        let _ = edges.insert((nodes_map[&pred], nodes_map[&succ]));
                    }
                }
            }
        }

        // Build a full dependency graph and topologically sort dependencies
        depgraph.extend_with_edges(&edges);
        let sorted_deps = unwrap!(algo::toposort(&depgraph, None));

        // Generate a top-level header and add custom user code
        let mut top_level_header = String::new();
        if !self.custom_code.is_empty() {
            top_level_header.push_str(&format!("{}\n", self.custom_code));
        }
        for node_id in sorted_deps {
            let header_name = &node_ids_map[&node_id];
            top_level_header.push_str(&format!("#include \"{}\"\n", header_name));
        }

        outputs.insert(
            format!("{}.h", self.lib_name),
            wrap_guard(&top_level_header, &format!("{}_root", self.lib_name)),
        );

        Ok(())
    }
}

/// Turn a Rust type with an associated name or type into a C type.
pub fn rust_to_c(ty: &syn::Type, assoc: &str) -> Result<CTypeNamed, Error> {
    match ty {
        // Function pointers make life an absolute pain here.
        syn::Type::BareFn(ref bare_fn) => {
            Ok(CTypeNamed(Default::default(), fn_ptr_to_c(bare_fn, assoc)?))
        }
        // All other types just have a name associated with them.
        _ => Ok(CTypeNamed(assoc.to_string(), anon_rust_to_c(ty)?)),
    }
}

/// Turn a Rust type into a C type.
fn anon_rust_to_c(ty: &syn::Type) -> Result<CType, Error> {
    match ty {
        // Function pointers should not be in this function.
        syn::Type::BareFn(..) => Err(Error {
            level: Level::Error,
            span: None, //there is no span for types in syn, hence none
            message:
                "C function ptrs must have a name or function declaration associated with them"
                    .into(),
        }),
        // Fixed-length arrays, converted into pointers.
        syn::Type::Array(..) => Ok(CType::Ptr(Box::new(anon_rust_to_c(ty)?), CPtrType::Const)),
        // Standard pointers.
        syn::Type::Ptr(ref ptr) => ptr_to_c(ptr, ty),
        // Plain old types.
        syn::Type::Path(ref path) => path_to_c(path),
        // Possibly void, likely not.
        _ => {
            let new_type = ty.to_string();
            if new_type == "()" {
                // Ok("void".into())
                Ok(CType::Void)
            } else {
                Err(Error {
                    level: Level::Error,
                    span: Some(ty.span),
                    message: format!("bindgen can not handle the type `{}`", new_type),
                })
            }
        }
    }
}

/// Turn a Rust pointer (*mut or *const) into the correct C form.
fn ptr_to_c(typeptr: &syn::TypePtr, ty: &syn::Type) -> Result<CType, Error> {
    let new_type = anon_rust_to_c(ty)?;
    let mut const_type = match typeptr.mutability {
        // *const T
        Some(..) => CPtrType::Mutable,
        _ => match typeptr.const_token {
            Some(..) => CPtrType::Const,
            _ => None,
        },
    };
    Ok(CType::Ptr(Box::new(new_type), const_type))
}

/// Turn a Rust function pointer into a C function pointer.
///
/// Rust function pointers are of the form
///
/// ```ignore
/// fn(arg1: Ty1, ...) -> RetTy
/// ```
///
/// C function pointers are of the form
///
/// ```C
/// RetTy (*inner)(Ty1 arg1, ...)
/// ```
///
/// where `inner` could either be a name or the rest of a function declaration.
fn fn_ptr_to_c(fn_ty: &syn::TypeBareFn, inner: &str) -> Result<CType, Error> {
    if !fn_ty.lifetimes.is_empty() {
        return Err(Error {
            level: Level::Error,
            span: None, //there is no span for types in syn, hence none
            message: "bindgen can not handle lifetimes".into(),
        });
    }

    let args = if fn_ty.inputs.is_empty() {
        // No args
        vec![]
    } else {
        let mut args = vec![];
        for arg in &fn_ty.inputs {
            let arg_name1 = arg.name.to_string();
            let arg_type = rust_to_c(&*arg.ty, &arg_name1)?;
            args.push(arg_type);
        }
        args
    };

    let return_type = match &fn_ty.output {
        syn::ReturnType::Type(_, ref ty) => {
            match ty.as_ref() {
                syn::Type::Never(..) => {
                    return Err(Error {
                        level: Level::Error,
                        span: Some(ty.span), //there is no span for types in syn, hence None
                        message: "panics across a C boundary are naughty!".into(),
                    });
                }
                _ => anon_rust_to_c(&*ty)?,
            }
        }
        syn::ReturnType::Default => CType::Void,
    };

    Ok(CType::FnDecl {
        inner: inner.to_string(),
        args,
        return_type: Box::new(return_type),
    })
}

/// Convert a Rust path type (e.g. `my_mod::MyType`) to a C type.
///
/// Types hidden behind modules are almost certainly custom types (which wouldn't work) except
/// types in `libc` which we special case.
fn path_to_c(path: &syn::TypePath) -> Result<CType, Error> {
    if path.path.segments.is_empty() {
        return Err(Error {
            level: Level::Bug,
            span: None, //there is no span for path in syn, hence none
            message: "invalid type".into(),
        });
    }

    // Types in modules, `my_mod::MyType`.
    if path.path.segments.len() > 1 {
        let ty = path.path.segments.last().unwrap().into_value();
        //.expect("already checked that there were at least two elements");            .split_last()
        let ty: &str = &ty.to_string();
        //        path
        //            .path
        //            .segments
        //            .last().clone()
        //            .expect("poping last element for module failed");
        let mut module = Vec::new();
        for segment in &path.path.segments.iter() {
            module.push(segment);
        }
        let module = module.join("::");
        match &*module.into_boxed_slice() {
            "libc" => Ok(libc_ty_to_c(ty)),
            "std::os::raw" => Ok(osraw_ty_to_c(ty)),
            _ => Err(Error {
                level: Level::Error,
                span: Some(path.span),
                message: "can not handle types in other modules (except `libc` and `std::os::raw`)"
                    .into(),
            }),
        }
    } else {
        Ok(rust_ty_to_c(&path.path.segments[0].ident.to_string()))
    }
}

/// Convert a Rust type from `libc` into a C type.
///
/// Most map straight over but some have to be converted.
fn libc_ty_to_c(ty: &str) -> CType {
    match ty {
        "c_void" => CType::Void,
        "c_float" => CType::Native("float"),
        "c_double" => CType::Native("double"),
        "c_char" => CType::Native("char"),
        "c_schar" => CType::Native("signed char"),
        "c_uchar" => CType::Native("unsigned char"),
        "c_short" => CType::Native("short"),
        "c_ushort" => CType::Native("unsigned short"),
        "c_int" => CType::Native("int"),
        "c_uint" => CType::Native("unsigned int"),
        "c_long" => CType::Native("long"),
        "c_ulong" => CType::Native("unsigned long"),
        "c_longlong" => CType::Native("long long"),
        "c_ulonglong" => CType::Native("unsigned long long"),
        // All other types should map over to C.
        ty => CType::Mapping(ty.to_string()),
    }
}

/// Convert a Rust type from `std::os::raw` into a C type.
///
/// These mostly mirror the libc crate.
fn osraw_ty_to_c(ty: &str) -> CType {
    match ty {
        "c_void" => CType::Void,
        "c_char" => CType::Native("char"),
        "c_double" => CType::Native("double"),
        "c_float" => CType::Native("float"),
        "c_int" => CType::Native("int"),
        "c_long" => CType::Native("long"),
        "c_longlong" => CType::Native("long long"),
        "c_schar" => CType::Native("signed char"),
        "c_short" => CType::Native("short"),
        "c_uchar" => CType::Native("unsigned char"),
        "c_uint" => CType::Native("unsigned int"),
        "c_ulong" => CType::Native("unsigned long"),
        "c_ulonglong" => CType::Native("unsigned long long"),
        "c_ushort" => CType::Native("unsigned short"),
        // All other types should map over to C.
        ty => CType::Mapping(ty.to_string()),
    }
}

/// Convert any Rust type into C.
///
/// This includes user-defined types. We currently trust the user not to use types which we don't
/// know the structure of (like String).
fn rust_ty_to_c(ty: &str) -> CType {
    match ty {
        "()" => CType::Void,
        "f32" => CType::Native("float"),
        "f64" => CType::Native("double"),
        "i8" => CType::Native("int8_t"),
        "i16" => CType::Native("int16_t"),
        "i32" => CType::Native("int32_t"),
        "i64" => CType::Native("int64_t"),
        "isize" => CType::Native("intptr_t"),
        "u8" => CType::Native("uint8_t"),
        "u16" => CType::Native("uint16_t"),
        "u32" => CType::Native("uint32_t"),
        "u64" => CType::Native("uint64_t"),
        "usize" => CType::Native("uintptr_t"),
        "bool" => CType::Native("bool"),
        ty => libc_ty_to_c(ty),
    }
}

/// Wrap a block of code with an extern declaration.
fn wrap_extern(code: &str) -> String {
    format!(
        r#"
#ifdef __cplusplus
extern "C" {{
#endif

{}

#ifdef __cplusplus
}}
#endif
"#,
        code
    )
}

/// Wrap a block of code with an include-guard.
fn wrap_guard(code: &str, id: &str) -> String {
    format!(
        r"
#ifndef bindgen_{0}
#define bindgen_{0}

{1}

#endif
",
        sanitise_id(id),
        code
    )
}

/// Transform a module name into a header name
fn header_name(module: &[String], lib_name: &str) -> Result<String, Error> {
    let mut module_name: Vec<String> = module.to_vec();
    if module_name[0] == "ffi" {
        module_name[0] = lib_name.to_string();

        // Top-level module for a library - e.g. safe_app/safe_app.h
        if module_name.len() == 1 {
            module_name.push(lib_name.to_string());
        }
    }

    let header_name = format!("{}.h", module_name.join(&path::MAIN_SEPARATOR.to_string()));

    Ok(header_name)
}

/// Remove illegal characters from the identifier.
///
/// This is because macros names must be valid C identifiers. Note that the identifier will always
/// be concatenated onto `cheddar_generated_` so can start with a digit.
pub fn sanitise_id(id: &str) -> String {
    // `char.is_digit(36)` ensures `char` is in `[A-Za-z0-9]`
    id.chars()
        .filter(|ch| ch.is_digit(36) || *ch == '_')
        .collect()
}
