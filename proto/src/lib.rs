use std::fmt::Display;

tonic::include_proto!("sesh");

impl Display for sesh_attach_request::Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            sesh_attach_request::Session::Id(id) => write!(f, "{}", id),
            sesh_attach_request::Session::Name(name) => write!(f, "{}", name),
        }
    }
}

impl Display for sesh_detach_request::Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            sesh_detach_request::Session::Id(id) => write!(f, "{}", id),
            sesh_detach_request::Session::Name(name) => write!(f, "{}", name),
        }
    }
}

impl Display for sesh_kill_request::Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            sesh_kill_request::Session::Id(id) => write!(f, "{}", id),
            sesh_kill_request::Session::Name(name) => write!(f, "{}", name),
        }
    }
}
