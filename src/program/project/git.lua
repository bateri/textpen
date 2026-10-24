-- What git says of a project: whether it is a repository, the files it lists, and its branch and
-- changed files. git is asked with a time limit, and a repository on a slow disk or a drive that
-- went away is given up on (the caller does without); what it said is kept for a moment, so that
-- the people who ask do not each start a git.
--
-- Plain values in and out: this file knows nothing of the page.

local pencere = require("pencere")
local json, process, time = pencere.json, pencere.process, pencere.time

local M = {}

-- How long git may take before it is given up on (the files are then walked).
local GIT_TIMEOUT = 15

local function now_ms()
	return math.floor(time.now() * 1000)
end

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

local Git = {}
Git.__index = Git

function M.new(root)
	local self = setmetatable({}, Git)
	self.root = root
	self.cache = nil
	self.running = false
	self.repository = nil
	return self
end

function Git:is_repository()
	if self.repository == nil then
		self.repository = (pcall(git_run, { "rev-parse", "--is-inside-work-tree" }, self.root))
	end
	return self.repository
end

-- The files git lists (tracked, and not tracked but not ignored), or nil when git cannot say.
function Git:files()
	if not self:is_repository() then
		return nil
	end
	local ok, out = pcall(git_run, { "ls-files", "-z", "--cached", "--others", "--exclude-standard" }, self.root, 64 * 1024 * 1024)
	if ok then
		return split_nul(out)
	end
	return nil
end

-- What git says of the project: its branch and changed files. Asked at most every few seconds.
function Git:status(max_age)
	max_age = max_age or 2500
	while self.running do
		time.sleep(0.01)
	end
	local cached = self.cache
	if cached and now_ms() - cached.at < max_age then
		return cached.value
	end
	if not self:is_repository() then
		return nil
	end
	self.running = true
	local ok, value = pcall(read_git, self.root)
	self.running = false
	if not ok then
		value = nil
	end
	self.cache = { at = now_ms(), value = value }
	return value
end

-- Forgets what was said: after the files changed.
function Git:forget()
	self.cache = nil
end

return M
