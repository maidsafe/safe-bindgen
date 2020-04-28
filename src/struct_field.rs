use std::collections::BTreeSet;
use syn::export::ToTokens;
use unwrap::unwrap;

#[derive(Debug, PartialEq)]
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

    pub fn name(&self) -> String {
        unwrap!(self.struct_field().ident.to_owned()).to_string()
    }
}

pub fn transform_struct_fields(fields: &[syn::Field]) -> Vec<StructField> {
    let mut results = Vec::new();
    let field_names: BTreeSet<_> = fields
        .iter()
        .map(|f| unwrap!(f.ident.to_owned()).to_string())
        .collect();

    for f in fields.iter() {
        let mut field_name: String = unwrap!(f.ident.to_owned()).to_string();
        let f = f.to_owned();
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
                        field: f.clone(),
                        len_field,
                        cap_field: if field_names.contains(&cap_field) {
                            Some(cap_field)
                        } else {
                            None
                        },
                    });
                } else {
                    match ptr.elem.clone().into_token_stream().to_string().as_str() {
                        // Strings
                        "c_char" => {
                            results.push(StructField::String(f.clone()));
                        }
                        // Other ptrs, most likely structs
                        _ => {
                            results.push(StructField::StructPtr {
                                field: f.clone(),
                                ty: ptr.clone(),
                            });
                        }
                    }
                }
            }

            syn::Type::Path(ref _path) => {
                results.push(if is_array_meta_field(&f) {
                    StructField::LenField(f.clone())
                } else {
                    StructField::Primitive(f.clone())
                });
            }

            _ => results.push(StructField::Primitive(f.clone())),
        }
    }

    results
}

fn is_array_meta_field(field: &syn::Field) -> bool {
    let ident = unwrap!(field.to_owned().ident)
        .into_token_stream()
        .to_string();
    if let syn::Type::Path(ref typepath) = field.ty {
        let ty = unwrap!(typepath.path.segments.last())
            .into_value()
            .ident
            .to_owned()
            .to_string();
        ty == "usize" && (ident.ends_with("_len") || ident.ends_with("_cap"))
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::{transform_struct_fields, StructField};
    use unwrap::unwrap;

    #[test]
    fn transform_fields_primitive() {
        let s: syn::ItemStruct = unwrap!(syn::parse_str(
            "struct Foo { a: u64, b: *const c_char, c: *mut c_char, s: *mut Foo }"
        ));
        let mut expected_fields = Vec::new();

        let transformed_fields = match s.fields {
            syn::Fields::Named(ref fields) => {
                expected_fields.push(StructField::Primitive(fields.named[0].clone()));
                expected_fields.push(StructField::String(fields.named[1].clone()));
                expected_fields.push(StructField::String(fields.named[2].clone()));

                match &fields.named[3].ty {
                    syn::Type::Ptr(ref ty) => {
                        expected_fields.push(StructField::StructPtr {
                            field: fields.named[3].clone(),
                            ty: ty.clone(),
                        });
                    }
                    x => panic!("parsed unexpected type {:?}", x),
                }

                transform_struct_fields(&fields.named.iter().cloned().collect::<Vec<_>>())
            }
            x => panic!("unexpected parse result: {:?}", x),
        };

        assert_eq!(transformed_fields, expected_fields);
    }

    #[test]
    fn transform_fields_array() {
        let s: syn::ItemStruct = unwrap!(syn::parse_str(
            "struct Foo { a: *const Foo, a_len: usize, a_cap: usize }"
        ));
        let mut expected_fields = Vec::new();

        let transformed_fields = match s.fields {
            syn::Fields::Named(ref fields) => {
                expected_fields.push(StructField::Array {
                    field: fields.named[0].clone(),
                    len_field: "a_len".to_string(),
                    cap_field: Some("a_cap".to_string()),
                });
                expected_fields.push(StructField::LenField(fields.named[1].clone()));
                expected_fields.push(StructField::LenField(fields.named[2].clone()));

                transform_struct_fields(&fields.named.iter().cloned().collect::<Vec<_>>())
            }
            x => panic!("unexpected parse result: {:?}", x),
        };

        assert_eq!(transformed_fields, expected_fields);
    }
}
