//! Utilities for emiting fragments of the target language code.
use super::intermediate::*;
use super::Context;
use crate::output::IndentedWriter;
use inflector::Inflector;
use std::fmt::Write;
use unwrap::unwrap;

const LEN_TYPE: &str = "UIntPtr";
const LEN_ZERO: &str = "UIntPtr.Zero";

macro_rules! emit {
    ($writer:expr, $($arg:tt)*) => {
        unwrap!(write!($writer, $($arg)*))
    }
}
macro_rules! emitln {
    ($writer:expr, $($arg:tt)*) => {
        unwrap!(writeln!($writer, $($arg)*))
    }
}

pub fn emit_wrapper_function_decl(
    writer: &mut IndentedWriter,
    context: &Context,
    modifiers: &str,
    name: &str,
    fun: &Function,
) {
    if !modifiers.is_empty() {
        emit!(writer, "{} ", modifiers);
    }

    if let Some(callback) = extract_first_callback(&fun.inputs) {
        emit_task(writer, context, &callback.inputs);
        emit!(writer, " {}Async(", name.to_pascal_case());
    } else {
        emit_type(writer, context, &fun.output, Mode::WrapperFunc);
        emit!(writer, " {}(", name.to_pascal_case());
    }

    emit_wrapper_function_params(writer, context, &fun.inputs, true);
    emit!(writer, ")");
}

pub fn emit_wrapper_function(
    writer: &mut IndentedWriter,
    context: &Context,
    name: &str,
    fun: &Function,
) {
    let callback = extract_first_callback(&fun.inputs);
    let mut has_return = false;
    // TODO: make sure this doesn't conflict with any arguments.
    let return_name = "ret";

    emit_wrapper_function_decl(writer, context, "public", name, fun);
    emitln!(writer, "\n{{");
    writer.indent();

    // Convert wrapper structs to native structs.
    for &(ref name, ref ty) in &fun.inputs {
        if context.is_native_type(ty) {
            emitln!(writer, "var {0}Native = {0}.ToNative();", name);
        }
    }

    if let Some(callback) = callback {
        emit!(
            writer,
            "var ({}, userData) = {}.PrepareTask",
            return_name,
            &context.utils_section.class
        );
        emit_task_generic_args(writer, context, &callback.inputs);
        emitln!(writer, "();");
        has_return = true;
    } else {
        match fun.output {
            Type::Unit => (),
            _ => {
                emit!(writer, "var {} = ", return_name);
                has_return = true;
            }
        }
    }

    emit!(writer, "{}(", extern_function_name(name));

    for (index, &(ref name, ref ty)) in fun.inputs.iter().enumerate() {
        if index > 0 {
            emit!(writer, ", ");
        }

        if let Some(callback) = extract_callback(ty) {
            emit!(writer, "DelegateOn");
            emit_callback_wrapper_name(writer, callback);
        } else {
            let name = param_name(name, index);

            match *ty {
                Type::Array(_, ArraySize::Dynamic) => emit!(
                    writer,
                    "{0}?.ToArray(), ({1})({0}?.Count ?? 0)",
                    name,
                    LEN_TYPE
                ),
                Type::Pointer(ref ty) => {
                    emit_pointer_use(writer, context, ty, &name.to_camel_case(), Mode::ExternFunc)
                }
                _ => emit!(writer, "{}", name),
            }

            if context.is_native_type(ty) {
                emit!(writer, "Native");
            }
        }
    }

    emitln!(writer, ");");

    // Free the native structs.
    for &(ref name, ref ty) in &fun.inputs {
        if context.is_native_type(ty) {
            emitln!(writer, "{0}Native.Free();", name);
        }
    }

    if has_return {
        emitln!(writer, "return {};", return_name);
    }

    writer.unindent();
    emitln!(writer, "}}\n");
}

pub fn emit_function_extern_decl(
    writer: &mut IndentedWriter,
    context: &Context,
    native_name: &str,
    fun: &Function,
) {
    let name = extern_function_name(native_name);

    emitln!(
        writer,
        "[DllImport(DllName, EntryPoint = \"{}\")]",
        native_name
    );
    emit!(writer, "private static extern ");
    emit_type(writer, context, &fun.output, Mode::ExternFunc);
    emit!(writer, " {}(", name);
    emit_native_function_params(writer, context, &fun.inputs);
    emitln!(writer, ");\n");
}

pub fn emit_callback_delegate(writer: &mut IndentedWriter, context: &Context, callback: &Function) {
    emit!(writer, "private delegate void ");
    emit_callback_wrapper_name(writer, callback);
    emit!(writer, "(");
    emit_callback_params(writer, context, &callback.inputs);
    emitln!(writer, ");\n");
}

pub fn emit_callback_wrapper(writer: &mut IndentedWriter, context: &Context, callback: &Function) {
    emitln!(writer, "#if __IOS__");
    emit!(writer, "[MonoPInvokeCallback(typeof(");
    emit_callback_wrapper_name(writer, callback);
    emitln!(writer, "))]");
    emitln!(writer, "#endif");

    emit!(writer, "private static void On");
    emit_callback_wrapper_name(writer, callback);
    emit!(writer, "(");
    emit_callback_params(writer, context, &callback.inputs);
    emitln!(writer, ")\n{{");
    writer.indent();

    emit!(writer, "{}.CompleteTask(", &context.utils_section.class);
    emit_args(writer, context, &callback.inputs[0..2], 0, Mode::Callback);

    if callback.inputs.len() > 2 {
        emit!(writer, ", () => ");

        if callback.inputs.len() > 3 {
            emit!(writer, "(");
        }

        emit_args(writer, context, &callback.inputs[2..], 2, Mode::Callback);

        if callback.inputs.len() > 3 {
            emit!(writer, ")");
        }
    }

    emitln!(writer, ");");

    writer.unindent();
    emitln!(writer, "}}\n");
    emit!(writer, "private static readonly ");
    emit_callback_wrapper_name(writer, callback);
    emit!(writer, " DelegateOn");
    emit_callback_wrapper_name(writer, callback);
    emit!(writer, " = On");
    emit_callback_wrapper_name(writer, callback);
    emitln!(writer, ";\n");
}

pub fn emit_callback_wrapper_name(writer: &mut IndentedWriter, callback: &Function) {
    emit_delegate_base_name(writer, callback);
    emit!(writer, "Cb");
}

pub fn emit_const(writer: &mut IndentedWriter, context: &Context, name: &str, item: &Const) {
    emit!(writer, "public ");

    match item.value {
        ConstValue::Array(..) | ConstValue::Struct(..) => emit!(writer, "static readonly "),
        _ => emit!(writer, "const "),
    }

    emit_type(writer, context, &item.ty, Mode::Const);
    emit!(writer, " {} = ", name.to_pascal_case());
    emit_const_value(writer, context, Some(&item.ty), &item.value);
    emitln!(writer, ";");
}

pub fn emit_enum(writer: &mut IndentedWriter, context: &Context, name: &str, item: &Enum) {
    emitln!(writer, "public enum {}\n{{", name);
    writer.indent();

    for variant in &item.variants {
        emit_docs(writer, context, &variant.docs);

        if let Some(value) = variant.value {
            emitln!(writer, "{} = {},", variant.name, value);
        } else {
            emitln!(writer, "{},", variant.name);
        }
    }

    writer.unindent();
    emitln!(writer, "}}\n");
}

pub fn emit_normal_struct(
    writer: &mut IndentedWriter,
    context: &Context,
    name: &str,
    item: &Struct,
) {
    emitln!(writer, "public struct {}\n{{", name);
    writer.indent();

    for field in &item.fields {
        emit_docs(writer, context, &field.docs);
        emit_struct_field(writer, context, field, StructMode::Normal);
    }

    writer.unindent();
    emitln!(writer, "}}\n");
}

pub fn emit_native_struct(
    writer: &mut IndentedWriter,
    context: &Context,
    name: &str,
    item: &Struct,
) {
    emitln!(writer, "internal struct {}Native\n{{", name);
    writer.indent();

    for field in &item.fields {
        emit_docs(writer, context, &field.docs);
        emit_struct_field(writer, context, field, StructMode::Normal);
    }

    // Emit `Free` method.
    emitln!(writer, "");
    emitln!(writer, "internal void Free()\n{{");
    writer.indent();

    for field in &item.fields {
        let name = field.name.to_pascal_case();

        if field.ty.is_dynamic_array() {
            emitln!(
                writer,
                "{0}.FreeList(ref {1}Ptr, ref {1}Len);",
                context.utils_section.class,
                name
            );
        } else if context.is_native_type(&field.ty) {
            emitln!(writer, "{}.Free();", name)
        }
    }

    writer.unindent();
    emitln!(writer, "}}");

    writer.unindent();
    emitln!(writer, "}}\n");
}

pub fn emit_wrapper_struct(
    writer: &mut IndentedWriter,
    context: &Context,
    name: &str,
    item: &Struct,
) {
    emitln!(writer, "public struct {}\n{{", name);
    writer.indent();

    for field in &item.fields {
        emit_struct_field(writer, context, field, StructMode::Wrapper);
    }

    emitln!(writer, "");

    // Emit constructor.
    emitln!(writer, "internal {0}({0}Native native)\n{{", name);
    writer.indent();

    for field in &item.fields {
        let name = field.name.to_pascal_case();

        emit!(writer, "{} = ", name);

        if let Type::Array(ref ty, ArraySize::Dynamic) = field.ty {
            emit_copy_to_utility_name(writer, context, ty, "List");
            emitln!(writer, "(native.{0}Ptr, (int)native.{0}Len);", name);
        } else if context.is_native_type(&field.ty) {
            emit!(writer, "new ");
            emit_type(writer, context, &field.ty, Mode::WrapperStruct);
            emitln!(writer, "(native.{0});", name);
        } else {
            emitln!(writer, "native.{};", name)
        }
    }

    writer.unindent();
    emitln!(writer, "}}\n");

    // Emit `ToNative` method.
    emitln!(writer, "internal {}Native ToNative()\n{{", name);
    writer.indent();

    emitln!(writer, "return new {}Native\n{{", name);
    writer.indent();

    for (index, field) in item.fields.iter().enumerate() {
        let name = field.name.to_pascal_case();

        if let Type::Array(ref ty, ArraySize::Dynamic) = field.ty {
            emit!(writer, "{}Ptr = ", name);
            emit_copy_from_utility_name(writer, context, ty);
            emitln!(writer, "({}),", name);
            emit!(writer, "{0}Len = ({1})({0}?.Count ?? 0)", name, LEN_TYPE);

            if field.has_cap {
                emitln!(writer, ",");
                emit!(writer, "{}Cap = {}", name, LEN_ZERO);
            }
        } else if context.is_native_type(&field.ty) {
            emit!(writer, "{0} = {0}.ToNative()", name);
        } else {
            emit!(writer, "{0} = {0}", name);
        }

        if index < item.fields.len() - 1 {
            emitln!(writer, ",");
        } else {
            emitln!(writer, "")
        }
    }

    writer.unindent();
    emitln!(writer, "}};");

    writer.unindent();
    emitln!(writer, "}}");

    writer.unindent();
    emitln!(writer, "}}\n");
}

pub fn emit_utilities(writer: &mut IndentedWriter, context: &Context) {
    let content = include_str!("../../resources/csharp/Utils.cs.template");
    let content = content.replace("@Namespace", &context.utils_section.namespace);
    let content = content.replace("@Class", &context.utils_section.class);

    emit!(writer, "{}", content);
}

pub fn emit_docs(writer: &mut IndentedWriter, context: &Context, docs: &str) {
    if context.preserve_comments {
        emit!(writer, "{}", docs);
    }
}

fn extern_function_name(name: &str) -> String {
    let mut name = name.to_pascal_case();
    name.push_str("Native");
    name
}

fn emit_task(writer: &mut IndentedWriter, context: &Context, params: &[(String, Type)]) {
    emit!(writer, "Task");
    emit_task_generic_args(writer, context, params);
}

fn emit_task_generic_args(
    writer: &mut IndentedWriter,
    context: &Context,
    params: &[(String, Type)],
) {
    // Note: assuming here the first param is user_data and the second is result.

    if params.len() <= 2 {
        return;
    }

    emit!(writer, "<");

    if params.len() > 3 {
        emit!(writer, "(");
    }

    for (index, &(_, ref ty)) in params[2..].iter().enumerate() {
        if index > 0 {
            emit!(writer, ", ");
        }

        emit_type(writer, context, ty, Mode::Generic);
    }

    if params.len() > 3 {
        emit!(writer, ")");
    }

    emit!(writer, ">");
}

fn emit_const_value(
    writer: &mut IndentedWriter,
    context: &Context,
    ty: Option<&Type>,
    value: &ConstValue,
) {
    match *value {
        ConstValue::Bool(true) => emit!(writer, "true"),
        ConstValue::Bool(false) => emit!(writer, "false"),
        ConstValue::Char(value) => emit!(writer, "{:?}", value),
        ConstValue::Int(value) => emit!(writer, "{}", value),
        ConstValue::Float(ref value) => emit!(writer, "{}", value),
        ConstValue::String(ref value) => emit!(writer, "{:?}", value),
        ConstValue::Array(ref elements) => {
            if let Some(&Type::Array(ref ty, ..)) = ty {
                emit!(writer, "new ");
                emit_type(writer, context, ty, Mode::Const);
                emit!(writer, "[] ");
            }

            emit!(writer, "{{ ");

            for (index, element) in elements.iter().enumerate() {
                if index > 0 {
                    emit!(writer, ", ");
                }

                emit_const_value(writer, context, None, element);
            }

            emit!(writer, " }}");
        }
        ConstValue::Struct(ref name, ref fields) => {
            emit!(writer, "new {} {{ ", name);

            for (index, (name, value)) in fields.iter().enumerate() {
                if index > 0 {
                    emit!(writer, ", ");
                }

                emit!(writer, "{} = ", name.to_camel_case());
                emit_const_value(writer, context, None, value);
            }

            emit!(writer, " }}");
        }
    }
}

fn emit_const_use(writer: &mut IndentedWriter, context: &Context, name: &str) {
    emit!(
        writer,
        "{}.{}",
        context.consts_section.class,
        name.to_pascal_case()
    );
}

fn emit_struct_field(
    writer: &mut IndentedWriter,
    context: &Context,
    field: &StructField,
    mode: StructMode,
) {
    let name = field.name.to_pascal_case();

    if field.ty.is_dynamic_array() && mode == StructMode::Normal {
        emitln!(writer, "public IntPtr {}Ptr;", name);
        emitln!(writer, "public {} {}Len;", LEN_TYPE, name);

        if field.has_cap {
            emitln!(writer, "public {} {}Cap;", LEN_TYPE, name);
        }
    } else {
        if mode == StructMode::Normal {
            emit_marshal_as(writer, context, &field.ty, None, "\n");
        }

        emit!(writer, "public ");
        emit_type(writer, context, &field.ty, mode.into());
        emitln!(writer, " {};", name);
    }
}

fn emit_wrapper_function_params(
    writer: &mut IndentedWriter,
    context: &Context,
    params: &[(String, Type)],
    skip_user_data: bool,
) {
    let mut index = 0;
    for &(ref name, ref ty) in params {
        // Skip the user data pointer.
        if skip_user_data && is_user_data(name, ty) {
            continue;
        }

        // Skip callbacks.
        if extract_callback(ty).is_some() {
            continue;
        }

        if index > 0 {
            emit!(writer, ", ");
        }

        emit_type(writer, context, ty, Mode::WrapperFunc);
        if name.is_empty() {
            emit!(writer, " arg{}", index);
        } else {
            emit!(writer, " {}", name.to_camel_case());
        }

        index += 1;
    }
}

#[allow(clippy::explicit_counter_loop)]
fn emit_native_function_params(
    writer: &mut IndentedWriter,
    context: &Context,
    params: &[(String, Type)],
) {
    let mut index = 0;
    for &(ref name, ref ty) in params {
        if index > 0 {
            emit!(writer, ", ");
        }

        emit_marshal_as(writer, context, ty, Some(index), " ");

        if let Some(callback) = extract_callback(ty) {
            emit_callback_wrapper_name(writer, callback);
        } else {
            emit_type(writer, context, ty, Mode::ExternFunc);
        }

        let name = param_name(name, index);
        emit!(writer, " {}", name);

        if ty.is_dynamic_array() {
            emit!(writer, ", {} {}Len", LEN_TYPE, name);
        }

        index += 1;
    }
}

fn emit_callback_params(writer: &mut IndentedWriter, context: &Context, params: &[(String, Type)]) {
    for (index, &(ref name, ref ty)) in params.iter().enumerate() {
        if index > 0 {
            emit!(writer, ", ");
        }

        let name = param_name(name, index);

        if let Type::Array(_, ArraySize::Dynamic) = *ty {
            emit!(writer, "IntPtr {0}Ptr, {1} {0}Len", name, LEN_TYPE);
        } else {
            emit_type(writer, context, ty, Mode::Callback);
            emit!(writer, " {}", name);
        }
    }
}

fn param_name(name: &str, index: usize) -> String {
    if name.is_empty() {
        format!("arg{}", index)
    } else {
        name.to_camel_case()
    }
}

fn emit_marshal_as(
    writer: &mut IndentedWriter,
    context: &Context,
    ty: &Type,
    index: Option<usize>,
    append: &str,
) {
    if let Some(unmanaged) = unmanaged_type(ty, index.is_none()) {
        emit!(writer, "[MarshalAs(UnmanagedType.{}", unmanaged);

        match *ty {
            Type::Array(ref ty, ref size) => {
                emit_array_marshal_as(writer, context, ty, size, index)
            }
            Type::Pointer(ref ty) => {
                if let Type::Array(ref ty, ref size) = **ty {
                    emit_array_marshal_as(writer, context, ty, size, index)
                }
            }
            _ => (),
        }

        emit!(writer, ")]{}", append);
    }
}

fn unmanaged_type(ty: &Type, field: bool) -> Option<&str> {
    match *ty {
        Type::Bool => Some("U1"),
        // TODO: consider marshaling as "LPUTF8Str", is possible and useful.
        Type::String => Some("LPStr"),
        Type::Array(_, ArraySize::Dynamic) => Some("LPArray"),
        Type::Array(..) if field => Some("ByValArray"),
        Type::Array(..) => Some("LPArray"),
        Type::Pointer(ref ty) => {
            if let Type::Array(_, _) = **ty {
                Some("LPArray")
            } else {
                None
            }
        }
        _ => None,
    }
}

fn emit_array_marshal_as(
    writer: &mut IndentedWriter,
    context: &Context,
    ty: &Type,
    size: &ArraySize,
    index: Option<usize>,
) {
    if let Some(unmanaged) = unmanaged_type(ty, false) {
        emit!(writer, ", ArraySubType = UnmanagedType.{}", unmanaged);
    }

    match *size {
        ArraySize::Lit(value) => emit!(writer, ", SizeConst = {}", value),
        ArraySize::Const(ref name) => {
            emit!(writer, ", SizeConst = (int)");
            emit_const_use(writer, context, name);
        }
        ArraySize::Dynamic => {
            if let Some(index) = index {
                emit!(writer, ", SizeParamIndex = {}", index + 1)
            }
        }
    }
}

// Mode in which a type or value is emitted.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Mode {
    // Parameter of a wrapper (managed) function.
    WrapperFunc,
    // Parameter of a extern function.
    ExternFunc,
    // Parameter of a callback.
    Callback,
    // Field in normal or native struct.
    NormalStruct,
    // Field in wrapper struct.
    WrapperStruct,
    // Constant definition.
    Const,
    // Generic argument
    Generic,
}

// Mode in which a struct is emitted.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum StructMode {
    Normal,
    Wrapper,
}

impl From<StructMode> for Mode {
    fn from(mode: StructMode) -> Self {
        match mode {
            StructMode::Normal => Mode::NormalStruct,
            StructMode::Wrapper => Mode::WrapperStruct,
        }
    }
}

fn emit_type(writer: &mut IndentedWriter, context: &Context, ty: &Type, mode: Mode) {
    match *ty {
        Type::Unit => emit!(writer, "void"),
        Type::Bool => emit!(writer, "bool"),
        Type::CChar | Type::I8 => emit!(writer, "sbyte"),
        Type::Char => emit!(writer, "char"),
        Type::F32 => emit!(writer, "float"),
        Type::F64 => emit!(writer, "double"),
        Type::I16 => emit!(writer, "short"),
        Type::I32 => emit!(writer, "int"),
        Type::I64 => emit!(writer, "long"),
        Type::ISize => emit!(writer, "IntPtr"),
        Type::U8 => emit!(writer, "byte"),
        Type::U16 => emit!(writer, "ushort"),
        Type::U32 => emit!(writer, "uint"),
        Type::U64 => emit!(writer, "ulong"),
        Type::USize => {
            if mode == Mode::Generic {
                emit!(writer, "ulong")
            } else {
                emit!(writer, "{}", LEN_TYPE)
            }
        }
        Type::String => emit!(writer, "string"),
        Type::Pointer(ref ty) => match **ty {
            Type::Array(ref ty, ref size) => emit_array(writer, context, ty, size, mode),
            Type::User(ref name) => {
                if mode == Mode::Callback
                    || mode == Mode::Const
                    || mode == Mode::NormalStruct
                    || mode == Mode::WrapperStruct
                    || context.is_opaque(name)
                {
                    emit!(writer, "IntPtr")
                } else if mode == Mode::Generic {
                    emit!(writer, "{}", name)
                } else if mode == Mode::ExternFunc && context.is_native_name(name) {
                    emit!(writer, "ref {}Native", name)
                } else {
                    emit!(writer, "ref {}", name)
                }
            }
            Type::Pointer(_) => {
                if mode == Mode::WrapperFunc || mode == Mode::ExternFunc {
                    emit!(writer, "out IntPtr")
                } else {
                    emit!(writer, "IntPtr")
                }
            }
            _ => emit!(writer, "IntPtr"),
        },
        Type::Array(ref ty, ref size) => emit_array(writer, context, ty, size, mode),
        Type::Function(..) => unimplemented!(),
        Type::User(ref name) => {
            if context.is_native_name(name)
                && (mode == Mode::Callback
                    || mode == Mode::Const
                    || mode == Mode::ExternFunc
                    || mode == Mode::NormalStruct)
            {
                emit!(writer, "{}Native", name)
            } else {
                emit!(writer, "{}", name)
            }
        }
    }
}

fn emit_array(
    writer: &mut IndentedWriter,
    context: &Context,
    ty: &Type,
    size: &ArraySize,
    mode: Mode,
) {
    if (mode == Mode::WrapperFunc || mode == Mode::WrapperStruct || mode == Mode::Generic)
        && *size == ArraySize::Dynamic
    {
        emit!(writer, "List<");
        emit_type(writer, context, ty, mode);
        emit!(writer, ">");
    } else if mode == Mode::Callback {
        emit!(writer, "IntPtr");
    } else {
        emit_type(writer, context, ty, mode);
        emit!(writer, "[]")
    }
}

fn emit_args(
    writer: &mut IndentedWriter,
    context: &Context,
    args: &[(String, Type)],
    offset: usize,
    mode: Mode,
) {
    for (index, &(ref name, ref ty)) in args.iter().enumerate() {
        if index > 0 {
            emit!(writer, ", ");
        }

        let name = param_name(name, offset + index);
        match *ty {
            Type::Array(ref ty, ref size) => emit_array_use(writer, context, ty, size, &name),
            Type::Pointer(ref ty) => match **ty {
                Type::Array(ref ty, ref size) => emit_array_use(writer, context, ty, size, &name),
                Type::User(ref type_name) if context.is_native_name(type_name) => {
                    emit!(writer, "new {}(", type_name);
                    emit_pointer_use(writer, context, ty, &name, mode);
                    emit!(writer, ")");
                }
                _ => emit_pointer_use(writer, context, ty, &name, mode),
            },
            Type::User(ref type_name) if context.is_native_name(type_name) => {
                emit!(writer, "new {}({})", type_name, name);
            }
            Type::USize => emit!(writer, "(ulong){}", name),
            _ => emit!(writer, "{}", name),
        }
    }
}

fn emit_pointer_use(
    writer: &mut IndentedWriter,
    context: &Context,
    ty: &Type,
    name: &str,
    mode: Mode,
) {
    match *ty {
        Type::User(ref pointee)
            if mode == Mode::WrapperFunc
                || mode == Mode::ExternFunc && !context.is_opaque(pointee) =>
        {
            emit!(writer, "ref {}", name);
        }
        Type::User(ref pointee) if mode == Mode::Callback && !context.is_opaque(pointee) => {
            emit!(writer, "Marshal.PtrToStructure<{}", pointee);

            if context.is_native_name(pointee) {
                emit!(writer, "Native");
            }

            emit!(writer, ">({})", name);
        }
        Type::Pointer(_) if mode == Mode::WrapperFunc || mode == Mode::ExternFunc => {
            emit!(writer, "out {}", name);
        }
        _ => emit!(writer, "{}", name),
    }
}

fn emit_array_use(
    writer: &mut IndentedWriter,
    context: &Context,
    ty: &Type,
    size: &ArraySize,
    name: &str,
) {
    let (collection, suffix) = if let ArraySize::Dynamic = *size {
        ("List", "Ptr")
    } else {
        ("Array", "")
    };

    emit_copy_to_utility_name(writer, context, ty, collection);
    emit!(writer, "({}{}, ", name, suffix);

    match *size {
        ArraySize::Lit(value) => emit!(writer, "{}", value),
        ArraySize::Const(ref name) => {
            emit!(writer, "(int)");
            emit_const_use(writer, context, name);
        }
        ArraySize::Dynamic => emit!(writer, "(int){}Len", name),
    }

    emit!(writer, ")");

    if let Type::User(ref name) = *ty {
        if context.is_native_name(name) {
            emit!(writer, ".Select(native => new {}(native)).ToList()", name);
        }
    }
}
fn emit_delegate_base_name(writer: &mut IndentedWriter, fun: &Function) {
    if fun.inputs.len() > 1 {
        // Skip the user data param.
        for &(_, ref ty) in &fun.inputs[1..] {
            emit_delegate_base_part_name(writer, ty);
        }
    } else {
        emit!(writer, "None");
    }
}

fn emit_delegate_base_part_name(writer: &mut IndentedWriter, ty: &Type) {
    match *ty {
        Type::Unit => emit!(writer, "Void"),
        Type::Bool => emit!(writer, "Bool"),
        Type::Char => emit!(writer, "Char"),
        Type::F32 => emit!(writer, "Float"),
        Type::F64 => emit!(writer, "Double"),
        Type::I8 | Type::CChar => emit!(writer, "SByte"),
        Type::I16 => emit!(writer, "Short"),
        Type::I32 => emit!(writer, "Int"),
        Type::I64 | Type::ISize => emit!(writer, "Long"),
        Type::U8 => emit!(writer, "Byte"),
        Type::U16 => emit!(writer, "UShort"),
        Type::U32 => emit!(writer, "UInt"),
        Type::U64 | Type::USize => emit!(writer, "ULong"),
        Type::String => emit!(writer, "String"),
        Type::Pointer(ref ty) => emit_delegate_base_part_name(writer, ty),
        Type::Array(ref ty, ref size) => {
            emit_delegate_base_part_name(writer, ty);

            match *size {
                ArraySize::Lit(value) => emit!(writer, "Array{}", value),
                ArraySize::Const(ref name) => emit!(writer, "Array{}", name.to_pascal_case()),
                ArraySize::Dynamic => emit!(writer, "List"),
            }
        }
        Type::User(ref name) => emit!(writer, "{}", name),
        _ => unimplemented!(),
    }
}

fn emit_copy_to_utility_name(
    writer: &mut IndentedWriter,
    context: &Context,
    ty: &Type,
    collection: &str,
) {
    emit!(writer, "{}.CopyTo", context.utils_section.class);
    emit_copy_utility_suffix(writer, context, ty, collection, true);
}

fn emit_copy_from_utility_name(writer: &mut IndentedWriter, context: &Context, ty: &Type) {
    emit!(writer, "{}.CopyFrom", context.utils_section.class);
    emit_copy_utility_suffix(writer, context, ty, "List", false);
}

fn emit_copy_utility_suffix(
    writer: &mut IndentedWriter,
    context: &Context,
    ty: &Type,
    collection: &str,
    add_type: bool,
) {
    match *ty {
        Type::F32 => emit!(writer, "Single{}", collection),
        Type::F64 => emit!(writer, "Double{}", collection),
        Type::I16 => emit!(writer, "Int16{}", collection),
        Type::I32 => emit!(writer, "Int32{}", collection),
        Type::I64 => emit!(writer, "Int64{}", collection),
        Type::U8 => emit!(writer, "Byte{}", collection),
        Type::String => emit!(writer, "String{}", collection),
        Type::User(ref name) => {
            emit!(writer, "Object{}", collection);

            if add_type {
                if context.is_native_name(name) {
                    emit!(writer, "<{}Native>", name);
                } else {
                    emit!(writer, "<{}>", name);
                }
            }
        }
        _ => panic!("cannot emit copy utility name for List of {:?}", ty),
    }
}
