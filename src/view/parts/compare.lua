-- A file side by side with what the last commit keeps of it: the commit's text on the left in an
-- editor that cannot be changed, the file's own editor on the right, the lines that differ in colour
-- (red where lines were taken away, green where they were added, blue where they were changed, and
-- in a changed line the words that differ), a hatched gap on the side that has fewer lines so that
-- the same lines stand at the same height, and the two scrolled together.
--
-- It is a mode of the file's tab (`tab.compare`), not a tab of its own: the right-hand editor is the
-- file's, so that typing, undo, the caret and saving are as ever, and the left one is made when the
-- mode begins and goes when it ends. What differs the program says (`compare`: the blocks, and the
-- words of the lines that are a pair; `compare-base`: the commit's text, a piece at a time); the page
-- asks again once what the person typed is with the program, and when git changed.
--
-- Alignment: a block that has more lines on one side puts a gap of the difference after its lines on
-- the other (a zone of the editor, in as many parts as a zone may be tall). How tall a row is the page
-- measures (an anchor on a line and its box), for the zones to be a whole number of rows; and the
-- editors' own scrolling is made to agree by telling the one the other's place, where a place that
-- was told is not told back (the editor says it scrolled to the place it was sent to).

local kit = require("../lib/kit")
local text = require("../lib/text")
local docs = require("./docs")
local changes = require("./changes")
local Syntax = require("@pencere/syntax")

local b, S, act = kit.b, kit.S, kit.act
local layout = b.layout

local compare = {}

-- The layers of decorations: the lines' colours, the gaps, and the anchor the row is measured by.
local GROUND = "compare"
local GAPS = "compare-gaps"
local ROW = "compare-row"
local ANCHOR = "compare-row"
local NUMBERS = { name = "numbers", pad = 14 }

-- The most rows one zone is told to hold (the kit's, 200) and the most points (4096, past which the kit
-- cuts a zone short): a gap that is taller is several zones one after the other.
local ZONE_ROWS = 200
local ZONE_POINTS = 4096
-- How many lines either side of what is in view a gap has an element for, how many gaps are hatched
-- at once, and how many words are marked on one side (a layer holds 20 000 decorations).
local WINDOW = 60
local HATCHED = 40
local WORDS_MOST = 6000
-- How many places told to a side are kept, to tell its echoes from what the person does.
local ECHOES = 32

-- The editors of the left-hand sides, by name, as `docs.by_id` has the files'.
local by_left = {}
local made = 0

local function tones()
	return {
		red = b.alpha("red", 0.16),
		green = b.alpha("green", 0.16),
		blue = b.alpha("blue", 0.14),
	}
end

-- --- Places -----------------------------------------------------------------------------------------------

-- Where the line `line` (from 0) of the editor `id` begins: a place of its text (bytes from 1); the end
-- of the text for a line it has not.
local function start_of(id, line)
	return layout.line_start(id, line + 1) or ((layout.editor_length(id) or 0) + 1)
end

local function whole(id)
	return layout.editor_version(id), 1, (layout.editor_length(id) or 0) + 1
end

-- --- The mode ------------------------------------------------------------------------------------------------

function compare.of(tab)
	return tab and tab.compare or nil
end

-- Whether the file shown (or `path`) is compared.
function compare.active(path)
	local tab = docs.tab_of(path or S.active)
	return tab ~= nil and tab.compare ~= nil
end

-- Whether the tab shows two columns: the program said there is something to compare.
function compare.columns(tab)
	local cmp = tab and tab.compare
	return cmp ~= nil and cmp.result ~= nil and cmp.result.why == nil
end

-- The editor on the left, for `tab`: its name, or nil.
function compare.left_of(tab)
	local cmp = tab and tab.compare
	return cmp and cmp.left or nil
end

local function ensure_syntax(cmp, doc)
	if cmp.lang == doc.lang then
		return
	end
	if cmp.syntax then
		kit.syntaxes[cmp.left] = nil
		for _, request in cmp.syntax:close() do
			kit.ask(cmp.left, request)
		end
		cmp.syntax = nil
	end
	cmp.lang = doc.lang
	if doc.lang.syntax then
		cmp.syntax = Syntax.new(doc.lang.syntax, cmp.left, { tab_width = S.settings and S.settings.tabSize or 4 })
		kit.syntaxes[cmp.left] = cmp.syntax
		if cmp.last and cmp.last > 0 then
			cmp.syntax:handle({ type = "viewport", editor = cmp.left, first = cmp.first, last = cmp.last })
		end
	end
end

-- Begins comparing the file `path` with the last commit; `how` is where the view goes once it is
-- laid out: "caret" (the line the caret is on, centred) or "first" (the first change).
function compare.enter(path, how)
	local tab = docs.tab_of(path)
	if not tab then
		act.open_file(path, { focus = how ~= "first" })
		tab = docs.tab_of(path)
	end
	if tab and tab.compare then
		-- Already beside the last commit: it comes to the front, and the view goes to the first change.
		act.open_file(path, { focus = how ~= "first" })
		if how == "first" and compare.columns(tab) and tab.compare.groups[1] then
			compare.arrived(tab, how)
		end
		return
	end
	if not tab or tab.status == "unopenable" or tab.status == "picture" or tab.status == "error" then
		return
	end
	made += 1
	local cmp = {
		left = "compare:" .. made,
		-- The left editor was made (`editor_ready`), and its text is all in it.
		mounted = false,
		loaded = false,
		-- What the program last said (`Compared`), and the commit's text on the left: which blob, the
		-- pieces read, how many loads the editor has yet to tell of.
		result = nil,
		shown = nil,
		pieces = nil,
		delivered = false,
		delivering = false,
		loads = 0,
		-- The lines in view on the left, as the editor says (from 1).
		first = 1,
		last = 0,
		-- How tall a row is, points, once measured, and the editor it was measured on.
		row = nil,
		on = nil,
		-- The layers are to be told again.
		dirty = false,
		drawn = false,
		-- The gaps made, by side; the groups of blocks that are one change, for going to the next.
		gaps = { left = {}, right = {} },
		groups = {},
		-- The places told to each side, for the echoes of its scrolling.
		asked = { left = {}, right = {} },
		-- Where the view goes when the layers are first drawn.
		arrive = how or "caret",
		asking = false,
		again = false,
		bucket = nil,
		closed = false,
	}
	tab.compare = cmp
	by_left[cmp.left] = cmp
	S.peek = nil
	if tab.doc and tab.doc.loaded then
		compare.refresh(tab.doc)
		changes.draw(tab.doc)
	end
	if path ~= S.active then
		act.open_file(path, { focus = how ~= "first" })
	end
	kit.gather(true)
end

-- Ends the comparison: the file's editor is as it was before it, the left one goes.
function compare.exit(path)
	local tab = docs.tab_of(path or S.active)
	local cmp = tab and tab.compare
	if not cmp then
		return
	end
	local doc = tab.doc
	-- What the layers put in the file's editor is taken off before the left one goes.
	if doc and not doc.closed and doc.mounted and doc.loaded then
		for _, layer in { GROUND, GAPS, ROW } do
			local version, from, to = whole(doc.id)
			kit.ask(doc.id, { decorate = layer, version = version, from = from, to = to, decorations = {} })
		end
	end
	compare.forget_tab(tab)
	if doc and not doc.closed then
		doc.extra = nil
		changes.refresh(doc)
		changes.draw(doc)
		if tab.path == S.active then
			act.focus_editor()
		end
	end
	kit.gather(true)
end

-- The comparison of `tab` is over, or its file is: nothing of it is kept.
function compare.forget_tab(tab)
	local cmp = tab.compare
	if not cmp then
		return
	end
	cmp.closed = true
	by_left[cmp.left] = nil
	kit.syntaxes[cmp.left] = nil
	tab.compare = nil
end

function compare.toggle(path)
	path = path or S.active
	if compare.active(path) then
		compare.exit(path)
	elseif path then
		compare.enter(path, "caret")
	end
end

-- A doc is gone (its tab closed): so is the comparison of it.
local forgot = docs.on_forget
docs.on_forget = function(doc)
	if forgot then
		forgot(doc)
	end
	local tab = docs.tab_of(doc.path)
	if tab and tab.compare and tab.doc == doc then
		compare.forget_tab(tab)
	end
end

-- --- Asking what differs ---------------------------------------------------------------------------------

-- The groups of blocks that touch on both sides: what the person takes for one change, and goes to.
local function grouped(blocks)
	local out = {}
	for _, block in blocks or {} do
		local last = out[#out]
		if last and block.oldLine <= last.old_end and block.newLine <= last.new_end then
			last.old_end = math.max(last.old_end, block.oldLine + block.oldCount)
			last.new_end = math.max(last.new_end, block.newLine + block.newCount)
		else
			out[#out + 1] = {
				line = block.newLine,
				old_line = block.oldLine,
				old_end = block.oldLine + block.oldCount,
				new_end = block.newLine + block.newCount,
			}
		end
	end
	return out
end

local function deliver(cmp)
	if cmp.delivering or cmp.closed or not cmp.mounted or not cmp.pieces then
		return
	end
	cmp.delivering = true
	local pieces = cmp.pieces
	cmp.pieces = nil
	cmp.delivered = false
	cmp.loads = 0
	local index = 0
	local function step()
		if cmp.closed then
			return nil
		end
		index += 1
		local piece = pieces[index]
		-- An empty text into an empty editor changes nothing, and is told of by nothing.
		local silent = piece == "" and (layout.editor_length(cmp.left) or 0) == 0
		kit.ask(cmp.left, { load = piece, more = index < #pieces })
		if not silent then
			cmp.loads += 1
		end
		if index < #pieces then
			kit.after(0, step)
		else
			cmp.delivering = false
			cmp.delivered = true
			compare.check_loaded(cmp)
		end
		return nil
	end
	kit.after(0, step)
end

-- The text of the left is all in its editor, and the editor has told of every piece.
function compare.check_loaded(cmp)
	if cmp.delivered and cmp.loads <= 0 and not cmp.loaded then
		cmp.loaded = true
		cmp.dirty = true
		local tab = compare.tab_of(cmp)
		if tab then
			compare.update(tab)
		end
	end
end

function compare.tab_of(cmp)
	for _, tab in S.tabs do
		if tab.compare == cmp then
			return tab
		end
	end
	return nil
end

-- The commit's text, read a piece at a time, goes into the left editor.
local function read_base(tab, cmp, base)
	local key = base.id or ""
	cmp.shown = key
	cmp.loaded = false
	cmp.delivered = false
	local doc = tab.doc
	if doc then
		doc.extra = base.length
	end
	if base.length == 0 or base.id == nil then
		cmp.pieces = { "" }
		deliver(cmp)
		return
	end
	local pieces = {}
	local function from(at)
		kit.call("compare-base", { path = tab.path, id = base.id, from = at }, function(piece, err)
			if cmp.closed or cmp.shown ~= key then
				return nil
			end
			local stop = piece and piece["end"]
			if err or type(stop) ~= "number" or stop <= at then
				-- The commit moved on while it was read: ask what differs again.
				cmp.shown = nil
				if tab.doc then
					kit.after(0.15, function()
						compare.refresh(tab.doc)
						return nil
					end)
				end
				return nil
			end
			pieces[#pieces + 1] = piece.text
			if stop < base.length then
				from(stop)
			else
				cmp.pieces = pieces
				deliver(cmp)
			end
			return nil
		end)
	end
	from(0)
end

-- Asks the program how the file differs from the last commit, and draws it.
function compare.refresh(doc)
	local tab = docs.tab_of(doc.path)
	local cmp = tab and tab.compare
	if not cmp or doc.closed or not doc.loaded then
		return
	end
	if cmp.asking then
		cmp.again = true
		return
	end
	if docs.unsent(doc) then
		-- What was typed goes first; its answer brings the question again.
		return
	end
	cmp.asking = true
	kit.call("compare", { path = doc.path }, function(result, err)
		cmp.asking = false
		if cmp.closed or doc.closed then
			return nil
		end
		if err or result == nil then
			cmp.result = { version = doc.version, why = "failed", message = err and err.message or nil }
			cmp.groups = {}
		elseif docs.unsent(doc) or result.version ~= doc.version then
			-- The text moved on while git was asked: what it said is of an older one.
			cmp.again = true
		else
			cmp.result = result
			cmp.groups = grouped(result.blocks)
			cmp.dirty = true
			if result.why == nil then
				local base = result.base
				local key = base.id or ""
				if cmp.shown ~= key then
					if docs.make_room(base.length) then
						read_base(tab, cmp, base)
					else
						kit.toast("Too much is open: close a tab with a large file first.", "bad")
						cmp.result = { version = result.version, why = "too-large" }
					end
				end
			end
		end
		compare.update(tab)
		if cmp.again then
			cmp.again = false
			kit.after(0.15, function()
				compare.refresh(doc)
				return nil
			end)
		end
		kit.gather(true)
		return nil
	end)
end

-- Every comparison again: git changed.
function compare.refresh_all()
	for _, tab in S.tabs do
		if tab.compare and tab.doc and tab.doc.loaded then
			compare.refresh(tab.doc)
		end
	end
end

-- The text of a file is new (its first, or one the program changed or took in): look again.
local was = docs.on_text
docs.on_text = function(doc)
	if was then
		was(doc)
	end
	compare.refresh(doc)
end

-- --- Drawing ------------------------------------------------------------------------------------------------

-- The decorations of the lines of one side that differ: a ground for the lines of each block, a mark
-- on the overview ruler, and the words of a pair that differ.
local function ground(which, id, blocks)
	local grounds = tones()
	local word_ground = b.alpha(which == "left" and "red" or "green", 0.34)
	local out, words = {}, 0
	for _, block in blocks do
		local line, count = block.newLine, block.newCount
		local kind = block.oldCount == 0 and "green" or "blue"
		local marked = block.newWords
		if which == "left" then
			line, count = block.oldLine, block.oldCount
			kind = block.newCount == 0 and "red" or "blue"
			marked = block.oldWords
		end
		if count > 0 then
			local from, to = start_of(id, line), start_of(id, line + count)
			out[#out + 1] = { from = from, to = to, line = grounds[kind] }
			out[#out + 1] = { from = from, to = to, ruler = kind, lane = "full" }
			for _, word in marked or {} do
				if words < WORDS_MOST then
					words += 1
					out[#out + 1] = { from = from + word[1], to = from + word[2], background = word_ground }
				end
			end
		end
	end
	return out
end

local function zone_name(which, index, part)
	return "gap:" .. which .. ":" .. index .. ":" .. part
end

-- The gaps of the two sides: where each is (the line it is by, from 0, and whether it is above that
-- line or below), how many rows tall. A block puts a gap of the lines it has more on one side after its
-- last line on the other, or, where that side has none of its lines, at the edge they would stand at.
local function gaps_of(cmp, rid, blocks)
	local lines = { left = layout.editor_lines(cmp.left) or 1, right = layout.editor_lines(rid) or 1 }
	local out = { left = {}, right = {} }
	local rows = { left = 0, right = 0 }
	local function put(which, count, line, have, side)
		if count <= 0 then
			return
		end
		local list = out[which]
		local place
		if have > 0 then
			place = { line = line + have - 1, side = "below", rows = count }
		elseif line < lines[which] then
			place = { line = line, side = "above", rows = count }
		else
			place = { line = lines[which] - 1, side = "below", rows = count }
		end
		list[#list + 1] = place
		rows[which] += count
	end
	for _, block in blocks do
		put("left", block.newCount - block.oldCount, block.oldLine, block.oldCount)
		put("right", block.oldCount - block.newCount, block.newLine, block.newCount)
	end
	-- What is left over is a line a text that ends with a line break has beyond its last line, which
	-- the other may not have.
	local left, right = lines.left + rows.left, lines.right + rows.right
	if left > right then
		put("right", left - right, lines.right, 0)
	elseif right > left then
		put("left", right - left, lines.left, 0)
	end
	return out
end

-- The zones of the gaps of one side, each as tall as its rows, in the parts one zone may be.
local function zones(which, id, gaps, row)
	local most = math.max(1, math.min(ZONE_ROWS, math.floor(ZONE_POINTS / row)))
	local finish = (layout.editor_length(id) or 0) + 1
	local out = {}
	for index, gap in gaps do
		local line, side = gap.line, gap.side
		local at = start_of(id, line)
		if at >= finish and finish > 1 and line > 0 then
			-- A mark at the very end of the text is not in the range a layer is told for, so it would stay
			-- when the layer is told again: a gap above the empty line the last break makes is the gap
			-- below the line before it.
			line, side = line - 1, "below"
			at = start_of(id, line)
		end
		local left, part = gap.rows, 0
		while left > 0 do
			local take = math.min(left, most)
			part += 1
			out[#out + 1] = {
				from = at, to = at, zone = zone_name(which, index, part), height = take * row, side = side,
			}
			left -= take
		end
	end
	return out
end

-- Tells both editors what differs and where the gaps are, as the program last said, in the places of the
-- text they have now. Once the program's text and the editor's are the same, the left text is in, and
-- how tall a row is is known.
function compare.update(tab)
	local cmp, doc = tab.compare, tab.doc
	if not (cmp and doc and doc.loaded and not doc.closed and doc.mounted) then
		return
	end
	local result = cmp.result
	if not result or result.why then
		return
	end
	ensure_syntax(cmp, doc)
	if not (cmp.mounted and cmp.loaded) then
		return
	end
	if docs.unsent(doc) or result.version ~= doc.version then
		return
	end
	if not cmp.row then
		compare.anchor(tab)
		return
	end
	if not cmp.dirty then
		return
	end
	cmp.dirty = false
	local blocks = result.blocks or {}
	cmp.gaps = gaps_of(cmp, doc.id, blocks)
	for _, side in { { "left", cmp.left }, { "right", doc.id } } do
		local which, id = side[1], side[2]
		local version, from, to = whole(id)
		kit.ask(id, { decorate = GROUND, version = version, from = from, to = to, decorations = ground(which, id, blocks) })
		kit.ask(id, {
			decorate = GAPS, version = version, from = from, to = to,
			decorations = zones(which, id, cmp.gaps[which], cmp.row),
		})
	end
	cmp.drawn = true
	kit.gather(true)
	if cmp.arrive then
		local how = cmp.arrive
		cmp.arrive = nil
		kit.after(0.05, function()
			compare.arrived(tab, how)
			return nil
		end)
	end
end

-- The view goes where it was asked to once the gaps are in: the caret's line in the middle, or the
-- first change; the left one follows to where the right one stands.
function compare.arrived(tab, how)
	local cmp, doc = tab.compare, tab.doc
	if not (cmp and doc) or cmp.closed or doc.closed then
		return
	end
	if how == "first" and cmp.groups[1] then
		local at = start_of(doc.id, cmp.groups[1].line)
		kit.ask(doc.id, { select = { { anchor = at, head = at } }, version = layout.editor_version(doc.id), main = 1 })
		kit.ask(doc.id, { reveal = at, version = layout.editor_version(doc.id), align = "center" })
	else
		docs.center(doc, docs.where(doc).line - 1)
	end
	kit.after(0.1, function()
		compare.sync_left(tab)
		return nil
	end)
	kit.gather(true)
end

-- The left editor is scrolled to where the right one is.
function compare.sync_left(tab)
	local cmp, doc = tab.compare, tab.doc
	if not (cmp and doc) or cmp.closed or doc.closed then
		return
	end
	local y, x = layout.scroll_offset(doc.id)
	local to = { x = x or 0, y = y or 0 }
	local asked = cmp.asked.left
	asked[#asked + 1] = to
	layout.scroll_to(cmp.left, to)
end

-- --- How tall a row is -----------------------------------------------------------------------------------------

-- Names a line in view of an editor that has text, for its box to be read: the file's own editor, or
-- the left one where the file is empty.
function compare.anchor(tab)
	local cmp, doc = tab.compare, tab.doc
	local id = (layout.editor_length(doc.id) or 0) > 0 and doc.id
		or ((layout.editor_length(cmp.left) or 0) > 0 and cmp.left or nil)
	if not id then
		-- Neither has a line to measure: nothing to be aligned either.
		cmp.row = cmp.row or 20
		cmp.dirty = true
		return
	end
	local finish = (layout.editor_length(id) or 0) + 1
	local line = (id == doc.id and doc.first or cmp.first) or 1
	local from = layout.line_start(id, line) or 1
	if from >= finish then
		from = 1
	end
	-- The line's first character alone: a range that takes in the line break is as tall as two rows.
	local to = from + 1
	for size = 1, 4 do
		if from + size <= finish and pcall(layout.editor_text, id, from, from + size) then
			to = from + size
			break
		end
	end
	cmp.on = id
	kit.ask(id, {
		decorate = ROW, version = layout.editor_version(id), from = 1, to = (layout.editor_length(id) or 0) + 1,
		decorations = { { from = from, to = to, anchor = ANCHOR } },
	})
	kit.gather(true)
end

local function row_now(cmp)
	if not cmp.on then
		return nil
	end
	local ok, box = pcall(function()
		return layout.bounds(b.range_of(cmp.on, ANCHOR))
	end)
	return ok and box and box.h and box.h > 0 and box.h or nil
end

-- The page was laid out: how tall a row is, read again, and the gaps written again where it changed.
function compare.settle()
	local tab = docs.tab_of(S.active)
	local cmp = tab and tab.compare
	if not cmp or cmp.closed or not compare.columns(tab) or not cmp.on then
		return false
	end
	local row = row_now(cmp)
	if row and (not cmp.row or math.abs(row - cmp.row) > 0.01) then
		cmp.row = row
		cmp.dirty = true
		kit.after(0, function()
			compare.update(tab)
			return nil
		end)
		return true
	end
	return false
end

-- --- Scrolling together ---------------------------------------------------------------------------------------

-- The two sides were scrolled (`scrolled`): the one that was not is sent to the place of the other. A
-- place this side was sent to is its echo and tells nothing; any other is the person's, and leads.
function compare.scrolled(event)
	local tab = docs.tab_of(S.active)
	local cmp = tab and tab.compare
	local doc = tab and tab.doc
	if not cmp or not doc or not compare.columns(tab) then
		return
	end
	for _, box in event.boxes or {} do
		local which, other = nil, nil
		if box.id == doc.id then
			which, other = "right", cmp.left
		elseif box.id == cmp.left then
			which, other = "left", doc.id
		end
		if which then
			local mine, echo = cmp.asked[which], false
			for index, place in mine do
				if math.abs(place.x - box.x) < 0.5 and math.abs(place.y - box.y) < 0.5 then
					for _ = 1, index do
						table.remove(mine, 1)
					end
					echo = true
					break
				end
			end
			if not echo then
				local theirs = cmp.asked[which == "left" and "right" or "left"]
				theirs[#theirs + 1] = { x = box.x, y = box.y }
				if #theirs > ECHOES then
					table.remove(theirs, 1)
				end
				layout.scroll_to(other, { x = box.x, y = box.y })
			end
		end
	end
end

-- --- Events of the left editor -----------------------------------------------------------------------------------

-- An event of an editor: whether it was a left-hand one's (the file's own are `docs`').
function compare.editor_event(event)
	local cmp = by_left[event.editor]
	if not cmp then
		return false
	end
	if cmp.syntax then
		cmp.syntax:handle(event)
	end
	local kind = event.type
	if kind == "editor_ready" then
		cmp.mounted = true
		deliver(cmp)
		local tab = compare.tab_of(cmp)
		if tab then
			compare.update(tab)
		end
	elseif kind == "edit" then
		if event.origin == "load" then
			cmp.loads -= 1
			compare.check_loaded(cmp)
		end
	elseif kind == "viewport" then
		cmp.first, cmp.last = event.first, event.last
	elseif kind == "editor_refused" and event.reason ~= "stale-version" and event.reason ~= "bad-pattern" then
		kit.toast("The editor did not do it: " .. event.reason, "warn")
	end
	return true
end

-- The lines in view of an editor of the comparison moved: whether the gaps that have an element are
-- others now, so that the page is drawn again.
function compare.moved(event)
	if event.type ~= "viewport" then
		return false
	end
	local cmp = by_left[event.editor]
	if not cmp then
		local doc = docs.by_id(event.editor)
		local tab = doc and docs.tab_of(doc.path)
		cmp = tab and tab.compare
	end
	if not cmp or not cmp.drawn then
		return false
	end
	local bucket = math.floor((event.first or 1) / (WINDOW / 3)) .. ":" .. event.editor
	if cmp.bucket == bucket then
		return false
	end
	cmp.bucket = bucket
	return true
end

-- --- What the editors are told ---------------------------------------------------------------------------------------

-- The gutter of either side: the numbers.
function compare.gutters()
	return { NUMBERS }
end

-- --- Elements -------------------------------------------------------------------------------------------------------------

local function hatch(width, height)
	local path = b.path()
	local at = -height
	while at < width do
		path:move_to(at, height)
		path:line_to(at + height, 0)
		at += 9
	end
	path:stroke(1, b.alpha("foreground", 0.09))
	return b.canvas({ path }):size(width, height)
end

-- The elements that stand in the gaps of one side, children of its editor's element: a pale ground,
-- hatched where it is small, for the gaps in or near view.
function compare.gaps(tab, which, editor)
	local cmp, doc = tab.compare, tab.doc
	if not (cmp and cmp.drawn and cmp.row and doc) then
		return editor
	end
	local list = cmp.gaps[which]
	if not list or #list == 0 then
		return editor
	end
	local first = which == "left" and cmp.first or doc.first or 1
	local last = which == "left" and cmp.last or doc.last or 0
	if last < first then
		last = first + 80
	end
	local width = math.ceil(S.width / 2) + 8
	local most = math.max(1, math.min(ZONE_ROWS, math.floor(ZONE_POINTS / cmp.row)))
	local hatched = 0
	for index, gap in list do
		if gap.line + 1 >= first - WINDOW and gap.line + 1 <= last + WINDOW then
			local left, part = gap.rows, 0
			while left > 0 do
				local take = math.min(left, most)
				part += 1
				local name = zone_name(which, index, part)
				local height = take * cmp.row
				local zone = b.div():id(name .. "/e"):zone(name):h(height):clip():bg(b.alpha("foreground", 0.025))
				if height <= 400 and hatched < HATCHED then
					hatched += 1
					zone:child(hatch(width, height))
				end
				editor:child(zone)
				left -= take
			end
		end
	end
	return editor
end

-- The commit and the number of the change, above the two sides, and what closes the comparison.
function compare.header(tab)
	local cmp = tab.compare
	local result = cmp.result
	local commit = result and result.commit
	local left = b.div():row():items_center():gap(8):grow(1):min_w(0):px(10):id("compare-commit")
		:child(b.text("Last Commit"):weight(650):no_shrink())
	if commit then
		left:child(b.text(string.sub(commit.id, 1, 7)):color("quiet"):no_shrink())
		left:tip(commit.subject)
	end
	local base = result and result.base
	if base and base.path ~= tab.path then
		left:child(b.text(text.basename(base.path)):color("quiet"):truncate():min_w(0))
	end
	local groups = cmp.groups
	local right = b.div():row():items_center():gap(6):grow(1):min_w(0):px(10)
	right:child(b.spacer())
	if #groups > 0 then
		local line = tab.doc and docs.where(tab.doc).line - 1 or 0
		local at = 0
		for _, group in groups do
			if group.line <= line then
				at += 1
			end
		end
		right:child(b.text(math.max(at, 1) .. " of " .. #groups):color("quiet"):no_shrink())
	else
		right:child(b.text("No changes"):color("quiet"):no_shrink())
	end
	right:child(kit.icon_button("compare-previous", "arrow-up", "Previous Change",
		{ keys = "alt+shift+f5", size = 24, disabled = #groups == 0 }))
	right:child(kit.icon_button("compare-next", "arrow-down", "Next Change",
		{ keys = "alt+f5", size = 24, disabled = #groups == 0 }))
	right:child(kit.icon_button("compare-close", "x", "Close", { keys = "escape", size = 24 }))
	return b.div():col():no_shrink()
		:child(b.div():row():items_center():h(34):no_shrink():bg("raised1"):child(left):child(kit.rule_y()):child(right))
		:child(kit.rule_x())
end

local WHY = {
	binary = "The file in the last commit is not text, so there are no lines to compare.",
	["too-large"] = "The file is too large to compare line by line.",
	["too-many"] = "There are too many changes to lay out side by side.",
	outside = "This file is not in the project's git repository, so there is no last commit to compare it with.",
}

-- What stands in the place of the two sides where there is nothing to lay out side by side: one
-- sentence, and the button that shows the file.
function compare.why(tab)
	local cmp = tab.compare
	local result = cmp.result
	local words = result.why == "failed" and ("The file could not be compared: " .. (result.message or "git did not answer."))
		or WHY[result.why] or "The file could not be compared."
	return b.div():col():grow(1):items_center():justify_center():gap(12):p(20)
		:child(kit.icon("file-diff", 34, "quiet"))
		:child(b.text(words):color("dim"):wrap():align("center"):max_w(480))
		:child(kit.button("compare-close", "Open File", { h = 26 }))
end

-- --- Going to a change ---------------------------------------------------------------------------------------------------

-- Goes to the change `step` (1 or -1) after the caret's, round the file.
function compare.go(step)
	local tab = docs.tab_of(S.active)
	local cmp, doc = tab and tab.compare, tab and tab.doc
	if not (cmp and doc and doc.loaded) then
		return
	end
	local groups = cmp.groups
	if #groups == 0 then
		kit.toast("No changes against the last commit in " .. text.basename(doc.path), "info")
		return
	end
	local line = docs.where(doc).line - 1
	local index
	if step > 0 then
		index = 1
		for at, group in groups do
			if group.line > line then
				index = at
				break
			end
		end
	else
		index = #groups
		for at = #groups, 1, -1 do
			if groups[at].line < line then
				index = at
				break
			end
		end
	end
	local at = start_of(doc.id, groups[index].line)
	act.focus_editor()
	local version = layout.editor_version(doc.id)
	kit.ask(doc.id, { select = { { anchor = at, head = at } }, version = version, main = 1 })
	kit.ask(doc.id, { reveal = at, version = version, align = "center" })
	kit.gather(true)
end

-- --- Presses -------------------------------------------------------------------------------------------------------------

function compare.press(id)
	if id == "compare-close" or id == "toggle-compare" then
		compare.toggle(S.active)
	elseif id == "compare-next" then
		compare.go(1)
	elseif id == "compare-previous" then
		compare.go(-1)
	else
		return false
	end
	return true
end

return compare
