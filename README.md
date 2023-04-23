# sesh

Terminal sessions, written in Rust.

Built on gRPC and unix sockets.

> **Warning**       
> This is a work in progress, and may contain some bugs.        

## Demo

<details>
<summary><h3>Local Sessions</h3></summary>

https://user-images.githubusercontent.com/38540736/233859560-83852798-896b-4913-b990-1e33a0ae726a.mp4

</details>

<details>
<summary><h3>Remote Sessions (SSH)</h3></summary>

https://user-images.githubusercontent.com/38540736/233859593-26629392-e97b-4f26-8c2a-6ea5024ed79f.mp4

</details>

## Installation

### From source

Release:

`cargo install --locked term-sesh`

Git:

`cargo install --git https://github.com/willothy/sesh`

## Usage:

See the `help` subcommand or [MANUAL.md](https://github.com/willothy/sesh/blob/main/MANUAL.md) for more info.


## Integration:

<details>
<summary><a href="https://starship.rs/">Starship</a></summary>

```toml
[custom.sesh]
command = "echo $SESH_NAME"
when = ''' test "$SESH_NAME" != "" '''
format = '\(sesh [$output]($style)\)'
```

</details>
