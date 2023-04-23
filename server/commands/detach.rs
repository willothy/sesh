use anyhow::Result;
use log::info;
use sesh_proto::{sesh_detach_request as req, SeshDetachResponse};

use crate::Seshd;

use super::CommandResponse;

impl Seshd {
    pub async fn exec_detach(&self, session: Option<req::Session>) -> Result<CommandResponse> {
        if let Some(session) = session {
            let sessions = self.sessions.lock().await;
            let name = match session {
                sesh_proto::sesh_detach_request::Session::Name(name) => Some(name),
                sesh_proto::sesh_detach_request::Session::Id(id) => {
                    let name = sessions
                        .iter()
                        .find(|(_, s)| s.id == id as usize)
                        .map(|(_, s)| s.name.clone());
                    name
                }
            };

            if let Some(name) = name {
                if let Some(session) = sessions.get(&name) {
                    info!(target: &session.log_group(), "Detaching");
                    session.detach().await?;
                    info!(target: &session.log_group(), "Detached");
                }
            }
        }
        Ok(CommandResponse::DetachSession(SeshDetachResponse {
            success: true,
        }))
    }
}
