# [master]

# [0.7.0] - 2018-08-28

- Upgrade unwrap version to 1.2.0
- Use rust 1.28.0 stable / 2018-07-07 nightly
- rustfmt 0.99.2 and clippy-0.0.212

# [0.6.0] - 2018-08-10

- Fix `illegal class name` error in Java/JNI
- Fix `pub(crate)` items being parsed as a part of public API
- Update `syntex_syntax` dependency to 0.59.0
- Add more tests for the C language

# [0.5.2] - 2018-07-03

- Make generated C# delegates readonly

# [0.5.1] - 2018-06-26

- Fix incorrectly generated C# delegates: they were garbage collected because of automatically
  created and recycled references. Instead, we use static references now.
- Change license to dual MIT/BSD.

# [0.5.0] - 2018-06-14

- Support multiple output languages.
- Add Java/JNI generators.
- Add C# generator.
- Fix C headers dependencies resolving (i.e. output header includes in the correct order).

# [0.4.1] - 2017-04-11

## Fixed

- Bump syntex dependency version.

# [0.4.0] - 2017-04-05

## Changed

- Rename to moz-cheddar.
- `enum` values are prefixed with the type name on the C side.
- nullable `Option<fn(..)>` types convert to function pointers.

## Fixed

- Bump dependency versions.
- Make `syntex` an optional feature.
- Documentation cleanup.

# [0.3.3] - 2016-05-03

## Fixed

- arbitrarily nested `const` pointers are handled correctly
- function declarations can now contain patterns
    - such as `fn foo(mut a: ...`
- zero argument functions are now written out as `func(void)`

# [0.3.2] - 2016-03-02

## Changed

- rusty-cheddar now correctly converts types in `std::os::raw`

# [0.3.1] - 2016-01-20

## Changed

- the api can now be placed in any arbitrary module

## Fixed

- the include guard is sanitised to avoid illegal characters in a macro definition


# [0.3.0] - 2016-01-10

## Changed

- the whole fucking thing!
    - no longer requires nightly
    - works as a library which leverages syntex
    - see the README for the new interface

## Added

- the `cheddar` executable which acts as a thin wrapper around the library functionality


# [0.2.0] - 2015-12-28

## Added

- support for function pointers
- support for opaque structs
    - `#[repr(C)] pub struct Foo(Vec<T>);`
    - `typedef struct Foo Foo;`
- the ability to hide your C API behind a module
    - can only be one module deep at this point in time

## Changed

- plugin arguments
    - you must now use key value pairs to specify `file` and `dir`
    - old: `#![plugin(cheddar(path,to,file))]`
    - new: `#![plugin(cheddar(dir = "path/to", file = "file.h"))]`

[master]: https://github.com/Sean1708/rusty-cheddar/compare/v0.3.3...HEAD
[0.3.3]: https://github.com/Sean1708/rusty-cheddar/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/Sean1708/rusty-cheddar/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/Sean1708/rusty-cheddar/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/Sean1708/rusty-cheddar/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/Sean1708/rusty-cheddar/compare/v0.1.0...v0.2.0
