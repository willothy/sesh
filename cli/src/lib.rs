use std::{fmt::Display, str::FromStr};

use clap::{Args, Subcommand};

#[derive(Debug, clap::Parser)]
#[clap(
    name = "sesh",
    version = "0.1.8",
    author = "Will Hopkins <willothyh@gmail.com>"
)]
#[group(required = false, multiple = true)]
/// A terminal session manager for unix systems. Run persistent, named tasks that you can
/// detach from and attach to at any time - both on your local machine, and over SSH.
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
    #[command(flatten)]
    pub args: CliArgs,
}

#[derive(Debug, Clone, Args)]
pub struct CliArgs {
    pub program: Option<String>,
    pub args: Vec<String>,
    #[arg(short, long)]
    pub name: Option<String>,
    #[arg(short, long)]
    pub detached: bool,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(alias = "r", verbatim_doc_comment)]
    /// Resume the last used session [alias: r]
    ///
    /// Specify --create / -c to create a new session if one does not exist
    Resume {
        /// Create a new session if one does not exist
        #[arg(short, long)]
        create: bool,
    },
    /// Start a new session, optionally specifying a name [alias: s]
    ///
    /// If no program is specified, the default shell will be used.
    /// If no name is specified, the name will be [program name]-[n-1] where n is the number of sessions
    /// with that program name.
    /// If --detached / -d is present, the session will not be attached to the client on creation
    /// and will run in the background.
    #[command(alias = "s", verbatim_doc_comment)]
    Start {
        #[arg(short, long)]
        name: Option<String>,
        program: Option<String>,
        args: Vec<String>,
        #[arg(short, long)]
        detached: bool,
    },
    #[command(alias = "a", verbatim_doc_comment)]
    /// Attach to a session [alias: a]
    ///
    /// Select a session by index or name.
    /// If --create / -c is present, a new session will be created if one does not exist.
    /// If the session was selected by name and the session was not present, the new session
    /// created by --create will have the specified name.
    Attach {
        /// Id or name of session
        session: SessionSelector,
        /// Create a new session if one does not exist
        #[arg(short, long)]
        create: bool,
    },
    /// Fuzzy select a session to attach to [alias: f]
    ///
    /// Opens a fuzzy selection window provided by the dialoguer crate.
    /// Type to fuzzy find files, or use the Up/Down arrows to navigate.
    /// Press Enter to confirm your selection, or Escape to cancel.
    #[command(alias = "f", verbatim_doc_comment)]
    Select,
    /// Detach from a session [alias: d]
    ///
    /// If no session is specified, detaches from the current session (if it exists).
    /// Otherwise, detaches the specified session from its owning client.
    #[command(alias = "d", verbatim_doc_comment)]
    Detach {
        /// Id or name of session
        session: Option<SessionSelector>,
    },
    #[command(alias = "k", verbatim_doc_comment)]
    /// Kill a session [alias: k]
    ///
    /// Kills a session and the process it owns.
    /// Select a session by name or index.
    Kill {
        /// Id or name of session
        session: SessionSelector,
    },
    /// List sessions [alias: ls]
    ///
    /// Prints a compact list of session names and indexes.
    /// With the --info / -i option, prints a nicely formatted table with info about each session.
    #[command(alias = "ls", verbatim_doc_comment)]
    List {
        /// Print detailed info about sessions
        #[arg(short, long)]
        info: bool,
    },
    /// Shutdown the server (kill all sessions)
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum SessionSelector {
    Id(usize),
    Name(String),
}

impl SessionSelector {
    pub fn name(self) -> Option<String> {
        match self {
            SessionSelector::Id(_) => None,
            SessionSelector::Name(name) => Some(name),
        }
    }
}

impl Display for SessionSelector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionSelector::Id(id) => write!(f, "{}", id),
            SessionSelector::Name(name) => write!(f, "{}", name),
        }
    }
}

impl FromStr for SessionSelector {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(id) = s.parse::<usize>() {
            Ok(SessionSelector::Id(id))
        } else {
            Ok(SessionSelector::Name(s.to_owned()))
        }
    }
}
