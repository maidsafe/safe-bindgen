//! Functions for actually parsing the source file.

use common::{Lang, Outputs};
use Error;

pub fn parse_usetree(usetree: &syn::UseTree) -> Vec<String> {
    let mut modules = Vec::new();

    if let syn::UseTree::Path(ref path) = usetree {
        modules.push(path.ident.to_owned().to_string());
        for x in parse_usetree(&*path.tree) {
            modules.push(x);
        }
    };
    if let syn::UseTree::Name(ref name) = usetree {
        modules.push(name.ident.to_owned().to_string());
    };
    if let syn::UseTree::Glob(ref _glob) = usetree {
        return modules;
    };
    modules
}

/// Returns a list of FFI submodules imported in a top-level module
pub fn imported_mods(import: &syn::ItemUse) -> Option<Vec<String>> {
    //    let mut imported: Vec<Sti> = Vec::new();
    // If it's not visible it can't be called from C.
    let mut segments = Vec::new();

    if let syn::Visibility::Inherited = import.vis {
        None
    } else {
        segments = parse_usetree(&import.tree);

        if &segments[0] == "ffi" {
            Some(segments)
        } else {
            None
        }
    }
}

/// The manager of bindgen and entry point when the crate is the module.
///
/// Iterates through all items in the module and dispatches to correct methods, then pulls all
/// the results together into a header.
pub fn parse_file<L: Lang>(
    lang: &mut L,
    module: &syn::File,
    mod_path: &[String],
    outputs: &mut Outputs,
) -> Result<(), Vec<Error>> {
    let mut errors = vec![];

    for item in module.items.to_owned() {
        // If it's not visible it can't be called from C.
        match item {
            syn::Item::Mod(ref item) => {
                if let syn::Visibility::Inherited = item.vis {
                    continue;
                }
                if let syn::Visibility::Crate(_) = item.vis {
                    continue;
                }
            }
            syn::Item::Const(ref item) => {
                if let syn::Visibility::Inherited = item.vis {
                    continue;
                }
                if let syn::Visibility::Crate(_) = item.vis {
                    continue;
                }
            }
            syn::Item::Type(ref item) => {
                if let syn::Visibility::Inherited = item.vis {
                    continue;
                }
                if let syn::Visibility::Crate(_) = item.vis {
                    continue;
                }
            }
            syn::Item::Enum(ref item) => {
                if let syn::Visibility::Inherited = item.vis {
                    continue;
                }
                if let syn::Visibility::Crate(_) = item.vis {
                    continue;
                }
            }
            syn::Item::Fn(ref item) => {
                if let syn::Visibility::Inherited = item.vis {
                    continue;
                }
                if let syn::Visibility::Crate(_) = item.vis {
                    continue;
                }
            }
            syn::Item::Struct(ref item) => {
                if let syn::Visibility::Inherited = item.vis {
                    continue;
                }
                if let syn::Visibility::Crate(_) = item.vis {
                    continue;
                }
            }
            _ => {}
        }

        // Dispatch to correct method.
        let res = match item {
            syn::Item::Mod(ref item) => {
                parse_mod(lang, item, mod_path, outputs)?;
                Ok(())
            }
            syn::Item::Const(ref item) => {
                lang.parse_const(item, mod_path, outputs)?;
                Ok(())
            }
            syn::Item::Type(ref item) => {
                lang.parse_ty(item, mod_path, outputs)?;
                Ok(())
            }
            syn::Item::Enum(ref item) => {
                lang.parse_enum(item, mod_path, outputs)?;
                Ok(())
            }
            syn::Item::Fn(ref item) => {
                lang.parse_fn(item, mod_path, outputs)?;
                Ok(())
            }
            syn::Item::Struct(ref item) => {
                lang.parse_struct(&item, mod_path, outputs)?;
                Ok(())
            }
            _ => Ok(()),
        };

        // Display any non-fatal errors, fatal errors are handled at cause.
        if let Err(error) = res {
            errors.push(error)
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub fn parse_mod<L: Lang>(
    lang: &mut L,
    module: &syn::ItemMod,
    mod_path: &[String],
    outputs: &mut Outputs,
) -> Result<(), Vec<Error>> {
    let mut errors = vec![];
    if module.to_owned().content.is_some() {
        for item in unwrap!(module.to_owned().content).1 {
            // If it's not visible it can't be called from C.
            match item {
                syn::Item::Mod(ref item) => {
                    if let syn::Visibility::Inherited = item.vis {
                        continue;
                    }
                    if let syn::Visibility::Crate(_) = item.vis {
                        continue;
                    }
                }
                syn::Item::Const(ref item) => {
                    if let syn::Visibility::Inherited = item.vis {
                        continue;
                    }
                    if let syn::Visibility::Crate(_) = item.vis {
                        continue;
                    }
                }
                syn::Item::Type(ref item) => {
                    if let syn::Visibility::Inherited = item.vis {
                        continue;
                    }
                    if let syn::Visibility::Crate(_) = item.vis {
                        continue;
                    }
                }
                syn::Item::Enum(ref item) => {
                    if let syn::Visibility::Inherited = item.vis {
                        continue;
                    }
                    if let syn::Visibility::Crate(_) = item.vis {
                        continue;
                    }
                }
                syn::Item::Fn(ref item) => {
                    if let syn::Visibility::Inherited = item.vis {
                        continue;
                    }
                    if let syn::Visibility::Crate(_) = item.vis {
                        continue;
                    }
                }
                syn::Item::Struct(ref item) => {
                    if let syn::Visibility::Inherited = item.vis {
                        continue;
                    }
                    if let syn::Visibility::Crate(_) = item.vis {
                        continue;
                    }
                }
                _ => {}
            }

            // Dispatch to correct method.
            let res = match item {
                syn::Item::Mod(ref item) => {
                    parse_mod(lang, item, mod_path, outputs)?;
                    Ok(())
                }
                syn::Item::Const(ref item) => {
                    lang.parse_const(item, mod_path, outputs)?;
                    Ok(())
                }
                syn::Item::Type(ref item) => {
                    lang.parse_ty(item, mod_path, outputs)?;
                    Ok(())
                }
                syn::Item::Enum(ref item) => {
                    lang.parse_enum(item, mod_path, outputs)?;
                    Ok(())
                }
                syn::Item::Fn(ref item) => {
                    lang.parse_fn(item, mod_path, outputs)?;
                    Ok(())
                }
                syn::Item::Struct(ref item) => {
                    lang.parse_struct(&item, mod_path, outputs)?;
                    Ok(())
                }
                _ => Ok(()),
            };

            // Display any non-fatal errors, fatal errors are handled at cause.
            if let Err(error) = res {
                errors.push(error)
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
