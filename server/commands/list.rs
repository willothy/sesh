use std::sync::atomic::Ordering;

use log::info;
use sesh_proto::SeshListResponse;

use crate::Seshd;

use super::CommandResponse;
use anyhow::Result;

impl Seshd {
    pub async fn exec_list(&self) -> Result<CommandResponse> {
        info!(target: "exec", "Listing sessions");
        let sessions = self.sessions.lock().await;
        let sessions = sessions
            .iter()
            .map(|(name, session)| sesh_proto::SeshInfo {
                id: session.id as u64,
                name: name.clone(),
                program: session.program.clone(),
                connected: session.connected.load(Ordering::Relaxed),
            })
            .collect::<Vec<_>>();
        if sessions.is_empty() && crate::EXIT_ON_EMPTY {
            self.exit_signal.clone().send(())?;
        }
        Ok(CommandResponse::ListSessions(SeshListResponse { sessions }))
    }
}
