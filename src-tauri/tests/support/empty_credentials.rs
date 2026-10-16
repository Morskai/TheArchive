use local_film_library_preview_lib::reset::Credentials;
use std::collections::BTreeMap;

/// Synthetic integration tests never access the current user's credential vault.
#[derive(Default)]
pub struct EmptyCredentials {
    keys: BTreeMap<(String, bool), String>,
}

impl Credentials for EmptyCredentials {
    fn read(&mut self, provider: &str, protected: bool) -> Result<Option<String>, String> {
        Ok(self.keys.get(&(provider.to_owned(), protected)).cloned())
    }

    fn save(&mut self, provider: &str, protected: bool, key: &str) -> Result<(), String> {
        self.keys.insert((provider.to_owned(), protected), key.to_owned());
        Ok(())
    }

    fn delete(&mut self, provider: &str, protected: bool) -> Result<(), String> {
        self.keys.remove(&(provider.to_owned(), protected));
        Ok(())
    }
}
