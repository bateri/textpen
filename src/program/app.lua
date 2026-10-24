-- textpen. In a terminal that shows pencere views, an editor opens in the pane; in one that does
-- not, the person's own `$EDITOR` does.
--
-- The view hands the pane over when the person asks for a shell: the program closes the view, runs
-- the shell in the terminal, and shows the view again where it was when the shell ends. What is not
-- saved lives in the program, so it crosses that gap, and a ⌃C or a dropped connection leaves it as
-- a draft for the next run.
--
-- This file is the entry and nothing more: it reads the command line, makes the state, wires the
-- calls and the topic of api/ to `serve`, and ends with the status a shell gives the program. The
-- view is src/view (`serve` is told none: it shows what the view directory has).
--
--   api/       everything the page talks to the program about; it knows the page
--   project/   the project on disk: the tree, git, the search; it knows nothing of the page
--   state/     what the program holds and remembers: open files, drafts, settings; same
--   terminal/  the command line, and the pane given to a shell or to the person's own editor; same

local pencere = require("pencere")

local buffers_api = require("api.buffers")
local changes_api = require("api.changes")
local command_line = require("terminal.command_line")
local files_api = require("api.files")
local handoff = require("terminal.handoff")
local search_api = require("api.search")
local session_api = require("api.session")
local session_module = require("state.session")

local line = command_line.read(pencere.args)

local session = session_module.new(line.root)
if #line.files > 0 then
	session:start_with(line.files, line.focus)
end

local shell = handoff.new(session, line.root)
session.desk = shell.desk

-- What the view may call: each area of api/ gives its calls.
local calls = {}
for _, area in ipairs({ files_api, buffers_api, search_api, session_api }) do
	for name, handler in pairs(area.calls(session)) do
		calls[name] = handler
	end
end
local topics = { changes = changes_api.topic(session) }

local exit_code = 0

local function serve()
	return pencere.serve({
		program = "textpen",
		-- What a terminal writes above the view, and the icon it draws beside it.
		title = "Editor",
		icons = {
			{ file = "assets/icon-32-dark.png", theme = "dark" },
			{ file = "assets/icon-64-dark.png", theme = "dark" },
			{ file = "assets/icon-32-light.png", theme = "light" },
			{ file = "assets/icon-64-light.png", theme = "light" },
		},
		calls = calls,
		topics = topics,
		-- The person's own editor stands in for a terminal that shows no views, at the start only.
		fallback = function(why)
			local status = shell:fallback(why, line.files)
			if status ~= nil then
				exit_code = status
			end
		end,
	})
end

while true do
	local result = serve()
	if not result.shown and shell.handed_off and result.reason ~= "interrupted" and shell:try_again(result.reason) then
		continue
	end
	if result.shown and result.ended == "signal" then
		exit_code = (result.signal == nil or result.signal == 2) and 130 or 128 + result.signal
	elseif result.shown and result.ended == "error" then
		exit_code = 1
	elseif not result.shown and result.reason == "interrupted" then
		exit_code = 130
	end
	break
end

session.desk = nil
session:ending()
pencere.exit(exit_code)
