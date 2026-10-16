//! Optional read-only Windows properties, requested only when the film drawer is opened.
use std::path::Path;
#[derive(Debug, serde::Serialize)]
pub struct VideoResolution {
    pub width: u32,
    pub height: u32,
}

pub fn dimensions(path: &Path) -> Option<VideoResolution> {
    #[cfg(windows)]
    {
        platform::dimensions(path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        None
    }
}
#[cfg(windows)]
mod platform {
    use super::*;
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        Win32::{
            Foundation::PROPERTYKEY,
            System::Com::{
                COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize, IBindCtx,
                StructuredStorage::{PropVariantClear, PropVariantToUInt32},
            },
            UI::Shell::PropertiesSystem::{
                GPS_DEFAULT, IPropertyStore, SHGetPropertyStoreFromParsingName,
            },
        },
        core::{GUID, PCWSTR},
    };
    struct Apartment(bool);
    impl Drop for Apartment {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() }
            }
        }
    }
    pub fn dimensions(path: &Path) -> Option<VideoResolution> {
        let mut name: Vec<u16> = path.as_os_str().encode_wide().collect();
        // Rust canonical paths use verbatim prefixes; Shell property handlers reject these.
        // Convert only ordinary disk/UNC prefixes, preserving UTF-16 and the authorized path.
        match path.components().next() {
            Some(std::path::Component::Prefix(prefix)) => match prefix.kind() {
                std::path::Prefix::VerbatimDisk(_) => { name.drain(..4); },
                std::path::Prefix::VerbatimUNC(_, _) => { name.splice(..8, [92u16, 92]); },
                _ => {}
            },
            _ => {}
        }
        if name.contains(&0) {
            return None;
        }
        name.push(0);
        // Every successful initialization, including S_FALSE, requires balancing uninitialization.
        let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if initialized.is_err() && initialized.0 != 0x80010106u32 as i32 {
            return None;
        }
        let _apartment = Apartment(initialized.is_ok());
        // GPS_DEFAULT opens read-only. No SetValue, Commit, or media modification is performed.
        let store: IPropertyStore = unsafe {
            SHGetPropertyStoreFromParsingName(PCWSTR(name.as_ptr()), None::<&IBindCtx>, GPS_DEFAULT)
        }
        .ok()?;
        let dimension = |pid| -> Option<u32> {
            let key = PROPERTYKEY {
                fmtid: GUID::from_u128(0x64440491_4c8b_11d1_8b70_080036b11a03),
                pid,
            };
            let mut value = unsafe { store.GetValue(&key) }.ok()?;
            let number = unsafe { PropVariantToUInt32(&value) }.ok();
            let _ = unsafe { PropVariantClear(&mut value) };
            number.filter(|value| *value > 0 && *value <= 65535)
        };
        Some(VideoResolution {
            width: dimension(3)?,
            height: dimension(4)?,
        })
    }
}
