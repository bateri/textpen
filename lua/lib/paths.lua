-- Paths by their text alone, the way Node's `path` has them: nothing here asks the disk, so a
-- file that is not there yet has a path, and a link is not followed.

local M = {}

function M.is_absolute(path)
	return string.sub(path, 1, 1) == "/"
end

-- The path with its `.` and `..` parts worked out, and no empty ones; "." for nothing at all.
function M.normalize(path)
	local absolute = M.is_absolute(path)
	local parts = {}
	for part in string.gmatch(path, "[^/]+") do
		if part == ".." then
			if #parts > 0 and parts[#parts] ~= ".." then
				parts[#parts] = nil
			elseif not absolute then
				parts[#parts + 1] = ".."
			end
		elseif part ~= "." then
			parts[#parts + 1] = part
		end
	end
	local joined = table.concat(parts, "/")
	if absolute then
		return "/" .. joined
	end
	return joined == "" and "." or joined
end

-- `path` made absolute from `base` (an absolute path), as `path.resolve(base, path)` does.
function M.resolve(base, path)
	if M.is_absolute(path) then
		return M.normalize(path)
	end
	return M.normalize(base .. "/" .. path)
end

function M.join(...)
	local parts = { ... }
	return M.normalize(table.concat(parts, "/"))
end

function M.dirname(path)
	local dir = string.match(path, "^(.*)/[^/]*$")
	if dir == nil then
		return "."
	end
	return dir == "" and "/" or dir
end

function M.basename(path)
	local trimmed = string.gsub(path, "/+$", "")
	return string.match(trimmed, "([^/]*)$") or ""
end

-- Where `target` is from `from`, both absolute and worked out: "" for the same place, and a path
-- that starts with ".." for one that is not below it.
function M.relative(from, to)
	if from == to then
		return ""
	end
	local prefix = from == "/" and "/" or from .. "/"
	if string.sub(to, 1, #prefix) == prefix then
		return string.sub(to, #prefix + 1)
	end
	local a, b = {}, {}
	for part in string.gmatch(from, "[^/]+") do
		a[#a + 1] = part
	end
	for part in string.gmatch(to, "[^/]+") do
		b[#b + 1] = part
	end
	local common = 0
	while common < #a and common < #b and a[common + 1] == b[common + 1] do
		common += 1
	end
	local out = {}
	for _ = common + 1, #a do
		out[#out + 1] = ".."
	end
	for index = common + 1, #b do
		out[#out + 1] = b[index]
	end
	return table.concat(out, "/")
end

return M
