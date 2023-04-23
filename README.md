# sesh

Terminal sessions, written in Rust.

Built on gRPC and unix sockets.

> **Warning**       
> This is a work in progress, and may contain some bugs.        

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

See [MANUAL.md](https://github.com/willothy/sesh/blob/main/MANUAL.md) for more info.
