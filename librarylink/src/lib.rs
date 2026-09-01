//! Wolfram LibraryLink transport for Hyperbolica's stable JSON C ABI.
//!
//! LibraryLink and the stable C ABI deliberately live in separate dynamic
//! libraries: upstream HyperFLINT assigns the same `hf_*` symbol names to
//! incompatible function signatures on the two surfaces.  This adapter loads
//! `libhyperbolica` once, delegates to its stable C ABI, and exposes the exact
//! LibraryLink entry points used by the pinned SubTropica loader.

use std::cell::RefCell;
use std::ffi::{CStr, CString, c_char, c_int};
use std::path::{Path, PathBuf};
#[cfg(any(target_os = "windows", test))]
use std::ptr;
use std::sync::OnceLock;

use libloading::Library;
use serde_json::json;
use wolfram_library_link_sys::{
    LIBRARY_FUNCTION_ERROR, LIBRARY_NO_ERROR, MArgument, WolframLibraryData, WolframLibraryVersion,
    mint,
};

const BACKEND_PATH_ENV: &str = "HYPERBOLICA_LIBRARY_PATH";
const VERSION_OVERRIDE: &str = env!("HYPERBOLICA_LIBRARYLINK_VERSION_OVERRIDE");
const SCHEMA_VERSION: u64 = 2;

type Operation = unsafe extern "C" fn(*const c_char) -> *mut c_char;
type FreeString = unsafe extern "C" fn(*mut c_char);
type VersionString = unsafe extern "C" fn() -> *const c_char;

/// Loaded stable-ABI kernel.  `Library` must outlive every copied function
/// pointer, so it is retained in the process-wide `BACKEND` cell.
struct Backend {
    _library: Library,
    free_string: FreeString,
    version_string: VersionString,
    find_lr_orders: Operation,
    factor_table: Operation,
    find_lr_orders_scan: Operation,
    hyperflint_sym: Operation,
}

impl Backend {
    unsafe fn load(path: &Path) -> Result<Self, String> {
        // SAFETY: loading a user-selected shared object is inherently unsafe.
        // We immediately validate every required symbol before publishing it.
        let library = unsafe { Library::new(path) }
            .map_err(|error| format!("could not load `{}`: {error}", path.display()))?;

        macro_rules! symbol {
            ($name:literal, $ty:ty) => {{
                // SAFETY: the stable ABI header pins each listed signature.
                let loaded = unsafe { library.get::<$ty>(concat!($name, "\0").as_bytes()) }
                    .map_err(|error| {
                        format!(
                            "backend `{}` is missing `{}`: {error}",
                            path.display(),
                            $name
                        )
                    })?;
                *loaded
            }};
        }

        let free_string = symbol!("hf_free_string", FreeString);
        let version_string = symbol!("hf_version_string", VersionString);
        let find_lr_orders = symbol!("hf_find_lr_orders", Operation);
        let factor_table = symbol!("hf_factor_table", Operation);
        let find_lr_orders_scan = symbol!("hf_find_lr_orders_scan", Operation);
        let hyperflint_sym = symbol!("hf_hyperflint_sym", Operation);

        // Fail during initialization rather than arming SubTropica with a
        // library whose version entry point is malformed.
        // SAFETY: `version_string` was resolved with the pinned ABI type.
        let version = unsafe { version_string() };
        if version.is_null() {
            return Err(format!(
                "backend `{}` returned NULL from hf_version_string",
                path.display()
            ));
        }
        // SAFETY: the stable ABI promises a process-lifetime C string.
        unsafe { CStr::from_ptr(version) }
            .to_str()
            .map_err(|error| {
                format!(
                    "backend `{}` returned a non-UTF-8 version: {error}",
                    path.display()
                )
            })?;

        Ok(Self {
            _library: library,
            free_string,
            version_string,
            find_lr_orders,
            factor_table,
            find_lr_orders_scan,
            hyperflint_sym,
        })
    }

    fn version(&self) -> Result<&str, String> {
        // SAFETY: the symbol was validated at load time and its return has
        // process lifetime while `_library` remains loaded.
        let pointer = unsafe { (self.version_string)() };
        if pointer.is_null() {
            return Err("hf_version_string returned NULL".into());
        }
        // SAFETY: guaranteed by the stable ABI contract.
        unsafe { CStr::from_ptr(pointer) }
            .to_str()
            .map_err(|error| format!("hf_version_string returned non-UTF-8: {error}"))
    }
}

static BACKEND: OnceLock<Result<Backend, String>> = OnceLock::new();

thread_local! {
    /// LibraryLink copies a returned UTF8String after the native call returns.
    /// Retaining the most recent result per thread follows Wolfram's official
    /// Rust binding and bounds adapter-owned return storage to one string per
    /// calling thread.
    static RETURNED_STRING: RefCell<Option<CString>> = const { RefCell::new(None) };
}

fn backend() -> Result<&'static Backend, &'static str> {
    match BACKEND.get_or_init(load_backend) {
        Ok(backend) => Ok(backend),
        Err(error) => Err(error.as_str()),
    }
}

fn load_backend() -> Result<Backend, String> {
    let candidates = backend_candidates()?;
    let mut failures = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        // SAFETY: `Backend::load` validates the complete required ABI surface.
        match unsafe { Backend::load(&candidate) } {
            Ok(backend) => return Ok(backend),
            Err(error) => failures.push(error),
        }
    }
    Err(format!(
        "no usable Hyperbolica stable-ABI library found; set {BACKEND_PATH_ENV} to its absolute path ({})",
        failures.join("; ")
    ))
}

fn backend_candidates() -> Result<Vec<PathBuf>, String> {
    if let Some(path) = std::env::var_os(BACKEND_PATH_ENV) {
        if path.is_empty() {
            return Err(format!("{BACKEND_PATH_ENV} is set but empty"));
        }
        return Ok(vec![PathBuf::from(path)]);
    }

    let name = backend_file_name();
    let mut paths = Vec::new();
    if let Some(directory) = current_library_directory() {
        paths.push(directory.join(name));
    }

    // Useful for an unstaged source checkout.  Production installs normally
    // use the same-directory candidate above.
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    if cfg!(debug_assertions) {
        paths.push(checkout.join("target").join("debug").join(name));
    } else {
        paths.push(checkout.join("target").join("release").join(name));
    }

    // Last resort: honor the platform dynamic-loader search path.
    paths.push(PathBuf::from(name));
    paths.dedup();
    Ok(paths)
}

#[cfg(target_os = "windows")]
fn backend_file_name() -> &'static str {
    "hyperbolica.dll"
}

#[cfg(target_os = "macos")]
fn backend_file_name() -> &'static str {
    "libhyperbolica.dylib"
}

#[cfg(all(unix, not(target_os = "macos")))]
fn backend_file_name() -> &'static str {
    "libhyperbolica.so"
}

#[cfg(not(any(unix, target_os = "windows")))]
compile_error!("Hyperbolica LibraryLink currently supports Unix and Windows targets");

#[cfg(unix)]
fn current_library_directory() -> Option<PathBuf> {
    // SAFETY: `dladdr` only reads the supplied code address and initializes
    // `info` on success.  `dli_fname` then points at a live loader-owned C
    // string.
    unsafe {
        let mut info: libc::Dl_info = std::mem::zeroed();
        let address = WolframLibrary_getVersion as *const () as *const libc::c_void;
        if libc::dladdr(address, &mut info) == 0 || info.dli_fname.is_null() {
            return None;
        }
        let path = CStr::from_ptr(info.dli_fname).to_str().ok()?;
        Path::new(path).parent().map(Path::to_path_buf)
    }
}

#[cfg(target_os = "windows")]
fn current_library_directory() -> Option<PathBuf> {
    use std::ffi::OsString;
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStringExt;

    const GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS: u32 = 0x0000_0004;
    const GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT: u32 = 0x0000_0002;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetModuleHandleExW(
            flags: u32,
            module_name_or_address: *const u16,
            module: *mut *mut c_void,
        ) -> i32;
        fn GetModuleFileNameW(module: *mut c_void, filename: *mut u16, size: u32) -> u32;
    }

    // SAFETY: FROM_ADDRESS makes the second argument a code address.  The
    // unchanged-refcount flag avoids changing module lifetime.
    unsafe {
        let mut module = ptr::null_mut();
        let address = WolframLibrary_getVersion as *const () as *const u16;
        if GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            address,
            &mut module,
        ) == 0
        {
            return None;
        }
        let mut buffer = vec![0_u16; 32_768];
        let length = GetModuleFileNameW(module, buffer.as_mut_ptr(), buffer.len() as u32);
        if length == 0 || length as usize >= buffer.len() {
            return None;
        }
        let path = PathBuf::from(OsString::from_wide(&buffer[..length as usize]));
        path.parent().map(Path::to_path_buf)
    }
}

fn effective_version(backend: &Backend) -> Result<&str, String> {
    if VERSION_OVERRIDE.is_empty() {
        backend.version()
    } else {
        Ok(VERSION_OVERRIDE)
    }
}

fn error_response(op: &str, message: impl ToString) -> CString {
    let version = backend()
        .ok()
        .and_then(|backend| effective_version(backend).ok())
        .unwrap_or_else(|| {
            if VERSION_OVERRIDE.is_empty() {
                "unavailable"
            } else {
                VERSION_OVERRIDE
            }
        });
    let value = json!({
        "op": op,
        "schema_version": SCHEMA_VERSION,
        "hf_version": version,
        "error": message.to_string(),
    });
    CString::new(serde_json::to_vec(&value).expect("JSON serialization cannot fail"))
        .expect("serialized JSON cannot contain NUL")
}

fn copy_response(backend: &Backend, pointer: *mut c_char, op: &str) -> CString {
    if pointer.is_null() {
        return error_response(op, "Hyperbolica backend returned NULL");
    }

    struct OwnedBackendString {
        pointer: *mut c_char,
        free: FreeString,
    }
    impl Drop for OwnedBackendString {
        fn drop(&mut self) {
            // SAFETY: `pointer` came from the corresponding stable ABI and is
            // released exactly once through its paired deallocator.
            unsafe { (self.free)(self.pointer) };
        }
    }

    let owned = OwnedBackendString {
        pointer,
        free: backend.free_string,
    };
    // SAFETY: a non-null stable-ABI result is NUL terminated and live until
    // the guard drops below.
    let bytes = unsafe { CStr::from_ptr(owned.pointer) }.to_bytes();

    if VERSION_OVERRIDE.is_empty() {
        return CString::new(bytes).expect("C string bytes cannot contain NUL");
    }

    restamp_response_version(bytes, VERSION_OVERRIDE)
        .unwrap_or_else(|message| error_response(op, message))
}

/// Replace only the JSON string value of the stable envelope's `hf_version`
/// field.  Avoiding a parse/re-serialize of potentially large integration
/// results keeps stamping to one linear byte copy with no expression tree.
fn restamp_response_version(bytes: &[u8], version: &str) -> Result<CString, String> {
    const KEY: &[u8] = b"\"hf_version\":";
    let key = bytes
        .windows(KEY.len())
        .position(|window| window == KEY)
        .ok_or_else(|| "backend response has no hf_version field".to_owned())?;
    let value_start = key + KEY.len();
    if bytes.get(value_start) != Some(&b'"') {
        return Err("backend response has a non-string hf_version field".into());
    }

    let mut escaped = false;
    let mut value_end = None;
    for (offset, &byte) in bytes[value_start + 1..].iter().enumerate() {
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
            value_end = Some(value_start + 1 + offset);
            break;
        }
    }
    let value_end = value_end
        .ok_or_else(|| "backend response has an unterminated hf_version field".to_owned())?;
    let encoded = serde_json::to_vec(version)
        .map_err(|error| format!("could not encode LibraryLink version: {error}"))?;
    let mut stamped =
        Vec::with_capacity(bytes.len() - (value_end - value_start + 1) + encoded.len());
    stamped.extend_from_slice(&bytes[..value_start]);
    stamped.extend_from_slice(&encoded);
    stamped.extend_from_slice(&bytes[value_end + 1..]);
    CString::new(stamped).map_err(|_| "backend response contains an interior NUL".into())
}

unsafe fn set_utf8_result(result: MArgument, value: CString) -> Result<(), ()> {
    // SAFETY: reading the selected union field is valid for a LibraryLink
    // UTF8String result.  The caller owns the outer pointer slot.
    let slot = unsafe { result.utf8string };
    if slot.is_null() {
        return Err(());
    }
    let pointer = value.as_ptr().cast_mut();
    RETURNED_STRING.with(|stored| {
        *stored.borrow_mut() = Some(value);
    });
    // SAFETY: `slot` was checked above and points to the kernel's result slot.
    unsafe { *slot = pointer };
    Ok(())
}

unsafe fn set_integer_result(result: MArgument, value: mint) -> Result<(), ()> {
    // SAFETY: reading the selected union field is valid for an Integer result.
    let slot = unsafe { result.integer };
    if slot.is_null() {
        return Err(());
    }
    // SAFETY: `slot` points to the kernel's integer result storage.
    unsafe { *slot = value };
    Ok(())
}

unsafe fn input_pointer(argc: mint, args: *mut MArgument) -> Result<*mut c_char, ()> {
    if argc < 1 || args.is_null() {
        return Err(());
    }
    // SAFETY: `argc >= 1` and the LibraryLink contract supplies at least one
    // live argument slot.
    let slot = unsafe { (*args).utf8string };
    if slot.is_null() {
        return Err(());
    }
    // SAFETY: the selected UTF8String slot contains the input C string.
    let input = unsafe { *slot };
    if input.is_null() {
        return Err(());
    }
    Ok(input)
}

unsafe fn disown_input(lib_data: WolframLibraryData, input: *mut c_char) {
    if lib_data.is_null() {
        return;
    }
    // SAFETY: `lib_data` is supplied by the Wolfram runtime.  The callback is
    // the first field in every supported WolframLibraryData version and is
    // represented by the official generated bindings used by this crate.
    if let Some(disown) = unsafe { (*lib_data).UTF8String_disown } {
        // SAFETY: this is precisely the input pointer obtained from the
        // LibraryLink MArgument, and it is disowned once after synchronous use.
        unsafe { disown(input) };
    }
}

unsafe fn call_operation(
    lib_data: WolframLibraryData,
    argc: mint,
    args: *mut MArgument,
    result: MArgument,
    op: &'static str,
    select: fn(&Backend) -> Operation,
) -> c_int {
    let input = match unsafe { input_pointer(argc, args) } {
        Ok(input) => input,
        Err(()) => return LIBRARY_FUNCTION_ERROR as c_int,
    };

    let output = match backend() {
        Ok(backend) => {
            // SAFETY: the operation was resolved from the validated stable ABI
            // and consumes the input synchronously without retaining it.
            let raw = unsafe { select(backend)(input.cast_const()) };
            // The kernel's input can be released as soon as the stable call
            // returns, including when that call reports an in-band error.
            unsafe { disown_input(lib_data, input) };
            copy_response(backend, raw, op)
        }
        Err(error) => {
            unsafe { disown_input(lib_data, input) };
            error_response(op, error)
        }
    };

    match unsafe { set_utf8_result(result, output) } {
        Ok(()) => LIBRARY_NO_ERROR as c_int,
        Err(()) => LIBRARY_FUNCTION_ERROR as c_int,
    }
}

fn contain(function: impl FnOnce() -> c_int) -> c_int {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(function))
        .unwrap_or(LIBRARY_FUNCTION_ERROR as c_int)
}

/// LibraryLink ABI version supported by the generated Wolfram bindings.
#[unsafe(no_mangle)]
pub extern "C" fn WolframLibrary_getVersion() -> mint {
    WolframLibraryVersion as mint
}

/// Eagerly validate the sibling Hyperbolica stable-ABI library.
#[unsafe(no_mangle)]
pub extern "C" fn WolframLibrary_initialize(_lib_data: WolframLibraryData) -> c_int {
    contain(|| match backend() {
        Ok(_) => LIBRARY_NO_ERROR as c_int,
        Err(_) => LIBRARY_FUNCTION_ERROR as c_int,
    })
}

/// Release adapter-owned state for the unloading thread.
#[unsafe(no_mangle)]
pub extern "C" fn WolframLibrary_uninitialize(_lib_data: WolframLibraryData) {
    let _ = std::panic::catch_unwind(|| {
        RETURNED_STRING.with(|stored| {
            stored.borrow_mut().take();
        });
    });
}

/// `hf_version[] -> UTF8String`.
///
/// # Safety
///
/// `result` must contain the writable UTF8String result slot supplied by
/// LibraryLink. The other raw arguments, when non-null, must originate from
/// the same LibraryLink invocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hf_version(
    _lib_data: WolframLibraryData,
    _argc: mint,
    _args: *mut MArgument,
    result: MArgument,
) -> c_int {
    contain(|| {
        let value = match backend().and_then(|backend| {
            effective_version(backend).map_err(|_| "backend returned an invalid version")
        }) {
            Ok(version) => match CString::new(version) {
                Ok(version) => version,
                Err(_) => return LIBRARY_FUNCTION_ERROR as c_int,
            },
            Err(_) => return LIBRARY_FUNCTION_ERROR as c_int,
        };
        // SAFETY: the raw result slot is validated by `set_utf8_result`.
        match unsafe { set_utf8_result(result, value) } {
            Ok(()) => LIBRARY_NO_ERROR as c_int,
            Err(()) => LIBRARY_FUNCTION_ERROR as c_int,
        }
    })
}

/// `hf_clear_state[] -> Integer`.
///
/// Hyperbolica's algebraic-letter table is already reset and exclusively held
/// by every request that can mutate it.  The adapter therefore owns no
/// cross-request algebraic state; clearing its last return buffer is the only
/// transport-local reset needed.
///
/// # Safety
///
/// `result` must contain the writable Integer result slot supplied by
/// LibraryLink. The other raw arguments, when non-null, must originate from
/// the same LibraryLink invocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hf_clear_state(
    _lib_data: WolframLibraryData,
    _argc: mint,
    _args: *mut MArgument,
    result: MArgument,
) -> c_int {
    contain(|| {
        RETURNED_STRING.with(|stored| {
            stored.borrow_mut().take();
        });
        // SAFETY: the raw result slot is validated by `set_integer_result`.
        match unsafe { set_integer_result(result, 0) } {
            Ok(()) => LIBRARY_NO_ERROR as c_int,
            Err(()) => LIBRARY_FUNCTION_ERROR as c_int,
        }
    })
}

macro_rules! operation {
    ($name:ident, $op:literal, $field:ident) => {
        #[doc = concat!("LibraryLink JSON wrapper for `", $op, "`.")]
        #[doc = ""]
        #[doc = "# Safety"]
        #[doc = ""]
        #[doc = "`args` must address at least `argc` LibraryLink argument slots, and"]
        #[doc = "`result` must contain a writable UTF8String result slot. `lib_data`"]
        #[doc = "must be null or the runtime pointer supplied for this invocation."]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(
            lib_data: WolframLibraryData,
            argc: mint,
            args: *mut MArgument,
            result: MArgument,
        ) -> c_int {
            contain(|| {
                // SAFETY: `call_operation` validates every raw argument slot
                // before dereferencing it and contains backend failures.
                unsafe {
                    call_operation(lib_data, argc, args, result, $op, |backend| backend.$field)
                }
            })
        }
    };
}

operation!(hf_find_lr_orders, "find_lr_orders", find_lr_orders);
operation!(hf_factor_table, "factor_table", factor_table);
operation!(
    hf_find_lr_orders_scan,
    "find_lr_orders_scan",
    find_lr_orders_scan
);
operation!(hf_hyperflint_sym, "hyperflint", hyperflint_sym);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_margument_layout_is_one_pointer_slot() {
        assert_eq!(
            std::mem::size_of::<MArgument>(),
            std::mem::size_of::<usize>()
        );
        assert_eq!(
            std::mem::align_of::<MArgument>(),
            std::mem::align_of::<usize>()
        );
        assert_eq!(WolframLibrary_getVersion(), 6);
    }

    #[test]
    fn clear_state_writes_a_machine_integer_without_a_runtime() {
        let mut output: mint = -1;
        let result = MArgument {
            integer: &mut output,
        };
        // SAFETY: `result` contains a live Integer result slot.
        let code = unsafe { hf_clear_state(ptr::null_mut(), 0, ptr::null_mut(), result) };
        assert_eq!(code, LIBRARY_NO_ERROR as c_int);
        assert_eq!(output, 0);
    }

    #[test]
    fn every_json_operation_rejects_missing_argument_slots() {
        type Function =
            unsafe extern "C" fn(WolframLibraryData, mint, *mut MArgument, MArgument) -> c_int;
        let operations: [Function; 4] = [
            hf_find_lr_orders,
            hf_factor_table,
            hf_find_lr_orders_scan,
            hf_hyperflint_sym,
        ];
        let mut output = ptr::null_mut();
        for operation in operations {
            let result = MArgument {
                utf8string: &mut output,
            };
            // SAFETY: the missing input is intentional and must be rejected
            // before any pointer dereference or backend load.
            let code = unsafe { operation(ptr::null_mut(), 0, ptr::null_mut(), result) };
            assert_eq!(code, LIBRARY_FUNCTION_ERROR as c_int);
            assert!(output.is_null());
        }
    }

    #[test]
    fn version_stamping_preserves_the_response_outside_one_string_value() {
        let input = br#"{"op":"find_lr_orders","schema_version":2,"hf_version":"0.1.0.0","result":{"hf_version_shadow":"keep"}}"#;
        let stamped = restamp_response_version(input, "1.2.13\"candidate").unwrap();
        assert_eq!(
            stamped.to_str().unwrap(),
            r#"{"op":"find_lr_orders","schema_version":2,"hf_version":"1.2.13\"candidate","result":{"hf_version_shadow":"keep"}}"#
        );
    }
}
