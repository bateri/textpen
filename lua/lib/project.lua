-- The project: the directory textpen was started in (or on), its files, what git says of them,
-- and a search through them. Paths travel relative to the root and `/`-separated; a file outside
-- the root travels as its absolute path.

local pencere = require("pencere")
local fs, json, process, time, text, regex = pencere.fs, pencere.json, pencere.process, pencere.time, pencere.text, pencere.regex

local paths = require("lib.paths")

local M = {}

-- Directories nobody means to browse or search: version control and installed packages.
local SKIPPED = {}
for _, name in ipairs({
	".git", ".hg", ".svn", "node_modules", ".venv", "venv", "__pycache__", ".mypy_cache",
	".pytest_cache", ".next", ".nuxt", ".turbo", ".cache", "target",
}) do
	SKIPPED[name] = true
end

local LIST_LIMIT = 5000
local INDEX_LIMIT = 50000

-- A file this large is not searched, and not opened for editing.
M.TEXT_LIMIT = 8 * 1024 * 1024
local SEARCH_FILE_LIMIT = 2 * 1024 * 1024
local SEARCH_MATCH_LIMIT = 2000
local SEARCH_LINE_WIDTH = 160

-- How long git may take before it is given up on (the files are then walked).
local GIT_TIMEOUT = 15

local function now_ms()
	return math.floor(time.now() * 1000)
end

-- A NUL in the first 8 KiB makes a file binary, as git and grep decide it.
function M.is_binary(bytes)
	return string.find(string.sub(bytes, 1, 8192), "\0", 1, true) ~= nil
end

-- The order names are listed in, in the tree and in the search: numbers as numbers (`file9` before
-- `file10`), small and capital letters together; a tie is broken by the bytes, so that the order is
-- the same on every run.
function M.compare(a, b)
	return pencere.text.compare(a, b, true) < 0
end

local function modified_ms(path)
	local ok, info = pcall(fs.stat, path)
	if ok and info then
		return info.modified * 1000
	end
	return nil
end
M.modified_ms = modified_ms

-- git, with its time limit. The output is not text: names are ended by NUL and given exactly.
local function git_run(args, cwd, max_output)
	local argv = { "git" }
	for _, arg in ipairs(args) do
		argv[#argv + 1] = arg
	end
	local result = process.run({ argv = argv, cwd = cwd, timeout = GIT_TIMEOUT, raw = true, max_output = max_output or 1024 * 1024 })
	if result.timed_out then
		error("git took too long", 0)
	elseif result.overflowed then
		error("git said too much", 0)
	elseif not result.ok then
		error("git failed" .. (result.stderr ~= "" and (": " .. result.stderr) or ""), 0)
	end
	return result.stdout
end

local function split_nul(out)
	local items, seen = {}, {}
	for item in string.gmatch(out, "[^\0]+") do
		if not seen[item] then
			seen[item] = true
			items[#items + 1] = item
		end
	end
	return items
end

local function state_of(xy)
	if string.find(xy, "R", 1, true) then
		return "renamed"
	end
	if string.sub(xy, 2, 2) == "D" or string.sub(xy, 1, 1) == "D" then
		return "deleted"
	end
	if string.sub(xy, 1, 1) == "A" then
		return "added"
	end
	return "modified"
end

local function split_spaces(line)
	local parts = {}
	local start = 1
	while true do
		local at = string.find(line, " ", start, true)
		if not at then
			parts[#parts + 1] = string.sub(line, start)
			return parts
		end
		parts[#parts + 1] = string.sub(line, start, at - 1)
		start = at + 1
	end
end

local function read_git(root)
	-- Where the root is in the repository, as git names it: a path git gives is made the view's by
	-- taking this off, whatever links lie between the two (/var and /private/var).
	local prefix = string.gsub(git_run({ "rev-parse", "--show-prefix" }, root), "^%s+", "")
	prefix = string.gsub(prefix, "%s+$", "")
	local out = git_run({ "status", "--porcelain=v2", "--branch", "-z", "--untracked-files=all" }, root, 32 * 1024 * 1024)
	local info = { branch = "", ahead = 0, behind = 0, changes = json.object() }
	local tokens = {}
	local start = 1
	while start <= #out do
		local at = string.find(out, "\0", start, true) or (#out + 1)
		tokens[#tokens + 1] = string.sub(out, start, at - 1)
		start = at + 1
	end
	-- git names paths from the top of the repository; the view names them from the root, and does
	-- not show what lies outside it.
	local function note(path, state)
		if string.sub(path, 1, #prefix) == prefix then
			info.changes[string.sub(path, #prefix + 1)] = state
		end
	end
	local i = 1
	while i <= #tokens do
		local token = tokens[i]
		local head = string.sub(token, 1, 2)
		if string.sub(token, 1, 14) == "# branch.head " then
			info.branch = string.sub(token, 15)
		elseif string.sub(token, 1, 13) == "# branch.oid " and info.branch == "" then
			info.branch = string.sub(token, 14, 20)
		elseif string.sub(token, 1, 12) == "# branch.ab " then
			local ahead, behind = string.match(string.sub(token, 13), "^(%S+) (%S+)")
			info.ahead = math.abs(tonumber(ahead) or 0)
			info.behind = math.abs(tonumber(behind) or 0)
		elseif head == "1 " or head == "2 " then
			local parts = split_spaces(token)
			local first = head == "1 " and 9 or 10
			local path = table.concat(parts, " ", first, #parts)
			if head == "2 " then
				i += 1 -- the original path follows
			end
			note(path, state_of(parts[2] or ""))
		elseif head == "u " then
			local parts = split_spaces(token)
			note(table.concat(parts, " ", 11, #parts), "conflict")
		elseif head == "? " then
			note(string.sub(token, 3), "untracked")
		end
		i += 1
	end
	if info.branch == "(detached)" then
		local ok, short = pcall(git_run, { "rev-parse", "--short", "HEAD" }, root)
		if ok then
			info.branch = string.gsub(short, "%s+$", "")
		end
	end
	return info
end

local Project = {}
Project.__index = Project

function M.new(root)
	local self = setmetatable({}, Project)
	self.root = paths.resolve(fs.cwd(), root)
	self.git_cache = nil
	self.git_running = false
	self.index = nil
	self.repository = nil
	-- Directories the view listed, with their modification time then.
	self.listed = {}
	return self
end

-- The absolute path of a path the view or the command line gave.
function Project:absolute(path)
	return paths.resolve(self.root, path)
end

-- How a path travels: relative to the root inside it, absolute outside.
function Project:name(absolute)
	local rel = paths.relative(self.root, absolute)
	if rel == "" then
		return ""
	end
	if string.sub(rel, 1, 2) == ".." or paths.is_absolute(rel) then
		return absolute
	end
	return rel
end

function Project:list(dir)
	local absolute = self:absolute(dir)
	local items = fs.list(absolute)
	local truncated = #items > LIST_LIMIT
	local entries = {}
	for index = 1, math.min(#items, LIST_LIMIT) do
		local item = items[index]
		local path = self:name(paths.join(absolute, item.name))
		if item.link then
			entries[#entries + 1] = {
				name = item.name,
				path = path,
				kind = item.kind == "dir" and "dir" or "file",
				link = true,
				-- (A link to nothing is of no size; the runner gives what it is: the length of its target's name.)
				size = item.kind == "other" and 0 or item.size,
			}
		elseif item.kind == "dir" then
			entries[#entries + 1] = { name = item.name, path = path, kind = "dir", size = 0 }
		else
			entries[#entries + 1] = { name = item.name, path = path, kind = "file", size = item.size }
		end
	end
	table.sort(entries, function(a, b)
		local a_dir, b_dir = a.kind == "dir", b.kind == "dir"
		if a_dir ~= b_dir then
			return a_dir
		end
		return M.compare(a.name, b.name)
	end)
	local mtime = modified_ms(absolute)
	if mtime then
		self.listed[self:name(absolute)] = mtime
	end
	return { dir = self:name(absolute), entries = entries, truncated = truncated }
end

-- The listed directories whose entries changed since they were listed (or that went).
function Project:changed_dirs()
	local changed = {}
	local names = {}
	for dir in pairs(self.listed) do
		names[#names + 1] = dir
	end
	table.sort(names)
	for _, dir in ipairs(names) do
		local now = modified_ms(self:absolute(dir))
		if now == nil then
			self.listed[dir] = nil
			changed[#changed + 1] = dir
		elseif now ~= self.listed[dir] then
			changed[#changed + 1] = dir
		end
	end
	if #changed > 0 then
		self.index = nil
	end
	return changed
end

-- Forgets what was learnt of the files: after the view created, renamed or removed one.
function Project:forget()
	self.index = nil
	self.git_cache = nil
end

-- Every file of the project, for the quick open: git's list in a repository, a walk elsewhere.
function Project:files()
	local cached = self.index
	if cached and now_ms() - cached.at < 15000 then
		return cached.value
	end
	local value
	if self:is_repository() then
		local ok, out = pcall(git_run, { "ls-files", "-z", "--cached", "--others", "--exclude-standard" }, self.root, 64 * 1024 * 1024)
		if ok then
			local files = split_nul(out)
			local truncated = #files > INDEX_LIMIT
			while #files > INDEX_LIMIT do
				files[#files] = nil
			end
			value = { files = files, truncated = truncated }
		end
	end
	if value == nil then
		value = self:walk()
	end
	self.index = { at = now_ms(), value = value }
	return value
end

function Project:walk()
	local files = {}
	local queue = { self.root }
	local head, visited = 1, 0
	while head <= #queue and #files < INDEX_LIMIT do
		local dir = queue[head]
		head += 1
		visited += 1
		-- The walk of a big tree does not hold the other calls up.
		if visited % 100 == 0 then
			time.sleep(0)
		end
		local ok, items = pcall(fs.list, dir)
		if ok then
			for _, item in ipairs(items) do
				local full = paths.join(dir, item.name)
				if item.link then
					files[#files + 1] = self:name(full)
				elseif item.kind == "dir" then
					if not SKIPPED[item.name] then
						queue[#queue + 1] = full
					end
				elseif item.kind == "file" then
					files[#files + 1] = self:name(full)
				end
			end
		end
	end
	local truncated = #files >= INDEX_LIMIT
	while #files > INDEX_LIMIT do
		files[#files] = nil
	end
	return { files = files, truncated = truncated }
end

function Project:is_repository()
	if self.repository == nil then
		self.repository = (pcall(git_run, { "rev-parse", "--is-inside-work-tree" }, self.root))
	end
	return self.repository
end

-- What git says of the project: its branch and changed files. Asked at most every few seconds.
function Project:git(max_age)
	max_age = max_age or 2500
	while self.git_running do
		time.sleep(0.01)
	end
	local cached = self.git_cache
	if cached and now_ms() - cached.at < max_age then
		return cached.value
	end
	if not self:is_repository() then
		return nil
	end
	self.git_running = true
	local ok, value = pcall(read_git, self.root)
	self.git_running = false
	if not ok then
		value = nil
	end
	self.git_cache = { at = now_ms(), value = value }
	return value
end

-- ---- search ---------------------------------------------------------------------------------

local function matcher(options)
	local source = options.regex and options.query or regex.escape(options.query)
	local ok, found = pcall(regex.new, source, { ignore_case = not options.caseSensitive, whole_word = options.wholeWord })
	if not ok then
		local words = string.gsub(tostring(found), "^regex%.new: ", "")
		error("Not a regular expression: " .. words, 0)
	end
	return found
end

-- The bytes of a line from unit `from` to unit `to` (UTF-16, as the view counts), moved in to
-- whole characters.
local function cut(line, from, to)
	local first = text.index(line, from) or text.index(line, from + 1) or (#line + 1)
	local last = text.index(line, to) or text.index(line, to - 1) or (#line + 1)
	if last < first then
		last = first
	end
	return string.sub(line, first, last - 1)
end

-- The matches of a file's text, line by line.
local function search_text(path, content, pattern, room)
	local file = nil
	local line_number = 0
	local position = 1
	local length = #content
	while position <= length + 1 and room > 0 do
		local stop = string.find(content, "[\r\n]", position)
		local line, next_position
		if stop == nil then
			line, next_position = string.sub(content, position), length + 2
		else
			line = string.sub(content, position, stop - 1)
			if string.sub(content, stop, stop + 1) == "\r\n" then
				next_position = stop + 2
			else
				next_position = stop + 1
			end
		end
		position = next_position
		line_number += 1
		if line ~= "" then
			local from = 1
			local line_units = nil
			while room > 0 do
				local first, last = pattern:find(line, from)
				if first == nil then
					break
				end
				if last < first then
					from = first + 1
				else
					line_units = line_units or text.units(line)
					local start = text.units(line, 1, first - 1)
					local stop_unit = start + text.units(line, first, last)
					-- A long line is cut around the match, so that the list shows what matched.
					local offset = line_units <= SEARCH_LINE_WIDTH and 0 or math.max(0, start - 40)
					local shown = line_units <= SEARCH_LINE_WIDTH and line or cut(line, offset, offset + SEARCH_LINE_WIDTH)
					file = file or { path = path, matches = {} }
					file.matches[#file.matches + 1] = {
						line = line_number,
						start = start - offset,
						["end"] = math.min(stop_unit - offset, text.units(shown)),
						text = shown,
						offset = offset,
					}
					room -= 1
					from = last + 1
				end
			end
		end
	end
	return file
end

-- A file's text, or nil when it is too large, unreadable or binary.
local function read_text(path, limit)
	local ok, info = pcall(fs.stat, path)
	if not ok or info == nil or info.kind ~= "file" or info.size > limit then
		return nil
	end
	local read, bytes = pcall(fs.read, path)
	if not read or M.is_binary(bytes) then
		return nil
	end
	return text.lossy(bytes)
end

function Project:search(options)
	local started = now_ms()
	local pattern = matcher(options)
	local index = self:files()
	local found = {}
	local matches, searched, truncated = 0, 0, false
	local files = {}
	for _, path in ipairs(index.files) do
		if not paths.is_absolute(path) then
			files[#files + 1] = path
		end
	end
	local count = 0
	for _, path in ipairs(files) do
		if truncated then
			break
		end
		count += 1
		-- A search through a big project does not hold the other calls up.
		if count % 25 == 0 then
			time.sleep(0)
		end
		local content = read_text(paths.join(self.root, path), SEARCH_FILE_LIMIT)
		if content ~= nil then
			searched += 1
			local file = search_text(path, content, pattern, SEARCH_MATCH_LIMIT - matches)
			if file ~= nil then
				found[#found + 1] = file
				matches += #file.matches
				if matches >= SEARCH_MATCH_LIMIT then
					truncated = true
				end
			end
		end
	end
	table.sort(found, function(a, b)
		return M.compare(a.path, b.path)
	end)
	return { files = found, matches = matches, searched = searched, truncated = truncated, ms = now_ms() - started }
end

return M
