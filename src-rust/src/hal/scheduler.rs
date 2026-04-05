use crate::hal::traits::Timer;

/// Task function signature — Rust ABI, no parameters.
/// C-defined tasks are wrapped in thin Rust functions (see lib.rs).
pub type TaskFn = unsafe fn();

/// A scheduled task with frequency control
pub struct Task {
    pub exec: TaskFn,
    pub frequency: u64,
    pub next_run: u64,
}

impl Task {
    pub const fn new(exec: TaskFn, frequency: u64) -> Self {
        Self {
            exec,
            frequency,
            next_run: 0,
        }
    }
}

/// Run a task if its scheduled time has arrived (testable version).
/// Returns true if the task was executed.
pub fn run_task_at(task: &mut Task, current_time: u64) -> bool {
    if current_time < task.next_run {
        return false;
    }

    task.next_run = current_time + task.frequency;
    unsafe { (task.exec)() };
    true
}

/// Run a task using hardware timestamp from a Timer trait impl.
pub fn run_task<T: Timer>(task: &mut Task, timer: &T) -> bool {
    let current_time = timer.now_us_64();
    run_task_at(task, current_time)
}

/// Run all tasks in a task list once.
pub fn run_all_tasks<T: Timer>(tasks: &mut [Task], timer: &T) {
    for task in tasks.iter_mut() {
        run_task(task, timer);
    }
}

/// Frequency helper: convert Hz to microsecond interval
pub const fn hz(freq: u64) -> u64 {
    if freq == 0 {
        return 0;
    }
    1_000_000 / freq
}

/// Frequency helper: run as often as possible
pub const fn top() -> u64 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicU32, Ordering};

    static CALL_COUNT: AtomicU32 = AtomicU32::new(0);

    unsafe fn mock_task() {
        CALL_COUNT.fetch_add(1, Ordering::Relaxed);
    }

    #[test]
    fn test_hz() {
        assert_eq!(hz(1), 1_000_000);
        assert_eq!(hz(1000), 1_000);
        assert_eq!(hz(30), 33_333);
    }

    #[test]
    fn test_top() {
        assert_eq!(top(), 0);
    }

    #[test]
    fn test_task_new() {
        let task = Task::new(mock_task, hz(30));
        assert_eq!(task.frequency, 33_333);
        assert_eq!(task.next_run, 0);
    }

    #[test]
    fn test_run_task_at_first_call() {
        CALL_COUNT.store(0, Ordering::Relaxed);
        let mut task = Task::new(mock_task, hz(30));
        // First call at time 0: next_run=0, should execute
        assert!(run_task_at(&mut task, 0));
        assert_eq!(CALL_COUNT.load(Ordering::Relaxed), 1);
        assert_eq!(task.next_run, 33_333);
    }

    #[test]
    fn test_run_task_at_too_early() {
        CALL_COUNT.store(0, Ordering::Relaxed);
        let mut task = Task::new(mock_task, hz(30));
        // First call runs
        run_task_at(&mut task, 1000);
        let count_after_first = CALL_COUNT.load(Ordering::Relaxed);

        // Second call too early — should NOT run
        assert!(!run_task_at(&mut task, 2000));
        assert_eq!(CALL_COUNT.load(Ordering::Relaxed), count_after_first);
    }

    #[test]
    fn test_run_task_at_on_schedule() {
        CALL_COUNT.store(0, Ordering::Relaxed);
        let mut task = Task::new(mock_task, hz(30));
        // Run at time 0
        run_task_at(&mut task, 0);
        // Run at time 33333 (exactly next_run)
        assert!(run_task_at(&mut task, 33_333));
        assert_eq!(CALL_COUNT.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn test_run_task_top_frequency() {
        CALL_COUNT.store(0, Ordering::Relaxed);
        let mut task = Task::new(mock_task, top());
        // frequency=0 means run every call
        run_task_at(&mut task, 100);
        run_task_at(&mut task, 101);
        run_task_at(&mut task, 102);
        assert_eq!(CALL_COUNT.load(Ordering::Relaxed), 3);
    }
}
