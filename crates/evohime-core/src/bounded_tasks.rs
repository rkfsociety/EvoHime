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
        let Some(permit) = self.try_acquire() else {
            return false;
        };
        self.spawn_reserved(permit, task).await;
        true
    }

    /// Spawns a task using capacity reserved before its durable work is created.
    pub(crate) async fn spawn_reserved<F>(
        &self,
        permit: tokio::sync::OwnedSemaphorePermit,
        task: F,
    ) where
        F: Future<Output = ()> + Send + 'static,
    {
        let mut tasks = self.tasks.lock().await;
        while tasks.try_join_next().is_some() {}
        tasks.spawn(async move {
            let _permit = permit;
            task.await;
        });
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

    #[tokio::test]
    async fn a_reserved_slot_stays_bounded_until_the_task_is_released() {
        let group = BoundedTaskGroup::new(1);
        let permit = group.try_acquire().expect("reserved capacity");
        let (start_tx, start_rx) = tokio::sync::oneshot::channel::<()>();
        group
            .spawn_reserved(permit, async move {
                if start_rx.await.is_ok() {
                    tokio::task::yield_now().await;
                }
            })
            .await;

        assert!(!group.try_spawn(async {}).await);
        let _ = start_tx.send(());
        let _ = group.tasks.lock().await.join_next().await;
        assert!(group.try_spawn(async {}).await);
    }
}
