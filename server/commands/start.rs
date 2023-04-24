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
    ) -> Result<CommandResponse> {
        let mut sessions = self.sessions.lock().await;
        let session_id = sessions.len();

        let name = PathBuf::from(&name)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or(name.replace("/", "_"));

        let mut session_name = name.clone();
        let mut i = 0;
        while sessions.contains_key(&session_name) {
            session_name = format!("{}-{}", name, i);
            i += 1;
        }

        let socket_path = self.runtime_dir.join(format!("{}.sock", session_name));

        let pty = Pty::new(&program)
            .args(args)
            .current_dir(pwd)
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
            session_id,
            session_name.clone(),
            program,
            pty,
            PathBuf::from(&socket_path),
        )?;
        info!(target: &session.log_group(), "Starting on {}", session.info.sock_path().display());
        tokio::task::spawn({
            let sock_path = session.info.sock_path().clone();
            let socket = session.listener.clone();
            // let file = session.pty.file().try_clone().await?;
            let file = session.pty.file().as_raw_fd();
            // Duplicate FD
            // I do not know why this makes the socket connection not die, but it does
            let file = unsafe { libc::fcntl(file, libc::F_DUPFD, file) };
            let connected = session.info.connected();
            let attach_time = session.info.attach_time.clone();
            async move {
                Session::start(sock_path, socket, file, connected, size, attach_time).await?;
                Result::<_, anyhow::Error>::Ok(())
            }
        });

        sessions.insert(session.name.clone(), session);
        Ok(CommandResponse::StartSession(SeshStartResponse {
            socket: socket_path.to_string_lossy().to_string(),
            name: session_name,
            pid,
        }))
    }
}
