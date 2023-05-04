local wezterm = require("wezterm")

local sesh = {}

sesh.create = {
	brief = "Sesh: Create session",
	icon = "mdi_plus",
	action = wezterm.action.PromptInputLine({
		description = "Enter new session name",
		action = wezterm.action_callback(function(window, pane, line)
			window:perform_action(
				wezterm.action.SpawnCommandInNewTab({
					args = { "sesh", "start", "--name", line },
				}),
				pane
			)
		end),
	}),
}

sesh.attach = {
	brief = "Sesh: Attach to session",
	icon = "seti_shell",
	action = wezterm.action_callback(function(window, pane, _line)
		local success, stdout, _stderr = wezterm.run_child_process({ "sesh", "ls", "--json" })
		if not success then
			return
		end
		local sessions = wezterm.json_parse(stdout)
		local choices = {}

		for _, session in ipairs(sessions) do
			if session.connected == false then
				table.insert(choices, {
					label = wezterm.format({
						{ Attribute = { Intensity = "Bold" } },
						{ Text = session.name },
						"ResetAttributes",
						{ Text = " - " },
						{ Text = session.program },
					}),
					id = session.name,
				})
			end
		end
		local tab = window.active_tab
		window:perform_action(
			wezterm.action.InputSelector({
				title = "Select session",
				action = wezterm.action_callback(function(w, p, name, label)
					if not name and not label then
						-- cancelled
						tab:activate()
					end
					w:perform_action(
						wezterm.action.SpawnCommandInNewTab({
							args = { "sesh", "attach", name },
						}),
						p
					)
				end),
				choices = choices,
			}),
			pane
		)
	end),
}

return sesh
