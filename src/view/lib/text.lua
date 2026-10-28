-- Text as the program and the editors count it, in characters (code points), and paths as the
-- program writes them ("/"-separated, relative to the project's root, absolute outside it).
--
-- Lua's own string functions count bytes; every place that crosses to the program or to an editor
-- (an edit, a match, a caret, a column) is a count of characters, so it is read and cut here.

local text = {}

-- How many characters `s` has (a byte that starts no character counts as one, as the editors'
-- texts never have one).
function text.len(s)
	return utf8.len(s) or #s
end

-- The byte where character `n` (from 1) starts; one past the end for a character past it.
function text.offset(s, n)
	if n <= 1 then
		return 1
	end
	return utf8.offset(s, n) or (#s + 1)
end

-- Characters `from` to `to` of `s`, from 1, both in it (as `string.sub` counts bytes).
function text.sub(s, from, to)
	local length = text.len(s)
	to = to or length
	if from < 1 then
		from = 1
	end
	if to > length then
		to = length
	end
	if from > to then
		return ""
	end
	return string.sub(s, text.offset(s, from), text.offset(s, to + 1) - 1)
end

-- `s` cut into pieces of at most `size` characters each (one piece for an empty text).
function text.pieces(s, size)
	local out = {}
	local at = 1
	local count = text.len(s)
	if count == 0 then
		return { "" }
	end
	local done = 0
	while done < count do
		local take = math.min(size, count - done)
		local stop = text.offset(s, done + take + 1)
		out[#out + 1] = string.sub(s, at, stop - 1)
		at = stop
		done += take
	end
	return out
end

-- The characters of `s`, one string each.
function text.chars(s)
	local out = {}
	for _, code in utf8.codes(s) do
		out[#out + 1] = utf8.char(code)
	end
	return out
end

-- Lower case, for comparing names as people read them: ASCII and the Latin letters of the common
-- alphabets (Luau's `string.lower` knows ASCII alone).
local LOWER = {
	["Ç"] = "ç", ["Ğ"] = "ğ", ["İ"] = "i", ["I"] = "ı", ["Ö"] = "ö", ["Ş"] = "ş", ["Ü"] = "ü",
	["Ä"] = "ä", ["Å"] = "å", ["Æ"] = "æ", ["Ø"] = "ø", ["É"] = "é", ["È"] = "è", ["Ê"] = "ê",
	["Á"] = "á", ["À"] = "à", ["Â"] = "â", ["Í"] = "í", ["Ó"] = "ó", ["Ò"] = "ò", ["Ô"] = "ô",
	["Ú"] = "ú", ["Ñ"] = "ñ",
}
function text.lower(s)
	if not string.find(s, "[\128-\255]") then
		return string.lower(s)
	end
	local out = {}
	for _, code in utf8.codes(s) do
		local c = utf8.char(code)
		out[#out + 1] = code < 128 and string.lower(c) or (LOWER[c] or c)
	end
	return table.concat(out)
end

-- How long ago `seconds` of elapsed time was, as a person says it: "just now", "4 minutes ago".
function text.ago(seconds)
	local function many(count, unit)
		return count .. " " .. unit .. (count == 1 and "" or "s") .. " ago"
	end
	if seconds < 45 then
		return "just now"
	elseif seconds < 3600 then
		return many(math.max(math.floor(seconds / 60 + 0.5), 1), "minute")
	elseif seconds < 86400 then
		return many(math.floor(seconds / 3600 + 0.5), "hour")
	end
	return many(math.floor(seconds / 86400 + 0.5), "day")
end

-- Whether `s` starts with `prefix`, or ends with `suffix`.
function text.starts(s, prefix)
	return string.sub(s, 1, #prefix) == prefix
end

function text.ends(s, suffix)
	return suffix == "" or string.sub(s, -#suffix) == suffix
end

function text.trim(s)
	return (string.gsub(s, "^%s+", ""):gsub("%s+$", ""))
end

-- --- Paths ---------------------------------------------------------------------------------------

function text.basename(path)
	return string.match(path, "([^/]*)$") or path
end

function text.dirname(path)
	return string.match(path, "^(.*)/[^/]*$") or ""
end

-- The extension of a file's name, lower case, without its dot ("" for none, or a name that is
-- only a dot and letters, as ".env").
function text.extension(path)
	local name = text.basename(path)
	local ext = string.match(name, "^.+%.([^%.]+)$")
	return ext and string.lower(ext) or ""
end

function text.join(dir, name)
	return dir == "" and name or (dir .. "/" .. name)
end

-- Whether `path` is `dir` or lies in it.
function text.within(path, dir)
	return dir == "" or path == dir or text.starts(path, dir .. "/")
end

-- The directories a path lies in, outermost first: "a/b/c.ts" is "a", "a/b".
function text.ancestors(path)
	local out = {}
	local parts = string.split(path, "/")
	local at = ""
	for index = 1, #parts - 1 do
		at = at == "" and parts[index] or (at .. "/" .. parts[index])
		out[#out + 1] = at
	end
	return out
end

-- A path for people: "~" for the home directory.
function text.friendly(path, home)
	if home and home ~= "" and (path == home or text.starts(path, home .. "/")) then
		return "~" .. string.sub(path, #home + 1)
	end
	return path
end

-- A size in bytes as people read it.
function text.bytes(size)
	if size < 1024 then
		return string.format("%d B", size)
	elseif size < 1024 * 1024 then
		return string.format(size < 10 * 1024 and "%.1f KB" or "%.0f KB", size / 1024)
	end
	return string.format("%.1f MB", size / 1024 / 1024)
end

-- Whether a list holds `value`, and where.
function text.index_of(list, value)
	for index, item in list do
		if item == value then
			return index
		end
	end
	return nil
end

-- A list without `value`.
function text.without(list, value)
	local out = {}
	for _, item in list do
		if item ~= value then
			out[#out + 1] = item
		end
	end
	return out
end

return text
