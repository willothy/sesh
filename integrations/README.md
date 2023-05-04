# Integrations

## Terminal emulators

### Wezterm

#### Installation with GitHub CLI:

Make sure you substitute the output file with your wezterm config location, if it's not in `~/.config/wezterm/`

```sh
curl $(gh api "https://api.github.com/repos/willothy/sesh/contents/integrations/wezterm/sesh.lua" --jq .download_url) -o ~/.config/wezterm/sesh.lua
```

#### Installation without GitHub CLI:

Copy the raw contents of the `integrations/wezterm/sesh.lua` into a file called `sesh.lua` in your wezterm config folder.

#### In your `wezterm.lua`:

```lua
local sesh = require("sesh")

wezterm.on("augment-command-palette", function(_window, _pane)
    return {
        -- Create a session and interactively name it
        sesh.create,
        -- Use wezterm's InputSelector to attach to available sessions
        sesh.attach,
        --
        -- ... the rest of your augment-command-palette config
    }
end)
```

## Shells / prompts

Shells can use the "$SESH_NAME" environment variable to display the name of the current session.

### Starship

<img src="https://user-images.githubusercontent.com/38540736/234249256-cbb399aa-683b-48af-85a3-70206347a4f7.png" />

Add this snippet to your `starship.toml`, or create your own.

```toml
[custom.sesh]
command = "echo $SESH_NAME"
when = ''' test "$SESH_NAME" != "" '''
format = '\(sesh [$output]($style)\)'
```
