use std::{os::fd::AsRawFd, path::PathBuf};

use anyhow::Result;
use log::info;
use sesh_proto::{SeshStartResponse, WinSize};
use sesh_shared::{pty::Pty, term::Size};

use crate::{Seshd, Session};

use super::CommandResponse;

impl Seshd {
    pub async fn exec_start(
        &self,
        name: String,
        program: String,
        args: Vec<String>,
        size: Option<WinSize>,
        pwd: String,
        env: Vec<(String, String)>,
    ) -> Result<CommandResponse> {
        let name = PathBuf::from(&name)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or(name.replace('/', "_"));

        let mut session_name = name.clone();
        let mut i = 0;
        while self.sessions.contains_key(&session_name) {
            session_name = format!("{}-{}", name, i);
            i += 1;
        }

        let socket_path = self.runtime_dir.join(format!("{}.sock", session_name));

        let pty = Pty::builder(&program)
            .args(args)
            .current_dir(pwd)
            .envs(env)
            .env("SESH_SESSION", socket_path.clone())
            .env("SESH_NAME", session_name.clone())
            .spawn(&Size::term_size()?)?;

        let pid = pty.pid();
        let size = if let Some(size) = size {
            Size {
                rows: size.rows as u16,
                cols: size.cols as u16,
            }
        } else {
            Size::term_size()?
        };
        pty.resize(&size)?;

        let session = Session::new(
            self.sessions.len(),
            session_name.clone(),
            program.clone(),
            pty,
            PathBuf::from(&socket_path),
        )?;
        self.sessions.insert(session.name.clone(), session);

        tokio::task::spawn({
            let session = self
                .sessions
                .get(&session_name)
                .expect("session should exist in sessions");
            let sock_path = session.info.sock_path().clone();
            let socket = session.listener.clone();
            let file = session.pty.file().as_raw_fd();
            // Duplicate FD
            // I do not know why this makes the socket connection not die, but it does
            let file = unsafe { libc::fcntl(file, libc::F_DUPFD, file) };
            let connected = session.info.connected();
            let attach_time = session.info.attach_time.clone();

            info!(target: &session.log_group(), "Starting on {}", session.info.sock_path().display());
            async move {
                Session::start(sock_path, socket, file, connected, size, attach_time).await?;
                Result::<_, anyhow::Error>::Ok(())
            }
        });

        Ok(CommandResponse::StartSession(SeshStartResponse {
            pid,
            program,
            name: session_name,
            socket: socket_path.to_string_lossy().to_string(),
        }))
    }
}
