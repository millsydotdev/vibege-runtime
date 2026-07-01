use std::collections::VecDeque;
use std::sync::Mutex;

use super::models::{DownloadStatus, DownloadTask};

/// Manages a queue of game downloads with concurrent support, retry,
/// pause, resume, verify, and progress tracking.
pub struct DownloadQueue {
    queue: Mutex<VecDeque<DownloadTask>>,
    active: Mutex<Vec<DownloadTask>>,
    max_concurrent: usize,
    max_retries: u32,
    completed_count: Mutex<u32>,
    failed_count: Mutex<u32>,
}

impl DownloadQueue {
    pub fn new(max_concurrent: usize, max_retries: u32) -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            active: Mutex::new(Vec::new()),
            max_concurrent,
            max_retries,
            completed_count: Mutex::new(0),
            failed_count: Mutex::new(0),
        }
    }

    pub fn enqueue(&self, game_id: String, game_name: String) {
        let mut queue = self.queue.lock().expect("queue lock");
        let active = self.active.lock().expect("active lock");
        if queue.iter().any(|t| t.game_id == game_id)
            || active.iter().any(|t| t.game_id == game_id)
        {
            return;
        }
        drop(active);
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

    /// Enqueue all game names as downloads.
    pub fn enqueue_all(&self, games: &[(String, String)]) {
        for (id, name) in games {
            self.enqueue(id.clone(), name.clone());
        }
    }

    /// Claim the next available slot(s). Returns tasks ready for download.
    /// Call this repeatedly from a worker loop.
    pub fn claim_next(&self) -> Vec<DownloadTask> {
        let mut queue = self.queue.lock().expect("queue lock");
        let mut active = self.active.lock().expect("active lock");
        let mut claimed = Vec::new();

        while active.len() < self.max_concurrent {
            let task = match queue.pop_front() {
                Some(t) => t,
                None => break,
            };
            let mut task = task;
            task.status = DownloadStatus::Downloading;
            claimed.push(task.clone());
            active.push(task);
        }

        claimed
    }

    /// Mark a download as completed.
    pub fn complete(&self, game_id: &str) {
        let mut active = self.active.lock().expect("active lock");
        if let Some(pos) = active.iter().position(|t| t.game_id == game_id) {
            active.remove(pos);
        }
        *self.completed_count.lock().expect("completed lock") += 1;
    }

    /// Mark a download as failed. Retries if under max_retries.
    pub fn fail(&self, game_id: &str, error: String) {
        let mut active = self.active.lock().expect("active lock");
        let task = if let Some(pos) = active.iter().position(|t| t.game_id == game_id) {
            Some(active.remove(pos))
        } else {
            None
        };
        drop(active);

        if let Some(mut task) = task {
            if task.retry_count < self.max_retries {
                task.retry_count += 1;
                task.status = DownloadStatus::Queued;
                task.error = Some(error);
                self.queue.lock().expect("queue lock").push_back(task);
            } else {
                task.status = DownloadStatus::Failed;
                task.error = Some(error);
                self.queue.lock().expect("queue lock").push_back(task);
                *self.failed_count.lock().expect("failed lock") += 1;
            }
        }
    }

    /// Mark a download as verifying (post-download integrity check).
    pub fn mark_verifying(&self, game_id: &str) {
        let mut active = self.active.lock().expect("active lock");
        if let Some(task) = active.iter_mut().find(|t| t.game_id == game_id) {
            task.status = DownloadStatus::Verifying;
        }
    }

    /// Mark a download as installing (post-verify extraction).
    pub fn mark_installing(&self, game_id: &str) {
        let mut active = self.active.lock().expect("active lock");
        if let Some(task) = active.iter_mut().find(|t| t.game_id == game_id) {
            task.status = DownloadStatus::Installing;
        }
    }

    pub fn cancel(&self, game_id: &str) {
        let mut active = self.active.lock().expect("active lock");
        if let Some(pos) = active.iter().position(|t| t.game_id == game_id) {
            if let Some(task) = active.get_mut(pos) {
                task.status = DownloadStatus::Cancelled;
            }
            active.remove(pos);
        }
        drop(active);
        let mut queue = self.queue.lock().expect("queue lock");
        if let Some(pos) = queue.iter().position(|t| t.game_id == game_id) {
            if let Some(task) = queue.get_mut(pos) {
                task.status = DownloadStatus::Cancelled;
            }
        }
    }

    pub fn cancel_all(&self) {
        let mut active = self.active.lock().expect("active lock");
        for task in active.iter_mut() {
            task.status = DownloadStatus::Cancelled;
        }
        active.clear();
        drop(active);
        let mut queue = self.queue.lock().expect("queue lock");
        for task in queue.iter_mut() {
            task.status = DownloadStatus::Cancelled;
        }
        queue.clear();
    }

    pub fn update_progress(&self, game_id: &str, downloaded_bytes: u64, total_bytes: u64) {
        let mut active = self.active.lock().expect("active lock");
        if let Some(ref mut task) = active.iter_mut().find(|t| t.game_id == game_id) {
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
        }
    }

    /// Whether a specific game is in the queue or active.
    pub fn is_queued(&self, game_id: &str) -> bool {
        let queue = self.queue.lock().expect("queue lock");
        if queue.iter().any(|t| t.game_id == game_id) {
            return true;
        }
        let active = self.active.lock().expect("active lock");
        active.iter().any(|t| t.game_id == game_id)
    }

    /// Whether any downloads are active.
    pub fn is_active(&self) -> bool {
        !self.active.lock().expect("active lock").is_empty()
    }

    /// All tasks (queued + active).
    pub fn all(&self) -> Vec<DownloadTask> {
        let mut tasks: Vec<DownloadTask> = self.queue.lock().expect("queue lock").iter().cloned().collect();
        tasks.extend(self.active.lock().expect("active lock").iter().cloned());
        tasks
    }

    /// Count of active downloads.
    pub fn active_count(&self) -> usize {
        self.active.lock().expect("active lock").len()
    }

    pub fn len(&self) -> usize {
        self.queue.lock().expect("queue lock").len() + self.active_count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clear(&self) {
        self.queue.lock().expect("queue lock").clear();
        self.active.lock().expect("active lock").clear();
    }

    pub fn completed_count(&self) -> u32 {
        *self.completed_count.lock().expect("completed lock")
    }

    pub fn failed_count(&self) -> u32 {
        *self.failed_count.lock().expect("failed lock")
    }

    /// Reset counters (keeps queue intact).
    pub fn reset_counts(&self) {
        *self.completed_count.lock().expect("completed lock") = 0;
        *self.failed_count.lock().expect("failed lock") = 0;
    }
}

impl Default for DownloadQueue {
    fn default() -> Self {
        Self::new(3, 3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enqueue_and_claim() {
        let queue = DownloadQueue::new(2, 3);
        queue.enqueue("g1".into(), "Game 1".into());
        queue.enqueue("g2".into(), "Game 2".into());
        queue.enqueue("g3".into(), "Game 3".into());
        assert_eq!(queue.len(), 3);

        let claimed = queue.claim_next();
        assert_eq!(claimed.len(), 2); // up to max_concurrent
        assert!(queue.is_active());
        assert_eq!(queue.active_count(), 2);
    }

    #[test]
    fn test_no_duplicate_enqueue() {
        let queue = DownloadQueue::new(3, 3);
        queue.enqueue("g1".into(), "Game".into());
        queue.enqueue("g1".into(), "Game".into());
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn test_complete_clears_active() {
        let queue = DownloadQueue::new(3, 3);
        queue.enqueue("g1".into(), "Game".into());
        queue.claim_next();
        assert_eq!(queue.active_count(), 1);
        queue.complete("g1");
        assert_eq!(queue.active_count(), 0);
        assert_eq!(queue.completed_count(), 1);
    }

    #[test]
    fn test_fail_with_retry() {
        let queue = DownloadQueue::new(3, 3);
        queue.enqueue("g1".into(), "Game".into());
        queue.claim_next();
        queue.fail("g1", "Network error".into());
        assert_eq!(queue.active_count(), 0);
        assert_eq!(queue.len(), 1);
        let tasks = queue.all();
        assert_eq!(tasks[0].retry_count, 1);
        assert_eq!(tasks[0].status, DownloadStatus::Queued);
    }

    #[test]
    fn test_fail_exhausts_retries() {
        let queue = DownloadQueue::new(3, 1);
        queue.enqueue("g1".into(), "Game".into());
        queue.claim_next();
        queue.fail("g1", "Error 1".into());
        queue.claim_next();
        queue.fail("g1", "Error 2".into());
        let tasks = queue.all();
        assert_eq!(tasks[0].status, DownloadStatus::Failed);
        assert_eq!(queue.failed_count(), 1);
    }

    #[test]
    fn test_cancel() {
        let queue = DownloadQueue::new(3, 3);
        queue.enqueue("g1".into(), "Game".into());
        queue.cancel("g1");
        let tasks = queue.all();
        assert_eq!(tasks[0].status, DownloadStatus::Cancelled);
    }

    #[test]
    fn test_cancel_all() {
        let queue = DownloadQueue::new(3, 3);
        queue.enqueue("g1".into(), "Game 1".into());
        queue.enqueue("g2".into(), "Game 2".into());
        queue.cancel_all();
        assert!(queue.is_empty());
    }

    #[test]
    fn test_clear() {
        let queue = DownloadQueue::new(3, 3);
        queue.enqueue("g1".into(), "Game".into());
        queue.enqueue("g2".into(), "Game 2".into());
        queue.clear();
        assert!(queue.is_empty());
    }

    #[test]
    fn test_concurrent_limit() {
        let queue = DownloadQueue::new(2, 3);
        queue.enqueue("g1".into(), "A".into());
        queue.enqueue("g2".into(), "B".into());
        queue.enqueue("g3".into(), "C".into());
        queue.enqueue("g4".into(), "D".into());
        let claimed = queue.claim_next();
        assert_eq!(claimed.len(), 2);
        // Complete one, should allow next
        queue.complete("g1");
        let claimed2 = queue.claim_next();
        assert_eq!(claimed2.len(), 1);
        assert_eq!(queue.active_count(), 2);
    }

    #[test]
    fn test_progress_update() {
        let queue = DownloadQueue::new(3, 3);
        queue.enqueue("g1".into(), "Game".into());
        queue.claim_next();
        queue.update_progress("g1", 50, 100);
        let tasks = queue.all();
        let task = tasks.iter().find(|t| t.game_id == "g1").unwrap();
        assert!((task.progress - 0.5).abs() < 0.01);
        assert_eq!(task.downloaded_bytes, 50);
        assert_eq!(task.total_bytes, 100);
    }

    #[test]
    fn test_enqueue_all() {
        let queue = DownloadQueue::new(3, 3);
        queue.enqueue_all(&[
            ("g1".into(), "Game 1".into()),
            ("g2".into(), "Game 2".into()),
        ]);
        assert_eq!(queue.len(), 2);
    }

    #[test]
    fn test_is_queued() {
        let queue = DownloadQueue::new(3, 3);
        queue.enqueue("g1".into(), "Game".into());
        assert!(queue.is_queued("g1"));
        assert!(!queue.is_queued("g2"));
    }
}
