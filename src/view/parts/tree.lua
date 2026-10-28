-- The project's files, as a tree: what git says of each, which are open, which have changes not
-- saved. Worked with the keys as in a terminal's file manager — j k h l and the arrows to move, ⏎ to
-- open, a to add, r to rename, d to delete, y to copy the path — or with the pointer.

local kit = require("../lib/kit")
local text = require("../lib/text")
local files = require("../lib/files")
local docs = require("./docs")

local b, S, SIZE, act = kit.b, kit.S, kit.SIZE, kit.act
local layout = b.layout

local tree = {}

tree.LIST = "tree"
local ROW = SIZE.row
local INDENT = 12

-- Never shown: the repository's own records and a Mac's folder notes.
local HIDDEN = { [".git"] = true, [".DS_Store"] = true }
-- Shown, but quieter: what tools put there.
local QUIET = {
	node_modules = true,
	dist = true,
	build = true,
	target = true,
	[".next"] = true,
	__pycache__ = true,
	[".venv"] = true,
	venv = true,
}

-- The rows as the tree shows them: each listed entry under its open directory.
function tree.rows()
	local out = {}
	local open = {}
	for _, dir in S.expanded do
		open[dir] = true
	end
	local function walk(dir, depth)
		local listing = S.listings[dir]
		if not listing then
			return
		end
		for _, entry in listing.entries do
			if not HIDDEN[entry.name] then
				local is_open = entry.kind == "dir" and open[entry.path] == true
				out[#out + 1] = { entry = entry, depth = depth, open = is_open, loading = is_open and not S.listings[entry.path] }
				if is_open then
					walk(entry.path, depth + 1)
				end
			end
		end
	end
	walk("", 0)
	return out
end

-- What git says of each directory: the strongest of what it says of the files in it.
local function dir_states()
	local states = {}
	local changes = S.git and S.git.changes or {}
	for path, state in changes do
		for _, dir in text.ancestors(path) do
			local known = states[dir]
			if not known or (files.GIT_RANK[state] or 0) > (files.GIT_RANK[known] or 0) then
				states[dir] = state
			end
		end
	end
	return states
end

local function dirty_paths()
	local out = {}
	for _, tab in S.tabs do
		if tab.info and tab.info.dirty then
			out[tab.path] = true
		end
	end
	return out
end

local function row_view(row, index, ctx)
	local entry = row.entry
	local path = entry.path
	local id = "row:" .. path
	local is_dir = entry.kind == "dir"
	local selected = S.tree_selection == path
	local focused = selected and S.focus == "sidebar" and S.sidebar == "files"
	local state = is_dir and ctx.dirs[path] or (ctx.git[path])
	local over = kit.hovered(id)
	local ground = nil
	if focused then
		ground = b.alpha("accent", 0.2)
	elseif selected or path == S.active then
		ground = b.alpha("foreground", 0.07)
	elseif over then
		ground = b.alpha("foreground", 0.045)
	end
	local quiet = QUIET[entry.name] or (string.sub(entry.name, 1, 1) == "." and not state)
	local color = quiet and "quiet" or "foreground"
	if state and files.GIT_COLOR[state] then
		color = files.GIT_COLOR[state]
	end
	local element = b.div()
		:press(id)
		:row()
		:items_center()
		:gap(6)
		:h(ROW)
		:pl(8 + row.depth * INDENT)
		:pr(10)
		:no_shrink()
		:bg(ground)
		:role("button")
		:label(entry.name)
		:expanded(is_dir and row.open)
		:selected(selected)
		:tip(path)
	for depth = 0, row.depth - 1 do
		element:child(b.div():absolute():inset(0, nil, 0, 14 + depth * INDENT):w(1):bg(b.alpha("border", 0.7)))
	end
	if is_dir then
		element:child(kit.icon(row.open and "chevron-down" or "chevron-right", 13, "quiet"))
		element:child(kit.icon(row.open and "folder-open" or "folder", 15, "accent"))
	else
		element:child(b.div():w(13):no_shrink())
		local kind = files.kind(path)
		element:child(kit.icon(kind.icon, 15, kind.color))
	end
	local name = entry.name .. (entry.link and " ↗" or "")
	element:child(b.text(name):color(color):weight(path == S.active and 650 or 400):truncate():grow(1):min_w(0))
	if row.loading then
		element:child(kit.icon("loader-2", 12, "quiet"))
	end
	if ctx.dirty[path] then
		element:child(b.div():size(7, 7):rounded(4):bg("foreground"):no_shrink())
	end
	if state and not is_dir then
		element:child(b.text(files.GIT_LETTER[state] or ""):color(files.GIT_COLOR[state] or "dim"):weight(700):text_size(11))
	elseif state then
		element:child(b.div():size(6, 6):rounded(3):bg(files.GIT_COLOR[state] or "dim"):no_shrink())
	end
	return element
end

function tree.view()
	local rows = tree.rows()
	tree.current = rows
	local error_words = S.listing_errors[""]
	if error_words then
		return b.div():p(12):child(b.text(error_words):color("error"):wrap())
	elseif not S.listings[""] then
		return b.div():row():gap(8):p(12):child(kit.icon("loader-2", 14, "quiet")):child(b.text("Reading the folder…"):color("dim"))
	elseif #rows == 0 then
		return b.div():col():gap(6):p(12)
			:child(b.text("This folder is empty."):color("dim"))
			:child(kit.button("tree-make-file", "Make a file", { h = 24 }))
	end
	local ctx = { dirs = dir_states(), git = S.git and S.git.changes or {}, dirty = dirty_paths() }
	return b.div()
		:id(tree.LIST)
		:col()
		:grow(1)
		:min_h(0)
		:scroll()
		:list(b.Rows.even(#rows, ROW):padded(0, 8), function(index)
			return row_view(rows[index], index, ctx)
		end)
end

-- --- What the tree does ------------------------------------------------------------------------------

local function index_of(path)
	for index, row in tree.current or tree.rows() do
		if row.entry.path == path then
			return index
		end
	end
	return nil
end

function tree.entry_of(path)
	for _, listing in S.listings do
		for _, entry in listing.entries do
			if entry.path == path then
				return entry
			end
		end
	end
	return nil
end

-- Keeps the chosen row in view in the list.
function tree.show(path)
	local index = index_of(path)
	if not index then
		return
	end
	local rows = b.Rows.even(#(tree.current or {}), ROW):padded(0, 8)
	local extent = layout.scroll_extent(tree.LIST)
	if not extent then
		return
	end
	local offset = layout.scroll_offset(tree.LIST)
	local to = rows:reveal(index, offset, extent.view)
	if to ~= offset then
		layout.scroll_to(tree.LIST, { y = to })
	end
end

local function choose(index)
	local rows = tree.current or tree.rows()
	local row = rows[math.clamp(index, 1, math.max(#rows, 1))]
	if row then
		S.tree_selection = row.entry.path
		tree.show(row.entry.path)
	end
end

-- A row opened: a directory folds or unfolds, a file comes up in the editor (the keyboard in it
-- where `focus`).
local function activate(row, focus)
	if row.entry.kind == "dir" then
		act.set_expanded(row.entry.path, not row.open)
	else
		act.open_file(row.entry.path, { focus = focus })
	end
end

-- The menu of a row (`entry`), or of the tree itself (nil).
function tree.menu(entry, at)
	local is_dir = entry and entry.kind == "dir"
	local dir = entry and (is_dir and entry.path or text.dirname(entry.path)) or ""
	local items = {
		{ "New File…", "a", function() act.create("file", dir) end },
		{ "New Folder…", "A", function() act.create("dir", dir) end },
	}
	if entry then
		items[#items + 1] = "-"
		if not is_dir then
			items[#items + 1] = { "Open", nil, function() act.open_file(entry.path, { focus = true }) end }
		end
		items[#items + 1] = { "Rename…", "r", function() act.rename(entry.path) end }
		items[#items + 1] = { "Delete…", "d", function() act.remove(entry.path, is_dir) end }
		items[#items + 1] = "-"
		items[#items + 1] = { "Copy Path", "y", function() act.copy_path(entry.path, false) end }
		items[#items + 1] = { "Copy Absolute Path", "Y", function() act.copy_path(entry.path, true) end }
	end
	act.menu(items, at)
end

-- A key while the tree has the keyboard: whether it was the tree's.
function tree.key(key, mods)
	if mods.command or mods.alt then
		return false
	end
	if mods.control and key ~= "n" and key ~= "p" then
		return false
	end
	local rows = tree.current or tree.rows()
	local index = index_of(S.tree_selection) or 0
	local row = rows[index]
	local page = math.max(math.floor(((layout.scroll_extent(tree.LIST) or { view = 240 }).view) / ROW) - 1, 1)
	if key == "down" or key == "j" or (mods.control and key == "n") then
		choose(index + 1)
	elseif key == "up" or key == "k" or (mods.control and key == "p") then
		choose(math.max(index - 1, 1))
	elseif key == "home" or key == "g" then
		choose(1)
	elseif key == "end" or key == "G" then
		choose(#rows)
	elseif key == "page_down" then
		choose(index + page)
	elseif key == "page_up" then
		choose(math.max(index - page, 1))
	elseif key == "right" or key == "l" then
		if row and row.entry.kind == "dir" then
			if not row.open then
				act.set_expanded(row.entry.path, true)
			elseif rows[index + 1] and rows[index + 1].depth == row.depth + 1 then
				choose(index + 1)
			end
		elseif row then
			act.open_file(row.entry.path, { focus = false })
		end
	elseif key == "left" or key == "h" then
		if row and row.entry.kind == "dir" and row.open then
			act.set_expanded(row.entry.path, false)
		elseif row then
			local parent = text.dirname(row.entry.path)
			if parent ~= "" then
				S.tree_selection = parent
				tree.show(parent)
			end
		end
	elseif key == "enter" or key == "o" then
		if row then
			activate(row, true)
		end
	elseif key == " " then
		if row then
			activate(row, false)
		end
	elseif key == "a" then
		act.create("file")
	elseif key == "A" then
		act.create("dir")
	elseif key == "r" or key == "f2" then
		if row then
			act.rename(row.entry.path)
		end
	elseif key == "d" or key == "delete" or key == "backspace" then
		if row then
			act.remove(row.entry.path, row.entry.kind == "dir")
		end
	elseif key == "y" or key == "Y" then
		if row then
			act.copy_path(row.entry.path, key == "Y")
		end
	elseif key == "R" then
		act.list_dir("")
		for _, dir in S.expanded do
			act.list_dir(dir)
		end
	elseif key == "/" then
		act.open_palette("files")
	elseif key == "." then
		if row then
			local bounds = layout.bounds("row:" .. row.entry.path)
			tree.menu(row.entry, bounds and { x = bounds.x + 40 + row.depth * INDENT, y = bounds.y + bounds.h } or { x = 60, y = 120 })
		end
	elseif key == "escape" then
		act.focus_editor()
	else
		return false
	end
	kit.gather(true)
	return true
end

-- A click on a row: it is chosen and the tree has the keys (r, d, . act on it); a file comes up in
-- the editor, which has the keys at a second click (and at once in a drawer, which then closes).
function tree.press(id, event)
	if id == "tree-make-file" then
		act.create("file", "")
		return true
	end
	local path = string.match(id, "^row:(.*)$")
	if not path then
		return false
	end
	local index = index_of(path)
	local row = index and (tree.current or {})[index]
	if not row then
		return true
	end
	S.tree_selection = path
	local to_editor = (event.count or 1) > 1 or S.layout == "narrow"
	if not to_editor then
		act.focus_sidebar("files")
	end
	if row.entry.kind == "dir" then
		if (event.count or 1) < 2 then
			activate(row, false)
		end
	else
		activate(row, to_editor)
	end
	return true
end

-- The right button on a row: its menu.
function tree.context(id, event)
	local path = string.match(id or "", "^row:(.*)$")
	if not path then
		return false
	end
	S.tree_selection = path
	tree.menu(tree.entry_of(path), { x = event.x, y = event.y })
	return true
end

return tree
