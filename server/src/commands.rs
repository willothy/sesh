use sesh_proto::*;

#[derive(Debug)]
pub enum Command {
    StartSession(SeshStartRequest),
    KillSession(SeshKillRequest),
    ListSessions,
    ShutdownServer,
    AttachSession(SeshAttachRequest),
    DetachSession(SeshDetachRequest),
    ResizeSession(SeshResizeRequest),
}

pub enum CommandResponse {
    StartSession(SeshStartResponse),
    KillSession(SeshKillResponse),
    ListSessions(SeshListResponse),
    ShutdownServer(ShutdownServerResponse),
    AttachSession(SeshAttachResponse),
    DetachSession(SeshDetachResponse),
    ResizeSession(SeshResizeResponse),
}
