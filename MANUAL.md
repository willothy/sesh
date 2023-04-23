# Command-Line Help for `sesh`

This document contains the help content for the `sesh` command-line program.

**Command Overview:**

* [`sesh`↴](#sesh)
* [`sesh start`↴](#sesh-start)
* [`sesh attach`↴](#sesh-attach)
* [`sesh detach`↴](#sesh-detach)
* [`sesh kill`↴](#sesh-kill)
* [`sesh list`↴](#sesh-list)
* [`sesh shutdown`↴](#sesh-shutdown)

## `sesh`

**Usage:** `sesh [COMMAND]`

###### **Subcommands:**

* `start` — Start a new session, optionally specifying a name [alias: s]
* `attach` — Attach to a session [alias: a]
* `detach` — Detach a session remotely [alias: d] Detaches the current session, or the one specified
* `kill` — Kill a session [alias: k]
* `list` — List sessions [alias: ls]
* `shutdown` — Shutdown the server (kill all sessions)



## `sesh start`

Start a new session, optionally specifying a name [alias: s]

**Usage:** `sesh start [OPTIONS] [PROGRAM] [ARGS]...`

###### **Arguments:**

* `<PROGRAM>`
* `<ARGS>`

###### **Options:**

* `-n`, `--name <NAME>`
* `-d`, `--detached`



## `sesh attach`

Attach to a session [alias: a]

**Usage:** `sesh attach <SESSION>`

###### **Arguments:**

* `<SESSION>` — Id or name of session



## `sesh detach`

Detach a session remotely [alias: d] Detaches the current session, or the one specified

**Usage:** `sesh detach [SESSION]`

###### **Arguments:**

* `<SESSION>` — Id or name of session



## `sesh kill`

Kill a session [alias: k]

**Usage:** `sesh kill <SESSION>`

###### **Arguments:**

* `<SESSION>` — Id or name of session



## `sesh list`

List sessions [alias: ls]

**Usage:** `sesh list`



## `sesh shutdown`

Shutdown the server (kill all sessions)

**Usage:** `sesh shutdown`



<hr/>

<small><i>
    This document was generated automatically by
    <a href="https://crates.io/crates/clap-markdown"><code>clap-markdown</code></a>.
</i></small>