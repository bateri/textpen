-- The project on disk: the directory textpen was started in (or on), the files in it, and what
-- is made of them (a listing, the index of every file for the quick open, a file created, renamed
-- or removed). Paths travel relative to the root and `/`-separated; a file outside the root travels
-- as its absolute path. What git says of the files is `project.git`, a search through them
-- `project.search`.
--
-- Plain values in and out: this file knows nothing of the page.

local pencere = require("pencere")
local fs, time = pencere.fs, pencere.time

local git_module = require("project.git")
local paths = require("project.paths")
local watch_module = require("project.watch")

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

local Tree = {}
Tree.__index = Tree

function M.new(root)
	local self = setmetatable({}, Tree)
	self.root = paths.resolve(fs.cwd(), root)
	self.git = git_module.new(self.root)
	-- Directories the view listed, and whether they changed since.
	self.listed = watch_module.new()
	self.index = nil
	return self
end

-- The absolute path of a path the view or the command line gave.
function Tree:absolute(path)
	return paths.resolve(self.root, path)
end

-- How a path travels: relative to the root inside it, absolute outside.
function Tree:name(absolute)
	local rel = paths.relative(self.root, absolute)
	if rel == "" then
		return ""
	end
	if string.sub(rel, 1, 2) == ".." or paths.is_absolute(rel) then
		return absolute
	end
	return rel
end

function Tree:list(dir)
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
	self.listed:note(self:name(absolute), modified_ms(absolute))
	return { dir = self:name(absolute), entries = entries, truncated = truncated }
end

-- The listed directories whose entries changed since they were listed (or that went).
function Tree:changed_dirs()
	local changed = self.listed:changed(function(dir)
		return modified_ms(self:absolute(dir))
	end)
	if #changed > 0 then
		self.index = nil
	end
	return changed
end

-- Forgets what was learnt of the files: after the view created, renamed or removed one.
function Tree:forget()
	self.index = nil
	self.git:forget()
end

-- Every file of the project, for the quick open: git's list in a repository, a walk elsewhere.
function Tree:files()
	local cached = self.index
	if cached and now_ms() - cached.at < 15000 then
		return cached.value
	end
	local value
	local listed = self.git:files()
	if listed ~= nil then
		local truncated = #listed > INDEX_LIMIT
		while #listed > INDEX_LIMIT do
			listed[#listed] = nil
		end
		value = { files = listed, truncated = truncated }
	end
	if value == nil then
		value = self:walk()
	end
	self.index = { at = now_ms(), value = value }
	return value
end

function Tree:walk()
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

-- A file or a directory made, where nothing is. The name it has in the project.
function Tree:create(path, kind)
	local absolute = self:absolute(path)
	if fs.exists(absolute) then
		error(path .. " already exists", 0)
	end
	if kind == "dir" then
		fs.mkdir(absolute)
	else
		fs.mkdir(paths.dirname(absolute))
		-- Made with no other file there: an existing file is not truncated, and a link that points
		-- at nothing is not written through to make its target.
		local ok, why = pcall(fs.create, absolute)
		if not ok then
			if string.find(tostring(why), "File exists", 1, true) then
				error(path .. " already exists", 0)
			end
			error(why, 0)
		end
	end
	self:forget()
	return self:name(absolute)
end

-- A file or a directory moved. Gives the names it had and has, and whether it was moved at all.
function Tree:rename(from, to)
	local source = self:absolute(from)
	local target = self:absolute(to)
	if source == target then
		return { from = self:name(source), to = self:name(target), moved = false }
	end
	-- A change of case alone is the same file on a disk that ignores case, and allowed.
	if string.lower(source) ~= string.lower(target) and fs.exists(target) then
		error(to .. " already exists", 0)
	end
	fs.mkdir(paths.dirname(target))
	fs.rename(source, target)
	self:forget()
	return { from = self:name(source), to = self:name(target), moved = true }
end

function Tree:remove(path)
	local absolute = self:absolute(path)
	if absolute == self.root then
		error("The project itself is not removed", 0)
	end
	fs.remove(absolute, true)
	self:forget()
end

return M
