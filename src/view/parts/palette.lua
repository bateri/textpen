-- The palette: one field for going anywhere. ⌘K finds a file (`name:12` at a line); `>` typed or ⌘K
-- again lists the commands, `:` goes to a line; ⌘E lists the open files by when they were seen.
-- ↑ ↓ (⌃N ⌃P) choose, ⏎ goes, Esc comes back.

local kit = require("../lib/kit")
local text = require("../lib/text")
local fuzzy = require("@pencere/fuzzy")
local files = require("../lib/files")
local language = require("../lib/language")
local docs = require("./docs")
local commands = require("./commands")

local b, S, act = kit.b, kit.S, kit.act
local layout = b.layout

local palette = {}

local FIELD = "palette-query"
local LIST = "palette-list"
local ROW = 30
local SHOWN = 10
local LIMIT = 120

local MODES = {
	files = { label = "Files", icon = "file-search", hint = "Go to file…  (> commands, : line)" },
	commands = { label = "Commands", icon = "command", hint = "Run a command…" },
	line = { label = "Line", icon = "hash", hint = "Line number, or line:column" },
	tabs = { label = "Open", icon = "stack-2", hint = "Switch to an open file…" },
	language = { label = "Language", icon = "command", hint = "Read this file as…" },
	indent = { label = "Indent", icon = "command", hint = "Indentation…" },
}

local recent_commands = {}

-- --- The project's files ---------------------------------------------------------------------------

local indexing = false
local indexed_at = -math.huge

-- Reads the project's files for the quick open, a message's worth at a time (again where the last
-- reading is older than a few seconds).
function act.file_index()
	if indexing or S.now - indexed_at < 10 then
		return
	end
	indexing = true
	local paths = {}
	-- The listing's pages are of the index its first page was taken from (its `generation`); where
	-- the program began another since, it is read again from the start, a few times at the most.
	local generation, tries = nil, 0
	local function page(from)
		kit.call("files", from == 0 and {} or { from = from, generation = generation }, function(answer, err)
			if err and err.code == "out-of-sync" and tries < 3 then
				tries += 1
				paths, generation = {}, nil
				page(0)
				return true
			end
			if err then
				indexing = false
				return true
			end
			generation = answer.generation
			for _, path in answer.files do
				paths[#paths + 1] = path
			end
			if answer.next ~= nil and answer.next > from then
				page(answer.next)
			else
				indexing = false
				indexed_at = S.now
				S.file_index = { files = paths, truncated = answer.truncated }
				palette.refresh()
			end
			return true
		end)
	end
	page(0)
end

-- --- The lists ------------------------------------------------------------------------------------------

-- The words of a line of the list as parts of a text of many looks: the characters at `positions`
-- (characters from 1) marked, `from` to `to` of `words` (all of it by default).
local function marked(words, positions, from, to)
	return fuzzy.spans(words, positions, nil, from, to)
end

local function plain(words)
	return { b.span(words) }
end

-- How many of the best files a search keeps: the ones opened lately are put before the rest of
-- these, and the first `LIMIT` of them are listed.
local KEPT = 400

-- The files the quick open looks through, in its order: the ones opened lately, then the open ones,
-- then the rest of the project; made once for each palette and each reading of the project.
local function candidates()
	local open = {}
	for _, tab in S.tabs do
		open[tab.path] = true
	end
	local index = S.file_index or { files = {} }
	local in_index = {}
	for _, path in index.files do
		in_index[path] = true
	end
	local paths, seen = {}, {}
	local function add(path)
		if not seen[path] then
			seen[path] = true
			paths[#paths + 1] = path
		end
	end
	for _, path in S.recent do
		if open[path] or in_index[path] then
			add(path)
		end
	end
	for _, tab in S.tabs do
		add(tab.path)
	end
	for _, path in index.files do
		add(path)
	end
	local rank = {}
	for position, path in S.recent do
		rank[path] = position
	end
	return { paths = paths, open = open, rank = rank, index = S.file_index }
end

-- A line of the list for a file; `positions` the characters the query is found at.
local function file_item(path, positions, pool, place)
	local total = text.len(path)
	local name_start = total - text.len(text.basename(path)) + 1
	local kind = files.kind(path)
	local state = S.git and S.git.changes and S.git.changes[path]
	return {
		id = path,
		icon = { kind.icon, kind.color },
		title = marked(path, positions, name_start, total),
		detail = name_start > 1 and marked(path, positions, 1, name_start - 2) or nil,
		badge = pool.open[path] and "open" or nil,
		git = state,
		run = function()
			act.open_file(path, { focus = true, line = place.line, column = place.column })
		end,
	}
end

-- The best of what a search found so far, as lines of the list: the files seen lately a little
-- above the same match in the rest.
local function search_items(search, pool, place)
	local best = {}
	for _, hit in search:results() do
		local rank = pool.rank[hit.item]
		best[#best + 1] = { hit = hit, score = hit.score + (rank and (12 - math.min(10, rank - 1)) or 0) }
	end
	table.sort(best, function(a, c)
		if a.score ~= c.score then
			return a.score > c.score
		end
		return a.hit.index < c.hit.index
	end)
	local out = {}
	for position = 1, math.min(#best, LIMIT) do
		out[#out + 1] = file_item(best[position].hit.item, best[position].hit.positions, pool, place)
	end
	return out
end

local function cancel_search()
	local overlay = S.overlay
	if overlay and overlay.kind == "palette" and overlay.search then
		overlay.search:cancel()
		overlay.search = nil
	end
end

local function file_items(raw)
	local place_query, line, column = string.match(raw, "^%s*(.-):(%d+):?(%d*)%s*$")
	local query = place_query or raw
	local place = { line = line and tonumber(line), column = column and column ~= "" and tonumber(column) or nil }
	local overlay = S.overlay
	if not overlay.pool or overlay.pool.index ~= S.file_index then
		cancel_search()
		overlay.pool = candidates()
	end
	local pool = overlay.pool
	if string.match(query, "^%s*$") then
		cancel_search()
		local out = {}
		for at = 1, math.min(#pool.paths, LIMIT) do
			out[#out + 1] = file_item(pool.paths[at], {}, pool, place)
		end
		return out
	end
	-- A query that goes on from the one before looks only through what that one found. The search
	-- goes through the files a step a frame, the list filling in as it does, so that typing in a
	-- project of tens of thousands never takes a call past its time.
	local before = overlay.search
	local search = before and before:narrow(query) or fuzzy.search(query, pool.paths, { path = true, limit = KEPT })
	overlay.search = search
	local sync = true
	search:run(function(running)
		local now = S.overlay
		if not (now and now.kind == "palette" and now.search == running) then
			return nil
		end
		now.items = search_items(running, pool, place)
		now.chosen = math.clamp(now.chosen, 1, math.max(#now.items, 1))
		palette.reveal()
		return not sync and kit.take(true) or nil
	end)
	sync = false
	return overlay.items
end

local function command_items(query)
	local found = fuzzy.filter(query, commands.available(), {
		text = function(command)
			return command.category .. ": " .. command.title
		end,
	})
	if string.match(query, "^%s*$") then
		local function rank(id)
			return text.index_of(recent_commands, id) or 1000
		end
		table.sort(found, function(a, c)
			local ra, rc = rank(a.item.id), rank(c.item.id)
			if ra ~= rc then
				return ra < rc
			end
			return a.index < c.index
		end)
	end
	local out = {}
	for position = 1, math.min(#found, LIMIT) do
		local hit = found[position]
		local command = hit.item
		local words = command.category .. ": " .. command.title
		local split = text.len(command.category) + 2
		out[#out + 1] = {
			id = command.id,
			lead = marked(words, hit.positions, 1, split),
			title = marked(words, hit.positions, split + 1, text.len(words)),
			keys = command.keys or command.shown,
			run = function()
				recent_commands = text.without(recent_commands, command.id)
				table.insert(recent_commands, 1, command.id)
				commands.run(command.id)
			end,
		}
	end
	return out
end

local function tab_items(query)
	local ordered, seen = {}, {}
	local function add(path)
		if path and docs.tab_of(path) and not seen[path] then
			seen[path] = true
			ordered[#ordered + 1] = path
		end
	end
	add(S.active)
	for _, path in S.recent do
		add(path)
	end
	for _, tab in S.tabs do
		add(tab.path)
	end
	local out = {}
	for _, hit in fuzzy.filter(query, ordered, { path = true }) do
		local path = hit.item
		local tab = docs.tab_of(path)
		local total = text.len(path)
		local name_start = total - text.len(text.basename(path)) + 1
		local kind = files.kind(path)
		out[#out + 1] = {
			id = path,
			icon = { kind.icon, kind.color },
			title = marked(path, hit.positions, name_start, total),
			detail = name_start > 1 and plain(text.dirname(path)) or nil,
			current = path == S.active,
			dirty = tab and tab.info and tab.info.dirty,
			run = function()
				act.open_file(path, { focus = true })
			end,
		}
	end
	return out
end

local function parse_line(query)
	local line, column = string.match(query, "^%s*(%d+)%s*[:,]?%s*(%d*)%s*$")
	if not line then
		return nil
	end
	return tonumber(line), column ~= "" and tonumber(column) or nil
end

local function line_items(query)
	local doc = docs.active()
	local line, column = parse_line(query)
	if not doc or not line then
		return {}
	end
	local lines = layout.editor_lines(doc.id) or 1
	line = math.clamp(line, 1, lines)
	return {
		{
			id = "line",
			title = plain("Go to line " .. line .. (column and (", column " .. column) or "")),
			detail = plain("of " .. lines),
			run = function()
				doc.arrive = { line = line, column = column }
				docs.arrive_now(doc)
				act.focus_editor()
			end,
		},
	}
end

local function language_items(query)
	local doc = docs.active()
	if not doc then
		return {}
	end
	local out = {}
	for _, hit in fuzzy.filter(query, language.names()) do
		local name = hit.item
		out[#out + 1] = {
			id = name,
			title = marked(name, hit.positions),
			current = doc.lang.name == name,
			run = function()
				local tab = docs.tab_of(S.active)
				if tab then
					tab.language = name
				end
				docs.set_language(doc, language.find(name))
				act.focus_editor()
			end,
		}
	end
	return out
end

local function indent_items(query)
	local settings = S.settings
	local items = {
		{ id = "spaces", words = "Indent with spaces", current = settings.insertSpaces, change = { insertSpaces = true } },
		{ id = "tabs", words = "Indent with tabs", current = not settings.insertSpaces, change = { insertSpaces = false } },
	}
	for _, size in { 2, 4, 8, 3, 1 } do
		items[#items + 1] = { id = "size-" .. size, words = "Tab size: " .. size, current = settings.tabSize == size, change = { tabSize = size } }
	end
	local out = {}
	local q = string.lower(text.trim(query))
	for _, item in items do
		if q == "" or string.find(string.lower(item.words), q, 1, true) then
			local change = item.change
			out[#out + 1] = {
				id = item.id,
				title = plain(item.words),
				current = item.current,
				run = function()
					act.change_settings(change)
				end,
			}
		end
	end
	return out
end

local LISTS = {
	files = file_items,
	commands = command_items,
	tabs = tab_items,
	line = line_items,
	language = language_items,
	indent = indent_items,
}

-- --- The palette ---------------------------------------------------------------------------------------

-- The items for what is typed, again.
function palette.refresh()
	local overlay = S.overlay
	if not (overlay and overlay.kind == "palette") then
		return
	end
	overlay.items = LISTS[overlay.mode](overlay.query)
	overlay.chosen = math.clamp(overlay.chosen, 1, math.max(#overlay.items, 1))
	kit.gather(true)
end

function act.open_palette(mode, query)
	cancel_search()
	local origin = nil
	local doc = docs.active()
	if mode == "line" and doc then
		origin = { doc = doc, y = layout.scroll_offset(doc.id) }
	end
	act.set_overlay({
		kind = "palette",
		mode = mode,
		query = query or "",
		chosen = mode == "tabs" and 2 or 1,
		items = {},
		origin = origin,
	})
	if mode == "files" then
		act.file_index()
	end
	palette.refresh()
	S.field = FIELD
	-- A query it is opened with is selected, so that what is typed takes its place.
	kit.gather({
		redraw = true,
		fill = { [FIELD] = { text = query or "", select = (query or "") ~= "" or nil } },
		keyboard = { field = FIELD },
	})
	palette.reveal()
end

local function close()
	cancel_search()
	act.set_overlay(nil)
	act.regain()
	kit.gather(true)
end

-- Esc: the palette goes, and Go to Line puts the view back where it was.
local function dismiss()
	local origin = S.overlay.origin
	if origin and origin.doc and not origin.doc.closed then
		layout.scroll_to(origin.doc.id, { y = origin.y })
	end
	close()
end

local function run_item(item)
	if not item then
		return
	end
	close()
	item.run()
end

-- Keeps the chosen line in view in the list.
function palette.reveal()
	local overlay = S.overlay
	local extent = layout.scroll_extent(LIST)
	if not (overlay and extent) then
		return
	end
	local rows = b.Rows.even(#overlay.items, ROW)
	local offset = layout.scroll_offset(LIST)
	local to = rows:reveal(overlay.chosen, offset, extent.view)
	if to ~= offset then
		layout.scroll_to(LIST, { y = to })
	end
end

local function title_view(spans, current)
	return b.rich(spans):truncate():min_w(0):weight(current and 600 or 400)
end

local function item_view(item, index, chosen)
	local id = "palette-item:" .. index
	local row = b.div()
		:press(id)
		:row()
		:items_center()
		:gap(9)
		:h(ROW)
		:px(10)
		:rounded(6)
		:bg(index == chosen and b.alpha("accent", 0.2) or (kit.hovered(id) and b.alpha("foreground", 0.05) or nil))
		:role("menu_item")
		:selected(index == chosen)
	if item.icon then
		row:child(kit.icon(item.icon[1], 15, item.icon[2]))
	end
	local words = b.div():row():items_center():gap(8):grow(1):min_w(0)
	if item.lead then
		-- The category of a command, before its title.
		words:child(b.div():row():items_center():min_w(0)
			:child(b.rich(item.lead):color("dim"):no_shrink())
			:child(title_view(item.title, item.current)))
	else
		words:child(title_view(item.title, item.current))
	end
	if item.detail then
		words:child(b.rich(item.detail):color("quiet"):truncate():min_w(0):text_size(11.5))
	end
	row:child(words)
	if item.badge then
		row:child(b.div():px(6):h(17):rounded(4):bg("raised3"):row():items_center()
			:child(b.text(item.badge):text_size(10.5):color("dim")))
	end
	if item.git then
		row:child(b.text(files.GIT_LETTER[item.git] or ""):weight(700):color(files.GIT_COLOR[item.git] or "dim"))
	end
	if item.dirty then
		row:child(b.div():size(7, 7):rounded(4):bg("foreground"))
	end
	if item.current then
		row:child(kit.icon("check", 14, "accent"))
	end
	if item.keys then
		row:child(b.text(kit.keys_of(item.keys)):color("quiet"):text_size(11.5))
	end
	return row
end

function palette.view(size)
	local overlay = S.overlay
	local mode = MODES[overlay.mode]
	local width = math.min(720, size.w - 24)
	local field = b.div()
		:row()
		:items_center()
		:gap(10)
		:h(44)
		:px(10)
		:child(b.div():row():items_center():gap(6):h(26):px(8):rounded(6):bg(b.alpha("accent", 0.14)):no_shrink()
			:child(kit.icon(mode.icon, 14, "accent"))
			:child(b.text(mode.label):weight(650):color("accent")))
		:child(b.div():text_field(FIELD):label(mode.label):grow(1):h(30):child(b.text(mode.hint)))
	local list = b.div():id(LIST):col():p(4):max_h(math.min(SHOWN * ROW + 8, math.max(size.h * 0.5, ROW * 3))):scroll()
	if #overlay.items == 0 then
		local words = "Nothing matches"
		if overlay.mode == "files" and S.file_index == nil then
			words = "Listing the project…"
		elseif overlay.mode == "line" then
			words = "Type a line number"
		end
		list:child(b.div():h(ROW):px(10):row():items_center():child(b.text(words):color("dim")))
	else
		for index, item in overlay.items do
			list:child(item_view(item, index, overlay.chosen))
		end
	end
	local foot = b.div():row():items_center():gap(14):h(30):px(12):color("quiet"):text_size(11.5)
	local function hint(keys, words)
		return b.div():row():items_center():gap(5):child(b.text(keys):color("dim")):child(b.text(words))
	end
	foot:child(hint("↑ ↓", "choose"))
	foot:child(hint("⏎", overlay.mode == "commands" and "run" or "go"))
	if overlay.mode == "files" or overlay.mode == "commands" then
		foot:child(hint(kit.keys_of("cmd+k"), overlay.mode == "files" and "commands" or "files"))
	end
	foot:child(hint("Esc", "close"))
	if S.file_index and S.file_index.truncated and overlay.mode == "files" then
		foot:child(b.spacer()):child(b.text("only part of the project is listed"):color("warning"))
	end
	local box = b.div()
		:press("palette")
		:col()
		:w(width)
		:rounded(10)
		:bg("raised2")
		:border(1, "border")
		:shadow(0, 18, 48, b.alpha("shadow", 0.5))
		:clip()
		:motion({ enter = { opacity = 0, dy = -6, seconds = 0.12 } })
		:id("palette-box")
		:child(field)
		:child(kit.rule_x())
		:child(list)
		:child(kit.rule_x())
		:child(foot)
	return b.div()
		:press("palette-scrim")
		:overlay()
		:inset(0, 0, 0, 0)
		:bg(b.alpha("shadow", 0.18))
		:col()
		:items_center()
		:pt(math.max(math.floor(size.h * 0.1), 8))
		:child(box)
end

-- --- What the person does ------------------------------------------------------------------------------

function palette.press(id)
	if id == "palette-scrim" then
		dismiss()
		return true
	end
	local index = tonumber(string.match(id, "^palette%-item:(%d+)$") or "")
	if index then
		run_item(S.overlay.items[index])
		return true
	end
	return id == "palette"
end

function palette.text(field, words)
	if field ~= FIELD then
		return false
	end
	local overlay = S.overlay
	if overlay.mode == "files" and string.sub(words, 1, 1) == ">" then
		act.open_palette("commands", text.trim(string.sub(words, 2)))
		return true
	elseif overlay.mode == "files" and string.sub(words, 1, 1) == ":" and docs.active() then
		act.open_palette("line", string.sub(words, 2))
		return true
	end
	overlay.query = words
	overlay.chosen = 1
	palette.refresh()
	if overlay.mode == "line" then
		local doc = docs.active()
		local line = parse_line(words)
		if doc and line then
			docs.center(doc, math.clamp(line, 1, layout.editor_lines(doc.id) or 1) - 1)
		end
	end
	palette.reveal()
	return true
end

function palette.key(key, mods)
	local overlay = S.overlay
	local count = #overlay.items
	local step = nil
	if key == "down" or (mods.control and key == "n") then
		step = 1
	elseif key == "up" or (mods.control and key == "p") then
		step = -1
	elseif key == "page_down" then
		step = SHOWN - 1
	elseif key == "page_up" then
		step = -(SHOWN - 1)
	end
	if step and count > 0 then
		overlay.chosen = math.clamp(overlay.chosen + step, 1, count)
		palette.reveal()
	elseif key == "enter" then
		run_item(overlay.items[overlay.chosen])
	elseif key == "escape" then
		dismiss()
	elseif key == "backspace" and overlay.query == "" and overlay.mode ~= "files" then
		-- Nothing left to take back: the files, where the commands (`>`) and the lines (`:`) came from.
		act.open_palette("files")
	end
	kit.gather(true)
	return true
end

-- A command while the palette is open: ⌘K goes between the files and the commands, ⌘E again goes
-- down the open files; the rest close it first.
function palette.command(id)
	local overlay = S.overlay
	if id == "files" then
		act.open_palette(overlay.mode == "files" and "commands" or "files")
		return true
	elseif id == "switch" and overlay.mode == "tabs" then
		overlay.chosen = overlay.chosen % math.max(#overlay.items, 1) + 1
		palette.reveal()
		kit.gather(true)
		return true
	elseif id == "commands" then
		act.open_palette("commands")
		return true
	end
	close()
	return false
end

-- What moves under the pointer: the line it is over is the one chosen.
function palette.hover(id)
	local index = tonumber(string.match(id or "", "^palette%-item:(%d+)$") or "")
	if index and S.overlay.chosen ~= index then
		S.overlay.chosen = index
		return true
	end
	return false
end

return palette
