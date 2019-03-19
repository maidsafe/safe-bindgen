use super::*;
use test_utils::fetch;

#[test]
fn non_repr_c_types_are_ignored() {
    let outputs = compile!(LangCSharp::default(), {
        pub struct Foo {
            bar: i32,
        }

        pub enum Meta {
            Foo,
            Bar,
            Baz,
        }
    });

    let actual = fetch(&outputs, "Types.cs");
    assert!(actual.is_empty());
}

#[test]
fn structs() {
    let outputs = compile!(LangCSharp::default(), {
        #[repr(C)]
        pub struct Record {
            id: u64,
            enabled: bool,
            name: *const c_char,
            random_numbers: [i32; 10],
            widget: Widget,
            gadgets: [Gadget; 100],
        }
    });

    let actual = fetch(&outputs, "Types.cs");
    let expected = indoc!(
        "using System;
         using System.Collections.Generic;
         using System.Runtime.InteropServices;
         using JetBrains.Annotations;

         namespace Backend {
           [PublicAPI]
           public struct Record {
             public ulong Id;
             [MarshalAs(UnmanagedType.U1)]
             public bool Enabled;
             [MarshalAs(UnmanagedType.LPStr)]
             public string Name;
             [MarshalAs(UnmanagedType.ByValArray, SizeConst = 10)]
             public int[] RandomNumbers;
             public Widget Widget;
             [MarshalAs(UnmanagedType.ByValArray, SizeConst = 100)]
             public Gadget[] Gadgets;
           }

         }
         "
    );

    assert_multiline_eq!(actual, expected);
}

#[test]
fn native_structs() {
    let outputs = compile!(LangCSharp::default(), {
        #[repr(C)]
        pub struct Entry {
            id: u32,
            key_ptr: *const u8,
            key_len: usize,
            records_ptr: *const Record,
            records_len: usize,
            records_cap: usize,
        }

        #[repr(C)]
        pub struct Wrapper {
            wrapped: Entry,
        }

        #[no_mangle]
        pub extern "C" fn fun0(entry: Entry) {}

        #[no_mangle]
        pub extern "C" fn fun1(entry: *const Entry) {}

        #[no_mangle]
        pub extern "C" fn fun2(
            user_data: *mut c_void,
            cb: extern "C" fn(
                user_data: *mut c_void,
                result: *const FfiResult,
                entry: *const Entry,
            ),
        ) {
        }

        #[no_mangle]
        pub extern "C" fn fun3(
            user_data: *mut c_void,
            cb: extern "C" fn(
                user_data: *mut c_void,
                result: *const FfiResult,
                entries: *const Entry,
                entries_len: usize,
            ),
        ) {
        }
    });

    // Wrapper types.
    let actual = fetch(&outputs, "Types.cs");
    let expected = indoc!(
        "using System;
         using System.Collections.Generic;
         using System.Runtime.InteropServices;
         using JetBrains.Annotations;

         namespace Backend {
           [PublicAPI]
           public struct Entry {
             public uint Id;
             public List<byte> Key;
             public List<Record> Records;

             internal Entry(EntryNative native) {
               Id = native.Id;
               Key = Utils.CopyToByteList(native.KeyPtr, (int) native.KeyLen);
               Records = Utils.CopyToObjectList<Record>(native.RecordsPtr, \
                                                        (int) native.RecordsLen);
             }

             internal EntryNative ToNative() {
               return new EntryNative() {
                 Id = Id,
                 KeyPtr = Utils.CopyFromByteList(Key),
                 KeyLen = (UIntPtr) (Key?.Count ?? 0),
                 RecordsPtr = Utils.CopyFromObjectList(Records),
                 RecordsLen = (UIntPtr) (Records?.Count ?? 0),
                 RecordsCap = UIntPtr.Zero
               };
             }
           }

           internal struct EntryNative {
             public uint Id;
             public IntPtr KeyPtr;
             public UIntPtr KeyLen;
             public IntPtr RecordsPtr;
             public UIntPtr RecordsLen;
             // ReSharper disable once NotAccessedField.Compiler
             public UIntPtr RecordsCap;

             internal void Free() {
               Utils.FreeList(ref KeyPtr, ref KeyLen);
               Utils.FreeList(ref RecordsPtr, ref RecordsLen);
             }
           }

           [PublicAPI]
           public struct Wrapper {
             public Entry Wrapped;

             internal Wrapper(WrapperNative native) {
               Wrapped = new Entry(native.Wrapped);
             }

             internal WrapperNative ToNative() {
               return new WrapperNative() {
                 Wrapped = Wrapped.ToNative()
               };
             }
           }

           internal struct WrapperNative {
             public EntryNative Wrapped;

             internal void Free() {
               Wrapped.Free();
             }
           }

         }
        "
    );
    assert_multiline_eq!(actual, expected);

    // Functions.
    let actual = fetch(&outputs, "Backend.cs");
    let expected = indoc!(
        "using System;
         using System.Collections.Generic;
         using System.Linq;
         using System.Runtime.InteropServices;
         using System.Threading.Tasks;

         namespace Backend {
           internal partial class Backend : IBackend {
             #if __IOS__
             private const string DllName = \"__Internal\";
             #else
             private const string DllName = \"backend\";
             #endif

             public void Fun0(Entry entry) {
               var entryNative = entry.ToNative();
               Fun0Native(entryNative);
               entryNative.Free();
             }

             [DllImport(DllName, EntryPoint = \"fun0\")]
             private static extern void Fun0Native(EntryNative entry);

             public void Fun1(ref Entry entry) {
               var entryNative = entry.ToNative();
               Fun1Native(ref entryNative);
               entryNative.Free();
             }

             [DllImport(DllName, EntryPoint = \"fun1\")]
             private static extern void Fun1Native(ref EntryNative entry);

             public Task<Entry> Fun2Async() {
               var (ret, userData) = Utils.PrepareTask<Entry>();
               Fun2Native(userData, DelegateOnFfiResultEntryCb);
               return ret;
             }

             [DllImport(DllName, EntryPoint = \"fun2\")]
             private static extern void Fun2Native(IntPtr userData, \
                                                   FfiResultEntryCb cb);

             public Task<List<Entry>> Fun3Async() {
               var (ret, userData) = Utils.PrepareTask<List<Entry>>();
               Fun3Native(userData, DelegateOnFfiResultEntryListCb);
               return ret;
             }

             [DllImport(DllName, EntryPoint = \"fun3\")]
             private static extern void Fun3Native(IntPtr userData, FfiResultEntryListCb cb);

             private delegate void FfiResultEntryCb(IntPtr userData, \
                                                    IntPtr result, \
                                                    IntPtr entry);

             #if __IOS__
             [MonoPInvokeCallback(typeof(FfiResultEntryCb))]
             #endif
             private static void OnFfiResultEntryCb(IntPtr userData, \
                                                    IntPtr result, \
                                                    IntPtr entry) {
               Utils.CompleteTask(userData, \
                                  Marshal.PtrToStructure<FfiResult>(result), \
                                  () => new Entry(Marshal.PtrToStructure<EntryNative>(entry)));
             }

             private static readonly FfiResultEntryCb DelegateOnFfiResultEntryCb = OnFfiResultEntryCb;

             private delegate void FfiResultEntryListCb(IntPtr userData, \
                                                        IntPtr result, \
                                                        IntPtr entriesPtr, \
                                                        UIntPtr entriesLen);

             #if __IOS__
             [MonoPInvokeCallback(typeof(FfiResultEntryListCb))]
             #endif
             private static void OnFfiResultEntryListCb(IntPtr userData, \
                                                        IntPtr result, \
                                                        IntPtr entriesPtr, \
                                                        UIntPtr entriesLen) {
               Utils.CompleteTask(\
                 userData, \
                 Marshal.PtrToStructure<FfiResult>(result), \
                 () => Utils.CopyToObjectList<EntryNative>(\
                   entriesPtr, \
                   (int) entriesLen).Select(native => new Entry(native)).ToList());
             }

             private static readonly FfiResultEntryListCb DelegateOnFfiResultEntryListCb = OnFfiResultEntryListCb;

           }
         }
        "
    );
    assert_multiline_eq!(actual, expected);
}

#[test]
fn type_aliases() {
    let outputs = compile!(LangCSharp::default(), {
        pub type Id = u64;
        // Double indirection.
        pub type UserId = Id;

        #[repr(C)]
        pub struct Message {
            id: Id,
            sender_id: UserId,
            receiver_ids: [Id; 10],
        }

        #[no_mangle]
        pub extern "C" fn fun(
            id: Id,
            user_data: *mut c_void,
            cb: extern "C" fn(*mut c_void, *const FfiResult, Id),
        ) {
        }
    });

    let actual = fetch(&outputs, "Types.cs");
    let expected = indoc!(
        "using System;
         using System.Collections.Generic;
         using System.Runtime.InteropServices;
         using JetBrains.Annotations;

         namespace Backend {
           [PublicAPI]
           public struct Message {
             public ulong Id;
             public ulong SenderId;
             [MarshalAs(UnmanagedType.ByValArray, SizeConst = 10)]
             public ulong[] ReceiverIds;
           }

         }
         "
    );
    assert_multiline_eq!(actual, expected);

    let actual = fetch(&outputs, "Backend.cs");
    let expected = indoc!(
        "using System;
         using System.Collections.Generic;
         using System.Linq;
         using System.Runtime.InteropServices;
         using System.Threading.Tasks;

         namespace Backend {
           internal partial class Backend : IBackend {
             #if __IOS__
             private const string DllName = \"__Internal\";
             #else
             private const string DllName = \"backend\";
             #endif

             public Task<ulong> FunAsync(ulong id) {
               var (ret, userData) = Utils.PrepareTask<ulong>();
               FunNative(id, userData, DelegateOnFfiResultULongCb);
               return ret;
             }

             [DllImport(DllName, EntryPoint = \"fun\")]
             private static extern void FunNative(ulong id, \
                                                  IntPtr userData, \
                                                  FfiResultULongCb cb);

             private delegate void FfiResultULongCb(IntPtr arg0, \
                                                    IntPtr arg1, \
                                                    ulong arg2);

             #if __IOS__
             [MonoPInvokeCallback(typeof(FfiResultULongCb))]
             #endif
             private static void OnFfiResultULongCb(IntPtr arg0, \
                                                    IntPtr arg1, \
                                                    ulong arg2) {
               Utils.CompleteTask(arg0, \
                                  Marshal.PtrToStructure<FfiResult>(arg1), \
                                  () => arg2);
             }

             private static readonly FfiResultULongCb DelegateOnFfiResultULongCb = OnFfiResultULongCb;

           }
         }
         "
    );
    assert_multiline_eq!(actual, expected);
}

#[test]
fn enums() {
    let outputs = compile!(LangCSharp::default(), {
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
        }
    });

    let actual = fetch(&outputs, "Types.cs");
    let expected = indoc!(
        "using System;
         using System.Collections.Generic;
         using System.Runtime.InteropServices;
         using JetBrains.Annotations;

         namespace Backend {
           [PublicAPI]
           public enum Mode {
             ReadOnly,
             WriteOnly,
             ReadAndWrite,
           }

           [PublicAPI]
           public enum Binary {
             Zero = 0,
             One = 1,
           }

         }
        "
    );

    assert_multiline_eq!(actual, expected);
}

#[test]
fn functions_without_extern_and_no_mangle_are_ignored() {
    let outputs = compile!(LangCSharp::default(), {
        pub extern "C" fn fun1() {}

        #[no_mangle]
        pub fn fun2() {}
    });

    let actual = fetch(&outputs, "Backend.cs");
    assert!(actual.is_empty());
}

#[test]
fn functions_taking_no_callbacks() {
    let outputs = compile!(LangCSharp::default(), {
        #[no_mangle]
        pub extern "C" fn fun0(engine: *mut Engine) {}
    });

    let actual = fetch(&outputs, "Backend.cs");
    let expected = indoc!(
        "using System;
         using System.Collections.Generic;
         using System.Linq;
         using System.Runtime.InteropServices;
         using System.Threading.Tasks;

         namespace Backend {
           internal partial class Backend : IBackend {
             #if __IOS__
             private const string DllName = \"__Internal\";
             #else
             private const string DllName = \"backend\";
             #endif

             public void Fun0(ref Engine engine) {
               Fun0Native(ref engine);
             }

             [DllImport(DllName, EntryPoint = \"fun0\")]
             private static extern void Fun0Native(ref Engine engine);

           }
         }
        "
    );

    assert_multiline_eq!(actual, expected);
}

#[test]
fn functions_taking_one_callback() {
    let outputs = compile!(LangCSharp::default(), {
        #[no_mangle]
        pub extern "C" fn fun1(
            num: i32,
            name: *const c_char,
            user_data: *mut c_void,
            cb: extern "C" fn(user_data: *mut c_void, result: *const FfiResult),
        ) {
        }
    });

    let actual = fetch(&outputs, "Backend.cs");
    let expected = indoc!(
        "using System;
         using System.Collections.Generic;
         using System.Linq;
         using System.Runtime.InteropServices;
         using System.Threading.Tasks;

         namespace Backend {
           internal partial class Backend : IBackend {
             #if __IOS__
             private const string DllName = \"__Internal\";
             #else
             private const string DllName = \"backend\";
             #endif

             public Task Fun1Async(int num, string name) {
               var (ret, userData) = Utils.PrepareTask();
               Fun1Native(num, name, userData, DelegateOnFfiResultCb);
               return ret;
             }

             [DllImport(DllName, EntryPoint = \"fun1\")]
             private static extern void Fun1Native(\
               int num, \
               [MarshalAs(UnmanagedType.LPStr)] string name, \
               IntPtr userData, \
               FfiResultCb cb);

             private delegate void FfiResultCb(IntPtr userData, IntPtr result);

             #if __IOS__
             [MonoPInvokeCallback(typeof(FfiResultCb))]
             #endif
             private static void OnFfiResultCb(IntPtr userData, IntPtr result) {
               Utils.CompleteTask(userData, Marshal.PtrToStructure<FfiResult>(result));
             }

             private static readonly FfiResultCb DelegateOnFfiResultCb = OnFfiResultCb;

           }
         }
        "
    );

    assert_multiline_eq!(actual, expected);
}

#[test]
fn functions_taking_multiple_callbacks() {
    // Only the native declaration should be produced.

    let outputs = compile!(LangCSharp::default(), {
        #[no_mangle]
        pub extern "C" fn fun(
            input: i32,
            user_data: *mut c_void,
            cb0: extern "C" fn(user_data: *mut c_void),
            cb1: extern "C" fn(user_data: *mut c_void, result: *const FfiResult, output: i32),
        ) {
        }
    });

    let actual = fetch(&outputs, "Backend.cs");
    let expected = indoc!(
        "using System;
         using System.Collections.Generic;
         using System.Linq;
         using System.Runtime.InteropServices;
         using System.Threading.Tasks;

         namespace Backend {
           internal partial class Backend : IBackend {
             #if __IOS__
             private const string DllName = \"__Internal\";
             #else
             private const string DllName = \"backend\";
             #endif

             [DllImport(DllName, EntryPoint = \"fun\")]
             private static extern void FunNative(int input, \
                                                   IntPtr userData, \
                                                   NoneCb cb0, \
                                                   FfiResultIntCb cb1);

             private delegate void FfiResultIntCb(IntPtr userData, \
                                                   IntPtr result, \
                                                   int output);

             private delegate void NoneCb(IntPtr userData);

           }
         }
        "
    );

    assert_multiline_eq!(actual, expected);
}

#[test]
fn functions_taking_array() {
    let outputs = compile!(LangCSharp::default(), {
        #[no_mangle]
        pub extern "C" fn fun0(data_ptr: *const u8, data_len: usize) {}

        // Different naming convention
        #[no_mangle]
        pub extern "C" fn fun1(data: *const u8, data_len: usize) {}

        // Params before and/or after the array
        #[no_mangle]
        pub extern "C" fn fun2(id: u64, data_ptr: *const u8, data_len: usize) {}

        // This one does not follow the naming convention and thus is not transformed
        // to array.
        #[no_mangle]
        pub extern "C" fn fun3(result: *const FfiResult, len: usize) {}
    });

    let actual = fetch(&outputs, "Backend.cs");
    let expected = indoc!(
        "using System;
         using System.Collections.Generic;
         using System.Linq;
         using System.Runtime.InteropServices;
         using System.Threading.Tasks;

         namespace Backend {
           internal partial class Backend : IBackend {
             #if __IOS__
             private const string DllName = \"__Internal\";
             #else
             private const string DllName = \"backend\";
             #endif

             public void Fun0(List<byte> data) {
               Fun0Native(data?.ToArray(), (UIntPtr) (data?.Count ?? 0));
             }

             [DllImport(DllName, EntryPoint = \"fun0\")]
             private static extern void Fun0Native(\
               [MarshalAs(UnmanagedType.LPArray, SizeParamIndex = 1)] byte[] data, \
               UIntPtr dataLen\
             );

             public void Fun1(List<byte> data) {
               Fun1Native(data?.ToArray(), (UIntPtr) (data?.Count ?? 0));
             }

             [DllImport(DllName, EntryPoint = \"fun1\")]
             private static extern void Fun1Native(\
               [MarshalAs(UnmanagedType.LPArray, SizeParamIndex = 1)] byte[] data, \
               UIntPtr dataLen\
             );

             public void Fun2(ulong id, List<byte> data) {
               Fun2Native(id, data?.ToArray(), (UIntPtr) (data?.Count ?? 0));
             }

             [DllImport(DllName, EntryPoint = \"fun2\")]
             private static extern void Fun2Native(\
               ulong id, \
               [MarshalAs(UnmanagedType.LPArray, SizeParamIndex = 2)] byte[] data, \
               UIntPtr dataLen\
             );

             public void Fun3(ref FfiResult result, UIntPtr len) {
               Fun3Native(ref result, len);
             }

             [DllImport(DllName, EntryPoint = \"fun3\")]
             private static extern void Fun3Native(ref FfiResult result, UIntPtr len);

           }
         }
        "
    );

    assert_multiline_eq!(actual, expected);
}

#[test]
fn functions_taking_callback_taking_const_size_array() {
    let outputs = compile!(LangCSharp::default(), {
        pub type Nonce = [u8; NONCE_LEN];

        // Literal.
        #[no_mangle]
        pub extern "C" fn fun2(
            user_data: *mut c_void,
            cb: extern "C" fn(user_data: *mut c_void, result: *const FfiResult, key: [u8; 32]),
        ) {
        }

        // Pointor to array via named constant.
        #[no_mangle]
        pub extern "C" fn fun3(
            user_data: *mut c_void,
            cb: extern "C" fn(
                user_data: *mut c_void,
                result: *const FfiResult,
                nonce: *const Nonce,
            ),
        ) {
        }
    });

    let actual = fetch(&outputs, "Backend.cs");
    let expected = indoc!(
        "using System;
         using System.Collections.Generic;
         using System.Linq;
         using System.Runtime.InteropServices;
         using System.Threading.Tasks;

         namespace Backend {
           internal partial class Backend : IBackend {
             #if __IOS__
             private const string DllName = \"__Internal\";
             #else
             private const string DllName = \"backend\";
             #endif

             public Task<byte[]> Fun2Async() {
               var (ret, userData) = Utils.PrepareTask<byte[]>();
               Fun2Native(userData, DelegateOnFfiResultByteArray32Cb);
               return ret;
             }

             [DllImport(DllName, EntryPoint = \"fun2\")]
             private static extern void Fun2Native(IntPtr userData, \
                                                   FfiResultByteArray32Cb cb);

             public Task<byte[]> Fun3Async() {
               var (ret, userData) = Utils.PrepareTask<byte[]>();
               Fun3Native(userData, DelegateOnFfiResultByteArrayNonceLenCb);
               return ret;
             }

             [DllImport(DllName, EntryPoint = \"fun3\")]
             private static extern void Fun3Native(IntPtr userData, \
                                                   FfiResultByteArrayNonceLenCb cb);

             private delegate void FfiResultByteArray32Cb(IntPtr userData, \
                                                          IntPtr result, \
                                                          IntPtr key);

             #if __IOS__
             [MonoPInvokeCallback(typeof(FfiResultByteArray32Cb))]
             #endif
             private static void OnFfiResultByteArray32Cb(IntPtr userData, \
                                                          IntPtr result, \
                                                          IntPtr key) {
               Utils.CompleteTask(userData, \
                                  Marshal.PtrToStructure<FfiResult>(result), \
                                  () => Utils.CopyToByteArray(key, 32));
             }

             private static readonly FfiResultByteArray32Cb DelegateOnFfiResultByteArray32Cb = OnFfiResultByteArray32Cb;

             private delegate void FfiResultByteArrayNonceLenCb(IntPtr userData, \
                                                                IntPtr result, \
                                                                IntPtr nonce);

             #if __IOS__
             [MonoPInvokeCallback(typeof(FfiResultByteArrayNonceLenCb))]
             #endif
             private static void OnFfiResultByteArrayNonceLenCb(IntPtr userData, \
                                                                IntPtr result, \
                                                                IntPtr nonce) {
               Utils.CompleteTask(\
                 userData, \
                 Marshal.PtrToStructure<FfiResult>(result), \
                 () => Utils.CopyToByteArray(nonce, (int) Constants.NonceLen));
             }

             private static readonly FfiResultByteArrayNonceLenCb DelegateOnFfiResultByteArrayNonceLenCb = OnFfiResultByteArrayNonceLenCb;

           }
         }
        "
    );

    assert_multiline_eq!(actual, expected);
}

#[test]
fn functions_taking_callback_taking_dynamic_array() {
    let outputs = compile!(LangCSharp::default(), {
        // Primitive type.
        #[no_mangle]
        pub extern "C" fn fun0(
            user_data: *mut c_void,
            cb: extern "C" fn(
                user_data: *mut c_void,
                result: *const FfiResult,
                data_ptr: *const u8,
                data_len: usize,
            ),
        ) {
        }

        // Structures.
        #[no_mangle]
        pub extern "C" fn fun1(
            user_data: *mut c_void,
            cb: extern "C" fn(
                user_data: *mut c_void,
                result: *const FfiResult,
                records_ptr: *const Record,
                records_len: usize,
            ),
        ) {
        }
    });

    let actual = fetch(&outputs, "Backend.cs");
    let expected = indoc!(
        "using System;
         using System.Collections.Generic;
         using System.Linq;
         using System.Runtime.InteropServices;
         using System.Threading.Tasks;

         namespace Backend {
           internal partial class Backend : IBackend {
             #if __IOS__
             private const string DllName = \"__Internal\";
             #else
             private const string DllName = \"backend\";
             #endif

             public Task<List<byte>> Fun0Async() {
               var (ret, userData) = Utils.PrepareTask<List<byte>>();
               Fun0Native(userData, DelegateOnFfiResultByteListCb);
               return ret;
             }

             [DllImport(DllName, EntryPoint = \"fun0\")]
             private static extern void Fun0Native(IntPtr userData, \
                                                   FfiResultByteListCb cb);

             public Task<List<Record>> Fun1Async() {
               var (ret, userData) = Utils.PrepareTask<List<Record>>();
               Fun1Native(userData, DelegateOnFfiResultRecordListCb);
               return ret;
             }

             [DllImport(DllName, EntryPoint = \"fun1\")]
             private static extern void Fun1Native(IntPtr userData, \
                                                   FfiResultRecordListCb cb);

             private delegate void FfiResultByteListCb(IntPtr userData, \
                                                       IntPtr result, \
                                                       IntPtr dataPtr, \
                                                       UIntPtr dataLen);

             #if __IOS__
             [MonoPInvokeCallback(typeof(FfiResultByteListCb))]
             #endif
             private static void OnFfiResultByteListCb(IntPtr userData, \
                                                       IntPtr result, \
                                                       IntPtr dataPtr, \
                                                       UIntPtr dataLen) {
               Utils.CompleteTask(userData, \
                                  Marshal.PtrToStructure<FfiResult>(result), \
                                  () => Utils.CopyToByteList(dataPtr, (int) dataLen));
             }

             private static readonly FfiResultByteListCb DelegateOnFfiResultByteListCb = OnFfiResultByteListCb;

             private delegate void FfiResultRecordListCb(IntPtr userData, \
                                                          IntPtr result, \
                                                          IntPtr recordsPtr, \
                                                          UIntPtr recordsLen);

             #if __IOS__
             [MonoPInvokeCallback(typeof(FfiResultRecordListCb))]
             #endif
             private static void OnFfiResultRecordListCb(IntPtr userData, \
                                                         IntPtr result, \
                                                         IntPtr recordsPtr, \
                                                         UIntPtr recordsLen) {
               Utils.CompleteTask(userData, \
                                  Marshal.PtrToStructure<FfiResult>(result), \
                                  () => Utils.CopyToObjectList<Record>(\
                                    recordsPtr, \
                                    (int) recordsLen));
             }

             private static readonly FfiResultRecordListCb DelegateOnFfiResultRecordListCb = OnFfiResultRecordListCb;

           }
         }
        "
    );

    assert_multiline_eq!(actual, expected);
}

#[test]
fn functions_with_return_values() {
    let outputs = compile!(LangCSharp::default(), {
        #[no_mangle]
        pub extern "C" fn fun(arg: i32) -> bool {}
    });

    let actual = fetch(&outputs, "Backend.cs");
    let expected = indoc!(
        "using System;
         using System.Collections.Generic;
         using System.Linq;
         using System.Runtime.InteropServices;
         using System.Threading.Tasks;

         namespace Backend {
           internal partial class Backend : IBackend {
             #if __IOS__
             private const string DllName = \"__Internal\";
             #else
             private const string DllName = \"backend\";
             #endif

             public bool Fun(int arg) {
               var ret = FunNative(arg);
               return ret;
             }

             [DllImport(DllName, EntryPoint = \"fun\")]
             private static extern bool FunNative(int arg);

           }
         }
        "
    );

    assert_multiline_eq!(actual, expected);
}

#[test]
fn functions_taking_out_param() {
    let outputs = compile!(LangCSharp::default(), {
        #[no_mangle]
        pub extern "C" fn fun(o_app: *mut *mut App) {}
    });

    let actual = fetch(&outputs, "Backend.cs");
    let expected = indoc!(
        "using System;
         using System.Collections.Generic;
         using System.Linq;
         using System.Runtime.InteropServices;
         using System.Threading.Tasks;

         namespace Backend {
           internal partial class Backend : IBackend {
             #if __IOS__
             private const string DllName = \"__Internal\";
             #else
             private const string DllName = \"backend\";
             #endif

             public void Fun(out IntPtr oApp) {
               FunNative(out oApp);
             }

             [DllImport(DllName, EntryPoint = \"fun\")]
             private static extern void FunNative(out IntPtr oApp);

           }
         }
        "
    );
    assert_multiline_eq!(actual, expected);
}

#[test]
fn constants() {
    let mut lang = LangCSharp::new();
    lang.add_const("byte", "Custom", 45);

    let outputs = compile!(lang, {
        pub const NUMBER: i32 = 123;
        pub const STRING: &'static str = "hello world";
        pub const ARRAY: [u8; 4] = [0, 1, 2, 3];

        pub const STRUCT_VALUE: Record = Record {
            id: 0,
            secret_code: "xyz",
        };

        pub const STRUCT_REF: &'static Record = &Record {
            id: 1,
            secret_code: "xyz",
        };

        pub const EMPTY_STR: *const c_char = 0 as *const c_char;
    });

    let actual = fetch(&outputs, "Constants.cs");
    let expected = indoc!(
        "using System;
         using JetBrains.Annotations;

         namespace Backend {
           [PublicAPI]
           public static class Constants {
             public const int Number = 123;
             public const string String = \"hello world\";
             public static readonly byte[] Array = new byte[] { 0, 1, 2, 3 };
             public static readonly Record StructValue = new Record { \
                 id = 0, secretCode = \"xyz\" };
             public static readonly Record StructRef = new Record { \
                 id = 1, secretCode = \"xyz\" };
             public const string EmptyStr = \"\";
             public const byte Custom = 45;
           }
         }
        "
    );

    assert_multiline_eq!(actual, expected);
}

#[test]
fn arrays() {
    let outputs = compile!(LangCSharp::default(), {
        pub const ARRAY_SIZE: usize = 20;
        pub type Key = [u8; 32];

        #[no_mangle]
        pub extern "C" fn fun0(a: [u8; 10], b: [u8; ARRAY_SIZE]) {}

        #[no_mangle]
        pub extern "C" fn fun1(a: *const [u8; 10]) {}

        #[no_mangle]
        pub extern "C" fn fun2(a: *const Key) {}
    });

    let actual = fetch(&outputs, "Backend.cs");
    let expected = indoc!(
        "using System;
         using System.Collections.Generic;
         using System.Linq;
         using System.Runtime.InteropServices;
         using System.Threading.Tasks;

         namespace Backend {
           internal partial class Backend : IBackend {
             #if __IOS__
             private const string DllName = \"__Internal\";
             #else
             private const string DllName = \"backend\";
             #endif

             public void Fun0(byte[] a, byte[] b) {
               Fun0Native(a, b);
             }

             [DllImport(DllName, EntryPoint = \"fun0\")]
             private static extern void Fun0Native(\
               [MarshalAs(UnmanagedType.LPArray, SizeConst = 10)] \
               byte[] a, \
               [MarshalAs(UnmanagedType.LPArray, SizeConst = (int) Constants.ArraySize)] \
               byte[] b);

             public void Fun1(byte[] a) {
               Fun1Native(a);
             }

             [DllImport(DllName, EntryPoint = \"fun1\")]
             private static extern void Fun1Native(\
               [MarshalAs(UnmanagedType.LPArray, SizeConst = 10)] \
               byte[] a);

             public void Fun2(byte[] a) {
               Fun2Native(a);
             }

             [DllImport(DllName, EntryPoint = \"fun2\")]
             private static extern void Fun2Native(\
               [MarshalAs(UnmanagedType.LPArray, SizeConst = 32)] \
               byte[] a);

           }
         }
        "
    );

    assert_multiline_eq!(actual, expected);
}

#[test]
fn opaque_types() {
    let mut lang = LangCSharp::new();
    lang.add_opaque_type("Handle");

    let outputs = compile!(lang, {
        #[no_mangle]
        pub extern "C" fn fun0(handle: *const Handle) {}

        #[repr(C)]
        pub struct Context {
            handle: *const Handle,
        }
    });

    let actual = fetch(&outputs, "Types.cs");
    let expected = indoc!(
        "using System;
         using System.Collections.Generic;
         using System.Runtime.InteropServices;
         using JetBrains.Annotations;

         namespace Backend {
           [PublicAPI]
           public struct Context {
             public IntPtr Handle;
           }

         }
        "
    );
    assert_multiline_eq!(actual, expected);

    let actual = fetch(&outputs, "Backend.cs");
    let expected = indoc!(
        "using System;
         using System.Collections.Generic;
         using System.Linq;
         using System.Runtime.InteropServices;
         using System.Threading.Tasks;

         namespace Backend {
           internal partial class Backend : IBackend {
             #if __IOS__
             private const string DllName = \"__Internal\";
             #else
             private const string DllName = \"backend\";
             #endif

             public void Fun0(IntPtr handle) {
               Fun0Native(handle);
             }

             [DllImport(DllName, EntryPoint = \"fun0\")]
             private static extern void Fun0Native(IntPtr handle);

           }
         }
        "
    );
    assert_multiline_eq!(actual, expected);
}

#[test]
fn interface() {
    let outputs = compile!(LangCSharp::default(), {
        #[no_mangle]
        pub extern "C" fn fun(
            enabled: bool,
            user_data: *mut c_void,
            cb: extern "C" fn(user_data: *mut c_void, result: *const FfiResult),
        ) {
        }
    });

    let actual = fetch(&outputs, "IBackend.cs");
    let expected = indoc!(
        "using System;
         using System.Collections.Generic;
         using System.Threading.Tasks;

         namespace Backend {
           public partial interface IBackend {
             Task FunAsync(bool enabled);
           }
         }
        "
    );

    assert_multiline_eq!(actual, expected);
}
