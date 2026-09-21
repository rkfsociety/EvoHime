use crate::runtime_contract::RuntimeError;
use rusqlite::Connection;
use std::ops::Deref;
use std::time::Duration;

/// Normative lock-retry schedule for chain-append transactions: attempt
/// immediately, then retry after 10ms, 50ms and 250ms before surfacing
/// `receipt.chain_conflict`. SQLite's own busy handler is disabled around
/// these attempts so this application-level schedule is authoritative.
const APPEND_RETRY_DELAYS_MS: [u64; 4] = [0, 10, 50, 250];

/// A hand-rolled `BEGIN IMMEDIATE` guard used only by the receipt-append
/// paths. `rusqlite::Connection::transaction_with_behavior` requires `&mut
/// self` and keeps that place mutably borrowed for the entire lifetime of
/// the returned `Transaction`, which makes an in-place retry-with-backoff
/// loop unrepresentable under NLL. Driving `BEGIN IMMEDIATE`/`COMMIT`/
/// `ROLLBACK` as raw statements over a shared `&Connection` sidesteps that.
pub(crate) struct RetryTransaction<'a> {
    connection: &'a Connection,
    finished: bool,
}

impl<'a> Deref for RetryTransaction<'a> {
    type Target = Connection;

    fn deref(&self) -> &Connection {
        self.connection
    }
}

impl<'a> RetryTransaction<'a> {
    pub(crate) fn begin(connection: &'a Connection) -> Result<Self, RuntimeError> {
        let _ = connection.busy_timeout(Duration::from_millis(0));
        for (index, delay) in APPEND_RETRY_DELAYS_MS.iter().enumerate() {
            if *delay > 0 {
                std::thread::sleep(Duration::from_millis(*delay));
            }
            match connection.execute_batch("BEGIN IMMEDIATE") {
                Ok(()) => {
                    let _ = connection.busy_timeout(Duration::from_secs(2));
                    if index > 0 {
                        crate::runtime::increment_metric_tx(
                            connection,
                            "receipt_append_busy_retries",
                            index as i64,
                        )?;
                    }
                    return Ok(Self {
                        connection,
                        finished: false,
                    });
                }
                Err(rusqlite::Error::SqliteFailure(err, _))
                    if matches!(
                        err.code,
                        rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
                    ) && index + 1 < APPEND_RETRY_DELAYS_MS.len() =>
                {
                    continue;
                }
                Err(rusqlite::Error::SqliteFailure(err, _))
                    if matches!(
                        err.code,
                        rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
                    ) =>
                {
                    let _ = connection.execute("INSERT INTO receipt_runtime_metrics(metric,value) VALUES('receipt_chain_conflicts',1) ON CONFLICT(metric) DO UPDATE SET value=value+1", []);
                    let _ = connection.busy_timeout(Duration::from_secs(2));
                    return Err(RuntimeError::Code("chain_conflict"));
                }
                Err(err) => {
                    let _ = connection.busy_timeout(Duration::from_secs(2));
                    return Err(RuntimeError::from(err));
                }
            }
        }
        unreachable!("APPEND_RETRY_DELAYS_MS is non-empty")
    }

    pub(crate) fn commit(mut self) -> Result<(), RuntimeError> {
        self.connection.execute_batch("COMMIT")?;
        self.finished = true;
        Ok(())
    }
}

impl Drop for RetryTransaction<'_> {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.connection.execute_batch("ROLLBACK");
        }
    }
}
