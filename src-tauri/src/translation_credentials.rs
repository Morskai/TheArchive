//! Translation secrets belong only in the Windows current-user credential vault.

fn credential_target(provider: &str) -> Result<String, String> {
    match provider {
        "deepl" | "google" | "azure" | "baidu" => Ok(format!("TheArchive/translation/{provider}")),
        _ => Err("translation_invalid_provider".into()),
    }
}

fn validated_key(key: &str) -> Result<&str, String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("translation_key_missing".into());
    }
    if key.len() > 2560 || !key.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err("translation_invalid_key".into());
    }
    Ok(key)
}

/// Non-secret configuration uses a presence flag; the settings input can explicitly read the key.
pub fn has_key(provider: &str) -> Result<bool, String> {
    platform::has(&credential_target(provider)?)
}

pub fn save_key(provider: &str, key: &str) -> Result<(), String> {
    let target = credential_target(provider)?;
    platform::save(&target, validated_key(key)?)
}

pub fn delete_key(provider: &str) -> Result<(), String> {
    platform::delete(&credential_target(provider)?)
}

pub fn read_key(provider: &str) -> Result<Option<String>, String> {
    platform::read(&credential_target(provider)?)
}

fn reset_target(provider: &str) -> Result<String, String> {
    credential_target(provider)?;
    Ok(format!("TheArchive/reset/{provider}"))
}

pub(crate) fn read_reset_key(provider: &str) -> Result<Option<String>, String> {
    platform::read(&reset_target(provider)?)
}
pub(crate) fn save_reset_key(provider: &str, key: &str) -> Result<(), String> {
    platform::save(&reset_target(provider)?, validated_key(key)?)
}
pub(crate) fn delete_reset_key(provider: &str) -> Result<(), String> {
    platform::delete(&reset_target(provider)?)
}

// Unit tests cannot reach the real credential store, including when cargo test runs on Windows.
#[cfg(any(not(windows), test))]
mod platform {
    pub fn has(_: &str) -> Result<bool, String> {
        Err("translation_credentials_unsupported".into())
    }
    pub fn save(_: &str, _: &str) -> Result<(), String> {
        Err("translation_credentials_unsupported".into())
    }
    pub fn delete(_: &str) -> Result<(), String> {
        Err("translation_credentials_unsupported".into())
    }
    pub fn read(_: &str) -> Result<Option<String>, String> {
        Err("translation_credentials_unsupported".into())
    }
}

#[cfg(all(windows, not(test)))]
mod platform {
    use windows_sys::Win32::Foundation::{ERROR_NOT_FOUND, GetLastError};
    use windows_sys::Win32::Security::Credentials::{
        CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree,
        CredReadW, CredWriteW,
    };

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }

    struct StoredCredential(*mut CREDENTIALW);
    impl Drop for StoredCredential {
        fn drop(&mut self) {
            // SAFETY: this allocation belongs to CredReadW, and CredFree is its matching deallocator.
            unsafe {
                let credential = &*self.0;
                if !credential.CredentialBlob.is_null() {
                    for i in 0..credential.CredentialBlobSize as usize {
                        std::ptr::write_volatile(credential.CredentialBlob.add(i), 0);
                    }
                }
                CredFree(self.0.cast());
            }
        }
    }

    fn load(target: &str) -> Result<Option<StoredCredential>, String> {
        let target = wide(target);
        let mut credential = std::ptr::null_mut();
        // SAFETY: nul-terminated target and valid out-pointer remain alive for the call.
        if unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) } == 0 {
            return if unsafe { GetLastError() } == ERROR_NOT_FOUND {
                Ok(None)
            } else {
                Err("translation_credentials_error".into())
            };
        }
        if credential.is_null() {
            return Err("translation_credentials_error".into());
        }
        Ok(Some(StoredCredential(credential)))
    }

    pub fn has(target: &str) -> Result<bool, String> {
        Ok(load(target)?.is_some())
    }

    pub fn read(target: &str) -> Result<Option<String>, String> {
        let Some(stored) = load(target)? else {
            return Ok(None);
        };
        // SAFETY: stored owns the credential until after this copy; blob size is checked before use.
        let credential = unsafe { &*stored.0 };
        if credential.CredentialBlob.is_null()
            || credential.CredentialBlobSize == 0
            || credential.CredentialBlobSize > 2560
        {
            return Err("translation_credentials_error".into());
        }
        let blob = unsafe {
            std::slice::from_raw_parts(
                credential.CredentialBlob,
                credential.CredentialBlobSize as usize,
            )
        };
        let key =
            std::str::from_utf8(blob).map_err(|_| "translation_credentials_error".to_owned())?;
        super::validated_key(key).map_err(|_| "translation_credentials_error".to_owned())?;
        Ok(Some(key.to_owned()))
    }

    pub fn save(target: &str, key: &str) -> Result<(), String> {
        let mut target = wide(target);
        let mut user = wide("TheArchive");
        let mut blob = key.as_bytes().to_vec();
        let credential = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: target.as_mut_ptr(),
            CredentialBlobSize: blob.len() as u32,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            UserName: user.as_mut_ptr(),
            ..Default::default()
        };
        // SAFETY: all pointers refer to owned live buffers, and Windows copies the generic blob.
        let result = unsafe { CredWriteW(&credential, 0) };
        for byte in &mut blob {
            unsafe { std::ptr::write_volatile(byte, 0) };
        }
        if result == 0 {
            Err("translation_credentials_error".into())
        } else {
            Ok(())
        }
    }

    pub fn delete(target: &str) -> Result<(), String> {
        let target = wide(target);
        // SAFETY: target is nul-terminated and remains alive for the call.
        if unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) } == 0
            && unsafe { GetLastError() } != ERROR_NOT_FOUND
        {
            return Err("translation_credentials_error".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_provider_targets_reject_arbitrary_names() {
        assert_eq!(
            credential_target("deepl").unwrap(),
            "TheArchive/translation/deepl"
        );
        assert_eq!(
            credential_target("google").unwrap(),
            "TheArchive/translation/google"
        );
        assert_eq!(
            credential_target("../private").unwrap_err(),
            "translation_invalid_provider"
        );
    }

    #[test]
    fn invalid_keys_fail_without_echoing_secret() {
        assert_eq!(
            validated_key("  fake-valid-key  ").unwrap(),
            "fake-valid-key"
        );
        assert_eq!(validated_key(" ").unwrap_err(), "translation_key_missing");
        assert_eq!(
            validated_key("fake\nprivate-key").unwrap_err(),
            "translation_invalid_key"
        );
        assert_eq!(
            validated_key(&"x".repeat(2561)).unwrap_err(),
            "translation_invalid_key"
        );
    }

    #[test]
    fn unit_tests_cannot_read_or_modify_user_credentials() {
        assert_eq!(
            save_key("google", "synthetic-key").unwrap_err(),
            "translation_credentials_unsupported"
        );
        assert_eq!(
            read_key("google").unwrap_err(),
            "translation_credentials_unsupported"
        );
        assert_eq!(
            has_key("google").unwrap_err(),
            "translation_credentials_unsupported"
        );
        assert_eq!(
            delete_key("google").unwrap_err(),
            "translation_credentials_unsupported"
        );
        assert_eq!(
            save_key("unknown", "synthetic-key").unwrap_err(),
            "translation_invalid_provider"
        );
    }
}
