-- The open files' texts: one editor of the terminal's for each open file, and the program kept in
-- step with it.
--
-- Each open file has an editor of its own, named by a number that stays with it through a rename
-- (`doc:3`), so that it keeps its undo history, its carets, its folds and where it was scrolled
-- while another file is shown: a hidden editor keeps its document, and loading another text into
-- one would forget what could be undone (plan, Phase 4a). The terminal holds the documents of a
-- page's editors to 64 MiB together; the page keeps what it opens under that, setting aside the
-- editors of files with nothing unsaved that were seen longest ago (`make_room`).
--
-- The program holds the text as well, and is the one that reads and writes the file. The page
-- tells it what the person changed (`edit`), as plain replacements made one after another, a moment
-- after the typing stops, cut so that no call carries more than 192 Ki characters; and every text
-- that comes from the program (a file opened, read again, a draft taken back) goes into the editor
-- as a load, the one change the page does not send back. A file's calls go one at a time, each
-- after the answer to the one before (`lane`): the program answers them in the page's order, and the
-- page knows the version each one is made on.

local kit = require("../lib/kit")
local text = require("../lib/text")
local languages = require("../lib/languages")

local b, S = kit.b, kit.S
local layout = b.layout

local docs = {}

-- The most characters of inserted text one `edit` carries (docs/calls.md).
local CALL_TEXT = 192 * 1024
-- How long after the last change the changes go to the program, seconds.
local SEND_DELAY = 0.35
-- What the page keeps its editors' documents under, bytes: the terminal's 64 MiB, less room for what
-- is typed into them.
local DOCUMENTS_BYTES = 56 * 1024 * 1024
-- How many characters of an editor's text are read at a time (a read answers 1 MiB at most).
local READ_CHARS = 128 * 1024

local count = 0
local by_id = {}

-- --- The tabs ---------------------------------------------------------------------------------------

function docs.tab_of(path)
	if path == nil then
		return nil
	end
	for _, tab in S.tabs do
		if tab.path == path then
			return tab
		end
	end
	return nil
end

function docs.by_id(id)
	return by_id[id]
end

-- The doc of the tab shown, once its text is in its editor.
function docs.active()
	local tab = docs.tab_of(S.active)
	local doc = tab and tab.doc
	if doc and doc.loaded then
		return doc
	end
	return nil
end

-- What the program says of a file now (`BufferInfo`), on its tab.
function docs.update_info(info)
	local tab = docs.tab_of(info.path)
	if tab then
		tab.info = info
	end
end

-- --- One file's calls, one at a time ---------------------------------------------------------------

local function run(doc)
	if doc.busy or #doc.lane == 0 or doc.closed then
		return
	end
	local op = table.remove(doc.lane, 1)
	doc.busy = true
	op(function()
		doc.busy = false
		run(doc)
	end)
end

-- Runs `op(next)` after the file's calls before it; `next()` lets the one after it go.
local function lane(doc, op)
	doc.lane[#doc.lane + 1] = op
	run(doc)
end

-- --- Into the editor ----------------------------------------------------------------------------------
--
-- A text from the program goes into the editor in pieces, each at most what one message held, and
-- one piece a wake for the whole page: a reply hands the editors 1 MiB of text at most.

local loads = {}
local loading = false

local function deliver()
	if loading then
		return
	end
	for index, load in loads do
		if load.doc.mounted and not load.doc.closed then
			loading = true
			table.remove(loads, index)
			kit.after(0, function()
				loading = false
				local doc = load.doc
				if not doc.closed then
					-- An empty text into an empty editor changes nothing, and is told of by nothing.
					local silent = load.text == "" and (layout.editor_length(doc.id) or 0) == 0
					kit.ask(doc.id, { load = load.text, more = load.more })
					doc.delivered += 1
					if not silent then
						doc.loads += 1
					end
					docs.check_arrived(doc)
				end
				if load.sent then
					load.sent()
				end
				deliver()
				return nil
			end)
			return
		elseif load.doc.closed then
			table.remove(loads, index)
			deliver()
			return
		end
	end
end

local function queue_load(doc, piece, more, sent)
	doc.bytes += #piece
	loads[#loads + 1] = { doc = doc, text = piece, more = more, sent = sent }
	deliver()
end

-- A text the program answered with (`Opened`), whole: what is read with `slice` after it where it
-- was too long for one message, each piece asked for once the one before is on its way to the
-- editor. `done()` once the last piece is queued; `failed(message)` where it could not be read.
local function take_text(doc, opened, done, failed)
	doc.version = opened.version
	doc.bytes = 0
	doc.loads, doc.loads_expected, doc.delivered = 0, nil, 0
	doc.loaded = false
	if not opened.partial then
		doc.loads_expected = 1
		queue_load(doc, opened.text or "", false)
		done(opened.text or "")
		return
	end
	local length = opened.partial.length
	local pieces, first_text = 0, nil
	local function from(at)
		kit.call("slice", { path = doc.path, version = doc.version, from = at }, function(piece, err)
			if err then
				failed(err.message)
				return nil
			end
			local stop = piece["end"]
			if type(stop) ~= "number" or stop <= at then
				failed(doc.path .. " could not be read to its end")
				return nil
			end
			pieces += 1
			first_text = first_text or piece.text
			local more = stop < length
			if not more then
				doc.loads_expected = pieces
			end
			queue_load(doc, piece.text, more, more and function()
				from(stop)
			end or nil)
			if not more then
				done(first_text)
			end
			return nil
		end)
	end
	from(0)
end

-- --- Opening -------------------------------------------------------------------------------------------

local function new_doc(path)
	count += 1
	local doc = {
		id = "doc:" .. count,
		path = path,
		-- The program's version of the text, as of its last answer.
		version = 0,
		-- The version of the editor's document the page was last told of.
		edited = nil,
		-- The editor is on the page (laid out at least once), and its text is all in it.
		mounted = false,
		in_tree = false,
		loaded = false,
		-- The pieces of a text from the program: how many there are (once the last is read), how many
		-- went to the editor, and how many of those it has not yet told of.
		loads_expected = nil,
		delivered = 0,
		loads = 0,
		bytes = 0,
		-- The file's calls, one at a time.
		lane = {},
		busy = false,
		-- What the person changed that the program has not been sent: replacements in order, each on
		-- the text the one before left ({ from, to, insert }, characters).
		pending = {},
		timer = nil,
		-- A call failed: the next sends the editor's whole text.
		broken = false,
		-- Where the carets are, and the lines in view.
		ranges = { { anchor = 0, head = 0 } },
		main = 1,
		first = 0,
		last = 0,
		lang = languages.plain,
		syntax = nil,
		-- Where the caret goes once the text is in: { line =, column =, select =, place =, keep = }.
		arrive = nil,
		closed = false,
	}
	by_id[doc.id] = doc
	return doc
end

local function set_language(doc, lang)
	local syntax = doc.syntax
	if syntax then
		kit.syntaxes[doc.id] = nil
		if doc.edited ~= nil then
			kit.ask(doc.id, {
				decorate = syntax.layer,
				version = doc.edited,
				from = 0,
				to = layout.editor_length(doc.id) or 0,
				decorations = {},
			})
		end
	end
	doc.lang = lang
	doc.syntax = nil
	if lang.syntax then
		doc.syntax = b.Syntax.new(lang.syntax, doc.id, { tab_width = S.settings and S.settings.tabSize or 4 })
		kit.syntaxes[doc.id] = doc.syntax
		if doc.loaded and doc.last > 0 then
			doc.syntax:handle({ type = "viewport", editor = doc.id, first = doc.first, last = doc.last })
		end
	end
end
docs.set_language = set_language

-- The text is all in the editor: the caret goes where it was asked to.
local function arrived(doc)
	doc.loaded = true
	local arrive = doc.arrive
	doc.arrive = nil
	local tab = docs.tab_of(doc.path)
	if tab and tab.status == "loading" then
		tab.status = "ready"
	end
	-- The file shown has the keyboard once its text is in, where the keys were the editor's.
	if doc.path == S.active and S.focus == "editor" and S.overlay == nil then
		kit.act.focus_editor()
	end
	if not arrive then
		return
	end
	local length = layout.editor_length(doc.id) or 0
	local lines = layout.editor_lines(doc.id) or 1
	local function clamp(at)
		return math.clamp(at, 0, length)
	end
	local anchor, head = nil, nil
	local center = nil
	if arrive.select or arrive.line then
		local line = math.clamp((arrive.select and arrive.select.line or arrive.line) - 1, 0, lines - 1)
		local start = layout.line_start(doc.id, line) or 0
		local stop = line + 1 < lines and (layout.line_start(doc.id, line + 1) - 1) or length
		if arrive.select then
			anchor = math.min(start + arrive.select.from, stop)
			head = math.min(start + arrive.select.to, stop)
		else
			anchor = math.min(start + math.max((arrive.column or 1) - 1, 0), stop)
			head = anchor
		end
		center = line
	elseif arrive.place then
		anchor, head = clamp(arrive.place.anchor), clamp(arrive.place.head)
	elseif arrive.keep then
		local ranges = {}
		for _, range in arrive.keep do
			ranges[#ranges + 1] = { anchor = clamp(range.anchor), head = clamp(range.head) }
		end
		kit.ask(doc.id, { select = ranges, version = doc.edited, main = math.min(arrive.main or 1, #ranges) })
		return
	end
	if anchor == nil then
		return
	end
	kit.ask(doc.id, { select = { { anchor = anchor, head = head } }, version = doc.edited, main = 1 })
	kit.ask(doc.id, { reveal = head, version = doc.edited })
	if center then
		docs.center(doc, center)
	end
end

-- Whether the program's text is all in the editor: every piece went, and the editor told of each.
function docs.check_arrived(doc)
	if doc.loads_expected and doc.delivered >= doc.loads_expected and doc.loads <= 0 then
		doc.loads_expected = nil
		doc.loads = 0
		arrived(doc)
	end
end

-- A loaded doc's caret goes where `doc.arrive` says, now.
function docs.arrive_now(doc)
	arrived(doc)
end

-- Scrolls the editor so that `line` is in the middle of what is in view, where the editor has
-- been laid out.
function docs.center(doc, line)
	local extent = layout.scroll_extent(doc.id)
	local lines = layout.editor_lines(doc.id) or 1
	if not extent or extent.content <= 0 then
		return
	end
	local row = extent.content / math.max(lines, 1)
	layout.scroll_to(doc.id, { y = math.max(line * row - extent.view / 2 + row / 2, 0) })
end

-- The documents the editors hold, bytes.
local function held()
	local sum = 0
	for _, doc in by_id do
		if not doc.closed then
			sum += doc.bytes
		end
	end
	return sum
end

-- Sets aside the editors of files with nothing unsaved, the one seen longest ago first, until a
-- file of `size` bytes fits; whether it does.
local function make_room(size)
	if held() + size <= DOCUMENTS_BYTES then
		return true
	end
	local order = {}
	for index = #S.recent, 1, -1 do
		order[#order + 1] = S.recent[index]
	end
	for _, path in order do
		local tab = docs.tab_of(path)
		local doc = tab and tab.doc
		if doc and path ~= S.active and doc.loaded and #doc.pending == 0 and not doc.busy
			and not (tab.info and tab.info.dirty) then
			docs.forget(doc)
			tab.doc = nil
			tab.status = "loading"
			if held() + size <= DOCUMENTS_BYTES then
				return true
			end
		end
	end
	return held() + size <= DOCUMENTS_BYTES
end

-- Opens the tab's file in an editor of its own: the program reads it, the page loads it.
-- `arrive` is where the caret goes once it is in.
function docs.open(tab, arrive, size)
	if not make_room(size or 0) then
		tab.status = "error"
		tab.error = "Too much is open: close a tab with a large file first."
		return
	end
	local doc = new_doc(tab.path)
	doc.arrive = arrive
	tab.doc = doc
	tab.status = "loading"
	lane(doc, function(next)
		S.busy += 1
		kit.call("open", { path = doc.path }, function(opened, err)
			S.busy -= 1
			if err or opened.reason then
				docs.forget(doc)
				tab.doc = nil
				if err then
					tab.status, tab.error = "error", err.message
				else
					tab.status, tab.unopenable = "unopenable", opened
				end
				next()
				return nil
			end
			local info = table.clone(opened)
			info.text, info.partial = nil, nil
			tab.info = info
			take_text(doc, opened, function(first)
				local line = string.match(first, "^[^\n]*") or ""
				set_language(doc, tab.language and languages.find(tab.language) or languages.detect(doc.path, line))
				next()
			end, function(message)
				tab.status, tab.error = "error", message
				docs.forget(doc)
				tab.doc = nil
				next()
			end)
			return nil
		end)
	end)
end

-- Reads the program's text into the editor again (the file changed on disk, a draft was taken, the
-- editor was not there for a load), the carets kept where they were where `keep`.
local function refill(doc, opened, keep)
	if keep then
		doc.arrive = { keep = doc.ranges, main = doc.main }
	end
	doc.pending = {}
	take_text(doc, opened, function() end, function(message)
		kit.toast(message, "bad")
	end)
end

-- Asks the program for the buffer's text again and loads it.
local function reload(doc, keep, done)
	lane(doc, function(next)
		kit.call("buffer", { path = doc.path, action = "text" }, function(opened, err)
			if err then
				kit.toast(err.message, "bad")
			else
				docs.update_info(opened)
				refill(doc, opened, keep)
			end
			next()
			if done then
				done(err == nil)
			end
			return nil
		end)
	end)
end
docs.reload = reload

-- --- From the editor to the program -------------------------------------------------------------------

-- A change of the person's, after the ones not yet sent: typing goes on one replacement.
local function push(doc, from, to, insert)
	local last = doc.pending[#doc.pending]
	if last and from == to and from == last[1] + text.len(last[3]) then
		last[3] ..= insert
		return
	end
	if last and insert == "" and from >= last[1] and to <= last[1] + text.len(last[3]) and to > from then
		local start, stop = from - last[1], to - last[1]
		last[3] = text.sub(last[3], 1, start) .. text.sub(last[3], stop + 1)
		return
	end
	doc.pending[#doc.pending + 1] = { from, to, insert }
end

-- The replacements in groups no call goes past the limit with; a long insertion is cut, its first
-- piece in place of the range, the rest each after the one before.
local function batches(edits)
	local pieces = {}
	for _, edit in edits do
		local length = text.len(edit[3])
		if length <= CALL_TEXT then
			pieces[#pieces + 1] = edit
		else
			local at = edit[1]
			for index, piece in text.pieces(edit[3], CALL_TEXT) do
				pieces[#pieces + 1] = index == 1 and { edit[1], edit[2], piece } or { at, at, piece }
				at += text.len(piece)
			end
		end
	end
	local groups, group, size = {}, {}, 0
	for _, edit in pieces do
		local weight = text.len(edit[3]) + 16
		if #group > 0 and size + weight > CALL_TEXT then
			groups[#groups + 1] = group
			group, size = {}, 0
		end
		group[#group + 1] = edit
		size += weight
	end
	if #group > 0 then
		groups[#groups + 1] = group
	end
	return groups
end

-- Sends `edits` one group a call, each on the version the one before came to.
local function send_all(doc, edits, done)
	local groups = batches(edits)
	local function send(index)
		if index > #groups then
			done(nil)
			return
		end
		kit.call("edit", { path = doc.path, base = doc.version, edits = groups[index] }, function(info, err)
			if err then
				done(err)
				return nil
			end
			doc.version = info.version
			docs.update_info(info)
			send(index + 1)
			return nil
		end)
	end
	send(1)
end

-- The program's text and the editor's differ: the editor's whole text takes the place of the
-- program's, read and sent a piece at a time.
local function resync(doc, done)
	kit.call("buffer", { path = doc.path, action = "text" }, function(opened, err)
		if err then
			done(err)
			return nil
		end
		doc.version = opened.version
		local length = opened.partial and opened.partial.length or text.len(opened.text or "")
		local total = layout.editor_length(doc.id) or 0
		local edits = {}
		local at = 0
		repeat
			local piece = layout.editor_text(doc.id, at, math.min(at + READ_CHARS, total)) or ""
			edits[#edits + 1] = at == 0 and { 0, length, piece } or { at, at, piece }
			at += READ_CHARS
		until at >= total
		-- What the person changed until now is in the text read.
		doc.pending = {}
		send_all(doc, edits, done)
		return nil
	end)
end

-- Sends what changed now; `done(ok)` once the program has it.
function docs.flush(doc, done)
	if doc.timer then
		doc.timer:cancel()
		doc.timer = nil
	end
	lane(doc, function(next)
		local function finish(err)
			if err then
				doc.broken = true
				kit.toast(
					"Could not send the changes to " .. text.basename(doc.path) .. ": " .. (err.message or err.code)
						.. ". They are kept here; saving tries again.",
					"bad",
					6
				)
			end
			next()
			if done then
				done(err == nil)
			end
		end
		if doc.broken then
			doc.broken = false
			resync(doc, finish)
			return
		end
		local edits = doc.pending
		doc.pending = {}
		if #edits == 0 or doc.closed then
			finish(nil)
			return
		end
		send_all(doc, edits, function(err)
			if err then
				doc.broken = false
				resync(doc, finish)
			else
				finish(nil)
			end
		end)
	end)
end

-- Sends every file's changes; `done()` once the program has them all.
function docs.flush_all(done)
	local waiting = 1
	local function one()
		waiting -= 1
		if waiting == 0 and done then
			done()
		end
	end
	for _, tab in S.tabs do
		if tab.doc and tab.doc.loaded then
			waiting += 1
			docs.flush(tab.doc, one)
		end
	end
	one()
end

-- Whether a file has changes the program has not been sent.
function docs.unsent(doc)
	return #doc.pending > 0 or doc.broken or doc.busy
end

-- --- What the editors tell ----------------------------------------------------------------------------

local function edited(doc, event)
	doc.edited = event.version
	if event.origin == "load" then
		doc.loads -= 1
		docs.check_arrived(doc)
		return
	end
	local changes = event.changes or {}
	if #changes == 0 then
		return
	end
	-- The changes are in the places of the text before them: made from the last to the first, each
	-- leaves the places of the ones before it where they were.
	for index = #changes, 1, -1 do
		local change = changes[index]
		push(doc, change.from, change.to, change.insert or "")
	end
	local tab = docs.tab_of(doc.path)
	if tab and tab.info and not tab.info.dirty then
		tab.info = table.clone(tab.info)
		tab.info.dirty = true
	end
	if doc.timer then
		doc.timer:cancel()
	end
	doc.timer = kit.after(SEND_DELAY, function()
		doc.timer = nil
		docs.flush(doc)
		return nil
	end)
end

-- An event of one of the editors: the doc's own bookkeeping; whether it was one.
function docs.handle(event)
	local doc = event.editor and by_id[event.editor]
	if not doc then
		return false
	end
	if doc.syntax then
		doc.syntax:handle(event)
	end
	local kind = event.type
	if kind == "edit" then
		edited(doc, event)
	elseif kind == "selection" then
		doc.ranges, doc.main = event.ranges, event.main
		doc.composing = event.composing
	elseif kind == "viewport" then
		doc.first, doc.last = event.first, event.last
	elseif kind == "editor_refused" then
		if event.reason == "no-editor" then
			-- The editor was not on the page yet: its text goes in again once it is.
			doc.mounted = false
			reload(doc, false)
		elseif event.reason == "too-large" then
			kit.toast(text.basename(doc.path) .. " is too large for what the editors may hold: close a tab first", "bad", 6)
		elseif event.reason ~= "stale-version" and event.reason ~= "bad-pattern" then
			kit.toast("The editor did not do it: " .. event.reason, "warn")
		end
	end
	return true
end

-- The page was laid out: an editor that is on the page now takes its text.
function docs.settle()
	local any = false
	for _, doc in by_id do
		if doc.in_tree and not doc.mounted and not doc.closed then
			doc.mounted = true
			any = true
		end
	end
	if any then
		deliver()
	end
	return any
end

-- --- Saving, closing, the program's words ---------------------------------------------------------------

-- Saves the file; asks before writing over one that changed on disk. `done(saved)`.
function docs.save(tab, force, done)
	local doc = tab.doc
	if not (doc and doc.loaded) then
		if done then
			done(false)
		end
		return
	end
	docs.flush(doc, function()
		lane(doc, function(next)
			S.busy += 1
			kit.call("save", { path = doc.path, version = doc.version, force = force or nil }, function(answer, err)
				S.busy -= 1
				next()
				if err then
					kit.toast("Could not save " .. text.basename(doc.path) .. ": " .. err.message, "bad", 6)
					if done then
						done(false)
					end
					return nil
				end
				docs.update_info(answer.info)
				if answer.saved then
					S.saved_at[doc.path] = S.now
					if done then
						done(true)
					end
					return nil
				end
				kit.act.ask({
					title = text.basename(doc.path) .. " changed on disk",
					message = "Another program wrote to it after textpen read it. Saving now writes over what it wrote.",
					actions = {
						{ id = "overwrite", label = "Overwrite", danger = true },
						{ id = "reload", label = "Reload from Disk" },
						{ id = "cancel", label = "Cancel", primary = true },
					},
				}, function(chosen)
					if chosen == "overwrite" then
						docs.save(tab, true, done)
					else
						if chosen == "reload" then
							docs.buffer(tab, "revert")
						end
						if done then
							done(false)
						end
					end
				end)
				return nil
			end)
		end)
	end)
end

-- A `buffer` action that answers with a text (`revert`, `restore`, `drop`) or with what the program
-- says of the file (`keep`, `lf`, `crlf`).
function docs.buffer(tab, action, done)
	local doc = tab.doc
	if not doc then
		return
	end
	docs.flush(doc, function()
		lane(doc, function(next)
			kit.call("buffer", { path = doc.path, action = action }, function(answer, err)
				next()
				if err then
					kit.toast(err.message, "bad")
				elseif answer.reason then
					kit.toast(answer.message, "bad")
				else
					if answer.text ~= nil or answer.partial then
						refill(doc, answer, true)
					end
					local info = table.clone(answer)
					info.text, info.partial = nil, nil
					docs.update_info(info)
				end
				if done then
					done(err == nil)
				end
				return nil
			end)
		end)
	end)
end

-- Tells the program the file is closed (`discard` its changes); `done(ok)`.
function docs.close(tab, discard, done)
	local doc = tab.doc
	local function closed()
		if doc then
			docs.forget(doc)
		end
		tab.doc = nil
		done(true)
	end
	if not (tab.info or doc) then
		closed()
		return
	end
	local function ask_close(next)
		kit.call("buffer", { path = tab.path, action = "close", discard = discard or nil }, function(_, err)
			if next then
				next()
			end
			if err then
				kit.toast(err.message, "bad")
				done(false)
			else
				closed()
			end
			return nil
		end)
	end
	if doc then
		docs.flush(doc, function()
			lane(doc, ask_close)
		end)
	else
		ask_close(nil)
	end
end

-- The doc is gone: its editor leaves the page, its language stops.
function docs.forget(doc)
	doc.closed = true
	if doc.timer then
		doc.timer:cancel()
	end
	kit.syntaxes[doc.id] = nil
	by_id[doc.id] = nil
end

-- A file was renamed (or a directory it is in): its doc follows, its editor stays.
function docs.renamed(doc, path)
	local before = doc.path
	doc.path = path
	if text.extension(before) ~= text.extension(path) or text.basename(before) ~= text.basename(path) then
		local first = layout.editor_text(doc.id, 0, math.min(layout.editor_length(doc.id) or 0, 400)) or ""
		local tab = docs.tab_of(path)
		if not (tab and tab.language) then
			set_language(doc, languages.detect(path, string.match(first, "^[^\n]*") or ""))
		end
	end
end

-- The program says a file changed on disk (`changes`): one with no changes of its own is read again;
-- one with changes is asked about on its tab.
function docs.changed(info)
	local tab = docs.tab_of(info.path)
	if not tab then
		return
	end
	local doc = tab.doc
	if doc and doc.loaded and not info.dirty and not info.conflict and info.version ~= doc.version
		and not docs.unsent(doc) then
		reload(doc, true)
		kit.toast(text.basename(info.path) .. " changed on disk and was read again", "info")
	end
	docs.update_info(info)
end

-- Where the caret is and which line is at the top, for the program to keep (`Place`).
function docs.place(doc)
	local range = doc.ranges[doc.main] or doc.ranges[1]
	if not range then
		return nil
	end
	return { anchor = range.anchor, head = range.head, top = doc.first }
end

-- The editors of the open files, for the page: the one shown, the others hidden with their
-- documents kept.
function docs.editors(options_of)
	local out = {}
	for _, doc in by_id do
		doc.in_tree = false
	end
	for _, tab in S.tabs do
		local doc = tab.doc
		if doc and not doc.closed then
			doc.in_tree = true
			out[#out + 1] = { tab = tab, doc = doc, element = b.editor(doc.id, options_of(tab, doc)) }
		end
	end
	return out
end

return docs
