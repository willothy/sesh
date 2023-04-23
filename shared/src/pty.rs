use anyhow::{anyhow, Context, Result};
use std::{
    ffi::OsStr,
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

const PTY_ERR: &str = "[pty.rs] Failed to open pty";
const PRG_ERR: &str = "[pty.rs] Failed to spawn shell";

pub struct Pty {
    /// Master FD
    fd: RawFd,
    /// R/W access to the PTY
    file: File,
    /// Pid of the child process
    pid: i32,
    kill_on_drop: bool,
}

pub struct PtyBuilder {
    inner: Command,
    daemonize: bool,
}

impl PtyBuilder {
    pub fn arg<S: AsRef<OsStr>>(mut self, arg: S) -> Self {
        self.inner.arg(arg);
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.inner.args(args);
        self
    }

    pub fn env_clear(mut self) -> Self {
        self.inner.env_clear();
        self
    }

    pub fn env<K, V>(mut self, key: K, val: V) -> Self
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.inner.env(key, val);
        self
    }

    pub fn envs<I, K, V>(mut self, vars: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.inner.envs(vars);
        self
    }

    pub fn daemonize(mut self) -> Self {
        self.daemonize = true;
        self
    }

    pub fn kill_on_drop(mut self) -> Self {
        self.daemonize = false;
        self
    }

    pub fn set_daemonize(&mut self, daemonize: bool) {
        self.daemonize = daemonize;
    }

    pub fn current_dir<P: AsRef<std::path::Path>>(mut self, dir: P) -> Self {
        self.inner.current_dir(dir);
        self
    }

    pub fn spawn(self, size: &Size) -> Result<Pty> {
        let (master, slave) = Pty::open(size)?;

        let mut cmd = self.inner;

        cmd.stdin(unsafe { Stdio::from_raw_fd(slave) })
            .stdout(unsafe { Stdio::from_raw_fd(slave) })
            .stderr(unsafe { Stdio::from_raw_fd(slave) });

        unsafe {
            cmd.pre_exec(Pty::pre_exec);
        }
        cmd.spawn().map_err(|_| anyhow!(PRG_ERR)).and_then(|e| {
            let pty = Pty {
                fd: master,
                file: unsafe { File::from_raw_fd(master) },
                pid: e.id() as i32,
                kill_on_drop: !self.daemonize,
            };

            pty.resize(&size)?;

            Ok(pty)
        })
    }
}

impl Pty {
    pub fn new(program: impl AsRef<str>) -> PtyBuilder {
        PtyBuilder {
            inner: Command::new(program.as_ref()),
            daemonize: false,
        }
    }

    pub fn spawn(program: &str, args: Vec<String>, size: &Size) -> Result<Pty> {
        Pty::new(program).args(args).spawn(size)
    }

    pub fn daemonize(&mut self) {
        self.kill_on_drop = false;
    }

    pub fn pid(&self) -> i32 {
        self.pid
    }

    pub fn file(&self) -> &File {
        &self.file
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
            .context(PTY_ERR)
        }
    }

    /// Creates a pty with the given size and returns the (master, slave)
    /// file descriptors attached to it.
    pub fn open(size: &Size) -> Result<(RawFd, RawFd)> {
        let mut master = 0;
        let mut slave = 0;

        unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                ptr::null_mut(),
                ptr::null_mut(),
                &mut size.into(),
            )
            .to_result()
            .context(PTY_ERR)?;

            // Configure master to be non blocking
            let current_config = libc::fcntl(master, libc::F_GETFL, 0)
                .to_result()
                .context(PTY_ERR)?;

            libc::fcntl(master, libc::F_SETFL, current_config)
                .to_result()
                .context(PTY_ERR)?;
        }

        Ok((master, slave))
    }

    // Runs between fork and exec calls
    fn pre_exec() -> io::Result<()> {
        unsafe {
            if libc::getpid() == 0 {
                std::process::exit(0);
            }
            // Create a new process group, this process being the master
            libc::setsid().to_result().map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Failed to create process group: {}", e),
                )
            })?;

            // Set this process as the controling terminal
            libc::ioctl(0, libc::TIOCSCTTY as u64, 1)
                .to_result()
                .map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::Other,
                        format!("Failed to set controlling terminal: {}", e),
                    )
                })?;
        }

        Ok(())
    }
}

/// Handle cleanup automatically
impl Drop for Pty {
    fn drop(&mut self) {
        unsafe {
            if self.kill_on_drop {
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
}
