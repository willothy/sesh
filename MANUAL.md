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

**Usage:** `sesh [OPTIONS] [PROGRAM] [ARGS]... [COMMAND]`

###### **Subcommands:**

* `resume` — Resume the last used session [alias: r]
* `start` — Start a new session, optionally specifying a name [alias: s]
* `attach` — Attach to a session [alias: a]
* `select` — Fuzzy select a session to attach to [alias: f]
* `detach` — Detach the current session or the specified session [alias: d]
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

**Usage:** `sesh resume`



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



## `sesh select`

Fuzzy select a session to attach to [alias: f]

**Usage:** `sesh select`



## `sesh detach`

Detach the current session or the specified session [alias: d]

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
