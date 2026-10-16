use tauri::{plugin::TauriPlugin, Runtime};
use std::{io, marker::PhantomData, rc::Rc};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0},
    System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject, INFINITE},
};

/// Serialize plugin initialization: its mutex can exist before its notification window is ready.
/// Keep this guard on the main thread until Builder::build has initialized the plugin.
pub struct StartupGuard {
    handle: HANDLE,
    _main_thread: PhantomData<Rc<()>>,
}

impl StartupGuard {
    pub fn acquire(identifier: &str) -> io::Result<Self> {
        let name: Vec<u16> = format!("{identifier}-startup").encode_utf16().chain(Some(0)).collect();
        // SAFETY: the name is NUL-terminated and no security attributes are supplied.
        let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        if handle.is_null() { return Err(io::Error::last_os_error()); }
        // SAFETY: this is our live mutex handle; abandoned ownership is also recoverable.
        let result = unsafe { WaitForSingleObject(handle, INFINITE) };
        if result != WAIT_OBJECT_0 && result != WAIT_ABANDONED {
            let error = io::Error::last_os_error();
            unsafe { CloseHandle(handle); }
            return Err(error);
        }
        Ok(Self { handle, _main_thread: PhantomData })
    }
}

impl Drop for StartupGuard {
    fn drop(&mut self) {
        // SAFETY: the guard cannot move across threads and owns this acquired mutex handle.
        unsafe { ReleaseMutex(self.handle); CloseHandle(self.handle); }
    }
}

/// Register before storage and other plugins. A repeated launch only restores the existing window.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    tauri_plugin_single_instance::init(|app, _args, _cwd| {
        crate::window_lifecycle::show_main(app);
    })
}
