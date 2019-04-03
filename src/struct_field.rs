use core::borrow::{Borrow, BorrowMut};
use std::collections::BTreeSet;
use syn::export::ToTokens;

#[derive(Debug)]
pub enum StructField {
    Primitive(syn::Field),
    Array {
        field: syn::Field,
        len_field: String,
        cap_field: Option<String>,
    },
    String(syn::Field),
    StructPtr {
        field: syn::Field,
        ty: syn::TypePtr,
    },
    LenField(syn::Field),
}

impl StructField {
    pub fn struct_field(&self) -> &syn::Field {
        match *self {
            StructField::Primitive(ref f)
            | StructField::Array { field: ref f, .. }
            | StructField::StructPtr { field: ref f, .. }
            | StructField::String(ref f)
            | StructField::LenField(ref f) => f,
        }
    }

    pub fn name(&self) -> &str {
        self.struct_field().ident.unwrap().to_string().as_str()
    }
}

pub fn transform_struct_fields(fields: &[syn::Field]) -> Vec<StructField> {
    let mut results = Vec::new();
    let field_names: BTreeSet<_> = fields
        .iter()
        .map(|f| f.ident.unwrap().to_string())
        .collect();

    for f in fields.iter() {
        let mut field_name: String = f.ident.unwrap().to_string();
        let f = *f;
        match f.ty {
            // Pointers
            syn::Type::Ptr(ref ptr) => {
                if field_name.ends_with("_ptr") {
                    field_name = field_name.chars().take(field_name.len() - 4).collect();
                }

                let len_field = format!("{}_len", field_name);
                let cap_field = format!("{}_cap", field_name);

                if field_names.contains(&len_field) {
                    results.push(StructField::Array {
                        field: f,
                        len_field,
                        cap_field: if field_names.contains(&cap_field) {
                            Some(cap_field)
                        } else {
                            None
                        },
                    });
                } else {
                    match ptr.into_token_stream().to_string().as_str() {
                        // Strings
                        "c_char" => {
                            results.push(StructField::String(f));
                        }
                        // Other ptrs, most likely structs
                        _ => {
                            results.push(StructField::StructPtr { field: f, ty: *ptr });
                        }
                    }
                }
            }

            syn::Type::Path(ref _path) => {
                results.push(if is_array_meta_field(&f) {
                    StructField::LenField(f)
                } else {
                    StructField::Primitive(f)
                });
            }

            _ => results.push(StructField::Primitive(f.into())),
        }
    }

    results
}

fn is_array_meta_field(field: &syn::Field) -> bool {
    let str_name = field
        .ident
        .unwrap()
        .into_token_stream()
        .to_string()
        .as_str();
    if let syn::Type::Path(ref path) = field.ty {
        let ty = path
            .path
            .segments
            .last()
            .unwrap()
            .value()
            .ident
            .into_token_stream()
            .to_string()
            .as_str();

        ty == "usize" && (str_name.ends_with("_len") || str_name.ends_with("_cap"))
    } else {
        false
    }
}
