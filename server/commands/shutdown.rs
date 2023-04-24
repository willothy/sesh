use crate::Seshd;
use anyhow::Result;
use sesh_proto::ShutdownServerResponse;

use super::CommandResponse;

impl Seshd {
    pub async fn exec_shutdown(&self) -> Result<CommandResponse> {
        self.exit_signal.send(())?;
        Ok(CommandResponse::ShutdownServer(ShutdownServerResponse {
            success: true,
        }))
    }
}
