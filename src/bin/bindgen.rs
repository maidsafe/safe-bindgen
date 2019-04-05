#[macro_use]
extern crate clap;
extern crate jni;
extern crate safe_bindgen;
#[macro_use]
extern crate unwrap;

use jni::signature::{JavaType, Primitive};
use safe_bindgen::{Bindgen, LangC, LangCSharp, LangJava};
use std::collections::HashMap;

fn main() {
    let matches = clap::App::new("bindgen")
        .version(crate_version!())
        .author("Sean Marshallsay <srm.1708@gmail.com>, MaidSafe Developers <dev@maidsafe.net>")
        .about("create binding files using a Rust source file")
        .arg(
            clap::Arg::with_name("FILE")
                .short("-f")
                .long("--file")
                .conflicts_with("STRING")
                .takes_value(true)
                .help("the root source file"),
        )
        .arg(
            clap::Arg::with_name("LANG")
                .short("-l")
                .long("--lang")
                .takes_value(true)
                .required(true)
                .help("target language")
                .possible_values(&["csharp", "java", "c"]),
        )
        .arg(
            clap::Arg::with_name("LIB")
                .long("--lib")
                .takes_value(true)
                .required(true)
                .help("name of the native library to link"),
        )
        .arg(
            clap::Arg::with_name("OUTPUT")
                .index(1)
                .help("set the output directory"),
        )
        .get_matches();

    let mut bindgen = Bindgen::new().expect("cargo manifest could not be read");
    let lang = unwrap!(matches.value_of("LANG"));
    let lib = unwrap!(matches.value_of("LIB"));

    if let Some(file) = matches.value_of("FILE") {
        bindgen.source_file(&file);
    }

    let output_dir = if let Some(output) = matches.value_of("OUTPUT") {
        output.to_string()
    } else {
        format!("bind-gen/{}", lang)
    };

    match lang {
        "c" => {
            let mut lang = LangC::new();
            lang.set_lib_name(lib);
            bindgen.run_build(&mut lang, &output_dir)
        }
        "csharp" => {
            let mut lang = LangCSharp::new();
            lang.set_lib_name(lib);
            bindgen.run_build(&mut lang, &output_dir)
        }
        "java" => {
            let mut type_map = HashMap::new();
            type_map.insert(
                "XorNameArray",
                JavaType::Array(Box::new(JavaType::Primitive(Primitive::Byte))),
            );
            type_map.insert(
                "SignSecretKey",
                JavaType::Array(Box::new(JavaType::Primitive(Primitive::Byte))),
            );
            type_map.insert(
                "SignPublicKey",
                JavaType::Array(Box::new(JavaType::Primitive(Primitive::Byte))),
            );
            type_map.insert(
                "SymSecretKey",
                JavaType::Array(Box::new(JavaType::Primitive(Primitive::Byte))),
            );
            type_map.insert(
                "SymNonce",
                JavaType::Array(Box::new(JavaType::Primitive(Primitive::Byte))),
            );
            type_map.insert(
                "AsymPublicKey",
                JavaType::Array(Box::new(JavaType::Primitive(Primitive::Byte))),
            );
            type_map.insert(
                "AsymSecretKey",
                JavaType::Array(Box::new(JavaType::Primitive(Primitive::Byte))),
            );
            type_map.insert(
                "AsymNonce",
                JavaType::Array(Box::new(JavaType::Primitive(Primitive::Byte))),
            );
            type_map.insert("CipherOptHandle", JavaType::Primitive(Primitive::Long));
            type_map.insert("EncryptPubKeyHandle", JavaType::Primitive(Primitive::Long));
            type_map.insert("EncryptSecKeyHandle", JavaType::Primitive(Primitive::Long));
            type_map.insert("MDataEntriesHandle", JavaType::Primitive(Primitive::Long));
            type_map.insert(
                "MDataEntryActionsHandle",
                JavaType::Primitive(Primitive::Long),
            );
            type_map.insert(
                "MDataPermissionsHandle",
                JavaType::Primitive(Primitive::Long),
            );
            type_map.insert(
                "SelfEncryptorReaderHandle",
                JavaType::Primitive(Primitive::Long),
            );
            type_map.insert(
                "SelfEncryptorWriterHandle",
                JavaType::Primitive(Primitive::Long),
            );
            type_map.insert("SEReaderHandle", JavaType::Primitive(Primitive::Long));
            type_map.insert("SEWriterHandle", JavaType::Primitive(Primitive::Long));
            type_map.insert("SignPubKeyHandle", JavaType::Primitive(Primitive::Long));
            type_map.insert("SignSecKeyHandle", JavaType::Primitive(Primitive::Long));
            type_map.insert("FileContextHandle", JavaType::Primitive(Primitive::Long));
            type_map.insert("App", JavaType::Primitive(Primitive::Long));
            type_map.insert("Authenticator", JavaType::Primitive(Primitive::Long));

            let mut java = LangJava::new(type_map);
            java.set_namespace(format!("net.maidsafe.{}", lib));
            java.set_model_namespace(format!("net.maidsafe.{}", lib));
            java.set_lib_name(lib);
            bindgen.run_build(&mut java, &output_dir);
        }
        _ => unreachable!(),
    }
}
