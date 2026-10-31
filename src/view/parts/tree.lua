-- The project's files, as a tree: what git says of each, which are open, which have changes not
-- saved. Worked with the keys as in a terminal's file manager — j k h l and the arrows to move, ⏎ to
-- open, a to add, r to rename, d to delete, y to copy the path — or with the pointer.

local kit = require("../lib/kit")
local text = require("../lib/text")
local files = require("../lib/files")
local docs = require("./docs")
local uploads = require("./uploads")

local b, S, SIZE, act = kit.b, kit.S, kit.SIZE, kit.act
local layout = b.layout

local tree = {}

tree.LIST = "tree"
local ROW = SIZE.row
local INDENT = 16

-- How wide the rows are where a name is wider than the tree (`tree.measure`); 0 where none is.
tree.width = 0
-- How wide the tree was made last, points.
local shown_width = 0

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
		local listed = {}
		for _, entry in listing.entries do
			listed[entry.path] = true
			-- A file on its way takes the place of the one it is replacing.
			local coming = uploads.of(entry.path)
			if not HIDDEN[entry.name] and not (coming and coming.state ~= "asking") then
				local is_open = entry.kind == "dir" and open[entry.path] == true
				out[#out + 1] = { entry = entry, depth = depth, open = is_open, loading = is_open and not S.listings[entry.path] }
				if is_open then
					walk(entry.path, depth + 1)
				end
			end
		end
		-- The files on their way into the directory, faint, after what is there; one whose name is
		-- taken is shown once the person has said what to do.
		for _, coming in uploads.in_dir(dir) do
			if not (coming.state == "asking" and listed[coming.path]) then
				local entry = { name = coming.name, path = coming.path, kind = "file", size = coming.size }
				out[#out + 1] = { entry = entry, depth = depth, upload = coming }
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

-- The row of a file on its way into the project: faint, with how far it has got at its end, and a
-- Cancel button under the pointer.
local function entry_name(row)
	return row.entry.name
end

local function upload_view(row, ctx)
	local up = row.upload
	local path = up.path
	local id = "row:" .. path
	local cancel = "upload-cancel:" .. up.id
	local retry = "upload-retry:" .. up.id
	local over = kit.hovered(id) or kit.hovered(cancel) or kit.hovered(retry)
	local selected = S.tree_selection == path
	local ground = nil
	if selected and S.focus == "sidebar" and S.sidebar == "files" then
		ground = b.alpha("accent", 0.2)
	elseif selected then
		ground = b.alpha("foreground", 0.07)
	elseif over then
		ground = b.alpha("foreground", 0.045)
	end
	local fade = up.state == "done" and 1 or 0.55
	local element = b.div()
		:press(id)
		:row()
		:h(ROW)
		:min_w(tree.width)
		:no_shrink()
		:bg(ground)
		:selected(selected)
		:tip(path)
	if up.state == "done" then
		element:role("button"):label(entry_name(row))
	else
		element:role("progress"):label(uploads.spoken(up)):value(uploads.words(up))
	end
	for depth = 0, row.depth - 1 do
		element:child(b.div():absolute():inset(0, nil, 0, 15 + depth * INDENT):w(1):bg(b.alpha("border", 0.7)))
	end
	-- The line is as wide as the tree and no wider: what is said at its end gives way to the name, and
	-- where it does not fit its last 22 points fade, the whole of it in the tip.
	local line = b.div()
		:id("rowline:" .. path)
		:row()
		:items_center()
		:gap(6)
		:h_full()
		:pl(8 + row.depth * INDENT)
		:pr(10)
		:w(ctx.width)
		:no_shrink()
		:clip()
	local kind = files.kind(path)
	line:child(kit.icon(kind.icon, 15, kind.color):opacity(fade))
	-- The name gives way first, with its last 22 points faded where it is cut; what is said at the end
	-- keeps the room it needs but for a little of the name.
	local ground = over and b.mix("raised1", "foreground", 0.045) or "raised1"
	local name = b.div():id("upname:" .. up.id):row():items_center():grow(1):min_w(0):clip():tip(up.name)
	name:child(b.div():id("upnamein:" .. up.id):no_shrink():child(b.text(up.name):color("foreground"):no_shrink():opacity(fade)))
	if S.upload_fade["name" .. up.id] then
		name:child(b.div():id("upnamefade:" .. up.id):absolute():inset(0, 0, 0, nil):w(22):bg_gradient(90, b.alpha(ground, 0), ground))
	end
	line:child(name)
	if up.state ~= "done" then
		local stalled = up.state == "stalled"
		-- The buttons keep their whole width at the end of the row; what is said before them is cut
		-- (its last 22 points faded) where the name, shrunk as far as it goes, leaves it no room.
		local buttons = {}
		if stalled then
			buttons[#buttons + 1] = kit.button(retry, "Retry", { h = 18, px = 8 })
		end
		if over then
			buttons[#buttons + 1] = kit.button(cancel, "Cancel", { h = 18, px = 8 })
		end
		local reserved = 0
		if #buttons > 0 then
			-- As wide as they were laid out; before they were, as wide as their words are.
			reserved = S.upload_buttons[up.id]
			if not reserved or reserved == 0 then
				reserved = (stalled and 52 or 0) + (over and 66 or 0) + 6 * (#buttons - 1)
			end
			reserved += 6
		end
		local room = math.max(ctx.width - (8 + row.depth * INDENT) - 15 - 6 - 10 - 40 - 6 - reserved, 30)
		local words = b.div():id("upend:" .. up.id):row():items_center():no_shrink():max_w(room):clip():tip(uploads.words(up))
		words:child(b.div():id("upin:" .. up.id):no_shrink():child(
			b.text(uploads.words(up)):color(stalled and "warning" or "dim"):text_size(11):no_shrink()))
		if S.upload_fade[up.id] then
			words:child(b.div():id("upfade:" .. up.id):absolute():inset(0, 0, 0, nil):w(22):bg_gradient(90, b.alpha(ground, 0), ground))
		end
		line:child(words)
		if #buttons > 0 then
			local box = b.div():id("upbtn:" .. up.id):row():items_center():gap(6):no_shrink()
			for _, button in buttons do
				box:child(button)
			end
			line:child(box)
		end
	end
	element:child(line)
	return element
end

local function row_view(row, index, ctx)
	if row.upload then
		return upload_view(row, ctx)
	end
	local entry = row.entry
	local path = entry.path
	local id = "row:" .. path
	local is_dir = entry.kind == "dir"
	local selected = S.tree_selection == path
	local focused = selected and S.focus == "sidebar" and S.sidebar == "files"
	local state = is_dir and ctx.dirs[path] or (ctx.git[path])
	local over = kit.hovered(id)
	local ground = nil
	if is_dir and uploads.dropping(path) then
		-- Files dragged over the page would go into this folder.
		ground = b.alpha("accent", 0.32)
	elseif focused then
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
	-- The row is as wide as the tree, or as the widest of its rows where one is wider (its ground
	-- goes all the way across wherever the tree is scrolled); its line is as wide as the tree, or as
	-- its own name where that is wider: what is said of the file stands at the tree's edge, or right
	-- after a name that goes past it.
	local element = b.div()
		:press(id)
		:row()
		:h(ROW)
		:min_w(tree.width)
		:no_shrink()
		:bg(ground)
		:role("button")
		:label(entry.name)
		:expanded(is_dir and row.open)
		:selected(selected)
		:tip(path)
	if is_dir and uploads.dropping(path) then
		element:border(1, "accent")
	end
	-- A guide under the middle of each ancestor's icon.
	for depth = 0, row.depth - 1 do
		element:child(b.div():absolute():inset(0, nil, 0, 15 + depth * INDENT):w(1):bg(b.alpha("border", 0.7)))
	end
	local line = b.div()
		:id("rowline:" .. path)
		:row()
		:items_center()
		:gap(6)
		:h_full()
		:pl(8 + row.depth * INDENT)
		:pr(10)
		:min_w(ctx.width)
		:no_shrink()
	-- A folder says whether it is open by its icon alone: no arrow beside it.
	if is_dir then
		line:child(kit.icon(row.open and "folder-open" or "folder", 15, "accent"))
	else
		local kind = files.kind(path)
		line:child(kit.icon(kind.icon, 15, kind.color))
	end
	-- The name is whole: a tree narrower than its names scrolls across.
	local name = entry.name .. (entry.link and " ↗" or "")
	line:child(b.text(name):color(color):weight(path == S.active and 650 or 400):no_shrink())
	line:child(b.spacer())
	if row.loading then
		line:child(kit.spinner(12, "quiet"))
	end
	if ctx.dirty[path] then
		line:child(b.div():size(7, 7):rounded(4):bg("foreground"):no_shrink())
	end
	if state and not is_dir then
		line:child(b.text(files.GIT_LETTER[state] or ""):color(files.GIT_COLOR[state] or "dim"):weight(700):text_size(11))
	elseif state then
		line:child(b.div():size(6, 6):rounded(3):bg(files.GIT_COLOR[state] or "dim"):no_shrink())
	end
	element:child(line)
	return element
end

-- While the sidebar's edge is dragged the page is made again at every move of the pointer, and the
-- pointer is held on the edge, so the tree cannot scroll: only the rows in view are made then, not
-- the screens more that the long list makes ahead of a scroll, which in a project with its folders
-- open are most of the page's work. The rows stand where the list has them and the box is as tall,
-- so the tree stays where it was scrolled. None before the tree is laid out.
local function rows_in_view(rows, ctx)
	local extent = layout.scroll_extent(tree.LIST)
	if not extent then
		return nil
	end
	local offset = layout.scroll_offset(tree.LIST)
	local first = math.max(math.floor(offset / ROW) + 1, 1)
	local last = math.min(math.ceil((offset + extent.view) / ROW), #rows)
	local box = b.div():id(tree.LIST):col():grow(1):min_h(0):scroll()
	box:child(b.div():h((first - 1) * ROW):no_shrink())
	for index = first, last do
		box:child(row_view(rows[index], index, ctx))
	end
	box:child(b.div():h((#rows - math.max(last, first - 1)) * ROW + 8):no_shrink())
	return box
end

-- The tree in a sidebar `width` points wide.
function tree.view(width)
	local rows = tree.rows()
	tree.current = rows
	local error_words = S.listing_errors[""]
	if error_words then
		return b.div():p(12):child(b.text(error_words):color("error"):wrap())
	elseif not S.listings[""] then
		return b.div():row():gap(8):p(12):child(kit.spinner(14, "quiet")):child(b.text("Reading the folder…"):color("dim"))
	elseif #rows == 0 then
		return b.div():col():gap(6):p(12)
			:child(b.text("This folder is empty."):color("dim"))
			:child(kit.button("tree-make-file", "Make a file", { h = 24 }))
	end
	shown_width = width
	local ctx = { dirs = dir_states(), git = S.git and S.git.changes or {}, dirty = dirty_paths(), width = width }
	if S.resizing then
		local box = rows_in_view(rows, ctx)
		if box then
			return box
		end
	end
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

-- How wide the rows are to be, from how wide their lines were laid out: as the widest, where a
-- line is wider than the tree, so that every row's ground goes as far as the tree scrolls; as the
-- tree where none is. Whether it changed, for the page to be made again with it. Not while the
-- sidebar's edge is dragged, which changes no name.
function tree.measure()
	if S.resizing then
		return false
	end
	local first, last = b.list_window(tree.LIST)
	local rows = tree.current
	if not (first and rows) then
		return false
	end
	local widest = 0
	for index = first, last do
		local row = rows[index]
		local line = row and layout.bounds("rowline:" .. row.entry.path)
		if line then
			widest = math.max(widest, line.w)
		end
	end
	-- What is said at the end of an upload's row fades where it does not fit.
	local faded = false
	local function measure_fade(key, box_id, content_id)
		local box, content = layout.bounds(box_id), layout.bounds(content_id)
		if box and content then
			local cut = content.w > box.w + 0.5
			if (S.upload_fade[key] or false) ~= cut then
				S.upload_fade[key] = cut
				faded = true
			end
		end
	end
	for _, up in S.uploads do
		local buttons = layout.bounds("upbtn:" .. up.id)
		local width = buttons and math.ceil(buttons.w) or 0
		if (S.upload_buttons[up.id] or 0) ~= width then
			S.upload_buttons[up.id] = width
			faded = true
		end
		measure_fade(up.id, "upend:" .. up.id, "upin:" .. up.id)
		measure_fade("name" .. up.id, "upname:" .. up.id, "upnamein:" .. up.id)
	end
	local width = widest > shown_width + 0.5 and math.ceil(widest) or 0
	if width == tree.width then
		return faded
	end
	tree.width = width
	return true
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
	local coming = uploads.of(path)
	if coming and coming.state ~= "asking" then
		return { name = coming.name, path = coming.path, kind = "file", size = coming.size, upload = coming }
	end
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
		{ "New Folder…", "shift+a", function() act.create("dir", dir) end },
	}
	if entry and entry.upload then
		items = { { "Cancel Upload", nil, function() uploads.cancel(entry.upload) end } }
		act.menu(items, at)
		return
	end
	-- A folder, and the tree itself (the project's own folder), take the person's files.
	if not entry or is_dir then
		items[#items + 1] = { "Upload Files", nil, function() uploads.choose(dir) end }
	end
	if entry then
		items[#items + 1] = "-"
		if not is_dir then
			items[#items + 1] = { "Open", nil, function() act.open_file(entry.path, { focus = true }) end }
			items[#items + 1] = { "Download", nil, function() uploads.save_as(entry.path) end }
		end
		items[#items + 1] = { "Rename…", "r", function() act.rename(entry.path) end }
		items[#items + 1] = { "Delete…", "d", function() act.remove(entry.path, is_dir) end }
		items[#items + 1] = "-"
		items[#items + 1] = { "Copy Path", "y", function() act.copy_path(entry.path, false) end }
		items[#items + 1] = { "Copy Absolute Path", "shift+y", function() act.copy_path(entry.path, true) end }
	end
	act.menu(items, at)
end

act.tree_entry = tree.entry_of

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
	if row and row.upload then
		-- A file on its way is not on the disk to rename, delete or copy the path of: Delete gives it
		-- up, and a picture is shown from the file the person gave.
		local up = row.upload
		if key == "d" or key == "delete" or key == "backspace" then
			uploads.cancel(up)
			return true
		elseif key == "enter" or key == "o" or key == " " or key == "right" or key == "l" then
			if up.image and up.state ~= "asking" then
				act.open_file(up.path, { focus = false })
			end
			return true
		elseif key == "r" or key == "f2" or key == "y" or key == "Y" or key == "." then
			return true
		end
	end
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
	if uploads.press(id) then
		return true
	end
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
	if row.upload then
		-- A picture on its way is shown from the file the person gave; no other file is open yet.
		act.focus_sidebar("files")
		if row.upload.image and row.upload.state ~= "asking" then
			act.open_file(path, { focus = false })
		end
		return true
	end
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
