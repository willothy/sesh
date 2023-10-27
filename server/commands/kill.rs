use crate::Seshd;

use anyhow::Result;
use log::info;
use sesh_proto::{sesh_kill_request as req, SeshKillResponse};

use super::CommandResponse;

impl Seshd {
    pub async fn exec_kill(&self, session: Option<req::Session>) -> Result<CommandResponse> {
        if let Some(session) = session {
            let name = match session {
                req::Session::Name(name) => Some(name),
                req::Session::Id(id) => self.sessions.iter().find_map(|e| {
                    let session = e.value();
                    if session.id == id as usize {
                        Some(session.name.clone())
                    } else {
                        None
                    }
                }),
            };

            let success = if let Some(name) = name {
                if let Some((_, session)) = self.sessions.remove(&name) {
                    info!(target: &session.log_group(), "Killing subprocess");
                    true
                } else {
                    false
                }
            } else {
                false
            };
            if self.sessions.is_empty() && crate::EXIT_ON_EMPTY {
                self.exit_signal.send(())?;
            }
            Ok(CommandResponse::KillSession(SeshKillResponse { success }))
        } else {
            // TODO: Kill the *current* session and exit?
            Ok(CommandResponse::KillSession(SeshKillResponse {
                success: false,
            }))
        }
    }
}
