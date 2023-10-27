use std::sync::atomic::Ordering;

use anyhow::Result;
use log::info;
use sesh_proto::{sesh_detach_request as req, SeshDetachResponse};

use crate::Seshd;

use super::CommandResponse;

impl Seshd {
    /// RPC handler for detaching a session
    pub async fn exec_detach(&self, session: Option<req::Session>) -> Result<CommandResponse> {
        if let Some(session) = session {
            let name = match session {
                sesh_proto::sesh_detach_request::Session::Name(name) => Some(name),
                sesh_proto::sesh_detach_request::Session::Id(id) => {
                    self.sessions.iter().find_map(|e| {
                        let session = e.value();
                        if session.id == id as usize {
                            Some(session.name.clone())
                        } else {
                            None
                        }
                    })
                }
            };

            if let Some(name) = name {
                if let Some(session) = self.sessions.get(&name) {
                    info!(target: &session.log_group(), "Detaching");
                    session.detach().await?;
                    session
                        .info
                        .attach_time
                        .store(chrono::Utc::now().timestamp_millis(), Ordering::Relaxed);
                    info!(target: &session.log_group(), "Detached");
                }
            }
        }
        Ok(CommandResponse::DetachSession(SeshDetachResponse {
            success: true,
        }))
    }
}
