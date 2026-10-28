-- The palette: one field for going anywhere. ⌘K finds a file (`name:12` at a line); `>` typed or ⌘K
-- again lists the commands, `:` goes to a line; ⌘E lists the open files by when they were seen.
-- ↑ ↓ (⌃N ⌃P) choose, ⏎ goes, Esc comes back.

local kit = require("../lib/kit")
local text = require("../lib/text")
local fuzzy = require("../lib/fuzzy")
local files = require("../lib/files")
local languages = require("../lib/languages")
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
	local paths, lower = {}, {}
	local function page(from)
		kit.call("files", from == 0 and {} or { from = from }, function(answer, err)
			if err then
				indexing = false
				return true
			end
			for _, path in answer.files do
				paths[#paths + 1] = path
				lower[#lower + 1] = string.lower(path)
			end
			if answer.next ~= nil and answer.next > from then
				page(answer.next)
			else
				indexing = false
				indexed_at = S.now
				S.file_index = { files = paths, lower = lower, truncated = answer.truncated }
				palette.refresh()
			end
			return true
		end)
	end
	page(0)
end

-- --- The lists ------------------------------------------------------------------------------------------

-- A line of the list: { id, icon = { name, color }, title = parts, detail = parts, badge, keys,
-- current, run }.
local function parts_of(words, positions, from, to)
	return fuzzy.marks(words, positions, from, to)
end

-- How many files one call scores: the rest are scored at the next wakes, the list filling in as they
-- are, so that typing in a project of many files never takes a call past its time.
local CHUNK = 1500

-- The files the quick open looks through, in its order: the ones opened lately, then the open ones,
-- then the rest of the project; made once for each palette and each reading of the project.
local function candidates()
	local open = {}
	for _, tab in S.tabs do
		open[tab.path] = true
	end
	local index = S.file_index or { files = {}, lower = {} }
	local in_index = {}
	for _, path in index.files do
		in_index[path] = true
	end
	local paths, lowers, seen = {}, {}, {}
	local function add(path, lower)
		if not seen[path] then
			seen[path] = true
			paths[#paths + 1] = path
			lowers[#lowers + 1] = lower or string.lower(path)
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
	for position, path in index.files do
		add(path, index.lower[position])
	end
	local rank = {}
	for position, path in S.recent do
		rank[path] = position
	end
	return { paths = paths, lowers = lowers, open = open, rank = rank, index = S.file_index }
end

local function file_item(path, positions, pool, place)
	local name_start = #path - #text.basename(path) + 1
	local kind = files.kind(path)
	local state = S.git and S.git.changes and S.git.changes[path]
	return {
		id = path,
		icon = { kind.icon, kind.color },
		title = parts_of(path, positions, name_start, #path),
		detail = name_start > 1 and parts_of(path, positions, 1, name_start - 2) or nil,
		badge = pool.open[path] and "open" or nil,
		git = state,
		run = function()
			act.open_file(path, { focus = true, line = place.line, column = place.column })
		end,
	}
end

-- The best of what a scan found so far, as lines of the list.
local function scan_items(scan)
	local best = table.clone(scan.scored)
	table.sort(best, function(a, c)
		if a.score ~= c.score then
			return a.score > c.score
		end
		return #scan.pool.paths[a.at] < #scan.pool.paths[c.at]
	end)
	local out = {}
	for position = 1, math.min(#best, LIMIT) do
		out[#out + 1] = file_item(scan.pool.paths[best[position].at], best[position].positions, scan.pool, scan.place)
	end
	return out
end

-- Scores the next chunk of a scan; goes on at the next wake while there is more.
local function scan_step(scan)
	local pool = scan.pool
	local list = scan.within
	local count = list and #list or #pool.paths
	local stop = math.min(scan.next + CHUNK - 1, count)
	for position = scan.next, stop do
		local at = list and list[position] or position
		local path = pool.paths[at]
		local found = fuzzy.path(scan.query, path, pool.lowers[at])
		if found then
			local rank = pool.rank[path]
			local bonus = rank and (12 - math.min(10, rank - 1)) or 0
			scan.scored[#scan.scored + 1] = { at = at, score = found.score + bonus, positions = found.positions }
			scan.matched[#scan.matched + 1] = at
		end
	end
	scan.next = stop + 1
	scan.done = scan.next > count
	if not scan.done then
		kit.after(0, function()
			local overlay = S.overlay
			if overlay and overlay.kind == "palette" and overlay.scan == scan then
				scan_step(scan)
				overlay.items = scan_items(scan)
				overlay.chosen = math.clamp(overlay.chosen, 1, math.max(#overlay.items, 1))
				palette.reveal()
			end
			return true
		end)
	end
end

local function file_items(raw)
	local place_query, line, column = string.match(raw, "^%s*(.-):(%d+):?(%d*)%s*$")
	local query = place_query or raw
	local place = { line = line and tonumber(line), column = column and column ~= "" and tonumber(column) or nil }
	local overlay = S.overlay
	if not overlay.pool or overlay.pool.index ~= S.file_index then
		overlay.pool = candidates()
		overlay.scan = nil
	end
	local pool = overlay.pool
	if string.match(query, "^%s*$") then
		overlay.scan = nil
		local out = {}
		for at = 1, math.min(#pool.paths, LIMIT) do
			out[#out + 1] = file_item(pool.paths[at], {}, pool, place)
		end
		return out
	end
	-- A query that goes on from the one before looks only through what that one found.
	local before = overlay.scan
	local within = nil
	if before and before.done and #query > #before.query and string.sub(query, 1, #before.query) == before.query then
		within = before.matched
	end
	local scan = { query = query, pool = pool, within = within, next = 1, scored = {}, matched = {}, done = false, place = place }
	overlay.scan = scan
	scan_step(scan)
	return scan_items(scan)
end

local function command_items(query)
	local scored = {}
	for _, command in commands.available() do
		local words = command.category .. ": " .. command.title
		local found = fuzzy.match(query, words)
		if found then
			scored[#scored + 1] = { command = command, found = found, words = words }
		end
	end
	if string.match(query, "^%s*$") then
		local function rank(id)
			return text.index_of(recent_commands, id) or 1000
		end
		table.sort(scored, function(a, c)
			local ra, rc = rank(a.command.id), rank(c.command.id)
			if ra ~= rc then
				return ra < rc
			end
			return false
		end)
	else
		table.sort(scored, function(a, c)
			return a.found.score > c.found.score
		end)
	end
	local out = {}
	for position = 1, math.min(#scored, LIMIT) do
		local entry = scored[position]
		local command = entry.command
		local split = #command.category + 2
		local category = parts_of(entry.words, entry.found.positions, 1, split)
		for _, part in category do
			part.quiet = true
		end
		local title = parts_of(entry.words, entry.found.positions, split + 1, #entry.words)
		for _, part in title do
			category[#category + 1] = part
		end
		out[#out + 1] = {
			id = command.id,
			title = category,
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
	for _, path in ordered do
		local found = fuzzy.path(query, path)
		if found then
			local tab = docs.tab_of(path)
			local name_start = #path - #text.basename(path) + 1
			local kind = files.kind(path)
			out[#out + 1] = {
				id = path,
				icon = { kind.icon, kind.color },
				title = parts_of(path, found.positions, name_start, #path),
				detail = name_start > 1 and { { text.dirname(path), false } } or nil,
				current = path == S.active,
				dirty = tab and tab.info and tab.info.dirty,
				run = function()
					act.open_file(path, { focus = true })
				end,
			}
		end
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
			title = { { "Go to line " .. line .. (column and (", column " .. column) or ""), false } },
			detail = { { "of " .. lines, false } },
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
	local scored = {}
	for _, lang in languages.all() do
		local found = fuzzy.match(query, lang.name)
		if found then
			scored[#scored + 1] = { lang = lang, found = found }
		end
	end
	table.sort(scored, function(a, c)
		if string.match(query, "^%s*$") then
			if a.lang == languages.plain or c.lang == languages.plain then
				return a.lang == languages.plain
			end
			return a.lang.name < c.lang.name
		end
		return a.found.score > c.found.score
	end)
	local out = {}
	for _, entry in scored do
		local lang = entry.lang
		out[#out + 1] = {
			id = lang.name,
			title = parts_of(lang.name, entry.found.positions),
			current = doc.lang == lang,
			run = function()
				local tab = docs.tab_of(S.active)
				if tab then
					tab.language = lang.name
				end
				docs.set_language(doc, lang)
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
				title = { { item.words, false } },
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
	local origin = nil
	local doc = docs.active()
	if mode == "line" and doc then
		origin = { doc = doc, y = layout.scroll_offset(doc.id) }
	end
	S.overlay = {
		kind = "palette",
		mode = mode,
		query = query or "",
		chosen = mode == "tabs" and 2 or 1,
		items = {},
		origin = origin,
	}
	if mode == "files" then
		act.file_index()
	end
	palette.refresh()
	S.field = FIELD
	kit.gather({ redraw = true, fill = { [FIELD] = query or "" }, keyboard = { field = FIELD } })
	palette.reveal()
end

local function close()
	S.overlay = nil
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

local function title_view(parts, current)
	local spans = {}
	for _, part in parts do
		local span = b.span(part[1])
		if part[2] then
			span:color("accent"):bold()
		elseif part.quiet then
			span:color("dim")
		end
		spans[#spans + 1] = span
	end
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
	local words = b.div():row():items_center():gap(8):grow(1):min_w(0):child(title_view(item.title, item.current))
	if item.detail then
		local detail = {}
		for _, part in item.detail do
			local span = b.span(part[1]):color(part[2] and "accent" or "quiet")
			detail[#detail + 1] = span
		end
		words:child(b.rich(detail):truncate():min_w(0):text_size(11.5))
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
		:child(b.div():text_field(FIELD):grow(1):h(30):child(b.text(mode.hint)))
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
		foot:child(hint("⌘K", overlay.mode == "files" and "commands" or "files"))
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
