//! The release binary uses the GUI subsystem so that double-clicking it doesn't open a console window.
//! When started from a terminal we attach to it so logs and `--help` still show up, and in headless
//! mode we create a console if there is none.

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;

    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
    const STD_ERROR_HANDLE: u32 = -12i32 as u32;
    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const FILE_SHARE_READ: u32 = 1;
    const FILE_SHARE_WRITE: u32 = 2;
    const OPEN_EXISTING: u32 = 3;
    const INVALID_HANDLE_VALUE: isize = -1;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn AttachConsole(process_id: u32) -> i32;
        fn AllocConsole() -> i32;
        fn GetStdHandle(std_handle: u32) -> *mut c_void;
        fn SetStdHandle(std_handle: u32, handle: *mut c_void) -> i32;
        fn GetConsoleMode(handle: *mut c_void, mode: *mut u32) -> i32;
        fn SetConsoleMode(handle: *mut c_void, mode: u32) -> i32;
        fn WriteConsoleInputW(
            handle: *mut c_void,
            buffer: *const InputRecord,
            length: u32,
            written: *mut u32,
        ) -> i32;
        fn CreateFileW(
            file_name: *const u16,
            desired_access: u32,
            share_mode: u32,
            security_attributes: *mut c_void,
            creation_disposition: u32,
            flags_and_attributes: u32,
            template_file: *mut c_void,
        ) -> *mut c_void;
    }

    /// Points stdout/stderr at the console unless they were already redirected to a file or pipe.
    fn bind_std_handles() {
        let conout: Vec<u16> = "CONOUT$\0".encode_utf16().collect();
        for which in [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            unsafe {
                let current = GetStdHandle(which);
                if !current.is_null() && current as isize != INVALID_HANDLE_VALUE {
                    continue;
                }
                let handle = CreateFileW(
                    conout.as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    std::ptr::null_mut(),
                    OPEN_EXISTING,
                    0,
                    std::ptr::null_mut(),
                );
                if handle as isize != INVALID_HANDLE_VALUE {
                    SetStdHandle(which, handle);
                }
            }
        }
    }

    /// The shell doesn't wait for a GUI-subsystem process, so it keeps reading the console itself
    /// and (PSReadLine) disables Ctrl+C signal handling. Keep it enabled so Ctrl+C reaches us.
    pub fn keep_ctrl_c_enabled() {
        const ENABLE_PROCESSED_INPUT: u32 = 1;
        let conin: Vec<u16> = "CONIN$\0".encode_utf16().collect();
        let handle = unsafe {
            CreateFileW(
                conin.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if handle as isize == INVALID_HANDLE_VALUE {
            return;
        }
        let handle = handle as usize;
        std::thread::spawn(move || {
            let handle = handle as *mut c_void;
            loop {
                let mut mode = 0u32;
                unsafe {
                    if GetConsoleMode(handle, &mut mode) != 0 && mode & ENABLE_PROCESSED_INPUT == 0
                    {
                        SetConsoleMode(handle, mode | ENABLE_PROCESSED_INPUT);
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        });
    }

    #[repr(C)]
    struct KeyEventRecord {
        key_down: i32,
        repeat_count: u16,
        virtual_key_code: u16,
        virtual_scan_code: u16,
        unicode_char: u16,
        control_key_state: u32,
    }

    #[repr(C)]
    struct InputRecord {
        event_type: u16,
        key: KeyEventRecord,
    }

    /// The shell printed its prompt when we started and doesn't know we've exited, so feed it an
    /// Enter keypress to make it draw a fresh prompt.
    pub fn restore_prompt() {
        const KEY_EVENT: u16 = 1;
        const VK_RETURN: u16 = 0x0D;
        let conin: Vec<u16> = "CONIN$\0".encode_utf16().collect();
        unsafe {
            let handle = CreateFileW(
                conin.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            );
            if handle as isize == INVALID_HANDLE_VALUE {
                return;
            }
            let record = |key_down| InputRecord {
                event_type: KEY_EVENT,
                key: KeyEventRecord {
                    key_down,
                    repeat_count: 1,
                    virtual_key_code: VK_RETURN,
                    virtual_scan_code: 0x1C,
                    unicode_char: '\r' as u16,
                    control_key_state: 0,
                },
            };
            let records = [record(1), record(0)];
            let mut written = 0u32;
            WriteConsoleInputW(handle, records.as_ptr(), records.len() as u32, &mut written);
        }
    }

    pub fn attach_parent() -> bool {
        let attached = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } != 0;
        if attached {
            bind_std_handles();
        }
        attached
    }

    pub fn alloc() {
        if unsafe { AllocConsole() } != 0 {
            bind_std_handles();
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn attach_parent() -> bool {
        true
    }

    pub fn alloc() {}

    pub fn keep_ctrl_c_enabled() {}

    pub fn restore_prompt() {}
}

/// Makes the parent shell draw a new prompt after we exit.
pub fn restore_prompt() {
    imp::restore_prompt()
}

/// Makes sure Ctrl+C is delivered to this process even if the parent shell keeps the console.
pub fn keep_ctrl_c_enabled() {
    imp::keep_ctrl_c_enabled()
}

/// Attaches to the parent terminal, if any. Returns whether one was found.
pub fn attach_parent() -> bool {
    imp::attach_parent()
}

/// Opens a new console window; use when no parent terminal exists.
pub fn alloc() {
    imp::alloc()
}
