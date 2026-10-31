// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026 Kirsikka (GitHub: Tom039224)

use crate::types::{LogEvent, ProgressEvent};
use serde::Serialize;
use std::collections::HashMap;
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

pub struct JobControl {
    cancel: AtomicBool,
    child: Mutex<Option<Arc<Mutex<Child>>>>,
}

pub struct JobManager {
    jobs: Mutex<HashMap<String, Arc<JobControl>>>,
}

impl JobManager {
    pub fn new() -> Self {
        Self {
            jobs: Mutex::new(HashMap::new()),
        }
    }

    pub fn begin(&self, app: &AppHandle, id: &str, tool: &str) -> Result<Job, String> {
        if id.trim().is_empty() {
            return Err("ジョブ ID が空です".into());
        }
        let mut jobs = self
            .jobs
            .lock()
            .map_err(|_| "ジョブ管理のロックに失敗しました")?;
        if jobs.contains_key(id) {
            return Err(format!("ジョブ {id} はすでに実行中です"));
        }
        let control = Arc::new(JobControl {
            cancel: AtomicBool::new(false),
            child: Mutex::new(None),
        });
        jobs.insert(id.to_string(), control.clone());
        Ok(Job {
            app: Some(app.clone()),
            id: id.to_string(),
            tool: tool.to_string(),
            control,
            range: None,
        })
    }

    pub fn finish(&self, id: &str) {
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.remove(id);
        }
    }

    pub fn cancel(&self, id: &str) -> bool {
        if let Ok(jobs) = self.jobs.lock() {
            if let Some(control) = jobs.get(id) {
                control.cancel.store(true, Ordering::SeqCst);
                if let Ok(slot) = control.child.lock() {
                    if let Some(child) = slot.as_ref() {
                        if let Ok(mut child) = child.lock() {
                            let _ = child.kill();
                        }
                    }
                }
                return true;
            }
        }
        false
    }

    pub fn running(&self) -> usize {
        self.jobs.lock().map(|j| j.len()).unwrap_or(0)
    }
}

#[derive(Clone)]
pub struct Job {
    app: Option<AppHandle>,
    pub id: String,
    pub tool: String,
    control: Arc<JobControl>,
    /// Maps this job's 0–100 progress into a sub-range of the overall bar.
    /// Used when several engines run sequentially under one job (download →
    /// compression).
    range: Option<(f64, f64)>,
}

impl Job {
    /// A job that does not emit UI events. Used by the `utilbox-cli` harness
    /// and by unit tests.
    pub fn headless(tool: &str) -> Self {
        Self {
            app: None,
            id: format!("headless-{}", uuid::Uuid::new_v4()),
            tool: tool.to_string(),
            control: Arc::new(JobControl {
                cancel: AtomicBool::new(false),
                child: Mutex::new(None),
            }),
            range: None,
        }
    }

    /// Returns a clone of this job whose progress events are mapped into
    /// `from`–`to` percent of the overall bar.
    pub fn scaled(&self, from: f64, to: f64) -> Job {
        let from = from.clamp(0.0, 100.0);
        let to = to.clamp(from, 100.0);
        let mut job = self.clone();
        job.range = Some((from, to));
        job
    }

    /// Converts a job-local percentage into the percentage actually emitted.
    pub fn map_percent(&self, percent: f64) -> f64 {
        match self.range {
            Some((from, to)) => from + (percent.clamp(0.0, 100.0) / 100.0) * (to - from),
            None => percent,
        }
    }

    pub fn progress(&self, percent: f64, phase: &str, message: impl Into<String>) {
        self.progress_detail(percent, phase, message, None);
    }

    pub fn progress_detail(
        &self,
        percent: f64,
        phase: &str,
        message: impl Into<String>,
        detail: Option<String>,
    ) {
        let ev = ProgressEvent {
            job_id: self.id.clone(),
            tool: self.tool.clone(),
            phase: phase.to_string(),
            percent: (self.map_percent(percent) * 10.0).round() / 10.0,
            message: message.into(),
            detail,
        };
        if let Some(app) = &self.app {
            let _ = app.emit("job-progress", &ev);
        }
    }

    pub fn emit_struct<T: Serialize + Clone>(&self, event: &str, payload: &T) {
        if let Some(app) = &self.app {
            let _ = app.emit(event, payload);
        }
    }

    pub fn log(&self, line: impl Into<String>) {
        let line = line.into();
        if line.trim().is_empty() {
            return;
        }
        if let Some(app) = &self.app {
            let _ = app.emit(
                "job-log",
                &LogEvent {
                    job_id: self.id.clone(),
                    line,
                },
            );
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.control.cancel.load(Ordering::SeqCst)
    }

    pub fn check(&self) -> Result<(), String> {
        if self.is_cancelled() {
            Err("キャンセルされました".into())
        } else {
            Ok(())
        }
    }

    pub fn attach_child(&self, child: Arc<Mutex<Child>>) {
        if let Ok(mut slot) = self.control.child.lock() {
            *slot = Some(child);
        }
    }

    pub fn detach_child(&self) {
        if let Ok(mut slot) = self.control.child.lock() {
            *slot = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_progress_into_a_range() {
        let job = Job::headless("test");
        let scaled = job.scaled(80.0, 100.0);
        assert_eq!(scaled.map_percent(0.0), 80.0);
        assert_eq!(scaled.map_percent(50.0), 90.0);
        assert_eq!(scaled.map_percent(100.0), 100.0);
        assert_eq!(scaled.map_percent(150.0), 100.0);
        assert_eq!(job.map_percent(42.0), 42.0);
    }

    #[test]
    fn clamps_reversed_ranges() {
        let job = Job::headless("test").scaled(90.0, 50.0);
        assert_eq!(job.map_percent(0.0), 90.0);
        assert_eq!(job.map_percent(100.0), 90.0);
    }
}
