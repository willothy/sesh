use crate::Seshd;

use anyhow::Result;
use log::info;
use sesh_proto::{sesh_kill_request as req, SeshKillResponse};

use super::CommandResponse;

impl Seshd {
    pub async fn exec_kill(&self, session: Option<req::Session>) -> Result<CommandResponse> {
        if let Some(session) = session {
            let mut sessions = self.sessions.lock().await;
            let name = match session {
                req::Session::Name(name) => Some(name),
                req::Session::Id(id) => {
                    let name = sessions
                        .iter()
                        .find(|(_, s)| s.id == id as usize)
                        .map(|(_, s)| s.name.clone());
                    name
                }
            };

            let success = if let Some(name) = name {
                if let Some(session) = sessions.remove(&name) {
                    info!(target: &session.log_group(), "Killing subprocess");
                    true
                } else {
                    false
                }
            } else {
                false
            };
            if sessions.is_empty() {
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
