-- Search in the project's files: the program reads them, the page lists what matched, by file. ⏎ in
-- the field searches; ↓ goes to the results, where ↑ ↓ choose, ⏎ opens the match and goes to it,
-- Space opens it and stays, ← → fold a file's matches. F4 and ⇧F4 go through them from anywhere.

local kit = require("../lib/kit")
local text = require("../lib/text")
local files = require("../lib/files")
local docs = require("./docs")
local tree = require("./tree")

local b, S, SIZE, act = kit.b, kit.S, kit.SIZE, kit.act
local layout = b.layout

local search = {}

local FIELD = "search-query"
local LIST = "search-results"
local ROW = SIZE.row
-- How long after the typing stops the search runs, seconds.
local DELAY = 0.28

local Q = S.search
local timer = nil
local run_id = 0

-- The rows of the results: a file, then its matches unless it is folded.
local function rows_of(result)
	local out = {}
	local collapsed = {}
	for _, path in Q.collapsed do
		collapsed[path] = true
	end
	for file_index, file in result.files do
		local folded = collapsed[file.path] == true
		out[#out + 1] = { kind = "file", file = file_index, path = file.path, count = #file.matches, folded = folded }
		if not folded then
			for match_index in file.matches do
				out[#out + 1] = { kind = "match", file = file_index, match = match_index }
			end
		end
	end
	return out
end

local function chosen_index(rows)
	local chosen = Q.chosen
	if not chosen then
		return nil
	end
	for index, row in rows do
		if row.file == chosen.file and (row.kind == "match" and row.match == chosen.match or (row.kind == "file" and chosen.match == 0)) then
			return index
		end
	end
	return nil
end

-- Runs the search now.
function search.run()
	if timer then
		timer:cancel()
		timer = nil
	end
	if text.trim(Q.query) == "" then
		Q.result, Q.error, Q.running, Q.chosen = nil, nil, false, nil
		kit.gather(true)
		return
	end
	run_id += 1
	local mine = run_id
	Q.running, Q.error = true, nil
	kit.call("search", { query = Q.query, regex = Q.regex, caseSensitive = Q.case, wholeWord = Q.word }, function(result, err)
		if mine ~= run_id then
			return nil
		end
		Q.running = false
		if err then
			Q.error, Q.result = err.message, nil
		else
			Q.result = result
			Q.collapsed = {}
			Q.chosen = #result.files > 0 and { file = 1, match = 1 } or nil
		end
		return true
	end)
	kit.gather(true)
end

local function schedule()
	if timer then
		timer:cancel()
	end
	timer = kit.after(DELAY, function()
		timer = nil
		search.run()
		return nil
	end)
end

-- Opens the match chosen, or the next or the one before it (`step`); the keyboard goes to the editor
-- where `focus`.
function act.open_match(step, focus)
	local result = Q.result
	if not result or #result.files == 0 then
		return
	end
	local flat = {}
	for file_index, file in result.files do
		for match_index in file.matches do
			flat[#flat + 1] = { file = file_index, match = match_index }
		end
	end
	local at = 0
	if Q.chosen then
		for index, each in flat do
			if each.file == Q.chosen.file and each.match == Q.chosen.match then
				at = index
			end
		end
	end
	if step == 0 or step == nil then
		at = math.max(at, 1)
	else
		at = (at - 1 + step) % #flat + 1
	end
	local next_one = flat[at]
	Q.chosen = next_one
	local file = result.files[next_one.file]
	local match = file.matches[next_one.match]
	act.open_file(file.path, {
		select = { line = match.line, from = match.offset + match.start, to = match.offset + match["end"] },
		focus = focus ~= false,
	})
	search.show()
end

-- Whether the keyboard is in the search's results (not its field).
function search.in_results()
	return Q.in_results and S.focus == "sidebar" and S.sidebar == "search"
end

-- The keyboard goes to the search's field (`results` false) or its results.
function act.focus_search(results)
	S.focus = "sidebar"
	S.sidebar = "search"
	Q.in_results = results == true and Q.result ~= nil
	if Q.in_results then
		S.field = nil
		kit.keyboard("page")
	else
		S.field = FIELD
		kit.gather({ redraw = true, fill = { [FIELD] = { text = Q.query, select = Q.query ~= "" or nil } }, keyboard = { field = FIELD } })
	end
end

-- The word at the caret, as a word: letters, digits, underscores (and any letter that is not ASCII).
local function word_at_caret(doc, range)
	local from, to = layout.word_at(doc.id, range.head)
	if not from or to <= from or to - from > 200 then
		return nil
	end
	local word = layout.editor_text(doc.id, from, to) or ""
	if word == "" or string.find(word, "[^%w_\128-\255]") then
		return nil
	end
	return word
end

-- ⇧⌘F: the words chosen in the editor are what is looked for; with `or_word`, the word at the
-- caret where nothing is chosen.
function act.search_selection(or_word)
	local doc = docs.active()
	if doc then
		local range = doc.ranges[doc.main]
		if range and range.anchor ~= range.head and math.abs(range.head - range.anchor) <= 500 then
			local words = layout.editor_text(doc.id, math.min(range.anchor, range.head), math.max(range.anchor, range.head)) or ""
			if words ~= "" and not string.find(words, "\n", 1, true) then
				Q.query = words
				schedule()
			end
		elseif range and or_word then
			local word = word_at_caret(doc, range)
			if word then
				Q.query = word
				schedule()
			end
		end
	end
	act.focus_sidebar("search")
end

-- --- The panel --------------------------------------------------------------------------------------------

local function toggle(id, on, label, glyph)
	local over = kit.hovered(id)
	return b.div()
		:press(id)
		:size(24, 22)
		:rounded(4)
		:row()
		:items_center()
		:justify_center()
		:no_shrink()
		:bg(on and b.alpha("accent", 0.22) or (over and b.alpha("foreground", 0.08) or nil))
		:role("checkbox")
		:checked(on)
		:label(label)
		:tip(label)
		:child(b.text(glyph):text_size(11.5):weight(on and 700 or 500):color(on and "accent" or "dim"))
end

-- Where a match's line is shown from: a little before the match, so that it is in sight.
local function cut(match)
	local before = text.sub(match.text, 1, match.start)
	local indent = text.len(before) - text.len((string.gsub(before, "^%s+", "")))
	if match.start - indent > 18 then
		return match.start - 12
	end
	return indent
end

local function match_view(row, index, chosen)
	local file = Q.result.files[row.file]
	local match = file.matches[row.match]
	local id = "match:" .. row.file .. ":" .. row.match
	local from = cut(match)
	local stop = match["end"]
	local spans = {}
	if match.offset > 0 or from > 0 then
		spans[#spans + 1] = b.span("…"):color("quiet")
	end
	spans[#spans + 1] = b.span(text.sub(match.text, from + 1, match.start))
	spans[#spans + 1] = b.span(text.sub(match.text, match.start + 1, stop)):bg(b.alpha("warning", 0.35)):color("foreground")
	spans[#spans + 1] = b.span(text.sub(match.text, stop + 1))
	local focused = Q.in_results and S.focus == "sidebar"
	return b.div()
		:press(id)
		:row()
		:items_center()
		:gap(8)
		:h(ROW)
		:pl(30)
		:pr(10)
		:no_shrink()
		:bg(index == chosen and (focused and b.alpha("accent", 0.2) or b.alpha("foreground", 0.07)) or (kit.hovered(id) and b.alpha("foreground", 0.045) or nil))
		:tip("Line " .. match.line)
		:child(b.text(tostring(match.line)):color("quiet"):text_size(11):no_shrink())
		:child(b.rich(spans):truncate():min_w(0):grow(1))
end

local function file_view(row, index, chosen)
	local id = "match-file:" .. row.file
	local focused = Q.in_results and S.focus == "sidebar"
	local kind = files.kind(row.path)
	return b.div()
		:press(id)
		:row()
		:items_center()
		:gap(6)
		:h(ROW)
		:px(8)
		:no_shrink()
		:bg(index == chosen and (focused and b.alpha("accent", 0.2) or b.alpha("foreground", 0.07)) or (kit.hovered(id) and b.alpha("foreground", 0.045) or nil))
		:expanded(not row.folded)
		:child(kit.icon(row.folded and "chevron-right" or "chevron-down", 13, "quiet"))
		:child(kit.icon(kind.icon, 15, kind.color))
		:child(b.rich({
			b.span(text.basename(row.path)):bold(),
			b.span(" " .. text.dirname(row.path)):color("quiet"),
		}):truncate():min_w(0):grow(1))
		:child(b.div():px(6):h(17):rounded(9):bg("raised3"):row():items_center():no_shrink()
			:child(b.text(tostring(row.count)):text_size(10.5):color("dim")))
end

function act.search_view()
	local field = b.div()
		:row()
		:items_center()
		:gap(4)
		:h(30)
		:pl(8)
		:pr(3)
		:rounded(6)
		:bg("background")
		:border(1, S.field == FIELD and b.alpha("accent", 0.8) or "border")
		:child(kit.icon("search", 14, "quiet"))
		:child(b.div():text_field(FIELD):label("Search in files"):grow(1):h_full():child(b.text("Search in files")))
		:child(toggle("search-case", Q.case, "Match case", "Aa"))
		:child(toggle("search-word", Q.word, "Whole words", "ab"))
		:child(toggle("search-regex", Q.regex, "Regular expression", ".*"))
	local summary
	local result = Q.result
	if Q.running then
		summary = b.div():row():items_center():gap(6):child(kit.spinner(12, "quiet")):child(b.text("Searching…"):color("dim"))
	elseif Q.error then
		summary = b.text(Q.error):color("error"):wrap()
	elseif result then
		if result.matches == 0 then
			summary = b.text("No results in " .. result.searched .. " files"):color("dim")
		else
			summary = b.rich({
				b.span(result.matches .. (result.truncated and "+" or "") .. " results in " .. #result.files .. " files"),
				b.span(" · " .. result.ms .. " ms"):color("quiet"),
			})
		end
	else
		summary = b.text("⏎ searches · ↓ goes to the results"):color("quiet")
	end
	local panel = b.div():col():grow(1):min_h(0)
		:child(b.div():col():gap(6):p(10):no_shrink():child(field):child(b.div():text_size(11.5):child(summary)))
	if result and #result.files > 0 then
		local rows = rows_of(result)
		search.current = rows
		local chosen = chosen_index(rows)
		panel:child(b.div():id(LIST):col():grow(1):min_h(0):scroll():list(b.Rows.even(#rows, ROW):padded(0, 8), function(index)
			local row = rows[index]
			if row.kind == "file" then
				return file_view(row, index, chosen)
			end
			return match_view(row, index, chosen)
		end))
	end
	return panel
end

-- Keeps the chosen result in view.
function search.show()
	local rows = search.current
	if not rows then
		return
	end
	local index = chosen_index(rows)
	local extent = layout.scroll_extent(LIST)
	if not (index and extent) then
		return
	end
	local list = b.Rows.even(#rows, ROW):padded(0, 8)
	local offset = layout.scroll_offset(LIST)
	local to = list:reveal(index, offset, extent.view)
	if to ~= offset then
		layout.scroll_to(LIST, { y = to })
	end
end

-- --- What the person does ----------------------------------------------------------------------------

local function flip(key)
	Q[key] = not Q[key]
	schedule()
	kit.gather(true)
end

function search.press(id, event)
	if id == "search-case" then
		flip("case")
	elseif id == "search-word" then
		flip("word")
	elseif id == "search-regex" then
		flip("regex")
	else
		local file_index, match_index = string.match(id, "^match:(%d+):(%d+)$")
		if file_index then
			Q.chosen = { file = tonumber(file_index), match = tonumber(match_index) }
			Q.in_results = true
			act.open_match(0, (event.count or 1) > 1 or S.layout == "narrow")
			return true
		end
		local only_file = tonumber(string.match(id, "^match%-file:(%d+)$") or "")
		if only_file then
			local path = Q.result.files[only_file].path
			if text.index_of(Q.collapsed, path) then
				Q.collapsed = text.without(Q.collapsed, path)
			else
				Q.collapsed[#Q.collapsed + 1] = path
			end
			Q.chosen = { file = only_file, match = 0 }
			kit.gather(true)
			return true
		end
		return false
	end
	return true
end

function search.text(field, words)
	if field ~= FIELD then
		return false
	end
	Q.query = words
	schedule()
	return true
end

-- A key in the search's field, or in its results (the page's own keys).
function act.search_key(key, mods)
	if S.field == FIELD then
		if mods.command and mods.alt and (key == "c" or key == "w" or key == "r") then
			flip(key == "c" and "case" or (key == "w" and "word" or "regex"))
		elseif key == "enter" then
			if mods.command then
				act.open_match(0, true)
			else
				search.run()
			end
		elseif key == "down" and Q.result and #Q.result.files > 0 then
			act.focus_search(true)
		elseif key == "escape" then
			act.focus_editor()
		else
			return false
		end
		kit.gather(true)
		return true
	end
	local rows = search.current
	if not (Q.in_results and rows and #rows > 0) then
		if key == "escape" then
			act.focus_editor()
			return true
		end
		return false
	end
	if mods.command or mods.alt then
		return false
	end
	local index = chosen_index(rows) or 0
	local row = rows[index]
	local function choose(at)
		local target = rows[math.clamp(at, 1, #rows)]
		Q.chosen = { file = target.file, match = target.kind == "match" and target.match or 0 }
		search.show()
	end
	if key == "down" or key == "j" or (mods.control and key == "n") then
		choose(index + 1)
	elseif key == "up" or key == "k" or (mods.control and key == "p") then
		if index <= 1 then
			act.focus_search(false)
		else
			choose(index - 1)
		end
	elseif key == "home" or key == "g" then
		choose(1)
	elseif key == "end" or key == "G" then
		choose(#rows)
	elseif key == "page_down" then
		choose(index + 12)
	elseif key == "page_up" then
		choose(index - 12)
	elseif key == "enter" or key == " " then
		if row and row.kind == "file" then
			search.press("match-file:" .. row.file, {})
		elseif row then
			act.open_match(0, key == "enter")
		end
	elseif key == "left" or key == "h" then
		if row then
			local path = Q.result.files[row.file].path
			if not text.index_of(Q.collapsed, path) then
				Q.collapsed[#Q.collapsed + 1] = path
			end
			Q.chosen = { file = row.file, match = 0 }
		end
	elseif key == "right" or key == "l" then
		if row and row.kind == "file" then
			Q.collapsed = text.without(Q.collapsed, row.path)
		end
	elseif key == "escape" then
		act.focus_search(false)
	else
		return false
	end
	kit.gather(true)
	return true
end

-- The keys of the status line's hints, run.
function act.hint(name)
	if name == "files" then
		act.open_palette("files")
	elseif name == "commands" then
		act.open_palette("commands")
	elseif name == "save" then
		act.save()
	elseif name == "find" then
		act.find(false)
	elseif name == "close" then
		if S.active then
			act.close_tab(S.active)
		end
	elseif name == "editor" then
		act.focus_editor()
	elseif name == "search-run" then
		search.run()
	elseif name == "search-results" then
		act.focus_search(true)
	elseif name == "search-regex" then
		flip("regex")
	elseif name == "search-open" or name == "search-show" or name == "search-field" then
		local keys = { ["search-open"] = "enter", ["search-show"] = " ", ["search-field"] = "escape" }
		act.search_key(keys[name], {})
	elseif string.sub(name, 1, 5) == "tree-" then
		local keys = { ["tree-open"] = "enter", ["tree-new"] = "a", ["tree-rename"] = "r", ["tree-delete"] = "d", ["tree-menu"] = "." }
		tree.key(keys[name], {})
	end
end

return search
