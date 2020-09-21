# Changelog

### [0.13.3](https://github.com/maidsafe/sn_bindgen/compare/v0.13.2...v0.13.3) 2020-09-01

* rename crate to sn_bindgen as per new naming convention
* add option to copy to/from List<string>

### [0.13.2](https://github.com/maidsafe/sn_bindgen/compare/v0.13.1...v0.13.2) 2019-11-03

* Add support for negative constants in C sharp

### [0.13.1] - 2019-09-02

* Fix C parser erroring on enum variants with values

### [0.13.0] - 2019-05-29

* Update clap dependency to the latest version

### [0.12.0] - 2019-04-12

* Switch from a deprecated parsing library `syntex` to `syn`.
* Fix inconsitencies and bugs in the C# module.
* Add more tests for Java & JNI modules.
* Use stable Rust (edition 2018).

### [0.11.0] - 2018-11-15

* Allow to filter symbols in Java bindgen. This can be used for manual reimplementation of
  certain JNI functions in special cases.
* Pass through the compiler feature flags for generated JNI functions (as some functions
  can be feature-gated).
* Fix leaking local references. Because the Android local reference table is limited to 512
  entries it is important to deallocate the local references as soon as possible.

### [0.10.0] - 2018-09-18

* Use `EnvGuard` from `ffi_utils` to detach JNI threads only when needed (i.e. if we
  successfully acquire JNI environment through `GetEnv()`, we do not need to detach the
  native thread)
* Change the class finder function signature (now it returns result wrapped
  as `Result<AutoLocal, JniError>`)

### [0.9.0] - 2018-09-13

* Use custom cached class loader for Java/JNI objects instantiation (this fixes the Android
  freezing issue)

### [0.8.0] - 2018-09-04

* Support new API to pass input source code as a string (`Bindgen::source_code`)

### [0.7.0] - 2018-08-28

* Upgrade unwrap version to 1.2.0
* Use rust 1.28.0 stable / 2018-07-07 nightly
* rustfmt 0.99.2 and clippy-0.0.212

### [0.6.0] - 2018-08-10

* Fix `illegal class name` error in Java/JNI
* Fix `pub(crate)` items being parsed as a part of public API
* Update `syntex_syntax` dependency to 0.59.0
* Add more tests for the C language

### [0.5.2] - 2018-07-03

* Make generated C# delegates readonly

### [0.5.1] - 2018-06-26

* Fix incorrectly generated C# delegates: they were garbage collected because of automatically
  created and recycled references. Instead, we use static references now.
* Change license to dual MIT/BSD.

### [0.5.0] - 2018-06-14

* Support multiple output languages.
* Add Java/JNI generators.
* Add C# generator.
* Fix C headers dependencies resolving (i.e. output header includes in the correct order).

### [0.4.1] - 2017-04-11

### Fixed

* Bump syntex dependency version.

### [0.4.0] - 2017-04-05

### Changed

* Rename to moz-cheddar.
* `enum` values are prefixed with the type name on the C side.
* nullable `Option<fn(..)>` types convert to function pointers.

### Fixed

* Bump dependency versions.
* Make `syntex` an optional feature.
* Documentation cleanup.

### [0.3.3] - 2016-05-03

### Fixed

* arbitrarily nested `const` pointers are handled correctly
* function declarations can now contain patterns
    - such as `fn foo(mut a: ...`
* zero argument functions are now written out as `func(void)`

### [0.3.2] - 2016-03-02

### Changed

* rusty-cheddar now correctly converts types in `std::os::raw`

### [0.3.1] - 2016-01-20

### Changed

* the api can now be placed in any arbitrary module

### Fixed

* the include guard is sanitised to avoid illegal characters in a macro definition


### [0.3.0] - 2016-01-10

### Changed

* the whole fucking thing!
    - no longer requires nightly
    - works as a library which leverages syntex
    - see the README for the new interface

### Added

* the `cheddar` executable which acts as a thin wrapper around the library functionality


### [0.2.0] - 2015-12-28

### Added

* support for function pointers
* support for opaque structs
    - `#[repr(C)] pub struct Foo(Vec<T>);`
    - `typedef struct Foo Foo;`
* the ability to hide your C API behind a module
    - can only be one module deep at this point in time

### Changed

* plugin arguments
    - you must now use key value pairs to specify `file` and `dir`
    - old: `#![plugin(cheddar(path,to,file))]`
    - new: `#![plugin(cheddar(dir = "path/to", file = "file.h"))]`

[master]: https://github.com/Sean1708/rusty-cheddar/compare/v0.3.3...HEAD
[0.3.3]: https://github.com/Sean1708/rusty-cheddar/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/Sean1708/rusty-cheddar/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/Sean1708/rusty-cheddar/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/Sean1708/rusty-cheddar/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/Sean1708/rusty-cheddar/compare/v0.1.0...v0.2.0
