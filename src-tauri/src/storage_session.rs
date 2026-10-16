//! Keep ordinary database commands serialized, while read-only scan I/O runs
//! outside that mutex. A scan lease prevents lifecycle replacement underneath it.
use crate::storage::Storage;
use std::{
    ops::Deref,
    sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard},
    sync::atomic::{AtomicU64, Ordering},
};

pub struct StorageSession {
    storage: Mutex<Storage>,
    scan_access: RwLock<()>,
    scans: Mutex<()>,
    generation: AtomicU64,
}

impl StorageSession {
    pub fn new(storage: Storage) -> Self {
        Self {
            storage: Mutex::new(storage),
            scan_access: RwLock::new(()),
            scans: Mutex::new(()),
            generation: AtomicU64::new(0),
        }
    }

    pub fn lock(&self) -> Result<MutexGuard<'_, Storage>, &'static str> {
        let storage = self.storage.lock().map_err(|_| "storage_unavailable")?;
        if crate::reset::recovery_pending(storage.data_directory()) { return Err("reset_recovery_required"); }
        Ok(storage)
    }
    pub(crate) fn generation(&self) -> u64 { self.generation.load(Ordering::Acquire) }
    pub(crate) fn reset_generation(&self) { self.generation.fetch_add(1, Ordering::AcqRel); }
    pub(crate) fn lock_at(&self, generation: u64) -> Result<MutexGuard<'_, Storage>, &'static str> {
        let storage = self.lock()?;
        if generation != self.generation() { return Err("reset_state_changed"); }
        Ok(storage)
    }
    pub(crate) fn exclusive_at(&self, generation: u64) -> Result<ExclusiveStorage<'_>, &'static str> {
        let storage = self.exclusive()?;
        if generation != self.generation() { return Err("reset_state_changed"); }
        Ok(storage)
    }
    /// Exit waits for a current transaction, without opening or changing data.
    pub(crate) fn wait_for_exit(&self) -> Result<MutexGuard<'_, Storage>, &'static str> {
        self.storage.lock().map_err(|_| "storage_unavailable")
    }

    // Acquire the lifecycle lease before the database mutex everywhere. Ordinary
    // commands never wait for this lease; they still use the original mutex.
    pub fn exclusive(&self) -> Result<ExclusiveStorage<'_>, &'static str> {
        let storage = self.exclusive_for_reset()?;
        if crate::reset::recovery_pending(storage.data_directory()) { return Err("reset_recovery_required"); }
        Ok(storage)
    }
    pub(crate) fn exclusive_for_reset(&self) -> Result<ExclusiveStorage<'_>, &'static str> {
        let access = self
            .scan_access
            .write()
            .map_err(|_| "storage_unavailable")?;
        let storage = self.storage.lock().map_err(|_| "storage_unavailable")?;
        Ok(ExclusiveStorage {
            storage,
            _access: access,
            generation: &self.generation,
        })
    }

    pub(crate) fn begin_scan(&self) -> Result<ScanLease<'_>, &'static str> {
        let serial = self.scans.lock().map_err(|_| "scan session unavailable")?;
        let access = self.scan_access.read().map_err(|_| "storage_unavailable")?;
        Ok(ScanLease {
            _access: access,
            _serial: serial,
        })
    }
    pub(crate) fn begin_scan_at(&self, generation: u64) -> Result<ScanLease<'_>, &'static str> {
        let lease = self.begin_scan()?;
        // Validate after acquiring the lifecycle lease, so a queued old request
        // cannot start reading or publish a preview into replaced data.
        drop(self.lock_at(generation)?);
        Ok(lease)
    }
    pub(crate) fn begin_image_read(&self) -> Result<RwLockReadGuard<'_, ()>, &'static str> {
        self.scan_access.read().map_err(|_| "cover_unavailable")
    }
}

pub struct ExclusiveStorage<'a> {
    storage: MutexGuard<'a, Storage>,
    _access: RwLockWriteGuard<'a, ()>,
    generation: &'a AtomicU64,
}
impl ExclusiveStorage<'_> {
    pub(crate) fn restore_backup(&self, backup_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.storage.restore_backup(backup_id)?;
        self.generation.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }
}
impl Deref for ExclusiveStorage<'_> {
    type Target = Storage;
    fn deref(&self) -> &Storage {
        &self.storage
    }
}

pub(crate) struct ScanLease<'a> {
    _access: RwLockReadGuard<'a, ()>,
    _serial: MutexGuard<'a, ()>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::{Arc,mpsc},time::Duration};
    struct NoKeys;
    impl crate::reset::Credentials for NoKeys {
        fn read(&mut self,_: &str,_: bool)->Result<Option<String>,String>{Ok(None)}
        fn save(&mut self,_: &str,_: bool,_: &str)->Result<(),String>{Err("synthetic_unexpected_key".into())}
        fn delete(&mut self,_: &str,_: bool)->Result<(),String>{Ok(())}
    }

    #[test]
    fn requests_prepared_before_backup_restore_cannot_write_into_restored_data() {
        let home = tempfile::tempdir().unwrap();
        let state = StorageSession::new(Storage::open(home.path()).unwrap());
        let backup = state.lock().unwrap().create_backup().unwrap();
        let generation = state.generation();
        {
            let storage = state.exclusive_at(generation).unwrap();
            storage.restore_backup(&backup).unwrap();
        }
        assert_eq!(state.lock_at(generation).err(), Some("reset_state_changed"));
        assert_eq!(state.exclusive_at(generation).err(), Some("reset_state_changed"));
        assert!(!crate::lifecycle::load_preferences(&state.lock().unwrap()).unwrap().reduce_motion);
    }

    #[test]
    fn failed_backup_restore_keeps_current_requests_valid() {
        let home = tempfile::tempdir().unwrap();
        let state = StorageSession::new(Storage::open(home.path()).unwrap());
        let generation = state.generation();
        {
            let storage = state.exclusive_at(generation).unwrap();
            assert!(storage.restore_backup("not-a-backup").is_err());
        }
        assert!(state.lock_at(generation).is_ok());
        assert!(state.exclusive_at(generation).is_ok());
    }

    #[test]
    fn a_setting_request_prepared_before_reset_cannot_write_after_reset() {
        let home=tempfile::tempdir().unwrap();
        let state=Arc::new(StorageSession::new(Storage::open(home.path()).unwrap()));
        state.lock().unwrap().with_connection(|db|{db.execute("UPDATE interface_preferences SET reduce_motion=1",[])?;Ok(())}).unwrap();
        let (ready_tx,ready_rx)=mpsc::channel(); let (resume_tx,resume_rx)=mpsc::channel();
        let worker=state.clone();
        let pending=std::thread::spawn(move||{
            let generation={let _ready=worker.lock().unwrap();worker.generation()};
            ready_tx.send(()).unwrap();resume_rx.recv().unwrap();
            match worker.lock_at(generation) {
                Ok(storage)=>{storage.with_connection(|db|{db.execute("UPDATE interface_preferences SET reduce_motion=1",[])?;Ok(())}).unwrap();Ok(())},
                Err(error)=>Err(error),
            }
        });
        ready_rx.recv().unwrap();
        {let storage=state.exclusive_for_reset().unwrap();crate::reset::restore_initial_state_with(&storage,&mut NoKeys).unwrap();state.reset_generation();}
        resume_tx.send(()).unwrap();
        assert_eq!(pending.join().unwrap(),Err("reset_state_changed"));
        assert!(!crate::lifecycle::load_preferences(&state.lock().unwrap()).unwrap().reduce_motion);
    }

    #[test]
    fn reset_waits_for_in_flight_image_and_removes_its_generated_cache() {
        let home=tempfile::tempdir().unwrap();let image=home.path().join("original.png");
        image::RgbImage::from_pixel(20,30,image::Rgb([10,20,30])).save(&image).unwrap();
        let storage=Storage::open(&home.path().join("data")).unwrap();
        let id=crate::actor_management::save(&storage,&crate::actor_management::SaveActorInput{actor_id:None,name:"合成演员".into(),aliases:vec![],profile:crate::actors::Profile::default(),photo_path:Some(image.to_string_lossy().into_owned()),remove_photo:false,film_ids:vec![]}).unwrap();
        let state=Arc::new(StorageSession::new(storage));
        let (image_tx,image_rx)=mpsc::channel();let (resume_tx,resume_rx)=mpsc::channel();
        let reading=state.clone();
        let image_job=std::thread::spawn(move||{
            let _lease=reading.begin_image_read().unwrap();let snapshot=reading.lock().unwrap().clone();
            crate::actors::thumbnail(&snapshot,&id,false).unwrap();image_tx.send(()).unwrap();
            resume_rx.recv().unwrap();crate::actors::thumbnail(&snapshot,&id,true).unwrap();
        });
        image_rx.recv().unwrap();
        let resetting=state.clone();let (started_tx,started_rx)=mpsc::channel();let (done_tx,done_rx)=mpsc::channel();
        let reset_job=std::thread::spawn(move||{
            started_tx.send(()).unwrap();let storage=resetting.exclusive_for_reset().unwrap();
            crate::covers::with_generated_cache_lock(||crate::reset::restore_initial_state_with(&storage,&mut NoKeys)).unwrap();
            resetting.reset_generation();done_tx.send(()).unwrap();
        });
        started_rx.recv().unwrap();assert!(done_rx.try_recv().is_err());resume_tx.send(()).unwrap();
        image_job.join().unwrap();done_rx.recv_timeout(Duration::from_secs(5)).unwrap();reset_job.join().unwrap();
        assert!(!home.path().join("data/cover-cache").exists());
        assert_eq!(state.lock().unwrap().with_connection(|db|db.query_row("SELECT COUNT(*) FROM actors",[],|r|r.get::<_,i64>(0))).unwrap(),0);
        assert!(image.exists());
    }

    #[test]
    fn pending_recovery_blocks_commands_but_allows_transaction_wait_for_exit() {
        let home=tempfile::tempdir().unwrap();let state=StorageSession::new(Storage::open(home.path()).unwrap());
        std::fs::create_dir(home.path().join(".reset-pending")).unwrap();
        assert_eq!(state.lock().err(),Some("reset_recovery_required"));
        assert_eq!(state.exclusive().err(),Some("reset_recovery_required"));
        assert!(state.wait_for_exit().is_ok());
    }
}
