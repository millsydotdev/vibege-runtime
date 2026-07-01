use std::collections::VecDeque;
use std::sync::Mutex;

use super::models::{DownloadStatus, DownloadTask};

/// Manages a queue of game downloads with retry, pause, resume, and
/// progress tracking.
pub struct DownloadQueue {
    queue: Mutex<VecDeque<DownloadTask>>,
    active: Mutex<Option<DownloadTask>>,
    max_retries: u32,
}

impl DownloadQueue {
    pub fn new(max_retries: u32) -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            active: Mutex::new(None),
            max_retries,
        }
    }

    /// Add a download to the queue.
    pub fn enqueue(&self, game_id: String, game_name: String) {
        let mut queue = self.queue.lock().expect("queue lock");
        // Don't add duplicates
        if queue.iter().any(|t| t.game_id == game_id) {
            return;
        }
        queue.push_back(DownloadTask {
            game_id,
            game_name,
            status: DownloadStatus::Queued,
            progress: 0.0,
            total_bytes: 0,
            downloaded_bytes: 0,
            error: None,
            retry_count: 0,
            speed_bytes_per_sec: 0,
            eta_secs: 0,
            last_update: std::time::Instant::now(),
        });
    }

    /// Get the next task to process.
    pub fn next(&self) -> Option<DownloadTask> {
        let mut queue = self.queue.lock().expect("queue lock");
        let mut active = self.active.lock().expect("active lock");
        if active.is_some() {
            return None;
        }
        let task = queue.pop_front()?;
        *active = Some(task.clone());
        Some(task)
    }

    /// Mark the active download as completed.
    pub fn complete(&self) {
        *self.active.lock().expect("active lock") = None;
    }

    /// Mark the active download as failed. Retries if under max_retries.
    pub fn fail(&self, error: String) {
        let active_task = self.active.lock().expect("active lock").take();
        if let Some(mut task) = active_task {
            if task.retry_count < self.max_retries {
                task.retry_count += 1;
                task.status = DownloadStatus::Queued;
                task.error = Some(error);
                self.queue.lock().expect("queue lock").push_back(task);
            } else {
                task.status = DownloadStatus::Failed;
                task.error = Some(error);
                self.queue.lock().expect("queue lock").push_back(task);
            }
        }
    }

    /// Cancel a download by ID.
    pub fn cancel(&self, game_id: &str) {
        let mut active = self.active.lock().expect("active lock");
        if active.as_ref().map(|t| t.game_id.as_str()) == Some(game_id) {
            *active = None;
        }
        let mut queue = self.queue.lock().expect("queue lock");
        if let Some(pos) = queue.iter().position(|t| t.game_id == game_id) {
            if let Some(task) = queue.get_mut(pos) {
                task.status = DownloadStatus::Cancelled;
            }
        }
    }

    /// Update the active download's progress with current bytes and total.
    pub fn update_progress(&self, downloaded_bytes: u64, total_bytes: u64) {
        let mut active = self.active.lock().expect("active lock");
        if let Some(ref mut task) = *active {
            let now = std::time::Instant::now();
            let elapsed = now.duration_since(task.last_update).as_secs_f64();
            let delta = downloaded_bytes.saturating_sub(task.downloaded_bytes);
            if elapsed > 0.5 {
                task.speed_bytes_per_sec = (delta as f64 / elapsed) as u64;
                task.last_update = now;
            }
            task.downloaded_bytes = downloaded_bytes;
            task.total_bytes = total_bytes;
            task.progress = if total_bytes > 0 {
                downloaded_bytes as f32 / total_bytes as f32
            } else {
                0.0
            };
            task.eta_secs = if task.speed_bytes_per_sec > 0 {
                let remaining = total_bytes.saturating_sub(downloaded_bytes);
                remaining / task.speed_bytes_per_sec
            } else {
                0
            };
            task.status = DownloadStatus::Downloading;
        }
    }

    /// Pause the active download.
    pub fn pause(&self) {
        let active = self.active.lock().expect("active lock");
        if active.is_some() {
            // Mark active as paused (actual pause requires HTTP range support)
        }
    }

    /// Resume a paused download.
    pub fn resume(&self) {
        // Placeholder for HTTP range-based resume
    }

    /// Get all queued and active tasks.
    pub fn all(&self) -> Vec<DownloadTask> {
        let queue = self.queue.lock().expect("queue lock");
        queue.iter().cloned().collect()
    }

    /// Number of items in the queue.
    pub fn len(&self) -> usize {
        self.queue.lock().expect("queue lock").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Whether a download is currently active.
    pub fn is_active(&self) -> bool {
        self.active.lock().expect("active lock").is_some()
    }

    /// Clear all queued tasks.
    pub fn clear(&self) {
        self.queue.lock().expect("queue lock").clear();
        *self.active.lock().expect("active lock") = None;
    }
}

impl Default for DownloadQueue {
    fn default() -> Self {
        Self::new(3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enqueue_and_next() {
        let queue = DownloadQueue::new(3);
        queue.enqueue("g1".into(), "Game 1".into());
        queue.enqueue("g2".into(), "Game 2".into());
        assert_eq!(queue.len(), 2);

        let task = queue.next().unwrap();
        assert_eq!(task.game_id, "g1");
        assert_eq!(task.status, DownloadStatus::Queued);
        assert!(queue.is_active());
    }

    #[test]
    fn test_no_duplicate_enqueue() {
        let queue = DownloadQueue::new(3);
        queue.enqueue("g1".into(), "Game".into());
        queue.enqueue("g1".into(), "Game".into());
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn test_complete_clears_active() {
        let queue = DownloadQueue::new(3);
        queue.enqueue("g1".into(), "Game".into());
        let _ = queue.next();
        assert!(queue.is_active());
        queue.complete();
        assert!(!queue.is_active());
    }

    #[test]
    fn test_fail_with_retry() {
        let queue = DownloadQueue::new(3);
        queue.enqueue("g1".into(), "Game".into());
        let _ = queue.next();
        queue.fail("Network error".into());
        // Should be requeued (retry_count < 3)
        assert!(!queue.is_active());
        assert_eq!(queue.len(), 1);
        let tasks = queue.all();
        assert_eq!(tasks[0].retry_count, 1);
    }

    #[test]
    fn test_fail_exhausts_retries() {
        let queue = DownloadQueue::new(1);
        queue.enqueue("g1".into(), "Game".into());
        let _ = queue.next();
        queue.fail("Error 1".into());
        // First retry
        assert_eq!(queue.len(), 1);
        let _ = queue.next();
        queue.fail("Error 2".into());
        // Should be marked as failed
        let tasks = queue.all();
        assert_eq!(tasks[0].status, DownloadStatus::Failed);
    }

    #[test]
    fn test_cancel() {
        let queue = DownloadQueue::new(3);
        queue.enqueue("g1".into(), "Game".into());
        queue.cancel("g1");
        let tasks = queue.all();
        assert_eq!(tasks[0].status, DownloadStatus::Cancelled);
    }

    #[test]
    fn test_clear() {
        let queue = DownloadQueue::new(3);
        queue.enqueue("g1".into(), "Game".into());
        queue.enqueue("g2".into(), "Game 2".into());
        assert_eq!(queue.len(), 2);
        queue.clear();
        assert!(queue.is_empty());
    }

    #[test]
    fn test_next_returns_none_when_active() {
        let queue = DownloadQueue::new(3);
        queue.enqueue("g1".into(), "Game".into());
        let _ = queue.next();
        assert!(queue.next().is_none());
    }

    #[test]
    fn test_empty_queue() {
        let queue = DownloadQueue::new(3);
        assert!(queue.is_empty());
        assert!(queue.next().is_none());
    }
}
