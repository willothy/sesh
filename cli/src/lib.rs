use std::{fmt::Display, str::FromStr};

use clap::{Args, Subcommand};

#[derive(Debug, clap::Parser)]
#[clap(
    name = "sesh",
    version = "0.1.0",
    author = "Will Hopkins <willothyh@gmail.com>"
)]
#[group(required = false, multiple = true)]
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
    #[command(alias = "s")]
    /// Start a new session, optionally specifying a name [alias: s]
    Start {
        #[arg(short, long)]
        name: Option<String>,
        program: Option<String>,
        args: Vec<String>,
        #[arg(short, long)]
        detached: bool,
    },
    #[command(alias = "a")]
    /// Attach to a session [alias: a]
    Attach {
        /// Id or name of session
        session: SessionSelector,
    },
    /// Fuzzy select a session to attach to [alias: f]
    #[command(alias = "f")]
    Select,
    /// Detach the current session or the specified session [alias: d]
    #[command(alias = "d")]
    Detach {
        /// Id or name of session
        session: Option<SessionSelector>,
    },
    #[command(alias = "k")]
    /// Kill a session [alias: k]
    Kill {
        /// Id or name of session
        session: SessionSelector,
    },
    /// List sessions [alias: ls]
    #[command(alias = "ls")]
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
