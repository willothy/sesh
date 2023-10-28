use std::{future::Future, time::Duration};

use anyhow::Result;
use termion;

pub struct Size {
    /// Number of columns
    pub cols: u16,
    /// Number of rows
    pub rows: u16,
}

impl Size {
    pub fn term_size() -> Result<Size> {
        let (cols, rows) = termion::terminal_size()?;
        Ok(Size { cols, rows })
    }
}

impl From<&Size> for libc::winsize {
    fn from(val: &Size) -> Self {
        libc::winsize {
            ws_row: val.rows,
            ws_col: val.cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        }
    }
}

/// Future that checks if a process exists and resolves when it doesn't.
struct ExitFuture {
    pid: i32,
    interval: tokio::time::Interval,
}

impl Future for ExitFuture {
    type Output = ();

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        unsafe {
            // This doesn't actually kill the process, it just checks if it exists
            if libc::kill(self.pid, 0) == -1 {
                // TODO: Figure out why this doesn't work on M1/M2 macs
                #[cfg(target_arch = "aarch64")]
                let errno = *libc::__error();
                #[cfg(not(target_arch = "aarch64"))]
                let errno = *libc::__errno_location();
                // process doesn't exist / has exited
                if errno == libc::ESRCH {
                    return std::task::Poll::Ready(());
                }
            }
            if self.interval.poll_tick(cx).is_ready() {
                cx.waker().wake_by_ref();
            }
            std::task::Poll::Pending
        }
    }
}

/// Wait for the given process to exit, polling every 20ms.
/// Resolves immediately if the process doesn't exist.
pub async fn process_exit(pid: i32) {
    ExitFuture {
        pid,
        interval: tokio::time::interval(Duration::from_millis(20)),
    }
    .await
}
