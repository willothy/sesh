use anyhow::{anyhow, Context, Result};
use std::{
    io,
    os::unix::{
        io::{FromRawFd, RawFd},
        process::CommandExt,
    },
    process::{Command, Stdio},
    ptr,
    time::Duration,
};
use tokio::fs::File;

use crate::{error::CResult, term::Size};

pub struct Pty {
    /// Master FD
    fd: RawFd,
    /// R/W access to the PTY
    file: File,
    /// Pid of the child process
    pid: i32,
}

impl Pty {
    pub fn spawn(shell: &str, args: Vec<String>, size: &Size) -> Result<Pty> {
        let (master, slave) = openpty(&size)?;

        let mut cmd = Command::new(&shell);
        cmd.args(args)
            .stdin(unsafe { Stdio::from_raw_fd(slave) })
            .stdout(unsafe { Stdio::from_raw_fd(slave) })
            .stderr(unsafe { Stdio::from_raw_fd(slave) });

        unsafe {
            cmd.pre_exec(pre_exec);
        }
        cmd.spawn()
            .map_err(|_| anyhow!("Failed to spawn shell"))
            .and_then(|e| {
                let pty = Pty {
                    fd: master,
                    file: unsafe { File::from_raw_fd(master) },
                    pid: e.id() as i32,
                };

                pty.resize(&size)?;

                Ok(pty)
            })
    }

    pub fn file(&mut self) -> &mut File {
        &mut self.file
    }

    pub fn fd(&self) -> RawFd {
        self.fd
    }

    /// Resizes the child pty.
    pub fn resize(&self, size: &Size) -> Result<()> {
        unsafe {
            libc::ioctl(
                self.fd,
                libc::TIOCSWINSZ,
                &Into::<libc::winsize>::into(size),
            )
            .to_result()
            .map(|_| ())
            .context("Failed to open pty")
        }
    }
}

/// Creates a pty with the given size and returns the (master, slave)
/// file descriptors attached to it.
fn openpty(size: &Size) -> Result<(RawFd, RawFd)> {
    let mut master = 0;
    let mut slave = 0;

    unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            ptr::null_mut(),
            ptr::null(),
            &size.into(),
        )
        .to_result()
        .context("Failed to open pty")?;

        // Configure master to be non blocking
        let current_config = libc::fcntl(master, libc::F_GETFL, 0)
            .to_result()
            .context("Failed to open pty")?;

        libc::fcntl(master, libc::F_SETFL, current_config)
            .to_result()
            .context("Failed to open pty")?;
    }

    Ok((master, slave))
}

// Runs between fork and exec calls
fn pre_exec() -> io::Result<()> {
    unsafe {
        // Create a new process group, this process being the master
        libc::setsid()
            .to_result()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, ""))?;

        // Set this process as the controling terminal
        libc::ioctl(0, libc::TIOCSCTTY, 1)
            .to_result()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, ""))?;
    }

    Ok(())
}

/// Handle cleanup automatically
impl Drop for Pty {
    fn drop(&mut self) {
        unsafe {
            let fd = self.fd.clone();
            let pid = self.pid.clone();
            // Close file descriptor
            libc::close(fd);
            // Kill the owned processed when the Pty is dropped
            libc::kill(pid, libc::SIGTERM);
            std::thread::sleep(Duration::from_millis(5));

            let mut status = 0;
            // make sure the process has exited
            libc::waitpid(pid, &mut status, libc::WNOHANG);

            // if it hasn't exited, force kill it and clean up the zombie process
            if status <= 0 {
                // The process exists but hasn't changed state, or there was an error
                libc::kill(pid, libc::SIGKILL);
                libc::waitpid(pid, &mut status, 0);
            }
        }
    }
}
