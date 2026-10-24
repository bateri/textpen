-- A search through the files of the project: the matches of a pattern, line by line, in the files
-- the project lists that are text. It runs in the program's one thread and lets the other calls in
-- between every 25 files.
--
-- Plain values in and out: this file knows nothing of the page.

local pencere = require("pencere")
local fs, time, text, regex = pencere.fs, pencere.time, pencere.text, pencere.regex

local paths = require("project.paths")
local tree_module = require("project.tree")

local M = {}

local SEARCH_FILE_LIMIT = 2 * 1024 * 1024
local SEARCH_MATCH_LIMIT = 2000
local SEARCH_LINE_WIDTH = 160

local function now_ms()
	return math.floor(time.now() * 1000)
end

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
	if not read or tree_module.is_binary(bytes) then
		return nil
	end
	return text.lossy(bytes)
end

-- The matches of `options` (`query`, `regex`, `caseSensitive`, `wholeWord`) in the files of the
-- project `tree`.
function M.run(tree, options)
	local started = now_ms()
	local pattern = matcher(options)
	local index = tree:files()
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
		local content = read_text(paths.join(tree.root, path), SEARCH_FILE_LIMIT)
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
		return tree_module.compare(a.path, b.path)
	end)
	return { files = found, matches = matches, searched = searched, truncated = truncated, ms = now_ms() - started }
end

return M
