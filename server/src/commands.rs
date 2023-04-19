use sesh_proto::*;

#[derive(Debug)]
pub enum Command {
    StartSession(SeshStartRequest),
    KillSession(SeshKillRequest),
    ListSessions(SeshListRequest),
}

pub enum CommandResponse {
    StartSession(SeshStartResponse),
    KillSession(SeshKillResponse),
    ListSessions(SeshListResponse),
}
