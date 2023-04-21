use sesh_proto::*;

#[derive(Debug)]
pub enum Command {
    StartSession(SeshStartRequest),
    KillSession(SeshKillRequest),
    ListSessions,
    ShutdownServer,
}

pub enum CommandResponse {
    StartSession(SeshStartResponse),
    KillSession(SeshKillResponse),
    ListSessions(SeshListResponse),
    ShutdownServer(ShutdownServerResponse),
}
