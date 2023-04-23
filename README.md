# sesh

Terminal sessions, written in Rust.

Built on gRPC and unix sockets.

## Demo

https://user-images.githubusercontent.com/38540736/233812701-8042efe8-4fc0-4787-a966-18e0108e7987.mp4


## Usage:

```
sesh [COMMAND]

Commands:
  start     Start a new session, optionally specifying a name [alias: s]
  attach    Attach to a session [alias: a]
  kill      Kill a session [alias: k]
  list      List sessions [alias: ls]
  shutdown  Shutdown the server (kill all sessions)
  help      Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

### Subcommands

#### Start

Start a new session, optionally specifying a name [alias: s]

```
Usage: sesh start [OPTIONS] [PROGRAM] [ARGS]...

Arguments:
  [PROGRAM]
  [ARGS]...

Options:
  -n, --name <NAME>
  -d, --detached
  -h, --help         Print help
```

#### Attach

Attach to a session [alias: a]

```
Usage: sesh attach <SESSION>

Arguments:
  <SESSION>  Id or name of session

Options:
  -h, --help  Print help
```

#### Kill

Kill a session [alias: k]

```
Usage: sesh kill <SESSION>

Arguments:
  <SESSION>  Id or name of session

Options:
  -h, --help  Print help
```

#### List

List sessions [alias: ls]

```
Usage: sesh list

Options:
  -h, --help  Print help
```

#### Shutdown

Shutdown the server (kill all sessions)

```
Usage: sesh shutdown

Options:
  -h, --help  Print help
```
