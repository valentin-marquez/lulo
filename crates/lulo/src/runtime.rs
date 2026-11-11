//! The tokio runtime network and keyring work runs on. GPUI has its own executor, so UI code
//! spawns here and awaits the join handle.

use std::future::Future;
use std::sync::LazyLock;

static RUNTIME: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("start the background runtime")
});

pub fn spawn<F>(future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    RUNTIME.spawn(future)
}

/// Runs blocking work (keyring calls) on the runtime's blocking pool.
pub async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    RUNTIME
        .spawn_blocking(work)
        .await
        .map_err(|err| err.to_string())
}
