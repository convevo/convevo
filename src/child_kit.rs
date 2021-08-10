/* Based on [shared_child](https://github.com/oconnor663/shared_child.rs), thanks! */
pub use sys::*;

#[cfg(unix)]
mod sys {
    extern crate libc;
    use std::io;

    fn send_signal(pid: u32, signal: libc::c_int) -> io::Result<()> {
        match unsafe { libc::kill(pid as libc::pid_t, signal) } {
            -1 => Err(io::Error::last_os_error()),
            _ => Ok(()),
        }
    }

    pub fn wait(pid: u32) -> io::Result<()> {
        loop {
            let ret = unsafe {
                let mut siginfo = std::mem::zeroed();
                libc::waitid(
                    libc::P_PID,
                    pid as libc::id_t,
                    &mut siginfo,
                    libc::WEXITED | libc::WNOWAIT,
                )
            };
            if ret == 0 {
                return Ok(());
            }
            let e = io::Error::last_os_error();
            if e.kind() != io::ErrorKind::Interrupted {
                return Err(e);
            }
            // We were interrupted. Loop and retry.
        }
    }

    pub fn kill(pid: u32) -> io::Result<()> {
        send_signal(pid, libc::SIGABRT)
    }

    pub fn suspend(pid: u32) -> io::Result<()> {
        send_signal(pid, libc::SIGTSTP)
    }

    pub fn resume(pid: u32) -> io::Result<()> {
        send_signal(pid, libc::SIGCONT)
    }
}

#[cfg(windows)]
#[allow(clippy::upper_case_acronyms)]
#[allow(non_snake_case)]
mod sys {
    // [oconnor663 / shared_child.rs] said:
    // Windows has actually always supported this, by preventing PID reuse
    // while there are still open handles to a child process.

    use std::io;
    use std::mem::transmute;
    use std::os::raw::{c_char, c_int, c_long, c_uint, c_ulong, c_void};

    // From winapi-rs
    type BOOL = c_int;
    type UINT = c_uint;
    type LONG = c_long;
    type DWORD = c_ulong;
    type HANDLE = *mut c_void;
    type HMODULE = *mut c_void;
    type LPCSTR = *const c_char;
    type FARPROC = *mut c_void;
    type NTSTATUS = c_long;
    type FnNtProcess = extern "stdcall" fn(HANDLE) -> NTSTATUS;

    const FALSE: BOOL = false as BOOL;
    const INFINITE: DWORD = 0xFFFFFFFF;
    const WAIT_OBJECT_0: DWORD = 0x00000000_u32;
    const STATUS_SUCCESS: LONG = 0x00000000;
    const STANDARD_RIGHTS_REQUIRED: DWORD = 0x000F0000;
    const SYNCHRONIZE: DWORD = 0x00100000;
    const PROCESS_ALL_ACCESS: DWORD = STANDARD_RIGHTS_REQUIRED | SYNCHRONIZE | 0xFFFF;

    #[link(name = "kernel32", kind = "dylib")]
    extern "C" {
        fn OpenProcess(dwDesiredAccess: DWORD, bInheritHandle: BOOL, dwProcessId: DWORD) -> HANDLE;
        fn CloseHandle(hObject: HANDLE) -> BOOL;
        fn TerminateProcess(hProcess: HANDLE, uExitCode: UINT) -> BOOL;
        fn GetProcAddress(hModule: HMODULE, lpProcName: LPCSTR) -> FARPROC;
        fn GetModuleHandleA(lpModuleName: LPCSTR) -> HMODULE;
        fn WaitForSingleObject(hHandle: HANDLE, dwMilliseconds: DWORD) -> DWORD;
    }

    #[derive(Copy, Clone)]
    pub struct Handle(HANDLE);

    unsafe impl std::marker::Send for Handle {}

    pub fn get_handle(child: &std::process::Child) -> Handle {
        Handle(unsafe { OpenProcess(PROCESS_ALL_ACCESS, FALSE, child.id()) })
    }

    pub fn close_handle(h: Handle) -> io::Result<()> {
        match unsafe { CloseHandle(h.0) } {
            FALSE => Err(io::Error::last_os_error()),
            _ => Ok(()),
        }
    }

    pub fn wait(h: Handle) -> io::Result<()> {
        match unsafe { WaitForSingleObject(h.0, INFINITE) } {
            WAIT_OBJECT_0 => Ok(()),
            _ => Err(io::Error::last_os_error()),
        }
    }

    pub fn kill(h: Handle) -> io::Result<()> {
        match unsafe { TerminateProcess(h.0, 0) } {
            FALSE => Err(io::Error::last_os_error()),
            _ => Ok(()),
        }
    }

    unsafe fn get_nt_function(name: &[u8]) -> FnNtProcess {
        let module_handle = GetModuleHandleA(b"ntdll\0".as_ptr() as LPCSTR);
        let address = GetProcAddress(module_handle, name.as_ptr() as LPCSTR);
        transmute::<*const usize, FnNtProcess>(address as *const usize)
    }

    pub fn suspend(h: Handle) -> io::Result<()> {
        match unsafe { get_nt_function(b"NtSuspendProcess\0")(h.0) } {
            STATUS_SUCCESS => Ok(()),
            _ => Err(io::Error::last_os_error()),
        }
    }

    pub fn resume(h: Handle) -> io::Result<()> {
        match unsafe { get_nt_function(b"NtResumeProcess\0")(h.0) } {
            STATUS_SUCCESS => Ok(()),
            _ => Err(io::Error::last_os_error()),
        }
    }
}
