// Only explicit edit-menu actions access Unicode text; never stored or logged.
#[cfg(windows)]
mod platform {
    use windows_sys::Win32::{Foundation::{GlobalFree, HWND}, System::{DataExchange::*, Memory::*}};
    const UNICODE_TEXT: u32 = 13;
    struct Clipboard;
    impl Drop for Clipboard { fn drop(&mut self) { unsafe { CloseClipboard(); } } }
    fn open(owner: HWND) -> Result<Clipboard, String> {
        if unsafe { OpenClipboard(owner) } == 0 { return Err("clipboard_busy".into()); }
        Ok(Clipboard)
    }
    pub fn read(owner: HWND) -> Result<String, String> {
        let _clipboard = open(owner)?;
        unsafe {
            if IsClipboardFormatAvailable(UNICODE_TEXT) == 0 { return Err("clipboard_not_text".into()); }
            let handle = GetClipboardData(UNICODE_TEXT);
            if handle.is_null() { return Err("clipboard_read_failed".into()); }
            let size = GlobalSize(handle);
            if size == 0 || size > 32 * 1024 * 1024 || size % 2 != 0 { return Err("clipboard_read_failed".into()); }
            let pointer = GlobalLock(handle) as *const u16;
            if pointer.is_null() { return Err("clipboard_read_failed".into()); }
            let units = std::slice::from_raw_parts(pointer, size / 2);
            let result = match units.iter().position(|unit| *unit == 0) {
                Some(end) => String::from_utf16(&units[..end]).map_err(|_| "clipboard_read_failed".to_owned()),
                None => Err("clipboard_read_failed".into()),
            };
            GlobalUnlock(handle);
            result
        }
    }
    pub fn write(owner: HWND, text: &str) -> Result<(), String> {
        if text.contains('\0') { return Err("clipboard_invalid_text".into()); }
        let units: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        let _clipboard = open(owner)?;
        unsafe {
            let handle = GlobalAlloc(GMEM_MOVEABLE, units.len() * 2);
            if handle.is_null() { return Err("clipboard_write_failed".into()); }
            let pointer = GlobalLock(handle) as *mut u16;
            if pointer.is_null() { GlobalFree(handle); return Err("clipboard_write_failed".into()); }
            std::ptr::copy_nonoverlapping(units.as_ptr(), pointer, units.len());
            GlobalUnlock(handle);
            if EmptyClipboard() == 0 || SetClipboardData(UNICODE_TEXT, handle).is_null() {
                GlobalFree(handle); return Err("clipboard_write_failed".into());
            }
            // Windows owns the allocation after SetClipboardData succeeds.
        }
        Ok(())
    }
}
#[tauri::command]
pub fn read_edit_clipboard(window: tauri::WebviewWindow) -> Result<String, String> {
    #[cfg(windows)] { platform::read(window.hwnd().map_err(|_| "clipboard_unavailable")?.0) }
    #[cfg(not(windows))] { let _ = window; Err("clipboard_unavailable".into()) }
}
#[tauri::command]
pub fn write_edit_clipboard(window: tauri::WebviewWindow, text: String) -> Result<(), String> {
    #[cfg(windows)] { platform::write(window.hwnd().map_err(|_| "clipboard_unavailable")?.0, &text) }
    #[cfg(not(windows))] { let _ = (window, text); Err("clipboard_unavailable".into()) }
}
