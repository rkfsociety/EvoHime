use std::{future::Future, sync::Arc};

use tokio::{
    sync::{Mutex, Semaphore},
    task::JoinSet,
};

/// Максимальное число одновременно выполняемых detached-задач одного Core.
///
/// Запуск не ставит работу в неограниченную очередь: вызывающий получает
/// `false`, если все слоты заняты. Это сохраняет bounded memory и позволяет
/// IPC-пути вернуть управляемую ошибку вместо бесконтрольного spawning.
pub(crate) const DEFAULT_CAPACITY: usize = 16;

pub(crate) struct BoundedTaskGroup {
    permits: Arc<Semaphore>,
    tasks: Mutex<JoinSet<()>>,
}

impl BoundedTaskGroup {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            permits: Arc::new(Semaphore::new(capacity.max(1))),
            tasks: Mutex::new(JoinSet::new()),
        }
    }

    pub(crate) fn try_acquire(&self) -> Option<tokio::sync::OwnedSemaphorePermit> {
        self.permits.clone().try_acquire_owned().ok()
    }

    /// Пытается запустить задачу, удерживая permit до её завершения.
    pub(crate) async fn try_spawn<F>(&self, task: F) -> bool
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let Ok(permit) = self.permits.clone().try_acquire_owned() else {
            return false;
        };
        let mut tasks = self.tasks.lock().await;
        while tasks.try_join_next().is_some() {}
        tasks.spawn(async move {
            let _permit = permit;
            task.await;
        });
        true
    }

    #[cfg(test)]
    async fn active_tasks(&self) -> usize {
        let mut tasks = self.tasks.lock().await;
        while tasks.try_join_next().is_some() {}
        tasks.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn caps_active_tasks_and_reaps_completed_tasks() {
        let group = BoundedTaskGroup::new(2);
        let first = Arc::new(tokio::sync::Notify::new());
        let second = Arc::clone(&first);
        let notifier = Arc::clone(&first);
        assert!(
            group
                .try_spawn(async move { second.notified().await })
                .await
        );
        assert!(group.try_spawn(async move { first.notified().await }).await);
        assert!(!group.try_spawn(async {}).await);

        tokio::time::sleep(Duration::from_millis(10)).await;
        notifier.notify_waiters();
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(group.active_tasks().await, 0);
        assert!(group.try_spawn(async {}).await);
    }
}
