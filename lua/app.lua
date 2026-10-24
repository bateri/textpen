-- textpen. In a terminal that shows pencere views, an editor opens in the pane; in one that does
-- not, the person's own `$EDITOR` does.
--
-- The view hands the pane over when the person asks for a shell: the program closes the view, runs
-- the shell in the terminal, and shows the view again where it was when the shell ends. What is not
-- saved lives in the program, so it crosses that gap, and a ⌃C or a dropped connection leaves it as
-- a draft for the next run.

local pencere = require("pencere")
local fs, io, time, env = pencere.fs, pencere.io, pencere.time, pencere.env

local paths = require("lib.paths")
local program_module = require("lib.program")
local textpen_module = require("lib.textpen")
local version = require("lib.version")
local json = pencere.json

local USAGE = [=[
usage: textpen [options] [+line] [path[:line[:column]] ...]

A code editor in the terminal pane: a file tree, tabs, search, and the
terminal's own colours and font, with the keyboard or the mouse.

  path                 a file to open, or the directory to open as the project
                       (the current directory by default, with the files that
                       were open there last time)
  +line, path:line     put the cursor on that line
      --dev            (run under `pencere dev lua`, which shows the view again
                       when its files change)
  -h, --help           this text
  -v, --version        the version

In a terminal that does not show pencere views it runs $VISUAL or $EDITOR.]=]

local function out(words)
	io.write(words .. "\n")
end

local function complain(words)
	io.ewrite(words .. "\n")
end

local function exists(path)
	return fs.exists(path)
end

local function is_directory(path)
	local ok, info = pcall(fs.stat, path)
	return ok and info ~= nil and info.kind == "dir"
end

-- `src/a.ts:12:4` is the file at a line and column, unless a file has that very name.
local function split_place(arg)
	local path, line, column = string.match(arg, "^(.-):(%d+):(%d+)$")
	if path == nil then
		path, line = string.match(arg, "^(.-):(%d+)$")
	end
	if path == nil or exists(arg) then
		return { path = arg }
	end
	return { path = path, line = tonumber(line), column = column and tonumber(column) or nil }
end

local function parse(argv)
	local options = { paths = {}, focus = nil, dev = false }
	local line = nil
	local rest = false
	for _, arg in ipairs(argv) do
		if not rest and (arg == "-h" or arg == "--help") then
			out(USAGE)
			pencere.exit(0)
		elseif not rest and (arg == "-v" or arg == "--version") then
			out(version)
			pencere.exit(0)
		elseif not rest and arg == "--dev" then
			options.dev = true
		elseif not rest and arg == "--" then
			rest = true
		elseif not rest and string.match(arg, "^%+%d+$") then
			line = tonumber(string.sub(arg, 2))
		elseif not rest and string.sub(arg, 1, 1) == "-" and arg ~= "-" then
			complain("textpen: unknown option " .. arg .. "\n\n" .. USAGE)
			pencere.exit(2)
		else
			local place = split_place(arg)
			options.paths[#options.paths + 1] = place.path
			if options.focus == nil and (place.line ~= nil or line ~= nil) then
				options.focus = { path = place.path, line = place.line or line, column = place.column }
			end
		end
	end
	return options
end

-- The project: the directory given, or the current one when the files given are in it, or else the
-- directory of the first file.
local function root_of(given)
	local cwd = fs.cwd()
	local first = given[1]
	if first ~= nil and is_directory(first) then
		local files = {}
		for index = 2, #given do
			if not is_directory(given[index]) then
				files[#files + 1] = given[index]
			end
		end
		return paths.resolve(cwd, first), files
	end
	local files = {}
	for _, path in ipairs(given) do
		if not is_directory(path) then
			files[#files + 1] = paths.resolve(cwd, path)
		end
	end
	local root = cwd
	if #files > 0 then
		local relative = paths.relative(cwd, files[1])
		if string.sub(relative, 1, 2) == ".." or paths.is_absolute(relative) then
			root = paths.dirname(files[1])
		end
	end
	return root, files
end

local options = parse(pencere.args)
if options.dev then
	complain("textpen: --dev is for `pencere dev lua`, which runs the program from its files and shows the view again when they change")
	pencere.exit(2)
end

local root, files = root_of(options.paths)
local textpen = textpen_module.new(root)
if #files > 0 then
	local focus = nil
	if options.focus then
		focus = { path = paths.resolve(fs.cwd(), options.focus.path), line = options.focus.line, column = options.focus.column }
	end
	textpen:start_with(files, focus)
end
local program = program_module.create(textpen)

-- ---- the pane given to a shell and back -----------------------------------------------------

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

-- Set once the view has been closed for a shell: from then on a view that does not come back is not
-- replaced with another program.
local handed_off = false
local shell_asked = false

-- The pane, handed to a shell in the project's directory: the view closes, the shell has the
-- terminal as if it had been started from the prompt, and when it ends the view is shown again,
-- where it was.
local function run_shell()
	local shell = env.get("SHELL")
	if shell == nil or shell == "" then
		shell = "/bin/sh"
	end
	local label = textpen.project.root
	local banner = bar .. "shell in \27[1m" .. label .. "\27[0m\27[0m  \27[2mexit or ⌃D goes back to the editor\27[0m"
	-- The banner is the shell's first word, once the view is gone and the terminal is the shell's.
	local started = time.monotonic()
	textpen.buffers:flush_drafts()
	local ok, result = pcall(pencere.handoff, {
		argv = { "/bin/sh", "-c", 'printf "%s\\n" "$1"; exec "$2"', "sh", banner, shell },
		cwd = root,
		env = SHELL_ENV,
	})
	handed_off = true
	shell_asked = false
	if not ok then
		local words = tostring(result)
		textpen.handoff_failed = string.find(words, "cannot close the view", 1, true)
				and "This terminal cannot hand its pane to a command and back."
			or "The view could not be closed just now."
		return
	end
	local returned = {
		exitCode = result.code == nil and json.null or result.code,
		duration = math.floor((time.monotonic() - started) * 1000),
	}
	if result.code == 126 or result.code == 127 then
		returned.error = "could not start " .. shell
	end
	textpen.returned = returned
end

textpen.desk = {
	shell = function()
		if shell_asked then
			return "busy"
		end
		shell_asked = true
		-- The answer to the view's call goes out first, so that it can show what comes.
		time.after(0.12, run_shell)
		return "scheduled"
	end,
}

-- ---- the person's own editor, for a terminal that shows no views ---------------------------------

-- `$VISUAL`, `$EDITOR` or vi, run as git runs it (`sh -c '$EDITOR "$@"'`), so that an editor given
-- with options works. textpen itself is passed over, so that `EDITOR=textpen` does not start textpen
-- again.
local function run_own_editor(targets)
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
	local ok, result = pcall(pencere.handoff, { argv = argv, cwd = root })
	if not ok then
		return 127
	end
	return result.code or 1
end

local exit_code = 0

local function fallback(why)
	if handed_off then
		return
	end
	if why.reason == "no-tty" then
		-- No terminal at all (a script, a pipe): there is no pane for any editor.
		complain("textpen: needs a terminal to edit in")
		exit_code = 1
		return
	end
	local targets = {}
	if #files > 0 then
		for _, file in ipairs(files) do
			local relative = paths.relative(root, file)
			targets[#targets + 1] = relative ~= "" and relative or file
		end
	else
		targets[1] = "."
	end
	exit_code = run_own_editor(targets)
end

-- The editor did not come back after the shell: the terminal did not answer, or would not show it.
-- What is open is still the program's, so the person chooses: once more, or quit (keeping what is
-- not saved as drafts). True to try again.
local function try_again(reason)
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

-- ---- the view ------------------------------------------------------------------------------------

local function serve()
	return pencere.serve({
		web = "web",
		program = "textpen",
		-- What a terminal writes above the view, and the icon it draws beside it.
		title = "Editor",
		icons = {
			{ file = "assets/icon-32-dark.png", theme = "dark" },
			{ file = "assets/icon-64-dark.png", theme = "dark" },
			{ file = "assets/icon-32-light.png", theme = "light" },
			{ file = "assets/icon-64-light.png", theme = "light" },
		},
		calls = program.calls,
		topics = program.topics,
		-- The person's own editor stands in for a terminal that shows no views, at the start only.
		fallback = fallback,
	})
end

while true do
	local result = serve()
	if not result.shown and handed_off and result.reason ~= "interrupted" and try_again(result.reason) then
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

textpen.desk = nil
textpen:ending()
pencere.exit(exit_code)
