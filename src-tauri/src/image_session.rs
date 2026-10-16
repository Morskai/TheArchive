//! Failed image display requests stop for this run until an explicit successful
//! operation releases their scope. This session never scans or retries on a timer.
use std::{collections::{BTreeMap,BTreeSet}, sync::Mutex};

#[derive(Default)]
pub struct ImageSession { state: Mutex<ImageState> }
#[derive(Default)]
struct ImageState { failures: BTreeMap<String,String>, owners: BTreeMap<String,BTreeSet<String>> }
impl ImageSession {
    pub fn read<T>(&self, key: &str, load: impl FnOnce() -> Result<T,String>) -> Result<T,String> {
        self.read_owned(key,key,load)
    }
    pub fn read_owned<T>(&self, owner: &str, key: &str, load: impl FnOnce() -> Result<T,String>) -> Result<T,String> {
        let mut state = self.state.lock().map_err(|_| "cover_load_failed")?;
        state.owners.entry(owner.to_owned()).or_default().insert(key.to_owned());
        if let Some(error) = state.failures.get(key) { return Err(error.clone()); }
        match load() {
            Ok(value) => Ok(value),
            Err(error) => { state.failures.insert(key.to_owned(), error.clone()); Err(error) }
        }
    }
    pub fn release(&self, keys: &[String]) {
        if let Ok(mut state) = self.state.lock() {
            for owner in keys {
                state.failures.remove(owner);
                if let Some(keys) = state.owners.remove(owner) { for key in keys { state.failures.remove(&key); } }
            }
        }
    }
    pub fn clear(&self) { if let Ok(mut state) = self.state.lock() { *state=ImageState::default(); } }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    #[test]
    fn failed_reads_stop_across_views_until_success_scope_or_restart() {
        let session = ImageSession::default();
        let reads = Cell::new(0);
        let attempt = || { reads.set(reads.get()+1); Err::<(),_>("cover_unavailable".into()) };
        for _ in 0..8 { assert!(session.read("actor:a", attempt).is_err()); }
        assert_eq!(reads.get(),1);
        session.release(&["actor:b".into()]);
        assert!(session.read("actor:a",attempt).is_err()); assert_eq!(reads.get(),1);
        session.release(&["actor:a".into()]);
        assert!(session.read("actor:a",attempt).is_err()); assert_eq!(reads.get(),2);
        session.clear(); assert!(session.read("actor:a",attempt).is_err()); assert_eq!(reads.get(),3);
        let restarted = ImageSession::default();
        assert_eq!(reads.get(),3); // startup itself performs no I/O
        assert!(restarted.read("actor:a",attempt).is_err()); assert_eq!(reads.get(),4);
    }
}
