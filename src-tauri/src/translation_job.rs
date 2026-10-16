use crate::translation_provider::TranslatedText;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct BatchRequests(HashMap<String, Result<TranslatedText, String>>);

impl BatchRequests {
    pub fn translate(
        &mut self,
        body: &str,
        action: impl FnOnce() -> Result<TranslatedText, String>,
    ) -> (Result<TranslatedText, String>, bool) {
        if let Some(result) = self.0.get(body) {
            return (result.clone(), true);
        }
        let result = action();
        self.0.insert(body.to_owned(), result.clone());
        (result, false)
    }
}

#[derive(Clone, Default, serde::Serialize)]
pub struct TranslationJobStatus {
    pub running: bool,
    pub cancelled: bool,
    pub total: usize,
    pub completed: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub reused: usize,
    pub api_requests: usize,
    pub last_error: Option<String>,
}

#[derive(Default)]
struct Job {
    token: u64,
    status: TranslationJobStatus,
    report: TranslationReport,
}

#[derive(Clone, Default)]
pub struct TranslationReport(Arc<Mutex<TranslationJobStatus>>);
impl Job {
    fn publish(&self) { *self.report.0.lock().unwrap_or_else(|e| e.into_inner()) = self.status.clone(); }
}

#[derive(Default)]
pub struct TranslationSession(Mutex<Job>);

impl TranslationSession {
    pub fn reset(&self) {
        let mut job = self.locked();
        if job.status.running { job.status.cancelled = true; }
        job.status.running = false; job.publish();
        job.token = job.token.wrapping_add(1);
        job.status = TranslationJobStatus::default();
        job.report = TranslationReport::default();
    }
    fn locked(&self) -> std::sync::MutexGuard<'_, Job> {
        self.0.lock().unwrap_or_else(|error| error.into_inner())
    }
    pub fn begin(&self, total: usize) -> Result<u64, String> {
        self.begin_report(total).map(|(token, _)| token)
    }
    pub fn begin_report(&self, total: usize) -> Result<(u64, TranslationReport), String> {
        let mut job = self.locked();
        if job.status.running {
            return Err("translation_busy".into());
        }
        if total == 0 || total > 10_000 {
            return Err("translation_invalid_selection".into());
        }
        job.token = job.token.wrapping_add(1);
        job.status = TranslationJobStatus {
            running: true,
            total,
            ..Default::default()
        };
        job.report = TranslationReport::default(); job.publish();
        Ok((job.token, job.report.clone()))
    }
    pub fn status(&self) -> TranslationJobStatus {
        self.locked().status.clone()
    }
    pub fn finish_report(&self, token: u64, report: &TranslationReport) -> TranslationJobStatus {
        let mut job = self.locked();
        if job.token == token { job.status.running = false; job.publish(); }
        report.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    pub fn active(&self, token: u64) -> bool {
        let job = self.locked();
        job.token == token && job.status.running && !job.status.cancelled
    }
    /// Caller acquires the storage lock first. The callback must not call session methods.
    /// Hold the session gate through the commit so cancellation cannot return before it finishes.
    pub fn with_active<T>(
        &self,
        token: u64,
        action: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        let job = self.locked();
        if job.token != token || !job.status.running || job.status.cancelled {
            return Err("translation_cancelled".into());
        }
        action()
    }
    pub fn cancel(&self) -> TranslationJobStatus {
        let mut job = self.locked();
        if job.status.running {
            job.status.cancelled = true;
        }
        job.publish();
        job.status.clone()
    }
    pub fn requested(&self, token: u64) {
        let mut job = self.locked();
        if job.token == token && job.status.running && !job.status.cancelled {
            job.status.api_requests += 1;
            job.publish();
        }
    }
    pub fn record(&self, token: u64, success: bool, reused: bool, error: Option<String>) {
        let mut job = self.locked();
        if job.token != token || !job.status.running || job.status.cancelled {
            return;
        }
        job.status.completed += 1;
        if success {
            job.status.succeeded += 1;
        } else {
            job.status.failed += 1;
        }
        if reused {
            job.status.reused += 1;
        }
        if error.is_some() {
            job.status.last_error = error;
        }
        job.publish();
    }
    pub fn finish(&self, token: u64) {
        let mut job = self.locked();
        if job.token == token {
            job.status.running = false;
            job.publish();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_report_retains_statistics_after_reset_and_does_not_read_the_next_job() {
        let session = TranslationSession::default();
        let (old, report) = session.begin_report(3).unwrap();
        session.requested(old); session.record(old, true, false, None);
        session.reset();
        let (new, new_report) = session.begin_report(7).unwrap();
        session.record(new, true, false, None);
        let finished = session.finish_report(old, &report);
        assert!(finished.cancelled); assert!(!finished.running);
        assert_eq!((finished.total, finished.completed, finished.api_requests), (3, 1, 1));
        assert!(session.active(new));
        let next = session.finish_report(new, &new_report);
        assert_eq!((next.total, next.completed), (7, 1)); assert!(!next.cancelled);
    }

    #[test]
    fn cancellation_blocks_late_result_and_next_job_is_independent() {
        let session = TranslationSession::default();
        let token = session.begin(3).unwrap();
        assert!(session.active(token));
        assert!(session.begin(1).is_err());
        session.record(token, true, false, None);
        session.cancel();
        assert!(!session.active(token));
        session.record(token, true, false, None);
        assert_eq!(session.status().succeeded, 1);
        session.finish(token);
        assert!(!session.status().running);
        let next = session.begin(1).unwrap();
        assert_ne!(next, token);
        session.finish(token);
        assert!(session.active(next));
        assert_eq!(session.status().succeeded, 0);
    }

    #[test]
    fn local_reuse_and_failures_are_counted_separately_from_requests() {
        let session = TranslationSession::default();
        let token = session.begin(3).unwrap();
        session.requested(token);
        session.record(token, true, false, None);
        session.record(token, true, true, None);
        session.requested(token);
        session.record(token, false, false, Some("translation_auth".into()));
        session.finish(token);
        let status = session.status();
        assert_eq!(
            (
                status.completed,
                status.succeeded,
                status.failed,
                status.reused,
                status.api_requests
            ),
            (3, 2, 1, 1, 2)
        );
        assert_eq!(status.last_error.as_deref(), Some("translation_auth"));
    }

    #[test]
    fn repeated_bodies_reuse_success_and_never_construct_provider_twice() {
        let calls = std::cell::Cell::new(0);
        let fake_provider = || {
            calls.set(calls.get() + 1);
            Ok(TranslatedText {
                text: "synthetic translation".into(),
                source_language: Some("en".into()),
            })
        };
        let mut batch = BatchRequests::default();
        let (first, reused) = batch.translate("same", fake_provider);
        assert_eq!(first.unwrap().text, "synthetic translation");
        assert!(!reused);
        let (second, reused) = batch.translate("same", || {
            panic!("cache hit must not initialize or call provider")
        });
        assert_eq!(second.unwrap().text, "synthetic translation");
        assert!(reused);
        let (third, reused) = batch.translate("different", fake_provider);
        assert_eq!(third.unwrap().source_language.as_deref(), Some("en"));
        assert!(!reused);
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn repeated_failure_is_cached_but_new_batch_can_retry() {
        let calls = std::cell::Cell::new(0);
        let failing_provider = || {
            calls.set(calls.get() + 1);
            Err("translation_invalid_response".into())
        };
        let mut batch = BatchRequests::default();
        let (first, reused) = batch.translate("same", failing_provider);
        assert_eq!(first.unwrap_err(), "translation_invalid_response");
        assert!(!reused);
        let (second, reused) = batch.translate("same", failing_provider);
        assert_eq!(second.unwrap_err(), "translation_invalid_response");
        assert!(reused);
        assert_eq!(calls.get(), 1);
        let mut next = BatchRequests::default();
        let (_, reused) = next.translate("same", failing_provider);
        assert!(!reused);
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn cancelled_or_old_token_never_executes_commit_callback() {
        let session = TranslationSession::default();
        let token = session.begin(1).unwrap();
        session.cancel();
        let commits = std::cell::Cell::new(0);
        let commit = || {
            commits.set(commits.get() + 1);
            Ok(())
        };
        assert_eq!(
            session.with_active(token, commit).unwrap_err(),
            "translation_cancelled"
        );
        session.finish(token);
        let next = session.begin(1).unwrap();
        assert_eq!(
            session.with_active(token, commit).unwrap_err(),
            "translation_cancelled"
        );
        assert_eq!(commits.get(), 0);
        session.with_active(next, commit).unwrap();
        assert_eq!(commits.get(), 1);
    }

    #[test]
    fn cancellation_serializes_after_an_entered_commit_gate_without_sleeps() {
        use std::sync::{Arc, Barrier, mpsc};
        let session = Arc::new(TranslationSession::default());
        let token = session.begin(1).unwrap();
        let release = Arc::new(Barrier::new(2));
        let (entered_tx, entered_rx) = mpsc::channel();
        let (events_tx, events_rx) = mpsc::channel();
        let worker_session = session.clone();
        let worker_release = release.clone();
        let worker_events = events_tx.clone();
        let worker = std::thread::spawn(move || {
            worker_session.with_active(token, || {
                let locked = matches!(
                    worker_session.0.try_lock(),
                    Err(std::sync::TryLockError::WouldBlock)
                );
                entered_tx.send(locked).unwrap();
                worker_release.wait();
                worker_events.send("commit").unwrap();
                Ok(())
            })
        });
        let held_gate = entered_rx.recv().unwrap();
        let cancel_session = session.clone();
        let cancel_started = Arc::new(Barrier::new(2));
        let thread_started = cancel_started.clone();
        let canceller = std::thread::spawn(move || {
            thread_started.wait();
            cancel_session.cancel();
            events_tx.send("cancel").unwrap();
        });
        cancel_started.wait();
        release.wait();
        worker.join().unwrap().unwrap();
        canceller.join().unwrap();
        assert!(held_gate, "commit callback must hold the cancellation gate");
        assert_eq!(events_rx.recv().unwrap(), "commit");
        assert_eq!(events_rx.recv().unwrap(), "cancel");
        let commits = std::cell::Cell::new(0);
        assert_eq!(
            session
                .with_active(token, || {
                    commits.set(1);
                    Ok(())
                })
                .unwrap_err(),
            "translation_cancelled"
        );
        assert_eq!(commits.get(), 0);
    }
}
