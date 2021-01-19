//! Functions to generate JNI bindings

use super::types::{callback_name, rust_ty_to_java};
use super::{Context, Outputs};
use crate::common::{
    append_output, is_array_arg, is_array_arg_barefn, is_user_data_arg, is_user_data_arg_barefn,
    take_out_ident_from_type, take_out_pat, transform_fnarg_to_argcap,
    transform_fnarg_to_argcap_option,
};
use crate::struct_field::StructField;
use inflector::Inflector;
use jni::signature::{self, JavaType, Primitive, TypeSignature};
use proc_macro2::Span;
use quote::ToTokens;
use quote::*;
use unwrap::unwrap;

fn to_jni_arg(arg: &syn::ArgCaptured, ty_name: &str) -> proc_macro2::TokenStream {
    let pat = take_out_pat(&arg.pat);
    let pat = syn::Ident::new(unwrap!(pat).ident.to_string().as_str(), Span::call_site());
    let ty_name = syn::Ident::new(ty_name, Span::call_site());
    quote! { #pat: #ty_name }
}

fn transform_jni_arg(arg: &syn::ArgCaptured) -> proc_macro2::TokenStream {
    match arg.ty {
        // Callback
        syn::Type::BareFn(ref _bare_fn) => to_jni_arg(arg, "JObject"),

        // Plain old types.
        syn::Type::Path(ref path) => {
            let ty = unwrap!(path.path.segments.last()).into_value();
            let ty = ty.ident.to_string();

            let jni_type = match ty.as_str() {
                "c_char" | "u8" | "i8" => "jbyte",
                "c_short" | "u16" | "i16" => "jshort",
                "c_int" | "u32" | "i32" => "jint",
                "c_long" | "u64" | "i64" | "c_usize" | "usize" | "isize" => "jlong",
                _ => ty.as_str(),
            };

            to_jni_arg(arg, jni_type)
        }

        // Standard pointers.
        syn::Type::Ptr(ref ptr) => {
            // Detect strings, which are *const c_char or *mut c_char
            match ptr.into_token_stream().to_string().as_str() {
                "* const c_char" | "* mut c_char" => to_jni_arg(arg, "JString"),
                "* mut App" | "* mut Authenticator" | "* const App" | "* const Authenticator" => {
                    to_jni_arg(arg, "jlong")
                } // Opaque ptr,
                _ => to_jni_arg(arg, "JObject"),
            }
        }

        _ => {
            let ty = &arg.ty;
            to_jni_arg(arg, &format!("{}", quote!(#ty)))
        }
    }
}

// Produces a fully qualified class name (i.e. with a Java package)
fn fully_qualified(ty: &str, context: &Context) -> String {
    match ty {
        "String" => "java/lang/String".to_string(),
        ty => format!("{}/{}", context.namespace_model.replace(".", "/"), ty),
    }
}

// Checks whether the type string is defined in the type map and if it is
// then returns the correct Object signature. Otherwise, uses the default `Ljava/lang/Object;`.
fn lookup_object_type(ty: &str, context: &Context) -> JavaType {
    if let Some(mapped) = context.type_map.get(ty) {
        (*mapped).clone()
    } else {
        JavaType::Object(fully_qualified(ty, context))
    }
}

fn rust_ty_to_signature(ty: &syn::Type, context: &Context) -> Option<JavaType> {
    match ty {
        // Callback
        syn::Type::BareFn(ref _bare_fn) => Some(JavaType::Object(From::from("java/lang/Object"))),

        // Plain old types.
        syn::Type::Path(ref path) => {
            let ty = unwrap!(path.path.segments.last()).into_value();
            let ty = ty.ident.to_string();
            rust_ty_to_java(ty.as_str()).or_else(|| Some(lookup_object_type(ty.as_str(), context)))
        }

        // Standard pointers.
        syn::Type::Ptr(ref ptr) => {
            // Detect strings, which are *const c_char or *mut c_char
            if format!("{}", quote!(#ptr)).as_str() == "* mut c_char"
                || format!("{}", quote!(#ptr)).as_str() == "* const c_char"
            {
                Some(JavaType::Object(From::from("java/lang/String")))
            } else {
                rust_ty_to_signature(&*ptr.elem, context)
            }
        }

        _ => None,
    }
}

struct JniArgResult {
    stmt: proc_macro2::TokenStream,
    call_args: Vec<proc_macro2::TokenStream>,
}

fn transform_string_arg(arg_name: &str) -> JniArgResult {
    // statements
    let arg_name = syn::Ident::new(arg_name, Span::call_site());
    let stmt = quote! {
        let #arg_name = jni_unwrap!(CString::from_java(&env, #arg_name));
    };

    // call arg value(s)
    let call_args = vec![quote! { #arg_name.as_ptr() }];

    JniArgResult { stmt, call_args }
}

fn transform_struct_arg(arg_name: &str, arg_ty: &syn::Type) -> JniArgResult {
    // statements
    let arg_name = syn::Ident::new(arg_name, Span::call_site());
    let struct_ty = syn::Ident::new(format!("{}", quote!(#arg_ty)).as_str(), Span::call_site());
    let stmt = quote! {
        let #arg_name = jni_unwrap!(#struct_ty::from_java(&env, #arg_name));
    };

    // call arg value(s)
    let call_args = vec![quote! { &#arg_name }];

    JniArgResult { stmt, call_args }
}

fn transform_array_arg(arg_name: &str) -> JniArgResult {
    // statements
    let arg_name = syn::Ident::new(arg_name.to_string().as_str(), Span::call_site());
    let stmt = quote! {
        let #arg_name = jni_unwrap!(Vec::from_java(&env, #arg_name));
    };

    // call arg value(s)
    let call_args = vec![quote! { #arg_name.as_ptr() }, quote! { #arg_name.len() }];

    JniArgResult { stmt, call_args }
}

fn transform_callbacks_arg(
    cb_idents: &[(&syn::TypeBareFn, syn::Ident)],
    cb_base_name: &str,
) -> JniArgResult {
    // statements
    let cb_ids: Vec<syn::Ident> = cb_idents
        .iter()
        .map(|&(_, ref ident)| ident.clone())
        .collect();

    let stmt = quote! {
        let ctx = gen_ctx!(env, #(#cb_ids),*);
    };

    // call arg value(s)
    let multi_callback = cb_idents.len() > 1;

    let call_args = cb_idents
        .iter()
        .enumerate()
        .map(|(idx, _)| {
            let cb_fn = if multi_callback {
                syn::Ident::new(
                    format!("{}_{}", cb_base_name, idx).as_str(),
                    Span::call_site(),
                )
            } else {
                syn::Ident::new(cb_base_name, Span::call_site())
            };
            quote! { #cb_fn }
        })
        .collect();

    JniArgResult { stmt, call_args }
}

fn transform_opaque_ptr(arg_name: &str, ty: &str) -> JniArgResult {
    // statements
    let arg_name = syn::Ident::new(arg_name, Span::call_site());
    let ty = syn::Ident::new(ty, Span::call_site());
    let stmt = quote! {
        let #arg_name = #arg_name as *mut #ty;
    };

    // call arg value(s)
    let call_args = vec![quote! { #arg_name }];

    JniArgResult { stmt, call_args }
}

/// Generates JNI function binding based on a native function
pub fn generate_jni_function(
    args: &[syn::FnArg],
    attrs: &[syn::Attribute],
    native_name: &str,
    func_name: &str,
    context: &mut Context,
    outputs: &mut Outputs,
) -> String {
    let func_name = syn::Ident::new(
        format!(
            "Java_{}_NativeBindings_{}",
            context.namespace.replace("_", "_1").replace(".", "_"),
            func_name
        )
        .as_str(),
        Span::call_site(),
    );
    let native_name_str = native_name;
    let native_name = syn::Ident::new(native_name, Span::call_site());

    // Generate inputs
    let mut call_args = Vec::new();
    let mut stmts = Vec::new();
    let mut callbacks = Vec::new();
    let mut jni_fn_inputs = Vec::new();

    let mut args_iter = args
        .iter()
        .filter(|arg| !is_user_data_arg(&unwrap!(transform_fnarg_to_argcap(arg))))
        .peekable();

    while let Some(arg) = args_iter.next() {
        let argcap = unwrap!(transform_fnarg_to_argcap(arg));
        let pat = take_out_pat(&argcap.pat);
        let arg_name = unwrap!(pat).ident.to_string();
        let res = if is_array_arg(
            transform_fnarg_to_argcap(&arg).unwrap(),
            transform_fnarg_to_argcap_option(args_iter.peek().cloned()),
        ) {
            args_iter.next();
            Some(transform_array_arg(&arg_name))
        } else {
            match unwrap!(transform_fnarg_to_argcap(arg)).ty {
                // Callback
                syn::Type::BareFn(ref bare_fn) => {
                    callbacks.push((
                        bare_fn,
                        syn::Ident::new(arg_name.as_str(), Span::call_site()),
                    ));
                    None
                }

                // Pointers
                syn::Type::Ptr(ref ptr) => {
                    let ident = unwrap!(take_out_ident_from_type(&*ptr.elem));
                    match ident.as_str() {
                        // Opaque pointer that should be passed as a long value
                        opaque @ "App" | opaque @ "Authenticator" => {
                            Some(transform_opaque_ptr(&arg_name, opaque))
                        }
                        // Detect strings, which are *const c_char or *mut c_char
                        "c_char" => Some(transform_string_arg(&arg_name)),
                        _ => Some(transform_struct_arg(&arg_name, &*ptr.elem)),
                    }
                }

                // Native types and others
                ref native_ty => {
                    let id = syn::Ident::new(arg_name.as_str(), Span::call_site());
                    Some(JniArgResult {
                        stmt: quote! {},
                        call_args: vec![quote! { #id as #native_ty }],
                    })
                }
            }
        };

        if let Some(jni_arg_res) = res {
            call_args.extend(jni_arg_res.call_args);
            stmts.push(jni_arg_res.stmt);
        }

        jni_fn_inputs.push(transform_jni_arg(unwrap!(transform_fnarg_to_argcap(&arg))));
    }

    if !callbacks.is_empty() {
        let cb_base_name = if callbacks.len() > 1 {
            format!("call_{}", native_name_str)
        } else {
            let &(ref cb, _) = &callbacks[0];
            let vec: Vec<_> = cb.inputs.iter().cloned().collect();
            format!("call_{}", unwrap!(callback_name(&vec.as_slice(), context)))
        };

        let cb_arg_res = transform_callbacks_arg(callbacks.as_slice(), &cb_base_name);
        call_args.push(quote! { ctx });
        call_args.extend(cb_arg_res.call_args);
        stmts.push(cb_arg_res.stmt);
    }

    if callbacks.len() > 1 {
        // Generate extra callbacks for multi-callback functions
        let count = callbacks.len();

        for (idx, &(ref cb, _)) in callbacks.iter().enumerate() {
            let full_cb_name = format!("call_{}_{}", native_name_str, idx);
            eprintln!("Generating JNI CB {}", full_cb_name);

            if !context.generated_jni_cbs.contains(&full_cb_name) {
                let mut jni = generate_multi_jni_callback(cb, &full_cb_name, idx, count, context);
                jni.push('\n');

                append_output(jni, "jni.rs", outputs);
                context.generated_jni_cbs.insert(full_cb_name);
            }
        }
    }

    let tokens = quote! {
        #[no_mangle]
        pub unsafe extern "system" fn #func_name(
            env: JNIEnv,
            _class: JClass,
            #(#jni_fn_inputs),*
        ) {
            #(#stmts)*
            #native_name(#(#call_args),*);
        }
    };

    let mut output = String::new();

    for attr in attrs {
        if attr.to_owned().path.into_token_stream().to_string() == "cfg" {
            output.push_str(format!("{}", quote!(#attr)).as_str());
            output.push('\n');
        }
    }

    output.push_str(&tokens.to_string());

    output
}

struct JniCallback {
    // Native function call parameters
    args: Vec<proc_macro2::TokenStream>,
    // Callback function statements
    stmts: Vec<proc_macro2::TokenStream>,
    // Arguments for the callback function
    jni_cb_inputs: Vec<proc_macro2::TokenStream>,
    // String Java type signature constructor
    arg_ty_str: String,
}

fn generate_callback(cb: &syn::TypeBareFn, context: &Context) -> JniCallback {
    let mut args: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut stmts: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut jni_cb_inputs = Vec::new();
    let mut arg_java_ty = Vec::new();

    let mut args_iter = cb
        .inputs
        .iter()
        .filter(|arg| !is_user_data_arg_barefn(*arg))
        .peekable();

    while let Some(arg) = args_iter.next() {
        let arg_name = unwrap!(arg.clone().name).0; // FIXME: get rid of unwrap - use default name if not present
        let arg_ty = &arg.ty;

        jni_cb_inputs.push(quote! { #arg_name: #arg_ty });
        args.push(quote! { #arg_name.into() });

        if is_array_arg_barefn(arg, args_iter.peek().cloned()) {
            // Handle array arguments
            let val_java_type = unwrap!(rust_ty_to_signature(&arg.ty, context));
            arg_java_ty.push(JavaType::Array(Box::new(val_java_type)));

            if let Some(len_arg) = args_iter.next() {
                let len_arg_name = unwrap!(len_arg.clone().name).0; // FIXME: get rid of unwrap - use default name if not present
                let len_arg_ty = &len_arg.ty;

                jni_cb_inputs.push(quote! { #len_arg_name: #len_arg_ty });

                stmts.push(quote! {
                    let #arg_name = jni_unwrap!(
                        slice::from_raw_parts(#arg_name, #len_arg_name).to_java(&env)
                    );
                });
            } else {
                // error: no length arg?
            }
        } else {
            let stmt = match arg.ty {
                // Pointers
                syn::Type::Ptr(ref ptr) => {
                    let ty = &*ptr.elem;
                    match ty.into_token_stream().to_string().as_str() {
                        // Opaque ptrs passed as long values
                        "App" | "Authenticator" => {
                            quote! {
                                let #arg_name = #arg_name as jlong;
                            }
                        }
                        // Strings
                        "c_char" => {
                            quote! {
                                let #arg_name: JObject = if #arg_name.is_null() {
                                    JObject::null()
                                } else {
                                    jni_unwrap!(#arg_name.to_java(&env))
                                        .into()
                                };
                            }
                        }
                        // Other ptrs
                        _ => {
                            quote! {
                                let #arg_name = if #arg_name.is_null() {
                                    JObject::null()
                                } else {
                                    jni_unwrap!((*#arg_name).to_java(&env))
                                };
                            }
                        }
                    }
                }
                _ => {
                    quote! {
                        let #arg_name = jni_unwrap!(#arg_name.to_java(&env));
                    }
                }
            };

            arg_java_ty.push(unwrap!(rust_ty_to_signature(&arg.ty, context)));
            stmts.push(stmt);
        }
    }

    let arg_ty_str = format!(
        "{}",
        TypeSignature {
            args: arg_java_ty,
            ret: JavaType::Primitive(signature::Primitive::Void),
        }
    );

    JniCallback {
        args,
        stmts,
        jni_cb_inputs,
        arg_ty_str,
    }
}

fn generate_multi_jni_callback(
    cb: &syn::TypeBareFn,
    cb_name: &str,
    callback_index: usize,
    callbacks_count: usize,
    context: &mut Context,
) -> String {
    let cb_name = syn::Ident::new(cb_name, Span::call_site());

    let JniCallback {
        args,
        jni_cb_inputs,
        stmts,
        arg_ty_str,
    } = generate_callback(cb, context);

    let tokens = quote! {
        extern "C" fn #cb_name(ctx: *mut c_void, #(#jni_cb_inputs),*) {
            unsafe {
                let guard = jni_unwrap!(EnvGuard::new(JVM.as_ref()));
                let env = guard.env();

                let mut cbs = Box::from_raw(ctx as *mut [Option<GlobalRef>; #callbacks_count]);

                if let Some(cb) = cbs[#callback_index].take() {
                    #(#stmts);*

                    jni_unwrap!(env.call_method(
                        cb.as_obj(),
                        "call",
                        #arg_ty_str,
                        &[ #(#args),* ],
                    ));
                }
            }
        }
    };

    tokens.to_string()
}

/// Generates a JNI callback function based on a native callback type
pub fn generate_jni_callback(cb: &syn::TypeBareFn, cb_name: &str, context: &mut Context) -> String {
    let cb_name = syn::Ident::new(cb_name, Span::call_site());

    let JniCallback {
        args,
        jni_cb_inputs,
        stmts,
        arg_ty_str,
    } = generate_callback(cb, context);

    let tokens = quote! {
        extern "C" fn #cb_name(ctx: *mut c_void, #(#jni_cb_inputs),*) {
            unsafe {
                let guard = jni_unwrap!(EnvGuard::new(JVM.as_ref()));
                let env = guard.env();

                let cb = jni_unwrap!(convert_cb_from_java(&env, ctx));

                #(#stmts);*

                jni_unwrap!(env.call_method(
                    cb.as_obj(),
                    "call",
                    #arg_ty_str,
                    &[ #(#args),* ],
                ));
            }
        }
    };

    tokens.to_string()
}

fn generate_struct_to_java(
    struct_ident: &syn::Ident,
    java_class_name: &str,
    fields: &[StructField],
    context: &Context,
) -> proc_macro2::TokenStream {
    let mut stmts = Vec::new();

    for f in fields {
        let field_name_str = f.name();
        let field_name = syn::Ident::new(field_name_str.as_str(), Span::call_site());
        let java_field_name = field_name_str.to_camel_case();

        let stmt = match *f {
            StructField::Array {
                ref len_field,
                ref field,
                ..
            } => {
                if let syn::Type::Ptr(ref ptr) = field.ty {
                    let len_field_ident =
                        syn::Ident::new(len_field.clone().as_str(), Span::call_site());
                    let len_field = len_field.to_camel_case();
                    let ty = &*ptr.elem;
                    let ty_str = format!("{}", quote! {#ty});
                    if ty_str.as_str() == "u8" || ty_str.as_str() == "i8" {
                        // Byte array
                        quote! {
                            let arr = env.new_byte_array(
                                self.#len_field_ident as jni::sys::jsize
                            )?;
                            let slice = unsafe {
                                slice::from_raw_parts(
                                    self.#field_name as *const i8,
                                    self.#len_field_ident
                                )
                            };
                            env.set_byte_array_region(arr, 0, slice)?;
                            let jobj = JObject::from(arr);
                            env.set_field(
                                output,
                                #java_field_name,
                                "[B",
                                jobj.into()
                            )?;
                            env.delete_local_ref(jobj)?;
                            env.set_field(
                                output,
                                #len_field,
                                "J",
                                self.#len_field_ident.to_java(env)?.into()
                            )?;
                        }
                    } else {
                        let full_ty = unwrap!(rust_ty_to_signature(&*ptr.elem, context));

                        // Extract the class name in form of 'java/lang/Abc' instead of 'Ljava/lang/Abc;'.
                        let full_ty_str = if let JavaType::Object(ref obj_name) = full_ty {
                            obj_name.to_string()
                        } else {
                            format!("{}", full_ty)
                        };

                        let arr_signature =
                            format!("{}", JavaType::Array(Box::new(full_ty.clone()),));

                        // Struct array
                        quote! {
                            let cls = unsafe { find_class(env, #full_ty_str)? };
                            let arr = env.new_object_array(
                                self.#len_field_ident as jni::sys::jsize,
                                &cls,
                                JObject::null()
                            )?;
                            let items = unsafe {
                                slice::from_raw_parts(self.#field_name, self.#len_field_ident)
                            };
                            for (idx, item) in items.iter().enumerate() {
                                let jobj = item.to_java(env)?;
                                env.set_object_array_element(
                                    arr,
                                    idx as jni::sys::jsize,
                                    jobj
                                )?;
                                env.delete_local_ref(jobj)?;
                            }
                            let jobj = JObject::from(arr);
                            env.set_field(
                                output,
                                #java_field_name,
                                #arr_signature,
                                jobj.into()
                            )?;
                            env.delete_local_ref(jobj)?;
                            env.set_field(
                                output,
                                #len_field,
                                "J",
                                self.#len_field_ident.to_java(env)?.into()
                            )?;
                        }
                    }
                } else {
                    quote! {}
                }
            }
            StructField::String(ref _f) => {
                quote! {
                    if !self.#field_name.is_null() {
                        let #field_name: JObject = self.#field_name.to_java(env)?.into();
                        env.set_field(
                            output,
                            #java_field_name,
                            "Ljava/lang/String;",
                            #field_name.into()
                        )?;
                        env.delete_local_ref(#field_name)?;
                    }
                }
            }
            StructField::StructPtr { ref ty, .. } => {
                let signature = format!("{}", unwrap!(rust_ty_to_signature(&*ty.elem, context)));

                quote! {
                    let jobj = self.#field_name.to_java(env)?;
                    env.set_field(
                        output,
                        #field_name_str,
                        #signature,
                        jobj.into()
                    )?;
                    env.delete_local_ref(jobj)?;
                }
            }
            StructField::LenField(ref _f) => {
                // Skip len/cap fields transformation - it's covered by `ArrayField`
                quote! {}
            }
            StructField::Primitive(ref f) => match f.ty {
                syn::Type::Path(ref path) => {
                    let ty = unwrap!(path.path.segments.last()).into_value();
                    let ty = ty.ident.to_owned().to_string();
                    let conv = rust_ty_to_java(ty.as_str())
                        .unwrap_or_else(|| lookup_object_type(ty.as_str(), context));
                    let signature = format!("{}", conv);
                    let del_ref = if let JavaType::Object(..) = conv {
                        quote! {
                            env.delete_local_ref(jobj)?;
                        }
                    } else {
                        quote! {}
                    };
                    quote! {
                        let jobj = self.#field_name.to_java(env)?;
                        env.set_field(
                            output,
                            #java_field_name,
                            #signature,
                            jobj.into()
                        )?;
                        #del_ref
                    }
                }
                _ => quote! {},
            },
        };

        stmts.push(stmt);
    }

    let fully_qualified_name = fully_qualified(java_class_name, context);

    quote! {
        impl<'a> ToJava<'a, JObject<'a>> for #struct_ident {
            fn to_java(&self, env: &'a JNIEnv) -> Result<JObject<'a>, JniError> {
                let cls = unsafe { find_class(env, #fully_qualified_name)? };
                let output = env.new_object(&cls, "()V", &[])?;
                #(#stmts)*
                Ok(output)
            }
        }
    }
}

fn generate_struct_from_java(
    struct_ident: &syn::Ident,
    fields: &[StructField],
    context: &Context,
) -> proc_macro2::TokenStream {
    let mut fields_values = Vec::new();
    let mut conversions = Vec::new();

    for f in fields {
        let field_name_str: &str = &f.name();
        let field_name = syn::Ident::new(field_name_str, Span::call_site());
        let java_field_name = field_name_str.to_string().to_camel_case();

        fields_values.push(quote! {
            #field_name
        });

        let conv = match *f {
            StructField::Array {
                ref len_field,
                ref cap_field,
                ref field,
            } => {
                let len_field = syn::Ident::new(len_field.clone().as_str(), Span::call_site());

                let cap = if let Some(ref cap_field) = *cap_field {
                    // If there's a capacity field in the struct, just get it from the
                    // generated Vec itself.
                    let cap_field = syn::Ident::new(cap_field.clone().as_str(), Span::call_site());
                    quote! {
                        let #cap_field = vec.capacity();
                    }
                } else {
                    quote! {}
                };

                if let syn::Type::Ptr(ref ptr) = field.ty {
                    let ty = &*ptr.elem;
                    let ty_str = format!("{}", quote!(#ty));

                    let ptr_mutability = if ptr.mutability.is_some() {
                        quote! { as_mut_ptr }
                    } else {
                        quote! { as_ptr }
                    };

                    if ty_str.as_str() == "u8" {
                        // Byte array
                        quote! {
                            let arr = env.get_field(
                                input,
                                #java_field_name,
                                "[B"
                            )?.l()?.into_inner() as jni::sys::jbyteArray;
                            let mut vec = env.convert_byte_array(arr)?;
                            let #len_field = vec.len();
                            #cap
                            let #field_name = vec.#ptr_mutability();
                            ::std::mem::forget(vec);
                        }
                    } else {
                        // Struct array
                        let ty = syn::Ident::new(ty_str.as_str(), Span::call_site());
                        let signature = format!(
                            "{}",
                            JavaType::Array(Box::new(unwrap!(rust_ty_to_signature(
                                &*ptr.elem, context
                            ))),)
                        );

                        quote! {
                            let arr = env.get_field(
                                input,
                                #java_field_name,
                                #signature
                            )?.l()?.into_inner() as jni::sys::jarray;
                            let #len_field = env.get_array_length(arr)? as usize;

                            let mut vec = Vec::with_capacity(#len_field);

                            for idx in 0..#len_field {
                                let item = env.get_object_array_element(
                                    arr,
                                    idx as jni::sys::jsize
                                );
                                let item = #ty::from_java(env, item?)?;
                                vec.push(item);
                            }

                            #cap
                            let #field_name = vec.#ptr_mutability();
                            ::std::mem::forget(vec);
                        }
                    }
                } else {
                    quote! {}
                }
            }
            StructField::StructPtr { ref ty, .. } => {
                let typ = &*ty.elem;
                let ty_str = typ.into_token_stream().to_string();
                let signature = format!("{}", unwrap!(rust_ty_to_signature(&typ, context)));

                let ty = syn::Ident::new(ty_str.as_str(), Span::call_site());

                quote! {
                    let #field_name = env.get_field(
                        input,
                        #java_field_name,
                        #signature
                    )?.l()?;
                    let #field_name = #ty::from_java(env, #field_name)?;
                }
            }
            StructField::LenField(ref _f) => {
                // Skip len/cap fields transformation - it's covered by `ArrayField`
                quote! {}
            }
            StructField::String(ref _f) => {
                quote! {
                    let #field_name = env.get_field(input, #java_field_name, "Ljava/lang/String;")?
                        .l()?
                        .into();
                    let #field_name = <*mut _>::from_java(env, #field_name)?;
                }
            }
            StructField::Primitive(ref f) => {
                match f.ty {
                    syn::Type::Path(ref path) => {
                        let ty = unwrap!(path.path.segments.last()).into_value();

                        let mut ty = ty.ident.to_owned().to_string();

                        if let Some(rewrite_ty) = context.type_map.get(ty.as_str()) {
                            // Rewrite type (it could be e.g. a handle)
                            ty = match *rewrite_ty {
                                JavaType::Primitive(Primitive::Long) => "u64".to_string(),
                                _ => ty,
                            };
                        }

                        let rust_ty = syn::Ident::new(ty.as_str(), Span::call_site());

                        let conv = match ty.as_str() {
                            "c_byte" | "i8" | "u8" => Some(("B", quote! { b() })),
                            "c_short" | "u16" | "i16" => Some(("S", quote! { s() })),
                            "c_int" | "u32" | "i32" => Some(("I", quote! { i() })),
                            "c_long" | "u64" | "i64" | "c_usize" | "usize" | "isize" => {
                                Some(("J", quote! { j() }))
                            }
                            "c_bool" | "bool" => Some(("Z", quote! { z() })),
                            _ => None,
                        };

                        if let Some((signature, unwrap_method)) = conv {
                            quote! {
                                let #field_name = env.get_field(
                                    input,
                                    #java_field_name,
                                    #signature
                                )?.#unwrap_method? as #rust_ty;
                            }
                        } else {
                            let obj_sig = format!("{}", lookup_object_type(ty.as_str(), context));
                            quote! {
                                let #field_name = env.get_field(input, #java_field_name, #obj_sig)?
                                    .l()?;
                                let #field_name = #rust_ty::from_java(env, #field_name)?;
                            }
                        }
                    }
                    _ => quote! {},
                }
            }
        };

        conversions.push(conv);
    }

    quote! {
        impl<'a> FromJava<JObject<'a>> for #struct_ident {
            fn from_java(env: &JNIEnv, input: JObject) -> Result<Self, JniError> {
                #(#conversions)*

                Ok(#struct_ident {
                    #(#fields_values),*
                })
            }
        }
    }
}

/// Generates JNI struct binding based on a native struct
pub fn generate_struct(
    fields: &[StructField],
    native_name: &str,
    java_class_name: &str,
    context: &Context,
) -> String {
    let struct_ident = syn::Ident::new(native_name, Span::call_site());

    let from_java = generate_struct_from_java(&struct_ident, fields, context);
    let to_java = generate_struct_to_java(&struct_ident, java_class_name, fields, context);

    let tokens = quote! {
        #from_java

        #to_java
    };

    tokens.to_string()
}

#[cfg(test)]
mod tests {
    use super::{generate_callback, transform_jni_arg};
    use crate::java::jni::generate_jni_function;
    use crate::java::Context;
    use indoc::indoc;
    use std::collections::HashMap;
    use unwrap::unwrap;

    // TODO: add more test cases
    #[test]
    fn callback_generation_app_ctx() {
        let ctx = Context::default();
        let rust_cb: syn::TypeBareFn = unwrap!(syn::parse_str("extern fn (app: *const App)"));

        let cb = generate_callback(&rust_cb, &ctx);
        assert_eq!("let app = app as jlong ;", cb.stmts[0].to_string());
    }

    #[test]
    fn jni_arg_transformation() {
        let rust_to_jni = [
            // Primitive types
            ("x: c_char", "x : jbyte"),
            ("x: u8", "x : jbyte"),
            ("x: libc::c_short", "x : jshort"),
            ("x: u16", "x : jshort"),
            ("x: libc::c_int", "x : jint"),
            ("x: u32", "x : jint"),
            ("x: libc::c_long", "x : jlong"),
            ("x: u64", "x : jlong"),
            // String types
            ("x: *const c_char", "x : JString"),
            ("x: *mut c_char", "x : JString"),
            // Object types
            ("x: *mut Foo", "x : JObject"),
            ("x: *const Bar", "x : JObject"),
            // Opaque pointers
            ("x: *mut App", "x : jlong"),
            ("x: *const App", "x : jlong"),
            ("x: *mut Authenticator", "x : jlong"),
            ("x: *const Authenticator", "x : jlong"),
            // Callback
            (
                "x: extern \"C\" fn(user_data: *const c_void)",
                "x : JObject",
            ),
        ];

        for &(rust_code, expected_jni_code) in &rust_to_jni {
            let jni_code = match unwrap!(syn::parse_str(rust_code)) {
                syn::FnArg::Captured(ref arg) => transform_jni_arg(arg),
                x => panic!("unexpected parse result {:?}", x),
            };
            assert_eq!(
                jni_code.to_string(),
                expected_jni_code,
                "unexpected output for '{}'",
                rust_code
            );
        }
    }

    #[test]
    fn check_gen_jni_function_namespace_and_cfg() {
        let func_name = "testDummyFuncName";
        let native_name = "test_dummy_func_name";
        let mut ctx = Context::default();
        let mut dummy_outputs = HashMap::new();
        let dummy_func_str = indoc!(
        "/// Comments are dumb, we don't need them here.
        #[cfg(feature = \"mock-routing\")]
        #[no_mangle]
        pub unsafe extern \"C\" fn test_dummy_func_name(
            dummy_app: *mut App,
            dummy_user_data: *mut c_void,
            dummy_o_cb: extern \"C\" fn(dummy_user_data: *mut c_void, dummy_result: *const FfiResult),
        ) {}"
        );
        let expected_jni_str = "# [ cfg ( feature = \"mock-routing\" ) ]\n\
        # [ no_mangle ] \
        pub unsafe extern \"system\" fn Java_net_maidsafe_dummy_NativeBindings_testDummyFuncName ( \
            env : JNIEnv , \
            _class : JClass , \
            dummy_app : jlong , \
            dummy_user_data : JObject , \
            dummy_o_cb : JObject \
        ) { \
        let dummy_app = dummy_app as * mut App ; \
        let dummy_user_data = jni_unwrap ! ( c_void :: from_java ( & env , dummy_user_data ) ) ; \
        let ctx = gen_ctx ! ( env , dummy_o_cb ) ; \
        test_dummy_func_name ( dummy_app , & dummy_user_data , ctx , call_CallbackVoidFfiResult ) ; }"
        ;
        let file: syn::File = unwrap!(syn::parse_str(dummy_func_str));
        for item in file.items {
            if let syn::Item::Fn(ref func) = item {
                let inputs: Vec<_> = func.decl.inputs.iter().cloned().collect();
                let generated_jni_string = generate_jni_function(
                    inputs.as_slice(),
                    &func.attrs[..],
                    native_name,
                    func_name,
                    &mut ctx,
                    &mut dummy_outputs,
                )
                .clone();
                assert_eq!(expected_jni_str, &generated_jni_string);
            }
        }
    }

    #[test]
    fn check_gen_jni_function_primitive_and_opaque_types() {
        let func_name = "testDummyFuncName";
        let native_name = "test_dummy_func_name";
        let mut ctx = Context::default();
        let mut dummy_outputs = HashMap::new();
        let dummy_func_str = indoc!(
            "pub unsafe extern \"C\" fn test_dummy_func_name(
                app: *const App,
                dummy_int_type: usize,
                dummy_int_type1: u64,
                dummy_int_type2: u32,
                dummy_int_type3: u16,
                dummy_char_type: *const c_char,
                dummy_c_type: libc::c_long,
                dummy_c_type2: libc::c_short,
                dummy_c_type3: libc::c_int,
                dummy_opaque_type: *const MDataInfo,
                dummy_opaque_type2: SignPubKeyHandle,
                dummy_opaque_type3: FileContextHandle,
        ) {}"
        );
        let expected_jni_str = "# [ no_mangle ] \
        pub unsafe extern \"system\" fn Java_net_maidsafe_dummy_NativeBindings_testDummyFuncName ( \
            env : JNIEnv , \
            _class : JClass , \
             app : jlong , \
             dummy_int_type : jlong , \
             dummy_int_type1 : jlong , \
             dummy_int_type2 : jint , \
             dummy_int_type3 : jshort , \
             dummy_char_type : JString , \
             dummy_c_type : jlong , \
             dummy_c_type2 : jshort , \
             dummy_c_type3 : jint , \
             dummy_opaque_type : JObject , \
             dummy_opaque_type2 : SignPubKeyHandle , \
             dummy_opaque_type3 : FileContextHandle \
         ) { \
         let app = app as * mut App ; \
         let dummy_char_type = jni_unwrap ! ( CString :: from_java ( & env , dummy_char_type ) ) ; \
         let dummy_opaque_type = jni_unwrap ! ( MDataInfo :: from_java ( & env , dummy_opaque_type ) ) ; \
          test_dummy_func_name ( app , \
                                 dummy_int_type as usize , \
                                 dummy_int_type1 as u64 , \
                                 dummy_int_type2 as u32 , \
                                 dummy_int_type3 as u16 , \
                                 dummy_char_type . as_ptr ( ) , \
                                 dummy_c_type as libc :: c_long , \
                                 dummy_c_type2 as libc :: c_short , \
                                 dummy_c_type3 as libc :: c_int , \
                                 & dummy_opaque_type , \
                                 dummy_opaque_type2 as SignPubKeyHandle , \
                                 dummy_opaque_type3 as FileContextHandle ) ; }";
        let file: syn::File = unwrap!(syn::parse_str(dummy_func_str));
        for item in file.items {
            if let syn::Item::Fn(ref func) = item {
                let inputs: Vec<_> = func.decl.inputs.iter().cloned().collect();
                let generated_jni_string = generate_jni_function(
                    inputs.as_slice(),
                    &func.attrs[..],
                    native_name,
                    func_name,
                    &mut ctx,
                    &mut dummy_outputs,
                );
                assert_eq!(expected_jni_str, &generated_jni_string);
            }
        }
    }
}
