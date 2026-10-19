//! Small host used by the C demo and by the reference C integration.
//!
//! It intentionally keeps old library handles mapped instead of unloading
//! them. That is conservative: a host-owned counter remains valid, while an
//! old callback cannot jump into an unmapped generation. Production hosts may
//! add a quiescence protocol before unloading old generations.

use crate::error::{MorphError, MorphResult};
use std::fs;
use std::path::{Path, PathBuf};

pub fn run(args: &[String]) -> MorphResult<()> {
    let library = flag_value(args, "--library")?;
    let control = flag_value(args, "--control")?;
    let status = flag_value(args, "--status")?;
    #[cfg(any(unix, windows))]
    {
        return run_supported(
            PathBuf::from(library),
            PathBuf::from(control),
            PathBuf::from(status),
        );
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (library, control, status);
        Err(MorphError::Unsupported(
            "the reference C host supports Unix dlopen and Windows LoadLibrary; use an SDK host on this platform"
                .to_string(),
        ))
    }
}

fn flag_value(args: &[String], flag: &str) -> MorphResult<String> {
    args.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
        .ok_or_else(|| {
            MorphError::InvalidArguments(format!("internal-c-host requires {flag} PATH"))
        })
}

#[cfg(any(unix, windows))]
#[repr(C)]
struct MorphPluginV1 {
    abi_version: u32,
    name: *const std::os::raw::c_char,
    render: Option<unsafe extern "C" fn(u64) -> *const std::os::raw::c_char>,
}

#[cfg(any(unix, windows))]
type GetPlugin = unsafe extern "C" fn() -> *const MorphPluginV1;

#[cfg(any(unix, windows))]
type RawHandle = *mut std::os::raw::c_void;

#[cfg(any(unix, windows))]
struct LoadedLibrary {
    handle: RawHandle,
    api: *const MorphPluginV1,
}

#[cfg(all(unix, target_os = "linux"))]
#[link(name = "dl")]
extern "C" {
    fn dlopen(filename: *const std::os::raw::c_char, flags: i32) -> *mut std::os::raw::c_void;
    fn dlsym(
        handle: *mut std::os::raw::c_void,
        symbol: *const std::os::raw::c_char,
    ) -> *mut std::os::raw::c_void;
    fn dlerror() -> *const std::os::raw::c_char;
    fn dlclose(handle: *mut std::os::raw::c_void) -> i32;
}

#[cfg(all(unix, not(target_os = "linux")))]
extern "C" {
    fn dlopen(filename: *const std::os::raw::c_char, flags: i32) -> *mut std::os::raw::c_void;
    fn dlsym(
        handle: *mut std::os::raw::c_void,
        symbol: *const std::os::raw::c_char,
    ) -> *mut std::os::raw::c_void;
    fn dlerror() -> *const std::os::raw::c_char;
    fn dlclose(handle: *mut std::os::raw::c_void) -> i32;
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryW(file_name: *const u16) -> *mut std::os::raw::c_void;
    fn GetProcAddress(
        module: *mut std::os::raw::c_void,
        procedure_name: *const std::os::raw::c_char,
    ) -> *mut std::os::raw::c_void;
    fn FreeLibrary(module: *mut std::os::raw::c_void) -> i32;
    fn GetLastError() -> u32;
}

#[cfg(any(unix, windows))]
fn run_supported(library: PathBuf, control: PathBuf, status: PathBuf) -> MorphResult<()> {
    use std::ffi::CStr;
    use std::time::Duration;

    let mut loaded = Vec::new();
    let mut active = None;
    let mut requested = String::new();
    let mut requested_generation = 0_u64;
    let mut counter = 1_u64;
    if fs::read_to_string(&control)
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        let _ = fs::write(&control, library.to_string_lossy().as_bytes());
    }
    println!(
        "MORPH_C_HOST_START pid={} control={}",
        std::process::id(),
        control.display()
    );
    loop {
        let command = fs::read_to_string(&control).unwrap_or_default();
        let command = command.trim().to_string();
        if command == "STOP" {
            break;
        }
        if !command.is_empty() && command != requested {
            requested_generation = requested_generation.saturating_add(1);
            match load(&PathBuf::from(&command)) {
                Ok(value) => {
                    let index = loaded.len();
                    loaded.push(value);
                    active = Some(index);
                    requested = command.clone();
                    let api = unsafe { &*loaded[index].api };
                    let name = if api.name.is_null() {
                        "unnamed".to_string()
                    } else {
                        unsafe { CStr::from_ptr(api.name).to_string_lossy().into_owned() }
                    };
                    println!(
                        "MORPH_C_RELOAD pid={} generation={} host_state={} name={}",
                        std::process::id(),
                        index + 1,
                        counter,
                        name
                    );
                    write_status(
                        &status,
                        requested_generation,
                        "ok",
                        "library generation activated",
                    );
                }
                Err(error) => {
                    eprintln!(
                        "MORPH_C_RELOAD_FAILED pid={} path={} error={error}",
                        std::process::id(),
                        command
                    );
                    write_status(&status, requested_generation, "error", &error.to_string());
                    requested = command;
                }
            }
        }
        if let Some(index) = active {
            let api = unsafe { &*loaded[index].api };
            let render = api.render.expect("validated render function");
            let value = unsafe { render(counter) };
            let value = if value.is_null() {
                "<plugin returned null>".to_string()
            } else {
                unsafe { CStr::from_ptr(value).to_string_lossy().into_owned() }
            };
            println!(
                "MORPH_C_TICK pid={} state={} value={}",
                std::process::id(),
                counter,
                value
            );
            counter = counter.saturating_add(1);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    println!(
        "MORPH_C_HOST_STOP pid={} state={}",
        std::process::id(),
        counter
    );
    write_status(&status, requested_generation, "stopped", "stop requested");
    // `loaded` intentionally owns all handles until process exit. Never call
    // the platform close operation here: old function pointers may still be
    // referenced by a host.
    let _handles = loaded.iter().map(|value| value.handle).collect::<Vec<_>>();
    Ok(())
}

#[cfg(any(unix, windows))]
fn load(path: &Path) -> MorphResult<LoadedLibrary> {
    let handle = open_library(path)
        .map_err(|error| MorphError::Backend(format!("cannot load {}: {error}", path.display())))?;
    let raw_symbol = match lookup_symbol(handle, b"morph_plugin_v1\0") {
        Ok(symbol) => symbol,
        Err(detail) => {
            close_library(handle);
            return Err(MorphError::Backend(format!(
                "{} does not export morph_plugin_v1: {}",
                path.display(),
                detail
            )));
        }
    };
    unsafe {
        let get_plugin: GetPlugin = std::mem::transmute(raw_symbol);
        let api = get_plugin();
        if api.is_null() {
            close_library(handle);
            return Err(MorphError::Backend(format!(
                "{} returned a null MorphPluginV1 descriptor",
                path.display()
            )));
        }
        if (*api).abi_version != 1 {
            let abi_version = (*api).abi_version;
            close_library(handle);
            return Err(MorphError::Backend(format!(
                "{} exposes ABI {}, expected 1",
                path.display(),
                abi_version
            )));
        }
        if (*api).render.is_none() {
            close_library(handle);
            return Err(MorphError::Backend(format!(
                "{} has no render function",
                path.display()
            )));
        }
        Ok(LoadedLibrary { handle, api })
    }
}

#[cfg(unix)]
fn open_library(path: &Path) -> Result<RawHandle, String> {
    use std::ffi::CString;

    const RTLD_NOW: i32 = 2;
    const RTLD_LOCAL: i32 = 0;
    let path_string = CString::new(path.to_string_lossy().as_bytes())
        .map_err(|_| "library path contains a NUL byte".to_string())?;
    let handle = unsafe { dlopen(path_string.as_ptr(), RTLD_NOW | RTLD_LOCAL) };
    if handle.is_null() {
        Err(unix_loader_error())
    } else {
        Ok(handle)
    }
}

#[cfg(unix)]
fn lookup_symbol(handle: RawHandle, symbol: &[u8]) -> Result<*mut std::os::raw::c_void, String> {
    let symbol = std::ffi::CString::new(&symbol[..symbol.len().saturating_sub(1)])
        .map_err(|_| "symbol contains a NUL byte".to_string())?;
    let raw_symbol = unsafe { dlsym(handle, symbol.as_ptr()) };
    if raw_symbol.is_null() {
        Err(unix_loader_error())
    } else {
        Ok(raw_symbol)
    }
}

#[cfg(unix)]
fn unix_loader_error() -> String {
    use std::ffi::CStr;

    let value = unsafe { dlerror() };
    if value.is_null() {
        "unknown dynamic-loader error".to_string()
    } else {
        unsafe { CStr::from_ptr(value) }
            .to_string_lossy()
            .into_owned()
    }
}

#[cfg(unix)]
fn close_library(handle: RawHandle) {
    unsafe {
        let _ = dlclose(handle);
    }
}

#[cfg(windows)]
fn open_library(path: &Path) -> Result<RawHandle, String> {
    use std::os::windows::ffi::OsStrExt;

    let path = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let handle = unsafe { LoadLibraryW(path.as_ptr()) };
    if handle.is_null() {
        Err(windows_loader_error())
    } else {
        Ok(handle)
    }
}

#[cfg(windows)]
fn lookup_symbol(handle: RawHandle, symbol: &[u8]) -> Result<*mut std::os::raw::c_void, String> {
    let raw_symbol =
        unsafe { GetProcAddress(handle, symbol.as_ptr() as *const std::os::raw::c_char) };
    if raw_symbol.is_null() {
        Err(windows_loader_error())
    } else {
        Ok(raw_symbol)
    }
}

#[cfg(windows)]
fn windows_loader_error() -> String {
    let code = unsafe { GetLastError() };
    format!("Windows loader error {code}")
}

#[cfg(windows)]
fn close_library(handle: RawHandle) {
    unsafe {
        let _ = FreeLibrary(handle);
    }
}

#[cfg(any(unix, windows))]
fn write_status(path: &Path, generation: u64, status: &str, message: &str) {
    let escape = |value: &str| {
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', " ")
    };
    let payload = format!(
        "{{\"generation\": {generation}, \"status\": \"{}\", \"message\": \"{}\", \"pid\": {}}}\n",
        escape(status),
        escape(message),
        std::process::id()
    );
    if let Err(error) = crate::file_ops::write_atomic(path, payload.as_bytes()) {
        eprintln!(
            "MORPH_C_STATUS_FAILED pid={} generation={} path={} error={error}",
            std::process::id(),
            generation,
            path.display()
        );
    }
}
