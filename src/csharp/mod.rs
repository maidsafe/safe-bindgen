#[macro_use]
mod emit;
mod intermediate;
#[cfg(test)]
mod tests;
extern crate inflector;
use self::emit::*;
use self::inflector::Inflector;
use self::intermediate::*;
use common::{self, FilterMode, Lang, Outputs};
use output::IndentedWriter;
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::{Display, Write};
use std::mem;
use std::ops::Deref;
use Error;
use Level;

const INDENT_WIDTH: usize = 2;

pub struct LangCSharp {
    filter: HashSet<String>,
    filter_mode: FilterMode,
    wrapper_function_blacklist: HashSet<String>,
    consts_enabled: bool,
    types_enabled: bool,
    utils_enabled: bool,
    context: Context,
    custom_consts: Vec<String>,
    consts: Vec<Snippet<Const>>,
    enums: Vec<Snippet<Enum>>,
    structs: Vec<Snippet<Struct>>,
    functions: Vec<Snippet<Function>>,
    aliases: HashMap<String, Type>,
}

pub struct Context {
    lib_name: String,
    interface_section: Section,
    functions_section: Section,
    consts_section: Section,
    types_section: Section,
    utils_section: Section,
    preserve_comments: bool,
    opaque_types: HashSet<String>,
    native_types: HashSet<String>,
}

impl Context {
    pub fn is_opaque(&self, name: &str) -> bool {
        self.opaque_types.contains(name)
    }

    pub fn is_native_name(&self, name: &str) -> bool {
        self.native_types.contains(name)
    }

    pub fn is_native_type(&self, ty: &Type) -> bool {
        match *ty {
            Type::Pointer(ref ty) => self.is_native_type(&*ty),
            Type::User(ref name) => self.is_native_name(name),
            _ => false,
        }
    }
}

pub struct Section {
    path: String,
    namespace: String,
    class: String,
}

impl Section {
    fn new<P, N, C>(path: P, namespace: N, class: C) -> Self
    where
        P: Into<String>,
        N: Into<String>,
        C: Into<String>,
    {
        Section {
            path: path.into(),
            namespace: namespace.into(),
            class: class.into(),
        }
    }
}

impl LangCSharp {
    pub fn new() -> Self {
        LangCSharp {
            filter_mode: FilterMode::Blacklist,
            filter: Default::default(),
            wrapper_function_blacklist: Default::default(),
            consts_enabled: true,
            types_enabled: true,
            utils_enabled: true,
            context: Context {
                lib_name: "backend".to_string(),
                interface_section: Section::new("IBackend.cs", "Backend", "IBackend"),
                functions_section: Section::new("Backend.cs", "Backend", "Backend"),
                consts_section: Section::new("Constants.cs", "Backend", "Constants"),
                types_section: Section::new("Types.cs", "Backend", ""),
                utils_section: Section::new("Utils.cs", "Backend", "Utils"),
                preserve_comments: false,
                opaque_types: Default::default(),
                native_types: Default::default(),
            },
            custom_consts: Vec::new(),
            consts: Vec::new(),
            enums: Vec::new(),
            structs: Vec::new(),
            functions: Vec::new(),
            aliases: Default::default(),
        }
    }

    /// Set the name of the native library. This also sets the class name.
    pub fn set_lib_name<T: Into<String>>(&mut self, name: T) {
        self.context.lib_name = name.into();
    }

    /// Set path, namespace and interface name of the interface section.
    pub fn set_interface_section<P, N, C>(&mut self, path: P, namespace: N, interface: C)
    where
        P: Into<String>,
        N: Into<String>,
        C: Into<String>,
    {
        self.context.interface_section = Section::new(path, namespace, interface)
    }

    /// Set path, namespace and class name of the functions section.
    pub fn set_functions_section<P, N, C>(&mut self, path: P, namespace: N, class: C)
    where
        P: Into<String>,
        N: Into<String>,
        C: Into<String>,
    {
        self.context.functions_section = Section::new(path, namespace, class)
    }

    /// Enabl/disable generation of constants.
    pub fn set_consts_enabled(&mut self, enabled: bool) {
        self.consts_enabled = enabled;
    }

    /// Set path, namespace and class name of the constants section.
    pub fn set_consts_section<P, N, C>(&mut self, path: P, namespace: N, class: C)
    where
        P: Into<String>,
        N: Into<String>,
        C: Into<String>,
    {
        self.context.consts_section = Section::new(path, namespace, class)
    }

    /// Enabl/disable generation of types.
    pub fn set_types_enabled(&mut self, enabled: bool) {
        self.types_enabled = enabled;
    }

    /// Set path and namespace of the types section.
    pub fn set_types_section<P, N>(&mut self, path: P, namespace: N)
    where
        P: Into<String>,
        N: Into<String>,
    {
        self.context.types_section = Section::new(path, namespace, "")
    }

    /// Enable/disable generation of the utils class.
    pub fn set_utils_enabled(&mut self, enabled: bool) {
        self.utils_enabled = enabled;
    }

    /// Set path, namespace and class name of the utilities section.
    pub fn set_utils_section<P, N, C>(&mut self, path: P, namespace: N, class: C)
    where
        P: Into<String>,
        N: Into<String>,
        C: Into<String>,
    {
        self.context.utils_section = Section::new(path, namespace, class)
    }

    /// Add definition of opaque type (type represented by an opaque pointer).
    pub fn add_opaque_type<T: Into<String>>(&mut self, name: T) {
        let _ = self.context.opaque_types.insert(name.into());
    }

    /// Add constant definition.
    pub fn add_const<T: Display>(&mut self, ty: &str, name: &str, value: T) {
        self.custom_consts.push(format!(
            "public const {} {} = {};",
            ty,
            name.to_pascal_case(),
            value
        ));
    }

    /// Clears the current filter and sets the filter mode.
    pub fn reset_filter(&mut self, filter_mode: FilterMode) {
        self.filter.clear();
        self.filter_mode = filter_mode;
    }

    /// Add the identifier to the filter.
    /// If the filter mode is `Blacklist` (the default), the identifiers in the
    /// filter are ignored.
    /// If it is `Whitelist`, the identifiers not in the filter are ignored.
    pub fn filter<T: Into<String>>(&mut self, ident: T) {
        let _ = self.filter.insert(ident.into());
    }

    /// Do not generate wrapper function for the given function.
    pub fn blacklist_wrapper_function<T: Into<String>>(&mut self, ident: T) {
        let _ = self.wrapper_function_blacklist.insert(ident.into());
    }

    pub fn reset_wrapper_function_blacklist(&mut self) {
        self.wrapper_function_blacklist.clear();
    }

    fn resolve_aliases(&mut self) {
        for snippet in &mut self.consts {
            resolve_alias(&self.aliases, &mut snippet.item.ty);
        }

        for snippet in &mut self.structs {
            if let Some(&Type::User(ref name)) = lookup_alias(&self.aliases, &snippet.name) {
                snippet.name = name.clone();
            }

            for field in &mut snippet.item.fields {
                resolve_alias(&self.aliases, &mut field.ty);
            }
        }

        for snippet in &mut self.functions {
            resolve_alias(&self.aliases, &mut snippet.item.output);

            for &mut (_, ref mut ty) in &mut snippet.item.inputs {
                resolve_alias(&self.aliases, ty)
            }
        }
    }

    fn resolve_native_types(&mut self) {
        let mut run = true;
        while run {
            run = false;

            for snippet in &self.structs {
                // If the struct is already marked as native, proceed to the next one.
                if self.context.is_native_name(&snippet.name) {
                    continue;
                }

                // Otherwise, check it one of its fields is native, and if it is,
                // mark the struct as native and reprocess the whole thing again,
                // to detect structs with newly identified native fields.
                let has_native_fields = snippet.item.fields.iter().any(|field| {
                    field.ty.is_dynamic_array() || self.context.is_native_type(&field.ty)
                });
                if has_native_fields {
                    let _ = self.context.native_types.insert(snippet.name.clone());
                    run = true;
                }
            }
        }
    }

    fn is_ignored(&self, ident: &str) -> bool {
        match self.filter_mode {
            FilterMode::Blacklist => self.filter.contains(ident),
            FilterMode::Whitelist => !self.filter.contains(ident),
        }
    }

    fn is_interface_function(&self, name: &str, item: &Function) -> bool {
        !self.wrapper_function_blacklist.contains(name) && num_callbacks(&item.inputs) <= 1
    }
}

impl Default for LangCSharp {
    fn default() -> Self {
        Self::new()
    }
}

impl Lang for LangCSharp {
    fn parse_ty(
        &mut self,
        item: &syn::ItemType,
        _module: &[String],
        _outputs: &mut Outputs,
    ) -> Result<(), Error> {
        let name = item.ident.to_string();
        if self.is_ignored(&name.as_str()) {
            return Ok(());
        }

        if !item.generics.params.is_empty() {
            println!(
                "parameterized type aliases not supported ({}). Skipping.",
                name,
            );
            return Ok(());
        }

        let ty = transform_type(&*item.ty).ok_or_else(|| Error {
            level: Level::Error,
            span: None, //NONE FOR NOW
            message: format!("bindgen can not handle the type `{}`", name),
        })?;

        self.aliases.insert(name, ty);

        Ok(())
    }

    fn parse_const(
        &mut self,
        item: &syn::ItemConst,
        _module: &[String],
        _outputs: &mut Outputs,
    ) -> Result<(), Error> {
        let name = item.ident.to_string();
        if self.is_ignored(&name.as_str()) {
            return Ok(());
        }

        let docs = common::parse_attr(&item.attrs, |_| true, retrieve_docstring).1;
        let item = transform_const(&*item.ty, &*item.expr).ok_or_else(|| Error {
            level: Level::Error,
            span: None, //NONE FOR NOW
            message: format!("bindgen can not handle constant {}", name),
        })?;

        self.consts.push(Snippet { docs, name, item });

        Ok(())
    }

    fn parse_enum(
        &mut self,
        item: &syn::ItemEnum,
        _module: &[String],
        _outputs: &mut Outputs,
    ) -> Result<(), Error> {
        let name = item.ident.to_string();
        if self.is_ignored(&name.as_str()) {
            return Ok(());
        }

        let (repr_c, docs) =
            common::parse_attr(&item.attrs[..], common::check_repr_c, retrieve_docstring);

        // If it's not #[repr(C)] ignore it.
        if !repr_c {
            return Ok(());
        }

        if !item.generics.params.is_empty() {
            return Err(unsupported_generics_error("enums"));
        }
        let mut var = Vec::new();
        for variant in item.to_owned().variants {
            var.push(variant);
        }
        let item = transform_enum(&var.as_slice()).ok_or_else(|| Error {
            level: Level::Error,
            span: None, //NONE FOR NOW
            message: format!("bindgen can not handle enum {}", item.ident.to_string()),
        })?;

        self.enums.push(Snippet { docs, name, item });

        Ok(())
    }

    fn parse_struct(
        &mut self,
        item: &syn::ItemStruct,
        _module: &[String],
        _outputs: &mut Outputs,
    ) -> Result<(), Error> {
        let name = item.ident.to_string();
        if self.is_ignored(&name) {
            return Ok(());
        }

        let (repr_c, docs) =
            common::parse_attr(&item.attrs[..], common::check_repr_c, retrieve_docstring);

        // If it's not #[repr(C)] ignore it.
        if !repr_c {
            return Ok(());
        }

        if !item.generics.params.is_empty() {
            return Err(unsupported_generics_error("structs"));
        }
        //TODO: syn doesn't have support for StructVariants
        //            if   {
        //                return Err(Error {
        //                    level: Level::Error,
        //                    span: Some(item.span),
        //                    message: format!("bindgen can not handle unit or tuple structs ({})", name),
        //                });
        //            }

        let item = transform_struct(item.to_owned().fields).ok_or_else(|| Error {
            level: Level::Error,
            span: None, //NONE FOR NOW
            message: format!("bindgen can not handle struct {}", item.ident.to_string()),
        })?;
        let name = name.to_string();
        self.structs.push(Snippet { docs, name, item });
        self.resolve_native_types();

        Ok(())
    }

    fn parse_fn(
        &mut self,
        item: &syn::ItemFn,
        _module: &[String],
        _outputs: &mut Outputs,
    ) -> Result<(), Error> {
        let name = item.ident.to_owned().to_string();
        if self.is_ignored(&name.as_str()) {
            return Ok(());
        }
        let (no_mangle, docs) =
            common::parse_attr(&item.attrs, common::check_no_mangle, retrieve_docstring);

        // Ignore function without #[no_mangle].
        if no_mangle {
            return Ok(());
        }

        if !common::is_extern(item.to_owned().abi.unwrap()) {
            return Ok(());
        }
        //TODO: There are no generics in syn's ItemFn
        //            if item {
        //                return Err(unsupported_generics_error(syn::Item::from(item), "extern functions"));
        //            }

        let function: Function = transform_function(*item.to_owned().decl).ok_or_else(|| {
            let string = item.to_owned().ident.to_string();
            Error {
                level: Level::Error,
                span: None, //NONE FOR NOW
                message: format!("bindgen can not handle function {}", string),
            }
        })?;

        self.functions.push(Snippet {
            docs,
            name: name,
            item: function,
        });

        Ok(())
    }

    fn finalise_output(&mut self, outputs: &mut Outputs) -> Result<(), Error> {
        self.resolve_aliases();

        if !self.functions.is_empty() {
            // Functions
            let mut writer = IndentedWriter::new(INDENT_WIDTH);

            emitln!(writer, "using System;");
            emitln!(writer, "using System.Collections.Generic;");
            emitln!(writer, "using System.Linq;");
            emitln!(writer, "using System.Runtime.InteropServices;");
            emitln!(writer, "using System.Threading.Tasks;\n");
            emitln!(
                writer,
                "namespace {} {{",
                self.context.functions_section.namespace
            );
            writer.indent();

            emitln!(
                writer,
                "internal partial class {} : I{} {{",
                self.context.functions_section.class,
                self.context.functions_section.class
            );
            writer.indent();

            // Define constant with the native library name, to be used in
            // the [DllImport] attributes.
            emitln!(writer, "#if __IOS__");
            emitln!(writer, "private const string DllName = \"__Internal\";");
            emitln!(writer, "#else");
            emitln!(
                writer,
                "private const string DllName = \"{}\";",
                self.context.lib_name
            );
            emitln!(writer, "#endif\n");

            for snippet in &self.functions {
                emit_docs(&mut writer, &self.context, &snippet.docs);
                if self.is_interface_function(&snippet.name, &snippet.item) {
                    emit_wrapper_function(&mut writer, &self.context, &snippet.name, &snippet.item);
                }
                emit_function_extern_decl(&mut writer, &self.context, &snippet.name, &snippet.item);
            }

            // Callback delegates and wrappers.
            {
                let callbacks = collect_callbacks(&self.functions);
                if !callbacks.is_empty() {
                    for (callback, single) in callbacks {
                        emit_callback_delegate(&mut writer, &self.context, callback);

                        if single {
                            emit_callback_wrapper(&mut writer, &self.context, callback);
                        }
                    }
                }
            }

            writer.unindent();
            emitln!(writer, "}}");

            writer.unindent();
            emitln!(writer, "}}");

            outputs.insert(
                self.context.functions_section.path.clone(),
                writer.into_inner(),
            );

            // Interface
            let functions: Vec<_> = mem::replace(&mut self.functions, Vec::new());
            let mut functions = functions
                .into_iter()
                .filter(|snippet| self.is_interface_function(&snippet.name, &snippet.item))
                .peekable();

            if functions.peek().is_some() {
                let mut writer = IndentedWriter::new(INDENT_WIDTH);

                emitln!(writer, "using System;");
                emitln!(writer, "using System.Collections.Generic;");
                emitln!(writer, "using System.Threading.Tasks;\n");
                emitln!(
                    writer,
                    "namespace {} {{",
                    self.context.interface_section.namespace
                );
                writer.indent();

                emitln!(
                    writer,
                    "public partial interface {} {{",
                    self.context.interface_section.class
                );
                writer.indent();

                for snippet in functions {
                    if num_callbacks(&snippet.item.inputs) <= 1 {
                        emit_wrapper_function_decl(
                            &mut writer,
                            &self.context,
                            "",
                            &snippet.name,
                            &snippet.item,
                        );
                        emitln!(writer, ";");
                    }
                }

                writer.unindent();
                emitln!(writer, "}}");

                writer.unindent();
                emitln!(writer, "}}");

                outputs.insert(
                    self.context.interface_section.path.clone(),
                    writer.into_inner(),
                );
            }
        }

        // Constants
        if self.consts_enabled && (!self.consts.is_empty() || !self.custom_consts.is_empty()) {
            let mut writer = IndentedWriter::new(INDENT_WIDTH);

            emitln!(writer, "using System;");
            emitln!(writer, "using JetBrains.Annotations;\n");

            emitln!(
                writer,
                "namespace {} {{",
                self.context.consts_section.namespace
            );
            writer.indent();

            emitln!(writer, "[PublicAPI]");
            emitln!(
                writer,
                "public static class {} {{",
                self.context.consts_section.class
            );
            writer.indent();

            for snippet in self.consts.drain(..) {
                emit_docs(&mut writer, &self.context, &snippet.docs);
                emit_const(&mut writer, &self.context, &snippet.name, &snippet.item);
            }

            if !self.custom_consts.is_empty() {
                for decl in self.custom_consts.drain(..) {
                    emitln!(writer, "{}", decl);
                }
            }

            writer.unindent();
            emitln!(writer, "}}");

            writer.unindent();
            emitln!(writer, "}}");

            outputs.insert(
                self.context.consts_section.path.clone(),
                writer.into_inner(),
            );
        }

        // Types
        if self.types_enabled && (!self.enums.is_empty() || !self.structs.is_empty()) {
            let mut writer = IndentedWriter::new(INDENT_WIDTH);

            emitln!(writer, "using System;");
            emitln!(writer, "using System.Collections.Generic;");
            emitln!(writer, "using System.Runtime.InteropServices;");
            emitln!(writer, "using JetBrains.Annotations;\n");

            emitln!(
                writer,
                "namespace {} {{",
                self.context.types_section.namespace
            );
            writer.indent();

            // Enums
            for snippet in self.enums.drain(..) {
                emit_docs(&mut writer, &self.context, &snippet.docs);
                emit_enum(&mut writer, &self.context, &snippet.name, &snippet.item);
            }

            // Structs
            for snippet in &self.structs {
                emit_docs(&mut writer, &self.context, &snippet.docs);

                if self.context.is_native_name(&snippet.name) {
                    emit_wrapper_struct(&mut writer, &self.context, &snippet.name, &snippet.item);
                    emit_native_struct(&mut writer, &self.context, &snippet.name, &snippet.item);
                } else {
                    emit_normal_struct(&mut writer, &self.context, &snippet.name, &snippet.item);
                }
            }

            writer.unindent();
            emitln!(writer, "}}");

            outputs.insert(self.context.types_section.path.clone(), writer.into_inner());
        }

        // Utilities
        if self.utils_enabled {
            let mut writer = IndentedWriter::new(INDENT_WIDTH);
            emit_utilities(&mut writer, &self.context);

            outputs.insert(self.context.utils_section.path.clone(), writer.into_inner());
        }

        // Other cleanup.
        self.context.opaque_types.clear();
        self.context.native_types.clear();

        Ok(())
    }
}

fn resolve_alias(aliases: &HashMap<String, Type>, new_ty: &mut Type) {
    let mut orig_new_ty = mem::replace(new_ty, Type::Unit);

    match orig_new_ty {
        Type::User(ref name) => {
            if let Some(old_ty) = lookup_alias(aliases, name) {
                *new_ty = old_ty.clone();
                return;
            }
        }
        Type::Pointer(ref mut ty) | Type::Array(ref mut ty, _) => {
            resolve_alias(aliases, ty);
        }
        Type::Function(ref mut fun) => {
            resolve_alias(aliases, &mut fun.output);
            for &mut (_, ref mut input) in &mut fun.inputs {
                resolve_alias(aliases, input);
            }
        }
        _ => (),
    }

    mem::replace(new_ty, orig_new_ty);
}

fn lookup_alias<'a>(aliases: &'a HashMap<String, Type>, name: &str) -> Option<&'a Type> {
    if let Some(ty) = aliases.get(name) {
        if let Type::User(ref name) = *ty {
            Some(lookup_alias(aliases, name).unwrap_or(ty))
        } else {
            Some(ty)
        }
    } else {
        None
    }
}

fn collect_callbacks(functions: &[Snippet<Function>]) -> Vec<(&Function, bool)> {
    let mut stash = BTreeMap::new();

    for snippet in functions {
        let callbacks = extract_callbacks(&snippet.item.inputs);
        let count = callbacks.len();

        for callback in callbacks {
            let name = callback_wrapper_name(callback);

            match stash.entry(name) {
                Entry::Vacant(entry) => {
                    let _ = entry.insert((callback, count == 1));
                }
                Entry::Occupied(mut entry) => {
                    if count == 1 {
                        entry.get_mut().1 = true;
                    }
                }
            }
        }
    }

    stash.into_iter().map(|(_, entry)| entry).collect()
}

fn callback_wrapper_name(callback: &Function) -> String {
    let mut writer = IndentedWriter::new(INDENT_WIDTH);
    emit_callback_wrapper_name(&mut writer, callback);
    writer.into_inner()
}

fn unsupported_generics_error(name: &str) -> Error {
    Error {
        level: Level::Error,
        span: None, //NONE FOR NOW
        message: format!("bindgen can not handle parameterized {}", name),
    }
}
