# Command-Line Help for `sesh`

This document contains the help content for the `sesh` command-line program.

**Command Overview:**

* [`sesh`↴](#sesh)
* [`sesh resume`↴](#sesh-resume)
* [`sesh start`↴](#sesh-start)
* [`sesh attach`↴](#sesh-attach)
* [`sesh select`↴](#sesh-select)
* [`sesh detach`↴](#sesh-detach)
* [`sesh kill`↴](#sesh-kill)
* [`sesh list`↴](#sesh-list)
* [`sesh shutdown`↴](#sesh-shutdown)

## `sesh`

A terminal session manager for unix systems. Run persistent, named tasks that you can detach from and attach to at any time - both on your local machine, and over SSH

**Usage:** `sesh [OPTIONS] [PROGRAM] [ARGS]... [COMMAND]`

###### **Subcommands:**

* `resume` — Resume the last used session [alias: r]
* `start` — Start a new session, optionally specifying a name [alias: s]
* `attach` — Attach to a session [alias: a]
* `select` — Fuzzy select a session to attach to [alias: f]
* `detach` — Detach from a session [alias: d]
* `kill` — Kill a session [alias: k]
* `list` — List sessions [alias: ls]
* `shutdown` — Shutdown the server (kill all sessions)

###### **Arguments:**

* `<PROGRAM>`
* `<ARGS>`

###### **Options:**

* `-n`, `--name <NAME>`
* `-d`, `--detached`



## `sesh resume`

Resume the last used session [alias: r]

Specify --create / -c to create a new session if one does not exist

**Usage:** `sesh resume [OPTIONS]`

###### **Options:**

* `-c`, `--create` — Create a new session if one does not exist



## `sesh start`

Start a new session, optionally specifying a name [alias: s]

If no program is specified, the default shell will be used.
If no name is specified, the name will be [program name]-[n-1] where n is the number of sessions
with that program name.
If --detached / -d is present, the session will not be attached to the client on creation
and will run in the background.

**Usage:** `sesh start [OPTIONS] [PROGRAM] [ARGS]...`

###### **Arguments:**

* `<PROGRAM>`
* `<ARGS>`

###### **Options:**

* `-n`, `--name <NAME>`
* `-d`, `--detached`



## `sesh attach`

Attach to a session [alias: a]

Select a session by index or name.
If --create / -c is present, a new session will be created if one does not exist.
If the session was selected by name and the session was not present, the new session
created by --create will have the specified name.

**Usage:** `sesh attach [OPTIONS] <SESSION>`

###### **Arguments:**

* `<SESSION>` — Id or name of session

###### **Options:**

* `-c`, `--create` — Create a new session if one does not exist



## `sesh select`

Fuzzy select a session to attach to [alias: f]

Opens a fuzzy selection window provided by the dialoguer crate.
Type to fuzzy find files, or use the Up/Down arrows to navigate.
Press Enter to confirm your selection, or Escape to cancel.

**Usage:** `sesh select`



## `sesh detach`

Detach from a session [alias: d]

If no session is specified, detaches from the current session (if it exists).
Otherwise, detaches the specified session from its owning client.

**Usage:** `sesh detach [SESSION]`

###### **Arguments:**

* `<SESSION>` — Id or name of session



## `sesh kill`

Kill a session [alias: k]

Kills a session and the process it owns.
Select a session by name or index.

**Usage:** `sesh kill <SESSION>`

###### **Arguments:**

* `<SESSION>` — Id or name of session



## `sesh list`

List sessions [alias: ls]

Prints a compact list of session names and indexes.
With the --info / -i option, prints a nicely formatted table with info about each session.

**Usage:** `sesh list [OPTIONS]`

###### **Options:**

* `-i`, `--info` — Print detailed info about sessions



## `sesh shutdown`

Shutdown the server (kill all sessions)

**Usage:** `sesh shutdown`



<hr/>

<small><i>
    This document was generated automatically by
    <a href="https://crates.io/crates/clap-markdown"><code>clap-markdown</code></a>.
</i></small>
