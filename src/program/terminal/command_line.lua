-- The command line: what textpen was asked to open. `+line`, `path:line:column`, the directory to
-- open as the project; the help, the version and the complaints go to the terminal as text.
--
-- Plain values in and out: this file knows nothing of the page.

local pencere = require("pencere")
local fs, io = pencere.fs, pencere.io

local paths = require("project.paths")
local version = require("version")

local M = {}

local USAGE = [=[
usage: textpen [options] [+line] [path[:line[:column]] ...]

A code editor in the terminal pane: a file tree, tabs, search, and the
terminal's own colours and font, with the keyboard or the mouse.

  path                 a file to open, or the directory to open as the project
                       (the current directory by default, with the files that
                       were open there last time)
  +line, path:line     put the cursor on that line
      --dev            (run under `pencere dev`, which shows the view again
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

-- What `argv` asks: `{ root =, files =, focus = }`, where `focus` is the file to put the cursor in
-- (`{ path =, line =, column = }`, absolute) when the command line named a line. Help, the version
-- and a command line that is not understood end the program.
function M.read(argv)
	local options = parse(argv)
	if options.dev then
		complain("textpen: --dev is for `pencere dev`, which runs the program from its files and shows the view again when they change")
		pencere.exit(2)
	end
	local root, files = root_of(options.paths)
	local focus = nil
	if #files > 0 and options.focus then
		focus = { path = paths.resolve(fs.cwd(), options.focus.path), line = options.focus.line, column = options.focus.column }
	end
	return { root = root, files = files, focus = focus }
end

return M
