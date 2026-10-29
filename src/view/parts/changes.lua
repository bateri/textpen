-- The lines of the file shown that differ from what git keeps of it (its index), as VS Code marks
-- them: in a column of the editor's gutter between the numbers and the buttons that fold, a bar along
-- the lines changed (blue) or added (green) and a triangle where lines were only taken away (red), the
-- same in the overview ruler down the editor's edge; a press on one, or ⌥F5 and ⇧⌥F5, opens a peek
-- under the change, in a gap the editor keeps open there: the lines as they were (on red) and as
-- they are (on green), with Revert Change, the change before and after, and Esc to close it.
--
-- What differs the program says (`diff`: the buffer's text against `git show :path`, so nothing of
-- the text is sent); the page asks again once the changes it made are with the program, and when
-- git or the file on disk changed. Markers and zone are decorations of the editor, so that they
-- stay with their lines while the person types, until the next answer.

local kit = require("../lib/kit")
local text = require("../lib/text")
local docs = require("./docs")

local b, S, act = kit.b, kit.S, kit.act
local layout = b.layout

local changes = {}

-- The layers of decorations, the column of the gutter, and the zone's name.
local MARKS = "git"
local PEEK = "peek"
local COLUMN = "changes"

-- What a change is drawn in: the terminal's blue, green and red (VS Code's modified, added and deleted).
local COLOR = { modified = "blue", added = "green", deleted = "red" }
-- How many rows of each side the peek shows, and how tall a row is, points.
local PEEK_ROWS = 9
local ROW = 18

-- --- What differs ----------------------------------------------------------------------------------------

local function enabled()
	return S.settings ~= nil and S.settings.diffDecorations == true
end

-- The marks and the peek of a file, drawn or taken off.
local function sync(doc)
	changes.draw(doc)
end

-- Whether a hunk is lines added, lines taken away, or lines taken away and put in their place.
local function kind_of(hunk)
	local old = #hunk.old
	if hunk.count == 0 then
		return "deleted"
	elseif old == 0 and not hunk.cut then
		return "added"
	end
	return "modified"
end

local function lines_of(count)
	return count == 1 and "1 line" or (count .. " lines")
end

-- What a hunk is, in words: lines added, lines removed, or (where it is both) the lines that were
-- there and the lines that are, so that "changed" is said only where the counts are the same.
local function words_of(hunk)
	local kind = kind_of(hunk)
	local old = #hunk.old
	local removed
	if hunk.cut then
		-- Some of the lines as they were are not here: what is said is at least how many.
		removed = (old == 0 and "Some lines" or (old .. "+ lines")) .. " removed"
	else
		removed = lines_of(old) .. " removed"
	end
	if kind == "deleted" then
		return removed
	elseif kind == "added" then
		return lines_of(hunk.count) .. " added"
	elseif not hunk.cut and old == hunk.count then
		return lines_of(hunk.count) .. " changed"
	end
	return removed .. ", " .. hunk.count .. " added"
end

-- The first and the last line of a hunk in the editor, where the lines taken away stand at the edge
-- between the one before and the one after.
local function last_line(hunk)
	if hunk.count > 0 then
		return hunk.line + hunk.count - 1
	end
	return math.max(hunk.line - 1, 0)
end

-- Where line `line` starts, in characters; the end of the text for a line the text has not.
local function start_of(doc, line)
	return layout.line_start(doc.id, line) or layout.editor_length(doc.id) or 0
end

-- The decorations of the file's changes, in the places of its text now.
local function marks_of(doc)
	local out = {}
	local lines = layout.editor_lines(doc.id) or 1
	local function bar(from, to, kind, index, words)
		out[#out + 1] = {
			from = start_of(doc, from), to = start_of(doc, to), gutter = COLOR[kind], column = COLUMN,
			shape = "bar", id = "hunk:" .. index, label = words,
		}
		out[#out + 1] = { from = start_of(doc, from), to = start_of(doc, to), ruler = COLOR[kind], lane = "left" }
	end
	local function triangle(line, below, index, words)
		out[#out + 1] = {
			from = start_of(doc, line), to = start_of(doc, line), gutter = COLOR.deleted, column = COLUMN,
			shape = below and "below" or "above", id = "hunk:" .. index, label = words,
		}
		out[#out + 1] = { from = start_of(doc, line), to = start_of(doc, line), ruler = COLOR.deleted, lane = "left" }
	end
	-- VS Code's quick diff: lines only added are green, lines only taken away are a red triangle at
	-- the edge where they were, and lines taken away and put in their place (whatever the counts) are
	-- the lines now, blue, with no triangle.
	for index, hunk in doc.hunks or {} do
		local words = words_of(hunk) .. ": peek the lines as they were"
		local kind = kind_of(hunk)
		if kind == "deleted" then
			-- Lines taken away stand at the edge above the line after them, or below the last line.
			if hunk.line >= lines then
				triangle(lines - 1, true, index, words)
			else
				triangle(hunk.line, false, index, words)
			end
		else
			bar(hunk.line, hunk.line + hunk.count, kind, index, words)
		end
	end
	return out
end

-- The gap the peek is shown in: after the last line of the change, as tall as the peek's element.
local function peek_marks(doc)
	local peek = S.peek
	local hunk = peek and peek.path == doc.path and doc.hunks and doc.hunks[peek.index]
	if not hunk then
		return {}
	end
	local at = start_of(doc, last_line(hunk))
	return { { from = at, to = at, zone = PEEK, height = changes.peek_height(hunk) } }
end

-- Draws what differs (and the peek) in the editor of `doc`: the marks as the program last said, in
-- the places of the text the editor has now.
function changes.draw(doc)
	if doc.closed or not doc.mounted or doc.edited == nil then
		return
	end
	local version = layout.editor_version(doc.id)
	local length = layout.editor_length(doc.id)
	if version == nil or length == nil then
		return
	end
	local shown = enabled() and doc.hunks ~= nil
	-- While what was typed is on its way to the program the marks stay as the editor carries them
	-- over the typing; the next answer draws them again.
	if shown and docs.unsent(doc) then
		shown = nil
	end
	if shown ~= nil and (shown or doc.drawn) then
		kit.ask(doc.id, {
			decorate = MARKS, version = version, from = 0, to = length,
			decorations = shown and marks_of(doc) or {},
		})
		doc.drawn = shown
	end
	local zone = peek_marks(doc)
	if #zone > 0 or doc.zoned then
		kit.ask(doc.id, { decorate = PEEK, version = version, from = 0, to = length, decorations = zone })
		doc.zoned = #zone > 0
	end
end

-- Asks the program what differs in `doc`, and draws it.
function changes.refresh(doc)
	if doc.closed or not doc.loaded then
		return
	end
	if not enabled() then
		doc.hunks = nil
		sync(doc)
		return
	end
	if doc.diffing then
		doc.diff_again = true
		return
	end
	if docs.unsent(doc) then
		-- What was typed goes first; its answer brings the question again.
		return
	end
	doc.diffing = true
	kit.call("diff", { path = doc.path }, function(result, err)
		doc.diffing = false
		if doc.closed then
			return nil
		end
		if err or result == nil then
			doc.hunks = nil
		elseif docs.unsent(doc) or result.version ~= doc.version then
			-- The text moved on while git was asked: what it said is of an older one.
			doc.diff_again = true
		else
			doc.hunks = result.hunks
			doc.hunks_more = result.more == true
		end
		if S.peek and S.peek.path == doc.path and (doc.hunks == nil or doc.hunks[S.peek.index] == nil) then
			S.peek = nil
		end
		sync(doc)
		if doc.diff_again then
			doc.diff_again = false
			kit.after(0.15, function()
				changes.refresh(doc)
				return nil
			end)
		end
		return nil
	end)
end

-- Every open file again: git changed, or the setting did.
function changes.refresh_all()
	for _, tab in S.tabs do
		if tab.doc and tab.doc.loaded then
			changes.refresh(tab.doc)
		end
	end
end

-- The text of a file is new (its first, or one the program changed or took in): look at it again.
docs.on_text = function(doc)
	changes.refresh(doc)
end

-- --- What the editor is told -----------------------------------------------------------------------------

-- The editor's gutter: the numbers, a column for the changes, the buttons that fold. A file that is
-- not marked has no column.
function changes.gutters()
	if not enabled() then
		return { "numbers", "folds" }
	end
	return { "numbers", { name = COLUMN, width = 8 }, "folds" }
end

-- --- The peek --------------------------------------------------------------------------------------------

function changes.peek_height(hunk)
	local old = math.min(#hunk.old, PEEK_ROWS) + ((#hunk.old > PEEK_ROWS or hunk.cut) and 1 or 0)
	local new = math.min(hunk.count, PEEK_ROWS) + (hunk.count > PEEK_ROWS and 1 or 0)
	return 36 + (old + new) * ROW + 12
end

-- The hunk the peek shows, and its file.
local function peeked()
	local peek = S.peek
	if not peek then
		return nil
	end
	local tab = docs.tab_of(peek.path)
	local doc = tab and tab.doc
	local hunk = doc and doc.hunks and doc.hunks[peek.index]
	if not hunk then
		return nil
	end
	return doc, hunk, peek.index
end

-- The lines of the current text of the change, from the editor.
local function new_lines(doc, hunk)
	if hunk.count == 0 then
		return {}
	end
	local from = start_of(doc, hunk.line)
	local to = start_of(doc, hunk.line + hunk.count)
	local length = layout.editor_length(doc.id) or 0
	local body = layout.editor_text(doc.id, from, math.min(to, length)) or ""
	body = string.gsub(body, "\n$", "")
	return string.split(body, "\n")
end

local function line_row(sign, line, ground, ink)
	return b.div():row():items_center():h(ROW):px(8):gap(8):bg(ground):no_shrink()
		:child(b.text(sign):color(ink):w(10):no_shrink())
		:child(b.text(line == "" and " " or line):color("foreground"):grow(1):min_w(0):truncate())
end

-- The peek's element for the editor of `doc`, a child of the editor's: in its zone, as wide as the
-- room and as tall as the zone's gap; nil where this file has none.
function changes.peek(doc)
	local shown, hunk, index = peeked()
	if shown ~= doc then
		return nil
	end
	local height = changes.peek_height(hunk)
	local total = #doc.hunks
	local head = b.div():row():items_center():gap(6):h(34):px(10):no_shrink()
		:child(kit.icon("file-diff", 15, "dim"))
		:child(b.text(words_of(hunk)):weight(650))
		:child(b.text(index .. " of " .. total):color("quiet"))
		:child(b.spacer())
		:child(kit.icon_button("peek-previous", "arrow-up", "Previous Change", { keys = "alt+shift+f5", size = 24 }))
		:child(kit.icon_button("peek-next", "arrow-down", "Next Change", { keys = "alt+f5", size = 24 }))
		:child(kit.button("peek-revert", "Revert Change", { tone = "danger", h = 24, keys = "ctrl+shift+backspace" }))
		:child(kit.icon_button("peek-close", "x", "Close", { keys = "escape", size = 24 }))
	local body = b.div():col():grow(1):min_h(0)
	local old, new = hunk.old, new_lines(doc, hunk)
	for row = 1, math.min(#old, PEEK_ROWS) do
		body:child(line_row("-", old[row], b.alpha("red", 0.16), "red"))
	end
	if #old > PEEK_ROWS or hunk.cut then
		body:child(b.div():h(ROW):px(26):row():items_center():child(b.text("…"):color("quiet")))
	end
	for row = 1, math.min(#new, PEEK_ROWS) do
		body:child(line_row("+", new[row], b.alpha("green", 0.16), "green"))
	end
	if #new > PEEK_ROWS then
		body:child(b.div():h(ROW):px(26):row():items_center():child(b.text("…"):color("quiet")))
	end
	return b.div():id("peek"):zone(PEEK):h(height):col():bg("raised2"):border(1, "border")
		:child(head):child(kit.rule_x()):child(body)
end

-- Opens the peek on hunk `index` of `doc`: a gap after its last line, the lines in it.
function changes.open(doc, index)
	if not (doc.hunks and doc.hunks[index]) then
		return
	end
	S.peek = { path = doc.path, index = index }
	changes.draw(doc)
	kit.gather(true)
end

function changes.close()
	local peek = S.peek
	if not peek then
		return
	end
	S.peek = nil
	local tab = docs.tab_of(peek.path)
	if tab and tab.doc then
		changes.draw(tab.doc)
	end
	kit.gather(true)
end

-- --- Going to a change, and taking it back ----------------------------------------------------------------

local function caret_line(doc)
	local range = doc.ranges[doc.main] or doc.ranges[1]
	return layout.line_of(doc.id, range and range.head or 0) or 0
end

-- The hunk the line `line` of `doc` is in (the lines of it, or the edge a deletion is at): its index.
function changes.hunk_of_line(doc, line)
	for index, hunk in doc.hunks or {} do
		if hunk.count > 0 and line >= hunk.line and line < hunk.line + hunk.count then
			return index
		elseif hunk.count == 0 and (line == hunk.line or line == hunk.line - 1) then
			return index
		end
	end
	return nil
end

-- The hunk the caret is in: its file and index.
function changes.at_caret()
	local doc = docs.active()
	if not (doc and doc.hunks) then
		return nil
	end
	local index = changes.hunk_of_line(doc, caret_line(doc))
	if index then
		return doc, index
	end
	return nil
end

-- Goes to the change `step` (1 or -1) after the one the peek is on, or the caret's; round the file.
function changes.go(step)
	local doc = docs.active()
	if not (doc and doc.hunks and #doc.hunks > 0) then
		kit.toast("No changes against git in " .. (doc and text.basename(doc.path) or "this file"), "info")
		return
	end
	local hunks = doc.hunks
	local index
	if S.peek and S.peek.path == doc.path then
		index = (S.peek.index - 1 + step) % #hunks + 1
	else
		local line = caret_line(doc)
		if step > 0 then
			index = 1
			for at, hunk in hunks do
				if hunk.line > line then
					index = at
					break
				end
			end
		else
			index = #hunks
			for at = #hunks, 1, -1 do
				if hunks[at].line < line then
					index = at
					break
				end
			end
		end
	end
	local hunk = hunks[index]
	act.focus_editor()
	local at = start_of(doc, hunk.line)
	kit.ask(doc.id, { select = { { anchor = at, head = at } }, version = layout.editor_version(doc.id), main = 1 })
	kit.ask(doc.id, { reveal = start_of(doc, last_line(hunk)), version = layout.editor_version(doc.id), align = "center" })
	changes.open(doc, index)
end

-- Puts the lines as they were in place of the change: an edit as any other, which can be undone.
function changes.revert(doc, index)
	local hunk = doc.hunks and doc.hunks[index]
	if not hunk or docs.unsent(doc) then
		return
	end
	if hunk.cut then
		kit.toast("Only some of the lines as they were are kept here: take this change back with git", "warn")
		return
	end
	local length = layout.editor_length(doc.id) or 0
	local broken = length > 0 and layout.editor_text(doc.id, length - 1, length) ~= "\n"
	local from = start_of(doc, hunk.line)
	local to = start_of(doc, hunk.line + hunk.count)
	local old = table.concat(hunk.old, "\n")
	local insert = ""
	if #hunk.old > 0 then
		if hunk.count == 0 and from >= length and broken then
			-- Lines taken away from the end of a text that has no break at its end: one comes first.
			insert = "\n" .. old
		elseif to >= length and broken then
			-- The change's lines are the last, with no break after them: neither do theirs have.
			insert = old
		else
			insert = old .. "\n"
		end
	end
	S.peek = nil
	kit.ask(doc.id, {
		change = { { from = from, to = to, insert = insert } },
		base = layout.editor_version(doc.id),
		tag = "revert-change",
	})
	act.focus_editor()
	changes.draw(doc)
	kit.gather(true)
end

-- --- Presses -------------------------------------------------------------------------------------------------

-- A press on a button of the peek.
function changes.press(id)
	if id == "peek-close" then
		changes.close()
	elseif id == "peek-next" then
		changes.go(1)
	elseif id == "peek-previous" then
		changes.go(-1)
	elseif id == "peek-revert" then
		local doc, _, index = peeked()
		if doc then
			changes.revert(doc, index)
		end
	else
		return false
	end
	return true
end

-- The pointer in the gutter's column of changes (an event of the editor `doc`): a press on a change
-- opens its peek, or closes it where it is the one shown.
function changes.gutter(doc, event)
	if event.column ~= COLUMN or event.kind ~= "press" or event.button ~= "left" or not event.marker then
		return
	end
	local index = tonumber(string.match(event.marker.id or "", "^hunk:(%d+)$"))
	if not index then
		return
	end
	if S.peek and S.peek.path == doc.path and S.peek.index == index then
		changes.close()
	else
		changes.open(doc, index)
	end
end

docs.on_gutter = changes.gutter

-- Whether a peek is open: Esc is the page's key then.
function changes.peeking()
	return S.peek ~= nil and peeked() ~= nil
end

return changes
