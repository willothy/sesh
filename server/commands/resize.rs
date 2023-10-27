use log::info;
use sesh_proto::{SeshResizeResponse, WinSize};
use sesh_shared::term::Size;

use crate::Seshd;
use sesh_proto::sesh_resize_request as req;

use super::CommandResponse;
use anyhow::Result;

impl Seshd {
    pub async fn exec_resize(
        &self,
        session: Option<req::Session>,
        size: Option<WinSize>,
    ) -> Result<CommandResponse> {
        let Some(size) = size else {
            return Err(anyhow::anyhow!("Invalid size"));
        };
        let sessions = self.sessions.lock().await;
        let Some(session) = session else {
            return Err(anyhow::anyhow!("Session not found"));
        };
        let Some(name) = (match session {
            req::Session::Name(name) => Some(name),
            req::Session::Id(id) => {
                let name = sessions
                    .iter()
                    .find(|(_, s)| s.id == id as usize)
                    .map(|(_, s)| s.name.clone());
                name
            }
        }) else {
            return Err(anyhow::anyhow!("Session not found"));
        };
        let session = sessions
            .get(&name)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", name))?;
        info!(target: &session.log_group(), "Resizing");

        session.pty.resize(&Size {
            cols: size.cols as u16,
            rows: size.rows as u16,
        })?;
        Ok(CommandResponse::ResizeSession(SeshResizeResponse {}))
    }
}
