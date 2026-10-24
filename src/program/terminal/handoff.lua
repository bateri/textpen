-- The pane given away and taken back. When the person asks for a shell the program closes the view,
-- runs the shell in the terminal, and the view is shown again where it was when the shell ends;
-- what is not saved lives in the program, so it crosses that gap. In a terminal that shows no views
-- the pane is given to the person's own `$VISUAL` or `$EDITOR` instead.
--
-- Plain values in and out: this file knows nothing of the page. How the shell ended, and a
-- hand-off the terminal could not make, are left in the session (`returned`, `handoff_failed`) as
-- plain values for api/ to tell the view.

local pencere = require("pencere")
local env, io, time = pencere.env, pencere.io, pencere.time

local paths = require("project.paths")

local M = {}

-- The shell integrations a person's rc file loads whatever the terminal, turned off in the shell
-- textpen starts: their prompt marks carry no identity of the terminal's own integration, and a
-- short-lived shell inside textpen's command has no prompt the terminal needs to know of — its marks
-- would only move the terminal's idea of the pane (bateri's phase and its dock) back and forth.
-- iTerm2's script is skipped when its own guard is set; WezTerm's has a switch for it.
local SHELL_ENV = {
	ITERM_SHELL_INTEGRATION_INSTALLED = "Yes",
	WEZTERM_SHELL_SKIP_ALL = "1",
	TEXTPEN_SHELL = "1",
}

local bar = "\27[7m textpen \27[0m "

local Handoff = {}
Handoff.__index = Handoff

-- The hand-offs of `session`, whose project is at `root`. `desk` is what the session offers the
-- view: `desk.shell()`. `handed_off` is set once the view has been closed for a shell: from then on
-- a view that does not come back is not replaced with another program.
function M.new(session, root)
	local self = setmetatable({ session = session, root = root }, Handoff)
	self.handed_off = false
	self.shell_asked = false
	self.desk = {
		shell = function()
			if self.shell_asked then
				return "busy"
			end
			self.shell_asked = true
			-- The answer to the view's call goes out first, so that it can show what comes.
			time.after(0.12, function()
				self:run_shell()
			end)
			return "scheduled"
		end,
	}
	return self
end

-- The pane, handed to a shell in the project's directory: the view closes, the shell has the
-- terminal as if it had been started from the prompt, and when it ends the view is shown again,
-- where it was.
function Handoff:run_shell()
	local session = self.session
	local shell = env.get("SHELL")
	if shell == nil or shell == "" then
		shell = "/bin/sh"
	end
	local label = session.project.root
	local banner = bar .. "shell in \27[1m" .. label .. "\27[0m\27[0m  \27[2mexit or ⌃D goes back to the editor\27[0m"
	-- The banner is the shell's first word, once the view is gone and the terminal is the shell's.
	local started = time.monotonic()
	session.buffers:flush_drafts()
	local ok, result = pcall(pencere.handoff, {
		argv = { "/bin/sh", "-c", 'printf "%s\\n" "$1"; exec "$2"', "sh", banner, shell },
		cwd = self.root,
		env = SHELL_ENV,
	})
	self.handed_off = true
	self.shell_asked = false
	if not ok then
		local words = tostring(result)
		session.handoff_failed = string.find(words, "cannot close the view", 1, true) and "unsupported"
			or "closing"
		return
	end
	local returned = {
		status = result.code,
		ms = math.floor((time.monotonic() - started) * 1000),
	}
	if result.code == 126 or result.code == 127 then
		returned.error = "could not start " .. shell
	end
	session.returned = returned
end

-- `$VISUAL`, `$EDITOR` or vi, run as git runs it (`sh -c '$EDITOR "$@"'`), so that an editor given
-- with options works. textpen itself is passed over, so that `EDITOR=textpen` does not start textpen
-- again.
function Handoff:run_own_editor(targets)
	local chosen = nil
	for _, name in ipairs({ "VISUAL", "EDITOR" }) do
		local editor = env.get(name)
		if editor ~= nil and editor ~= "" then
			local trimmed = string.gsub(editor, "^%s+", "")
			trimmed = string.gsub(trimmed, "%s+$", "")
			local is_textpen = string.match(trimmed, "^textpen$") or string.match(trimmed, "^textpen%s")
				or string.match(trimmed, "/textpen$") or string.match(trimmed, "/textpen%s")
			if not is_textpen then
				chosen = editor
				break
			end
		end
	end
	local editor = chosen or "vi"
	local argv = { "sh", "-c", editor .. ' "$@"', editor }
	for _, target in ipairs(targets) do
		argv[#argv + 1] = target
	end
	local ok, result = pcall(pencere.handoff, { argv = argv, cwd = self.root })
	if not ok then
		return 127
	end
	return result.code or 1
end

-- The person's own editor stands in for a terminal that shows no views, at the start only. Gives
-- the status the program ends with, or nil when it is not to change.
function Handoff:fallback(why, files)
	if self.handed_off then
		return nil
	end
	if why.reason == "no-tty" then
		-- No terminal at all (a script, a pipe): there is no pane for any editor.
		io.ewrite("textpen: needs a terminal to edit in\n")
		return 1
	end
	local targets = {}
	if #files > 0 then
		for _, file in ipairs(files) do
			local relative = paths.relative(self.root, file)
			targets[#targets + 1] = relative ~= "" and relative or file
		end
	else
		targets[1] = "."
	end
	return self:run_own_editor(targets)
end

-- The editor did not come back after the shell: the terminal did not answer, or would not show it.
-- What is open is still the program's, so the person chooses: once more, or quit (keeping what is
-- not saved as drafts). True to try again.
function Handoff:try_again(reason)
	local why = reason == "no-host" and "the terminal did not answer" or ("the terminal said " .. tostring(reason))
	io.write(
		"\27[7m textpen \27[0m the editor did not come back: " .. why .. ".\n"
			.. "\27[2m         What is not saved is kept. ⏎ tries again, q and ⏎ quits.\27[0m "
	)
	local answer = io.read_line()
	if answer == nil then
		return false
	end
	answer = string.lower(string.gsub(answer, "^%s*(.-)%s*$", "%1"))
	return answer ~= "q"
end

return M
