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
                connected: session.info.connected().load(Ordering::Relaxed),
                attach_time: session.info.attach_time.load(Ordering::Relaxed),
                start_time: session.info.start_time,
                socket: session.info.sock_path().to_string_lossy().to_string(),
                pid: session.pid(),
            })
            .collect::<Vec<_>>();
        Ok(CommandResponse::ListSessions(SeshListResponse { sessions }))
    }
}
