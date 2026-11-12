//! The Python on this machine, borrowed at run time for note scripts.
//!
//! Nothing is linked when the program is built: the interpreter's library is looked for when the first
//! script wants it, and only the calls every Python 3 keeps from version to version are used. A machine
//! without Python simply has no scripts; notes then pass the script unit untouched.
//!
//! All the talking is text: one function of our own module is called with strings and answers with one.

use std::ffi::{c_char, c_int, c_void, CString};
use std::sync::OnceLock;

type Obj = *mut c_void;

pub struct Py {
    _lib: libloading::Library,
    ensure: unsafe extern "C" fn() -> c_int,
    release: unsafe extern "C" fn(c_int),
    call: unsafe extern "C" fn(Obj, ...) -> Obj,
    text: unsafe extern "C" fn(*const c_char, isize) -> Obj,
    utf8: unsafe extern "C" fn(Obj, *mut isize) -> *const c_char,
    decref: unsafe extern "C" fn(Obj),
    clear: unsafe extern "C" fn(),
    door: Obj,
    pub version: String,
}

// the interpreter is only ever entered with its lock held
unsafe impl Send for Py {}
unsafe impl Sync for Py {}

static PY: OnceLock<Option<Py>> = OnceLock::new();

/// The names the interpreter's library goes by, newest first.
fn names() -> Vec<String> {
    let mut v = Vec::new();
    if let Ok(p) = std::env::var("CEREAL_PYTHON_LIB") {
        v.push(p);
    }
    for minor in (8..=24).rev() {
        if cfg!(windows) {
            v.push(format!("python3{minor}.dll"));
        } else if cfg!(target_os = "macos") {
            v.push(format!("libpython3.{minor}.dylib"));
        } else {
            v.push(format!("libpython3.{minor}.so.1.0"));
            v.push(format!("libpython3.{minor}.so"));
        }
    }
    if cfg!(windows) {
        v.push("python3.dll".into());
    } else if !cfg!(target_os = "macos") {
        v.push("libpython3.so".into());
    }
    v
}

fn open(name: &str) -> Option<libloading::Library> {
    #[cfg(unix)]
    unsafe {
        // the interpreter's own add-on modules look its symbols up by name, so they have to be on show
        use libloading::os::unix::{Library, RTLD_GLOBAL, RTLD_NOW};
        Library::open(Some(name), RTLD_NOW | RTLD_GLOBAL).ok().map(Into::into)
    }
    #[cfg(not(unix))]
    unsafe {
        libloading::Library::new(name).ok()
    }
}

/// The interpreter, started the first time it is asked for. `None` when the machine has none, or scripts are off.
pub fn get() -> Option<&'static Py> {
    PY.get_or_init(|| {
        if std::env::var_os("CEREAL_NO_SCRIPTS").is_some() {
            return None;
        }
        names().iter().find_map(|n| open(n).and_then(|lib| unsafe { Py::start(lib) }))
    })
    .as_ref()
}

impl Py {
    unsafe fn start(lib: libloading::Library) -> Option<Py> {
        macro_rules! sym {
            ($name:literal, $t:ty) => {
                *lib.get::<$t>($name).ok()?
            };
        }
        let is_up = sym!(b"Py_IsInitialized\0", unsafe extern "C" fn() -> c_int);
        let init = sym!(b"Py_InitializeEx\0", unsafe extern "C" fn(c_int));
        let compile = sym!(b"Py_CompileString\0", unsafe extern "C" fn(*const c_char, *const c_char, c_int) -> Obj);
        let eval = sym!(b"PyEval_EvalCode\0", unsafe extern "C" fn(Obj, Obj, Obj) -> Obj);
        let dict_new = sym!(b"PyDict_New\0", unsafe extern "C" fn() -> Obj);
        let dict_set = sym!(b"PyDict_SetItemString\0", unsafe extern "C" fn(Obj, *const c_char, Obj) -> c_int);
        let dict_get = sym!(b"PyDict_GetItemString\0", unsafe extern "C" fn(Obj, *const c_char) -> Obj);
        let builtins = sym!(b"PyEval_GetBuiltins\0", unsafe extern "C" fn() -> Obj);
        let save = sym!(b"PyEval_SaveThread\0", unsafe extern "C" fn() -> *mut c_void);
        let ver = sym!(b"Py_GetVersion\0", unsafe extern "C" fn() -> *const c_char);
        let incref = sym!(b"Py_IncRef\0", unsafe extern "C" fn(Obj));
        let ensure = sym!(b"PyGILState_Ensure\0", unsafe extern "C" fn() -> c_int);
        let release = sym!(b"PyGILState_Release\0", unsafe extern "C" fn(c_int));
        let call = sym!(b"PyObject_CallFunctionObjArgs\0", unsafe extern "C" fn(Obj, ...) -> Obj);
        let text = sym!(b"PyUnicode_FromStringAndSize\0", unsafe extern "C" fn(*const c_char, isize) -> Obj);
        let utf8 = sym!(b"PyUnicode_AsUTF8AndSize\0", unsafe extern "C" fn(Obj, *mut isize) -> *const c_char);
        let decref = sym!(b"Py_DecRef\0", unsafe extern "C" fn(Obj));
        let clear = sym!(b"PyErr_Clear\0", unsafe extern "C" fn());

        // ours to start, and then the starting thread lets go of it so that any thread may come in
        let ours = is_up() == 0;
        let held = if ours {
            init(0);
            if is_up() == 0 {
                return None;
            }
            None
        } else {
            Some(ensure())
        };
        let leave = |ok: bool| {
            match held {
                Some(h) => release(h),
                None => {
                    save();
                }
            }
            ok
        };
        let version = std::ffi::CStr::from_ptr(ver()).to_string_lossy().split_whitespace().next().unwrap_or("3").to_string();
        let src = CString::new(include_str!("flvfx_shim.py")).ok()?;
        let code = compile(src.as_ptr(), c"<breakfast cereal note scripts>".as_ptr(), 257);
        let globals = dict_new();
        if code.is_null() || globals.is_null() {
            clear();
            leave(false);
            return None;
        }
        dict_set(globals, c"__builtins__".as_ptr(), builtins());
        let name = text(c"cereal_flvfx".as_ptr(), 12);
        dict_set(globals, c"__name__".as_ptr(), name);
        decref(name);
        let done = eval(code, globals, globals);
        decref(code);
        if done.is_null() {
            clear();
            leave(false);
            return None;
        }
        decref(done);
        // the dictionary is kept for good, and with it the function
        let door = dict_get(globals, c"_call".as_ptr());
        if door.is_null() {
            leave(false);
            return None;
        }
        incref(door);
        leave(true);
        Some(Py { _lib: lib, ensure, release, call, text, utf8, decref, clear, door, version })
    }

    /// One call through the door: a verb, and two texts for it. `None` only when the interpreter itself failed.
    pub fn call(&self, op: &str, a: &str, b: &str) -> Option<String> {
        unsafe {
            let g = (self.ensure)();
            let args = [op, a, b].map(|s| (self.text)(s.as_ptr() as *const c_char, s.len() as isize));
            let mut out = None;
            if args.iter().all(|a| !a.is_null()) {
                let r = (self.call)(self.door, args[0], args[1], args[2], std::ptr::null_mut::<c_void>());
                if !r.is_null() {
                    let mut len = 0isize;
                    let p = (self.utf8)(r, &mut len);
                    if !p.is_null() && len >= 0 {
                        out = Some(String::from_utf8_lossy(std::slice::from_raw_parts(p as *const u8, len as usize)).into_owned());
                    }
                    (self.decref)(r);
                }
            }
            if out.is_none() {
                (self.clear)();
            }
            for a in args {
                if !a.is_null() {
                    (self.decref)(a);
                }
            }
            (self.release)(g);
            out
        }
    }
}
