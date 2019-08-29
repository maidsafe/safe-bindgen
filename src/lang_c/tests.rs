use super::types::CType;
use crate::lang_c::LangC;
use crate::test_utils::{fetch, ty};
use indoc::indoc;
use unwrap::unwrap;

#[test]
fn sanitise_id() {
    assert!(super::sanitise_id("") == "");
    assert!(super::sanitise_id("!@£$%^&*()_+") == "_");
    // https://github.com/Sean1708/rusty-cheddar/issues/29
    assert!(super::sanitise_id("filename.h") == "filenameh");
}

#[test]
fn pure_rust_types() {
    let type_map = [
        ("()", CType::Void),
        ("f32", CType::Native("float")),
        ("f64", CType::Native("double")),
        ("i8", CType::Native("int8_t")),
        ("i16", CType::Native("int16_t")),
        ("i32", CType::Native("int32_t")),
        ("i64", CType::Native("int64_t")),
        ("isize", CType::Native("intptr_t")),
        ("u8", CType::Native("uint8_t")),
        ("u16", CType::Native("uint16_t")),
        ("u32", CType::Native("uint32_t")),
        ("u64", CType::Native("uint64_t")),
        ("usize", CType::Native("uintptr_t")),
    ];

    let name = "gabriel";

    for &(rust_type, ref correct_c_type) in &type_map {
        let parsed_c_type = unwrap!(
            super::anon_rust_to_c(&ty(rust_type)),
            "error while parsing {:?} with no name",
            rust_type
        );
        assert_eq!(&parsed_c_type, correct_c_type);

        let parsed_c_type = unwrap!(
            super::rust_to_c(&ty(rust_type), name),
            "error while parsing {:?} with name {:?}",
            rust_type,
            name
        );
        assert_eq!(
            format!("{}", parsed_c_type),
            format!("{} {}", correct_c_type, name)
        );
    }
}

#[test]
fn libc_types() {
    let type_map = [
        ("libc::c_void", "void"),
        ("libc::c_float", "float"),
        ("libc::c_double", "double"),
        ("libc::c_char", "char"),
        ("libc::c_schar", "signed char"),
        ("libc::c_uchar", "unsigned char"),
        ("libc::c_short", "short"),
        ("libc::c_ushort", "unsigned short"),
        ("libc::c_int", "int"),
        ("libc::c_uint", "unsigned int"),
        ("libc::c_long", "long"),
        ("libc::c_ulong", "unsigned long"),
        ("libc::c_longlong", "long long"),
        ("libc::c_ulonglong", "unsigned long long"),
        // Some other common ones.
        ("libc::size_t", "size_t"),
        ("libc::dirent", "dirent"),
        ("libc::FILE", "FILE"),
    ];

    let name = "lucifer";

    for &(rust_type, correct_c_type) in &type_map {
        let parsed_c_type = unwrap!(
            super::anon_rust_to_c(&ty(rust_type)),
            "error while parsing {:?} with no name",
            rust_type
        );
        assert_eq!(format!("{}", parsed_c_type), correct_c_type);

        let parsed_c_type = unwrap!(
            super::rust_to_c(&ty(rust_type), name),
            "error while parsing {:?} with name {:?}",
            rust_type,
            name
        );
        assert_eq!(
            format!("{}", parsed_c_type),
            format!("{} {}", correct_c_type, name)
        );
    }
}

#[test]
fn const_pointers() {
    let name = "maalik";

    let source = "*const u8";
    let parsed_type = unwrap!(
        super::anon_rust_to_c(&ty(source)),
        "error while parsing {:?} with no name",
        source
    );
    assert_eq!(format!("{}", parsed_type), "uint8_t const*");

    let source = "*const ()";
    let parsed_type = unwrap!(
        super::rust_to_c(&ty(source), name),
        "error while parsing {:?} with name {:?}",
        source,
        name
    );
    assert_eq!(format!("{}", parsed_type), format!("void const* {}", name));

    let source = "*const *const f64";
    let parsed_type = unwrap!(
        super::anon_rust_to_c(&ty(source)),
        "error while parsing {:?} with no name",
        source
    );
    assert_eq!(format!("{}", parsed_type), "double const* const*");

    let source = "*const *const i64";
    let parsed_type = unwrap!(
        super::rust_to_c(&ty(source), name),
        "error while parsing {:?} with name {:?}",
        source,
        name
    );
    assert_eq!(
        format!("{}", parsed_type),
        format!("int64_t const* const* {}", name)
    );
}

#[test]
fn mut_pointers() {
    let name = "raphael";

    let source = "*mut u16";
    let parsed_type = unwrap!(
        super::anon_rust_to_c(&ty(source)),
        "error while parsing {:?} with no name",
        source
    );
    assert_eq!(format!("{}", parsed_type), "uint16_t*");

    let source = "*mut f32";
    let parsed_type = unwrap!(
        super::rust_to_c(&ty(source), name),
        "error while parsing {:?} with name {:?}",
        source,
        name
    );
    assert_eq!(format!("{}", parsed_type), format!("float* {}", name));

    let source = "*mut *mut *mut i32";
    let parsed_type = unwrap!(
        super::anon_rust_to_c(&ty(source)),
        "error while parsing {:?} with no name",
        source
    );
    assert_eq!(format!("{}", parsed_type), "int32_t***");

    let source = "*mut *mut i8";
    let parsed_type = unwrap!(
        super::rust_to_c(&ty(source), name),
        "error while parsing {:?} with name {:?}",
        source,
        name
    );
    assert_eq!(format!("{}", parsed_type), format!("int8_t** {}", name));
}

#[test]
fn mixed_pointers() {
    let name = "samael";

    let source = "*const *mut *const bool";
    let parsed_type = unwrap!(
        super::anon_rust_to_c(&ty(source)),
        "error while parsing {:?} with no name",
        source
    );
    assert_eq!(format!("{}", parsed_type), "bool const** const*");

    let source = "*mut *mut *const libc::c_ulonglong";
    let parsed_type = unwrap!(
        super::rust_to_c(&ty(source), name),
        "error while parsing {:?} with name {:?}",
        source,
        name
    );
    assert_eq!(
        format!("{}", parsed_type),
        format!("unsigned long long const*** {}", name)
    );

    let source = "*const *mut *mut i8";
    let parsed_type = unwrap!(
        super::rust_to_c(&ty(source), name),
        "error while parsing {:?} with name {:?}",
        source,
        name
    );
    assert_eq!(
        format!("{}", parsed_type),
        format!("int8_t** const* {}", name)
    );
}

#[test]
fn function_pointers() {
    let name = "sariel";

    let source = "fn(a: bool)";
    let parsed_type = super::anon_rust_to_c(&ty(source));
    assert!(
        parsed_type.is_err(),
        "C function pointers should have an inner or name associated"
    );

    // let source = "fn(a: i8) -> f64";
    // let parsed_type = super::rust_to_c(&ty(source), name).unwrap_or_else(|_| panic!(
    //     "error while parsing {:?} with name {:?}",
    //     source,
    //     name
    // ));
    // assert!(parsed_type.is_none(), "parsed a non-C function pointer");

    let source = "extern fn(hi: libc::c_int) -> libc::c_double";
    let parsed_type = unwrap!(
        super::rust_to_c(&ty(source), name),
        "error while parsing {:?} with name {:?}",
        source,
        name
    );
    assert_eq!(
        format!("{}", parsed_type),
        format!("double (*{})(int hi)", name)
    );
}

#[test]
fn paths() {
    let name = "zachariel";

    let source = "MyType";
    let parsed_type = unwrap!(
        super::anon_rust_to_c(&ty(source)),
        "error while parsing {:?} with no name",
        source
    );
    assert_eq!(format!("{}", parsed_type), "MyType");

    let source = "SomeType";
    let parsed_type = unwrap!(
        super::rust_to_c(&ty(source), name),
        "error while parsing {:?} with name {:?}",
        source,
        name
    );
    assert_eq!(format!("{}", parsed_type), format!("SomeType {}", name));

    let source = "my_mod::MyType";
    let parsed_type = super::anon_rust_to_c(&ty(source));
    assert!(
        parsed_type.is_err(),
        "can't use a multi-segment path which isn't `libc`"
    );

    let source = "some_mod::SomeType";
    let parsed_type = super::rust_to_c(&ty(source), name);
    assert!(
        parsed_type.is_err(),
        "can't use a multi-segment path which isn't `libc`"
    );
}

#[test]
fn enums() {
    let outputs = compile!(LangC::default(), {
        #[repr(C)]
        pub enum Mode {
            ReadOnly,
            WriteOnly,
            ReadAndWrite,
        }

        #[repr(C)]
        pub enum Binary {
            Zero = 0,
            One = 1,
            Four = 4,
        }
    });

    let actual = fetch(&outputs, ".h");
    let expected = indoc!(
        "

         #ifndef bindgen_h
         #define bindgen_h


         #ifdef __cplusplus
         extern \"C\" {
         #endif

         #include <stdint.h>
         #include <stdbool.h>

         typedef enum Mode {
             Mode_ReadOnly,
             Mode_WriteOnly,
             Mode_ReadAndWrite,
         } Mode;

         typedef enum Binary {
             Binary_Zero = 0,
             Binary_One = 1,
             Binary_Four = 4,
         } Binary;



         #ifdef __cplusplus
         }
         #endif


         #endif
        "
    );

    assert_multiline_eq!(actual, expected);
}

#[test]
fn async_functions() {
    let outputs = compile!(LangC::default(), {
        #[no_mangle]
        pub unsafe extern "C" fn foo(
            user_data: *mut c_void,
            o_cb: extern "C" fn(user_data: *mut c_void, result: *const FfiResult),
        ) {
            ()
        }
    });

    let expected = indoc!(
        "\n\n#ifndef bindgen_h
#define bindgen_h


#ifdef __cplusplus
extern \"C\" {
#endif

#include <stdint.h>
#include <stdbool.h>

void foo(void* user_data, void (*o_cb)(void* user_data, FfiResult const* result));



#ifdef __cplusplus
}
#endif


#endif\n\n"
    );

    let actual = fetch(&outputs, ".h");

    assert_multiline_eq!(actual, expected);
}

#[test]
fn sync_functions() {
    let outputs = compile!(LangC::default(), {
        #[no_mangle]
        pub extern "C" fn new_block(
            payload: *const u8,
            public_ids: *const *const u8,
            votes: *const *const Vote,
            votes_len: usize,
            o_block: *mut *const Block,
        ) -> *const FfiResult {
        }
    });

    let expected = indoc!(
        "\n\n#ifndef bindgen_h
#define bindgen_h


#ifdef __cplusplus
extern \"C\" {
#endif

#include <stdint.h>
#include <stdbool.h>

FfiResult const* new_block(uint8_t const* payload, uint8_t const* const* public_ids, Vote const* const* votes, uintptr_t votes_len, Block const** o_block);



#ifdef __cplusplus
}
#endif


#endif\n\n"
    );

    let actual = fetch(&outputs, ".h");

    assert_multiline_eq!(actual, expected);
}
