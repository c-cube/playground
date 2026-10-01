//! The handful of Tcl/Tk C functions we need, declared by hand (no bindgen).
//! Only ever called from the thread running `Ui::run`.

use std::ffi::{CStr, CString, c_char, c_int, c_void};

use anyhow::{Result, bail};

#[repr(C)]
pub struct Interp {
    _private: [u8; 0],
}
#[repr(C)]
pub struct Obj {
    _private: [u8; 0],
}
pub type Channel = *mut c_void;
pub type ObjCmdProc = unsafe extern "C" fn(*mut c_void, *mut Interp, c_int, *const *mut Obj) -> c_int;
pub type CmdDeleteProc = unsafe extern "C" fn(*mut c_void);

pub const TCL_OK: c_int = 0;
pub const TCL_ERROR: c_int = 1;
pub const TCL_READABLE: c_int = 1 << 1;
pub const TCL_WRITABLE: c_int = 1 << 2;
const TCL_GLOBAL_ONLY: c_int = 1;
const TCL_EVAL_GLOBAL: c_int = 0x020000;

#[link(name = "tcl8.6")]
unsafe extern "C" {
    pub fn Tcl_FindExecutable(argv0: *const c_char);
    pub fn Tcl_CreateInterp() -> *mut Interp;
    pub fn Tcl_Init(interp: *mut Interp) -> c_int;
    pub fn Tcl_MakeFileChannel(handle: *mut c_void, mode: c_int) -> Channel;
    pub fn Tcl_RegisterChannel(interp: *mut Interp, chan: Channel);
    pub fn Tcl_GetChannelName(chan: Channel) -> *const c_char;
    pub fn Tcl_CreateObjCommand(
        interp: *mut Interp,
        name: *const c_char,
        proc_: ObjCmdProc,
        client_data: *mut c_void,
        delete: Option<CmdDeleteProc>,
    ) -> *mut c_void;
    pub fn Tcl_GetStringFromObj(obj: *mut Obj, len: *mut c_int) -> *const c_char;
    pub fn Tcl_NewStringObj(bytes: *const c_char, len: c_int) -> *mut Obj;
    pub fn Tcl_SetObjResult(interp: *mut Interp, obj: *mut Obj);
    fn Tcl_EvalEx(interp: *mut Interp, script: *const c_char, len: c_int, flags: c_int) -> c_int;
    fn Tcl_GetStringResult(interp: *mut Interp) -> *const c_char;
    fn Tcl_GetVar2(interp: *mut Interp, name1: *const c_char, name2: *const c_char, flags: c_int)
    -> *const c_char;
}

#[link(name = "tk8.6")]
unsafe extern "C" {
    pub fn Tk_Init(interp: *mut Interp) -> c_int;
    pub fn Tk_MainLoop();
}

pub unsafe fn result(interp: *mut Interp) -> String {
    unsafe { CStr::from_ptr(Tcl_GetStringResult(interp)).to_string_lossy().into_owned() }
}

/// The string value of a Tcl object.
pub unsafe fn obj_str(obj: *mut Obj) -> String {
    let mut len: c_int = 0;
    unsafe {
        let p = Tcl_GetStringFromObj(obj, &mut len);
        String::from_utf8_lossy(std::slice::from_raw_parts(p as *const u8, len as usize)).into_owned()
    }
}

pub unsafe fn set_result(interp: *mut Interp, s: &str) {
    // Tcl copies the bytes; strings over 2GB are truncated, which is fine here
    let len = c_int::try_from(s.len()).unwrap_or(c_int::MAX);
    unsafe { Tcl_SetObjResult(interp, Tcl_NewStringObj(s.as_ptr() as *const c_char, len)) }
}

/// Evaluate at global level; on error, return the Tcl `errorInfo`.
pub unsafe fn eval(interp: *mut Interp, script: &str) -> Result<()> {
    let len = c_int::try_from(script.len())?;
    let script = CString::new(script)?;
    if unsafe { Tcl_EvalEx(interp, script.as_ptr(), len, TCL_EVAL_GLOBAL) } == TCL_OK {
        return Ok(());
    }
    let info = unsafe { Tcl_GetVar2(interp, c"errorInfo".as_ptr(), std::ptr::null(), TCL_GLOBAL_ONLY) };
    if info.is_null() {
        bail!("{}", unsafe { result(interp) })
    }
    bail!("{}", unsafe { CStr::from_ptr(info) }.to_string_lossy())
}
