use std::{os::fd::AsRawFd, sync::atomic::Ordering};

use anyhow::Result;
use log::info;
use sesh_proto::{sesh_attach_request, SeshAttachResponse, WinSize};
use sesh_shared::term::Size;

use crate::{Seshd, Session};

use super::CommandResponse;

impl Seshd {
    pub async fn exec_attach(
        &self,
        session: Option<sesh_attach_request::Session>,
        size: Option<WinSize>,
    ) -> Result<CommandResponse> {
        if let Some(session) = session {
            let sessions = self.sessions.lock().await;
            let session = match &session {
                sesh_proto::sesh_attach_request::Session::Name(name) => sessions.get(name),
                sesh_proto::sesh_attach_request::Session::Id(id) => sessions
                    .iter()
                    .find(|(_, s)| s.id == *id as usize)
                    .map(|(_, s)| s),
            }
            .ok_or_else(|| anyhow::anyhow!("Session {} not found", session))?;
            if session.connected.load(Ordering::Relaxed) {
                return Err(anyhow::anyhow!("Session already connected"));
            }
            info!(target: &session.log_group(), "Attaching");
            let size = if let Some(size) = size {
                Size {
                    rows: size.rows as u16,
                    cols: size.cols as u16,
                    ..Size::term_size()?
                }
            } else {
                Size::term_size()?
            };
            session.pty.resize(&Size {
                cols: (size.cols as u16).checked_sub(2).unwrap_or(2),
                rows: (size.rows as u16).checked_sub(2).unwrap_or(2),
            })?;
            tokio::task::spawn({
                let sock_path = session.sock_path.clone();
                let socket = session.listener.clone();
                let file = session.pty.file().as_raw_fd();
                let file = unsafe { libc::fcntl(file, libc::F_DUPFD, file) };
                let connected = session.connected.clone();
                async move {
                    Session::start(sock_path, socket, file, connected, size).await?;
                    Result::<_, anyhow::Error>::Ok(())
                }
            });

            Ok(CommandResponse::AttachSession(SeshAttachResponse {
                socket: session.sock_path.to_string_lossy().to_string(),
                pid: session.pid(),
                name: session.name.clone(),
            }))
        } else {
            anyhow::bail!("No session specified");
        }
    }
}
