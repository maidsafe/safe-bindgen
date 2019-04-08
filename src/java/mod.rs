//! Functions for generating Java glue code.

mod jni;
mod types;

use crate::common::{
    self, append_output, check_no_mangle, is_array_arg, is_array_arg_barefn, is_user_data_arg,
    is_user_data_arg_barefn, parse_attr, retrieve_docstring, take_out_pat,
    transform_fnarg_to_argcap, FilterMode, Outputs,
};
extern crate inflector;
use self::inflector::Inflector;
use crate::java::types::{callback_name, java_type_to_str, rust_to_java, struct_to_java_classname};
use crate::struct_field::{transform_struct_fields, StructField};
use crate::{Error, Level};
use ::jni::signature::JavaType;
use quote::*;
use ::rustfmt;
use std::collections::{BTreeSet, HashMap, HashSet};
use unwrap::unwrap;

pub struct LangJava {
    context: Context,
    filter: HashSet<String>,
    filter_mode: FilterMode,
}

pub struct Context {
    /// Native library name
    lib_name: String,
    /// Namespace
    namespace: String,
    /// Model namespace (structures go into this one)
    namespace_model: String,
    /// Maps types from Rust to Java
    type_map: HashMap<&'static str, JavaType>,
    /// Keeps track of which JNI callback functions has been generated already
    generated_jni_cbs: BTreeSet<String>,
}

impl Default for Context {
    fn default() -> Self {
        Self {
            lib_name: "safe".to_string(),
            namespace: "net.maidsafe.dummy".to_string(),
            namespace_model: "net.maidsafe.dummy".to_string(),
            type_map: Default::default(),
            generated_jni_cbs: Default::default(),
        }
    }
}

impl LangJava {
    pub fn new(type_map: HashMap<&'static str, JavaType>) -> Self {
        LangJava {
            filter: Default::default(),
            filter_mode: FilterMode::Blacklist,
            context: Context {
                type_map,
                lib_name: "backend".to_owned(),
                namespace: "net.maidsafe.bindings".to_owned(),
                namespace_model: "net.maidsafe.model".to_owned(),
                generated_jni_cbs: BTreeSet::new(),
            },
        }
    }

    /// Set the name of the native library. This also sets the class name.
    pub fn set_lib_name<T: Into<String>>(&mut self, name: T) {
        self.context.lib_name = name.into();
    }

    /// Set the namespace to put the NativeBindings class in.
    pub fn set_namespace<T: Into<String>>(&mut self, namespace: T) {
        self.context.namespace = namespace.into();
    }

    /// Set the namespace to put all structures/classes in.
    pub fn set_model_namespace<T: Into<String>>(&mut self, namespace: T) {
        self.context.namespace_model = namespace.into();
    }

    /// Applies rustfmt to JNI code to improve debuggability
    fn format_jni_output(&self, input: &mut String) {
        let mut output: Vec<u8> = Vec::with_capacity(input.len() * 2);
        let mut cfg = rustfmt::config::Config::default();
        cfg.set().write_mode(rustfmt::config::WriteMode::Plain);
        rustfmt::format_input(rustfmt::Input::Text(input.clone()), &cfg, Some(&mut output))
            .unwrap();

        *input = String::from_utf8(output).expect("Invalid Rustfmt output found");
    }

    /// Adds package info to the NativeBindings Java module and indents lines
    fn format_native_functions(&self, funcs: &mut String) {
        // Indent lines
        let lines = funcs.lines().fold(String::new(), |mut output, line| {
            output.push_str(&format!("\t{}\n", line));
            output
        });
        *funcs = format!(
            "package {namespace};\n\n
                         public class NativeBindings {{\n
                         {lines}\n
                         }}",
            namespace = self.context.namespace,
            lines = lines
        );
    }

    /// Add the identifier to the filter.
    /// If the filter mode is `Blacklist` (the default), the identifiers in the
    /// filter are ignored.
    /// If it is `Whitelist`, the identifiers not in the filter are ignored.
    pub fn filter<T: Into<String>>(&mut self, ident: T) {
        let _ = self.filter.insert(ident.into());
    }

    /// Clears the current filter and sets the filter mode.
    pub fn reset_filter(&mut self, filter_mode: FilterMode) {
        self.filter.clear();
        self.filter_mode = filter_mode;
    }

    fn is_ignored(&self, ident: &str) -> bool {
        match self.filter_mode {
            FilterMode::Blacklist => self.filter.contains(ident),
            FilterMode::Whitelist => !self.filter.contains(ident),
        }
    }
}

impl common::Lang for LangJava {
    /// Convert a Rust function declaration into Java.
    fn parse_fn(
        &mut self,
        item: &syn::ItemFn,
        _module: &[String],
        outputs: &mut Outputs,
    ) -> Result<(), Error> {
        let ident = &item.ident;
        let name = format!("{}", quote!(#ident));
        if self.is_ignored(name.as_str()) {
            return Ok(());
        }

        let (no_mangle, docs) = parse_attr(&item.attrs[..], check_no_mangle, |attr| {
            retrieve_docstring(attr, "")
        });
        // If it's not #[no_mangle] then it can't be called from C.
        if !no_mangle {
            return Ok(());
        }
        if !common::is_extern(unwrap!(item.to_owned().abi)) {
            // If it doesn't have a C ABI it can't be called from C.
            return Ok(());
        }

        if !item.decl.generics.params.is_empty() {
            return Err(Error {
                level: Level::Error,
                span: None, //NONE FOR NOW
                message: "cheddar can not handle parameterized extern functions".into(),
            });
        }

        transform_native_fn(
            *item.to_owned().decl,
            &item.attrs[..],
            &docs,
            &format!("{}", name),
            outputs,
            &mut self.context,
        )?;

        Ok(())
    }

    /// Convert a Rust struct into a Java class.
    fn parse_struct(
        &mut self,
        item: &syn::ItemStruct,
        _module: &[String],
        outputs: &mut Outputs,
    ) -> Result<(), Error> {
        let name = item.ident.to_string();
        if self.is_ignored(&name) {
            return Ok(());
        }
        let (repr_c, docs) = parse_attr(&item.attrs, common::check_repr_c, |attr| {
            retrieve_docstring(attr, "")
        });
        // If it's not #[repr(C)] then it can't be called from C.
        if !repr_c {
            return Ok(());
        }

        let mut buffer = String::new();
        buffer.push_str(&format!("package {};\n\n", self.context.namespace));
        buffer.push_str(&docs);

        let orig_name = item.ident.to_owned().to_string();
        let name = struct_to_java_classname(&*orig_name);
        buffer.push_str(&format!("public class {}", name));

        if !item.generics.params.is_empty() {
            return Err(Error {
                level: Level::Error,
                span: None, // NONE FOR NOW
                message: "cheddar can not handle parameterized `#[repr(C)]` structs".into(),
            });
        }

        let mut vec = vec![];
        for x in item.fields.iter() {
            vec.push(x.clone());
        }
        let struct_fields = transform_struct_fields(vec.as_slice());
        let fields = transform_struct_into_class_fields(&struct_fields, &self.context)?;

        buffer.push_str(" {\n");

        // Class fields
        buffer.push_str(&generate_class_fields(&fields)?);
        buffer.push_str("\n");

        // Default constructor that should initialise object fields
        buffer.push_str(&generate_default_constructor(&name, &fields)?);

        // Parametrised constructor
        buffer.push_str(&generate_parametrised_constructor(&name, &fields)?);

        // Getters & setters
        buffer.push_str(&generate_getters_setters(&fields)?);
        buffer.push_str("}");

        let jni = jni::generate_struct(&struct_fields, &orig_name, &name, &self.context);
        append_output(jni, "jni.rs", outputs);

        buffer.push_str("\n\n");

        outputs.insert(format!("{}.java", name), buffer);

        Ok(())
    }

    fn finalise_output(&mut self, outputs: &mut Outputs) -> Result<(), Error> {
        match outputs.get_mut("jni.rs") {
            Some(input) => {
                //self.format_jni_output(input);
            }
            None => {
                return Err(Error {
                    level: Level::Error,
                    span: None, //NONE FOR NOW
                    message: "no jni bindings generated?".to_owned(),
                });
            }
        }

        match outputs.get_mut("NativeBindings.java") {
            Some(input) => {
                self.format_native_functions(input);
                Ok(())
            }
            None => Err(Error {
                level: Level::Error,
                span: None, //NONE FOR NOW
                message: "no native bindings generated?".to_owned(),
            }),
        }
    }
}

/// Contains all information necessary to construct a Java class
/// field, transformed from `StructField`.
struct JavaClassField {
    name: String,
    ty: JavaType,
    ty_str: String,
}

/// Transforms a list of struct fields into Java class fields
fn transform_struct_into_class_fields(
    fields: &[StructField],
    context: &Context,
) -> Result<Vec<JavaClassField>, Error> {
    let mut class_fields = Vec::new();

    for field in fields {
        let name = field.name().to_camel_case();
        let struct_field = field.struct_field().clone();
        let mut ty = rust_to_java(&struct_field.ty, context)?;
        if let StructField::Array { .. } = *field {
            // Wrap a type into an array
            ty = JavaType::Array(Box::new(ty));
        }
        let ty_str = java_type_to_str(&ty)?;

        class_fields.push(JavaClassField { name, ty, ty_str });
    }

    Ok(class_fields)
}

/// Generates getters and setters for a struct transformed into a Java class
fn generate_getters_setters(fields: &[JavaClassField]) -> Result<String, Error> {
    let mut buffer = String::new();

    for field in fields {
        buffer.push_str(&format!(
            "\tpublic {ty} get{capitalized}() {{\n\t\treturn {name};\n\t}}\n\n",
            ty = field.ty_str,
            name = field.name,
            capitalized = field.name.to_class_case(),
        ));
        buffer.push_str(&format!(
            "\tpublic void set{capitalized}(final {ty} val) {{\n\t\tthis.{name} \
             = val;\n\t}}\n\n",
            ty = field.ty_str,
            name = field.name,
            capitalized = field.name.to_class_case(),
        ));
    }

    Ok(buffer)
}

/// Generates fields for a struct transformed into a Java class
fn generate_class_fields(fields: &[JavaClassField]) -> Result<String, Error> {
    let mut buffer = String::new();

    for field in fields {
        buffer.push_str(&format!("\tprivate {} {};\n", field.ty_str, field.name));
    }

    Ok(buffer)
}

/// Generates code for the default constructor initialising class fields with
/// default values
fn generate_default_constructor(
    class_name: &str,
    fields: &[JavaClassField],
) -> Result<String, Error> {
    let mut default_obj_fields = Vec::new();

    for field in fields {
        // Initialise object and array fields with default values to prevent them from being null
        match field.ty {
            JavaType::Array(..) => {
                default_obj_fields.push(format!(
                    "\t\tthis.{name} = new {ty} {{}};",
                    name = field.name,
                    ty = field.ty_str
                ));
            }
            JavaType::Object(ref obj) => {
                default_obj_fields.push(format!(
                    "\t\tthis.{name} = new {obj}();",
                    name = field.name,
                    obj = obj
                ));
            }
            _ => (),
        }
    }
    Ok(format!(
        "\tpublic {name}() {{\n{default_obj_fields}\n\t}}\n",
        name = class_name,
        default_obj_fields = default_obj_fields.join("\n")
    ))
}

/// Generates code for the parametrised constructor (which is taking arguments to
/// initialise default values)
fn generate_parametrised_constructor(
    class_name: &str,
    fields: &[JavaClassField],
) -> Result<String, Error> {
    let mut constructor_fields = Vec::new();
    let mut constructor_assignments = Vec::new();

    for field in fields {
        constructor_fields.push(format!("{} {}", field.ty_str, field.name));
        constructor_assignments.push(format!("\t\tthis.{name} = {name};", name = field.name));
    }

    Ok(format!(
        "\tpublic {name}({constructor_fields}) {{\n{constructor_assignments}\n\t}}\n",
        name = class_name,
        constructor_fields = constructor_fields.join(", "),
        constructor_assignments = constructor_assignments.join("\n")
    ))
}

/// Transform a Rust FFI function into a Java native function
pub fn transform_native_fn(
    fn_decl: syn::FnDecl,
    attrs: &[syn::Attribute],
    docs: &str,
    name: &str,
    outputs: &mut Outputs,
    context: &mut Context,
) -> Result<(), Error> {
    let mut args_str = Vec::new();

    let mut fn_args = fn_decl
        .inputs
        .iter()
        .filter(|arg| !is_user_data_arg(&unwrap!(transform_fnarg_to_argcap(*arg))))
        .peekable();

    while let Some(arg) = fn_args.next() {
        let pat = take_out_pat(&unwrap!(transform_fnarg_to_argcap(arg)).pat);
        let arg_name = unwrap!(pat).ident.to_string();

        // Generate function arguments
        let mut java_type = rust_to_java(&unwrap!(transform_fnarg_to_argcap(arg)).ty, context)?;
        let mut next_arg: Option<&syn::ArgCaptured>;
        if fn_args.peek().is_some() {
            next_arg = Some(unwrap!(transform_fnarg_to_argcap(unwrap!(fn_args.peek()))))
        } else {
            next_arg = None
        }
        if is_array_arg(unwrap!(transform_fnarg_to_argcap(arg)), next_arg) {
            // Skip the length args - e.g. for a case of `ptr: *const u8, ptr_len: usize`
            // we're going to skip the `len` part.
            java_type = JavaType::Array(Box::new(java_type));
            fn_args.next();
        }

        let java_type = java_type_to_str(&java_type)?;
        args_str.push(format!("{} {}", java_type, arg_name.to_camel_case()));
        let argcap = unwrap!(transform_fnarg_to_argcap(arg));
        // Generate a callback class - if it wasn't generated already
        if let syn::Type::BareFn(ref bare_fn) = argcap.ty {
            let mut vec = vec![];
            for input in bare_fn.inputs.to_owned() {
                vec.push(input);
            }
            let cb_class = callback_name(&vec.as_slice(), context)?;
            let cb_file = format!("{}.java", cb_class);

            if outputs.get(&cb_file).is_none() {
                eprintln!("Generating CB {}", cb_class);

                let cb_output = transform_callback(&argcap.ty, &cb_class, context)?;
                let _ = outputs.insert(cb_file, cb_output);

                // Generate JNI callback fn
                let jni_cb_name = format!("call_{}", cb_class);
                if !context.generated_jni_cbs.contains(&jni_cb_name) {
                    let mut jni = jni::generate_jni_callback(bare_fn, &jni_cb_name, context);
                    jni.push_str("\n");

                    append_output(jni, "jni.rs", outputs);
                    context.generated_jni_cbs.insert(jni_cb_name);
                }
            }
        }
    }

    let output_type = &fn_decl.output;
    let return_type = match &*output_type {
        syn::ReturnType::Type(_, ref ty) if check_type_never(&*ty) => {
            return Err(Error {
                level: Level::Error,
                span: None, //NONE FOR NOW
                message: "panics across a C boundary are naughty!".into(),
            });
        }
        syn::ReturnType::Default => String::from("public static native void"),
        syn::ReturnType::Type(_, ref ty) => {
            java_type_to_str(&unwrap!(rust_to_java(&*ty, context)))?
        }
    };

    let java_name = name.to_camel_case();
    let func_decl = format!(
        "{} {}({})",
        return_type,
        &java_name,
        args_str.as_slice().join(", ")
    );

    let mut buffer = String::new();
    buffer.push_str("/**\n");
    buffer.push_str(&docs.replace("///", " *"));
    buffer.push_str(" */\n");
    buffer.push_str(&func_decl);
    buffer.push_str(";\n\n");

    append_output(buffer, "NativeBindings.java", outputs);

    // Append the function declaration to import it as an "extern fn"
    let mut fn_attrs = String::new();
    for attr in attrs {
        if attr.into_token_stream().to_owned().to_string() == "cfg" {
            fn_attrs.push_str(&attr.into_token_stream().to_owned().to_string());
        }
    }
    //    let fn_decl_import = pprust::fun_to_string(
    //        fn_decl,
    //        ast::Unsafety::Normal,
    //        ast::Constness::NotConst,
    //        ast::Ident::from_str(name),
    //        &ast::Generics::default(),
    //    );
    let mut fn_declaration: String = format!("fn {} (", quote!(#name));
    let len = fn_decl.inputs.len();
    let mut count: usize = 1;
    for x in &fn_decl.inputs {
        let argcap = transform_fnarg_to_argcap(x).unwrap();
        if count != len {
            fn_declaration.push_str(&format!("{}, \n", quote!(#argcap)));
        } else {
            fn_declaration.push_str(&format!("{} \n", quote!(#argcap)));
        }
        count = count + 1;
    }
    fn_declaration.push_str(")");
    let mut jni = format!(
        "\n{attrs}#[link(name = \"{libname}\")]\nextern {{ {fndecl}; }}\n",
        attrs = fn_attrs,
        libname = context.lib_name,
        fndecl = fn_declaration,
    );
    let vec: Vec<_> = fn_decl.inputs.iter().cloned().collect();
    // Generate the JNI part of the interface
    jni.push_str(&jni::generate_jni_function(
        &vec, attrs, name, &java_name, context, outputs,
    ));
    jni.push_str("\n");
    append_output(jni, "jni.rs", outputs);

    Ok(())
}

fn check_type_never(ty: &syn::Type) -> bool {
    if let syn::Type::Never(ref _never) = ty {
        return true;
    } else {
        return false;
    }
}

/// Turn a Rust callback function type into a Java interface.
pub fn transform_callback<S: AsRef<str>>(
    ty: &syn::Type,
    class_name: S,
    context: &Context,
) -> Result<String, Error> {
    match ty {
        syn::Type::BareFn(ref bare_fn) => Ok(format!(
            "package {namespace};\n\n\
             public interface {name} {{\n\
             \tpublic void call({types});\n}}\n",
            namespace = context.namespace_model,
            name = class_name.as_ref(),
            types = callback_to_java(bare_fn, context)?,
        )),
        // All other types just have a name associated with them.
        _ => Err(Error {
            level: Level::Error,
            span: None, //NONE FOR NOW
            message: "Invalid callback type".into(),
        }),
    }
}

/// Transform a Rust FFI callback into Java function signature
fn callback_to_java(fn_ty: &syn::TypeBareFn, context: &Context) -> Result<String, Error> {
    match unwrap!(unwrap!(fn_ty.to_owned().abi).name)
        .value()
        .to_owned()
        .as_str()
    {
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

    if fn_ty.to_owned().lifetimes.is_some() {
        return Err(Error {
            level: Level::Error,
            span: None, //NONE FOR NOW
            message: "can not handle lifetimes".into(),
        });
    }

    let mut args = Vec::new();

    let mut args_iter = fn_ty
        .inputs
        .iter()
        .filter(|arg| !is_user_data_arg_barefn(arg))
        .peekable();

    while let Some(arg) = args_iter.next() {
        let mut arg_name = unwrap!(arg.to_owned().name)
            .0
            .into_token_stream()
            .to_string();
        let mut java_type = rust_to_java(&arg.ty, context)?;

        if is_array_arg_barefn(arg, args_iter.peek().cloned()) {
            // Detect array ptrs: skip the length args and add array to the type sig
            java_type = JavaType::Array(Box::new(java_type));
            args_iter.next();
        }

        let java_type = java_type_to_str(&java_type)?;
        args.push(format!("{} {}", java_type, arg_name.to_camel_case()));
    }

    Ok(args.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn;

    #[test]
    fn cb_names() {
        fn get_inputs(source: &str) -> Vec<syn::BareFnArg> {
            let item: syn::TypeBareFn = unwrap!(syn::parse_str(source));
            item.inputs.into_iter().collect()
        }

        let context = Context {
            type_map: HashMap::new(),
            lib_name: "backend".to_owned(),
            namespace: "net.maidsafe.bindings".to_owned(),
            namespace_model: "net.maidsafe.model".to_owned(),
            generated_jni_cbs: BTreeSet::new(),
        };

        let inputs = get_inputs("fn ()");
        assert_eq!("CallbackVoid", unwrap!(callback_name(&inputs, &context)));

        let inputs = get_inputs("fn (user_data: *mut c_void, result: *const FfiResult)");
        assert_eq!("CallbackResult", unwrap!(callback_name(&inputs, &context)));

        let inputs =
            get_inputs("fn (user_data: *mut c_void, result: *const FfiResult, b: u64, c: u32)");
        assert_eq!(
            "CallbackResultLongInt",
            unwrap!(callback_name(&inputs, &context))
        );

        let inputs =
            get_inputs("fn (user_data: *mut c_void, result: *const FfiResult, b: *const MyStruct)");
        assert_eq!(
            "CallbackResultMyStruct",
            unwrap!(callback_name(&inputs, &context))
        );

        let inputs =
            get_inputs("fn (user_data: *mut c_void, result: *const FfiResult, b: *const c_char)");
        assert_eq!(
            "CallbackResultString",
            unwrap!(callback_name(&inputs, &context))
        );
    }
}
