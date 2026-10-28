-- Fuzzy matching for the palette: every character of the query, in order, somewhere in the text,
-- scored so that matches at the starts of words and in a run come first — and, for a path, in the
-- file's own name before its directories.
--
-- Texts are compared a byte at a time, lowered as ASCII (a name in another script matches by its
-- bytes, the query's characters in it as they are); the places matched are bytes from 1, and
-- `fuzzy.marks` turns them into the characters to draw marked.

local fuzzy = {}

local BOUNDARY = { [" "] = true, ["-"] = true, ["_"] = true, ["."] = true, ["/"] = true, [":"] = true, ["\\"] = true }

local function is_upper(c)
	return c >= "A" and c <= "Z"
end

-- Whether a word starts at byte `at`: the first byte, one after a separator, or a capital after a
-- small letter (camelCase).
local function boundary_at(text, lower, at)
	if at == 1 then
		return true
	end
	if BOUNDARY[string.sub(lower, at - 1, at - 1)] then
		return true
	end
	local here, before = string.sub(text, at, at), string.sub(text, at - 1, at - 1)
	return is_upper(here) and not is_upper(before)
end

-- Matches `query` in `text` from byte `start` on; nil where a character is missing.
local function scan(query, text, lower, start)
	local positions = {}
	local score, run, from = 0, 0, start
	for index = 1, #query do
		local c = string.sub(query, index, index)
		if c ~= " " then
			local first = string.find(lower, c, from, true)
			if not first then
				return nil
			end
			local at = first
			-- A character that goes on a run stays there; one that does not is taken at a word start
			-- a little further on, where there is one.
			if positions[#positions] ~= first - 1 then
				local probe = first
				while probe and probe < first + 24 do
					if boundary_at(text, lower, probe) then
						at = probe
						break
					end
					probe = string.find(lower, c, probe + 1, true)
				end
			end
			local on_boundary = boundary_at(text, lower, at)
			if #positions > 0 and positions[#positions] == at - 1 then
				run += 1
			else
				run = 0
			end
			score += 1 + run * 3 + (on_boundary and 5 or 0) - math.min(at - from, 20) * 0.15
			positions[#positions + 1] = at
			from = at + 1
		end
	end
	return { score = score, positions = positions }
end

-- Characters spread over a whole name match it by chance.
local function scattered(positions)
	local count = #positions
	if count < 2 then
		return false
	end
	local span = positions[count] - positions[1] + 1
	return span > math.max(count * 3, count + 4)
end

local function run_of(from, count)
	local out = {}
	for index = 0, count - 1 do
		out[#out + 1] = from + index
	end
	return out
end

-- `query` in `text`: { score =, positions = } or nil.
function fuzzy.match(query, text)
	local q = string.lower(string.match(query, "^%s*(.-)%s*$"))
	if q == "" then
		return { score = 0, positions = {} }
	end
	local lower = string.lower(text)
	local direct = string.find(lower, q, 1, true)
	if direct then
		local start = boundary_at(text, lower, direct) and 30 or 0
		return { score = 100 + start - (direct - 1) * 0.5 - #lower * 0.05, positions = run_of(direct, #q) }
	end
	local found = scan(q, text, lower, 1)
	if not found or scattered(found.positions) then
		return nil
	end
	found.score -= #lower * 0.05
	return found
end

-- `query` in a path: its characters may be spread over the directories, but a match in the file's
-- name scores far above one in them, and a shorter path above a longer. `lower` is the path lowered
-- (`string.lower`), where the caller keeps it.
function fuzzy.path(query, path, lower)
	local q = string.lower(string.gsub(string.match(query, "^%s*(.-)%s*$"), "\\", "/"))
	if q == "" then
		return { score = 0, positions = {} }
	end
	lower = lower or string.lower(path)
	local name_start = (string.match(path, "^.*()/") or 0) + 1
	local slashes = select(2, string.gsub(path, "/", ""))
	local penalty = #path * 0.04 + slashes * 0.6
	if not string.find(q, "/", 1, true) then
		local name = string.sub(lower, name_start)
		local direct = string.find(name, q, 1, true)
		if direct then
			local exact = (name == q or string.sub(name, 1, #q + 1) == q .. ".") and 40 or 0
			return {
				score = 200 + exact + (direct == 1 and 30 or 0) - (direct - 1) - penalty,
				positions = run_of(name_start + direct - 1, #q),
			}
		end
		local in_name = scan(q, path, lower, name_start)
		if in_name and not scattered(in_name.positions) then
			return { score = 120 + in_name.score - penalty, positions = in_name.positions }
		end
	end
	local direct = string.find(lower, q, 1, true)
	if direct then
		return { score = 90 + (boundary_at(path, lower, direct) and 20 or 0) - penalty, positions = run_of(direct, #q) }
	end
	local spread = scan(q, path, lower, 1)
	if not spread then
		return nil
	end
	local positions = spread.positions
	-- The last characters of the query belong in the file's name, as people type a path.
	if positions[#positions] < name_start then
		return nil
	end
	-- In the directories a character starts a word or goes on a run (`s/l/or` for
	-- src/lib/orders.ts); one picked from the middle of a word is a coincidence.
	for index, at in positions do
		if at >= name_start then
			break
		end
		if not BOUNDARY[string.sub(lower, at, at)] and positions[index - 1] ~= at - 1 and not boundary_at(path, lower, at) then
			return nil
		end
	end
	local in_name = {}
	for _, at in positions do
		if at >= name_start then
			in_name[#in_name + 1] = at
		end
	end
	if scattered(in_name) then
		return nil
	end
	return { score = spread.score - penalty, positions = positions }
end

-- The parts of `text` (bytes `from` to `to`, from 1) to draw, each { words, marked }: the
-- characters whose first byte is in `positions` marked.
function fuzzy.marks(text, positions, from, to)
	from = from or 1
	to = to or #text
	local marked = {}
	for _, at in positions or {} do
		marked[at] = true
	end
	local parts = {}
	local run, run_marked = {}, false
	local at = from
	while at <= to do
		local byte = string.byte(text, at)
		local size = byte < 0x80 and 1 or (byte < 0xE0 and 2 or (byte < 0xF0 and 3 or 4))
		local char = string.sub(text, at, at + size - 1)
		local is_marked = marked[at] == true
		if is_marked ~= run_marked and #run > 0 then
			parts[#parts + 1] = { table.concat(run), run_marked }
			run = {}
		end
		run_marked = is_marked
		run[#run + 1] = char
		at += size
	end
	if #run > 0 then
		parts[#parts + 1] = { table.concat(run), run_marked }
	end
	return parts
end

return fuzzy
