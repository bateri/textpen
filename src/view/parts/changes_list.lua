-- The files git says are changed, as a panel of the sidebar beside the files and the search: those
-- with changes (`Changes`) and those git does not keep (`Unversioned Files`), each a row with its
-- icon, its name, the folder it is in and the letter git says of it. A press on a row, or a key that
-- moves to it, shows the file beside the last commit (`compare`); the keys stay in the panel, so that
-- it is gone through file after file, and ⏎ goes into the editor.
--
-- The button at the head of the panel, or T, stands the files under their folders instead, as the tree
-- of the project has them: a folder folds as it does there, and says the strongest of what git says of
-- what is in it.
--
-- What it lists is what git said (`S.git.changes`, asked every two seconds by the topic), so it follows
-- the repository with nothing of its own to keep. Only the rows in view are made, as the tree's are.

local kit = require("../lib/kit")
local text = require("../lib/text")
local files = require("../lib/files")
local docs = require("./docs")
local compare = require("./compare")

local b, S, SIZE, act = kit.b, kit.S, kit.SIZE, kit.act
local layout = b.layout

local changes_list = {}

changes_list.LIST = "changes-list"
local ROW = SIZE.row

-- The rows as the panel shows them: a heading and the files under it, for each group that has any.
-- Kept while what git said is the same, the panel stands as it did and no folder folded. A row of a file
-- is `{ path =, state = }`, with `depth =` under its folders; a row of a folder `{ key =, dir =, name =,
-- depth =, open =, state = }`. The same folder can be in both groups, each with its own row: `key` is
-- which.
local cached = nil

local function by_lower(a, c)
	return text.lower(a) < text.lower(c)
end

-- The rows of `entries` (in the order of their paths) under their folders, folders first as the tree
-- has them, after the rows that are there.
local function tree_rows(group, entries, rows)
	local root = { dirs = {}, entries = {} }
	for _, entry in entries do
		local node, at = root, nil
		local parts = string.split(entry.path, "/")
		for index = 1, #parts - 1 do
			local name = parts[index]
			at = at and (at .. "/" .. name) or name
			local dir = node.dirs[name]
			if not dir then
				dir = { name = name, path = at, dirs = {}, entries = {} }
				node.dirs[name] = dir
			end
			if (files.GIT_RANK[entry.state] or 0) > (dir.state and files.GIT_RANK[dir.state] or 0) then
				dir.state = entry.state
			end
			node = dir
		end
		node.entries[#node.entries + 1] = entry
	end
	local function walk(node, depth)
		local names = {}
		for name in node.dirs do
			names[#names + 1] = name
		end
		table.sort(names, by_lower)
		for _, name in names do
			local dir = node.dirs[name]
			local key = "dir:" .. group .. ":" .. dir.path
			local open = not S.changes_folded[key]
			rows[#rows + 1] = { key = key, dir = dir.path, name = name, depth = depth, open = open, state = dir.state }
			if open then
				walk(dir, depth + 1)
			end
		end
		for _, entry in node.entries do
			rows[#rows + 1] = { path = entry.path, state = entry.state, depth = depth }
		end
	end
	walk(root, 0)
end

function changes_list.rows()
	local git = S.git
	if cached and cached.git == git and cached.tree == S.changes_tree then
		return cached.rows
	end
	local kept, unversioned = {}, {}
	for path, state in git and git.changes or {} do
		local entry = { path = path, state = state }
		if state == "untracked" then
			unversioned[#unversioned + 1] = entry
		else
			kept[#kept + 1] = entry
		end
	end
	local function order(a, c)
		return text.lower(a.path) < text.lower(c.path)
	end
	table.sort(kept, order)
	table.sort(unversioned, order)
	local rows = {}
	for _, group in { { "Changes", kept }, { "Unversioned Files", unversioned } } do
		if #group[2] > 0 then
			rows[#rows + 1] = { heading = group[1], count = #group[2] }
			if S.changes_tree then
				tree_rows(group[1], group[2], rows)
			else
				for _, entry in group[2] do
					rows[#rows + 1] = entry
				end
			end
		end
	end
	cached = { git = git, tree = S.changes_tree, rows = rows }
	return rows
end

-- What a row is chosen by: a file's path, or a folder's key.
local function chosen_by(row)
	return row.key or row.path
end

local function index_of(chosen)
	for index, row in changes_list.rows() do
		if chosen ~= nil and chosen_by(row) == chosen then
			return index
		end
	end
	return nil
end

-- What ⏎ does to the row chosen, for the status line.
function act.changes_enter_word()
	local row = changes_list.rows()[index_of(S.changes_selection) or 0]
	if row and row.dir then
		return row.open and "fold" or "unfold"
	end
	return "edit"
end

-- --- Drawing -----------------------------------------------------------------------------------------------

local function ground_of(id, chosen, path)
	local selected = S.changes_selection == chosen
	if selected and S.focus == "sidebar" and S.sidebar == "changes" then
		return b.alpha("accent", 0.2)
	elseif selected or (path ~= nil and path == S.active) then
		return b.alpha("foreground", 0.07)
	elseif kit.hovered(id) then
		return b.alpha("foreground", 0.045)
	end
	return nil
end

local function folder_view(row)
	local id = "change-dir:" .. row.key
	local element = b.div():press(id):row():items_center():gap(6):h(ROW):pl(8 + row.depth * kit.INDENT):pr(10):no_shrink()
		:bg(ground_of(id, row.key)):role("button"):label(row.name):expanded(row.open)
		:selected(S.changes_selection == row.key):tip(row.dir)
	kit.indent_guides(element, row.depth)
	return element
		:child(kit.icon(row.open and "folder-open" or "folder", 15, "accent"))
		:child(b.text(row.name):color(files.GIT_COLOR[row.state] or "foreground"):truncate():min_w(0))
		:child(b.spacer())
		:child(b.div():size(6, 6):rounded(3):bg(files.GIT_COLOR[row.state] or "dim"):no_shrink())
end

local function row_view(row)
	if row.heading then
		return b.div():row():items_center():gap(6):h(ROW):px(12):no_shrink()
			:child(b.text(row.heading):weight(700):text_size(11.5):color("dim"))
			:child(b.text(tostring(row.count)):text_size(11.5):color("quiet"))
	elseif row.dir then
		return folder_view(row)
	end
	local path = row.path
	local id = "change:" .. path
	local selected = S.changes_selection == path
	local name = text.basename(path)
	local folder = text.dirname(path)
	local kind = files.kind(path)
	local nested = row.depth ~= nil
	local element = b.div():press(id):row():items_center():gap(6):h(ROW):pl(nested and (8 + row.depth * kit.INDENT) or 10):pr(10):no_shrink()
		:bg(ground_of(id, path, path)):role("button"):label(name):selected(selected):tip(path)
	if nested then
		kit.indent_guides(element, row.depth)
	end
	element:child(kit.icon(kind.icon, 15, kind.color))
	local words = b.text(name):color(files.GIT_COLOR[row.state] or "foreground")
	if nested then
		element:child(words:truncate():min_w(0))
	else
		element:child(words:no_shrink())
		if folder ~= "" then
			element:child(b.text(folder):color("quiet"):text_size(11.5):truncate():min_w(0))
		end
	end
	element:child(b.spacer())
	element:child(b.text(files.GIT_LETTER[row.state] or ""):color(files.GIT_COLOR[row.state] or "dim"):weight(700):text_size(11):no_shrink())
	return element
end

local function words(headline, more)
	local box = b.div():col():gap(6):p(12):child(b.text(headline):color("dim"):wrap())
	if more then
		box:child(b.text(more):color("quiet"):text_size(11.5):wrap())
	end
	return box
end

-- The panel in a sidebar `width` points wide.
function changes_list.view(width)
	local _ = width
	if S.git == nil then
		return words("Not a git repository")
	end
	local rows = changes_list.rows()
	if #rows == 0 then
		return words("No changes")
	end
	return b.div()
		:id(changes_list.LIST)
		:col()
		:grow(1)
		:min_h(0)
		:scroll()
		:list(b.Rows.even(#rows, ROW):padded(4, 8), function(index)
			return row_view(rows[index])
		end)
end

-- Keeps the chosen row in view in the list.
function changes_list.show(path)
	local index = index_of(path)
	local extent = layout.scroll_extent(changes_list.LIST)
	if not (index and extent) then
		return
	end
	local rows = b.Rows.even(#changes_list.rows(), ROW):padded(4, 8)
	local offset = layout.scroll_offset(changes_list.LIST)
	local to = rows:reveal(index, offset, extent.view)
	if to ~= offset then
		layout.scroll_to(changes_list.LIST, { y = to })
	end
end

-- --- What the panel does ------------------------------------------------------------------------------------

-- Shows `path` beside the last commit: the file comes up in its tab (the tab a person goes through
-- with the keys is the one tab: the one the panel opened last, with nothing of the person's in it, is
-- closed as the next comes, as a preview tab is) and the keys stay in the panel, or go to the editor
-- where `to_editor` (and in a page too narrow for both, where the panel is a drawer that closes).
function changes_list.open(path, to_editor)
	S.changes_selection = path
	local before = S.changes_preview
	local had = docs.tab_of(path) ~= nil
	-- The one the person has been in (the keyboard was in the editor) is theirs.
	local theirs = S.focus == "editor"
	compare.enter(path, "first")
	if before and before ~= path and not theirs then
		local tab = docs.tab_of(before)
		if tab and tab.compare and not (tab.info and tab.info.dirty) and tab.path ~= S.active then
			act.close_tab(before)
		end
	end
	if not had then
		S.changes_preview = path
	elseif before ~= path then
		S.changes_preview = nil
	end
	if to_editor or S.layout == "narrow" then
		S.changes_preview = nil
		act.focus_editor()
	else
		act.focus_sidebar("changes")
	end
	changes_list.show(path)
	kit.gather(true)
end

-- What the keys go through, in order: every row but the headings.
local function stops()
	local out = {}
	for _, row in changes_list.rows() do
		if not row.heading then
			out[#out + 1] = chosen_by(row)
		end
	end
	return out
end

-- The panel has the keys: a row is chosen where none is (the file shown, or the first file; a folder
-- where all are folded).
function changes_list.focused()
	if S.changes_selection == nil or index_of(S.changes_selection) == nil then
		local first = nil
		for _, row in changes_list.rows() do
			if row.path then
				first = row.path
				break
			end
		end
		S.changes_selection = (S.active and index_of(S.active) and S.active) or first or stops()[1]
	end
end

-- A row is chosen: a file is shown beside the last commit, a folder only chosen.
local function choose(chosen)
	local row = changes_list.rows()[index_of(chosen) or 0]
	if row and row.dir then
		S.changes_selection = chosen
		changes_list.show(chosen)
		kit.gather(true)
	elseif row then
		changes_list.open(row.path, false)
	end
end

local function move(step, to)
	local all = stops()
	if #all == 0 then
		return
	end
	local at = 0
	for index, chosen in all do
		if chosen == S.changes_selection then
			at = index
		end
	end
	local index = to or math.clamp(at + step, 1, #all)
	if index ~= at or to then
		choose(all[index])
	end
end

-- A folder folded or unfolded; it is what is chosen then, so that nothing chosen goes with what folds.
local function fold(row, open)
	S.changes_folded[row.key] = (not open) or nil
	S.changes_selection = row.key
	cached = nil
	kit.gather(true)
end

-- The panel stands its files the other way: `focused` finds a row where the one chosen is not there.
function act.toggle_changes_tree()
	S.changes_tree = not S.changes_tree
	changes_list.focused()
	changes_list.show(S.changes_selection)
	kit.gather(true)
end

-- The menu of a row of a folder.
local function folder_menu(row, at)
	act.menu({
		{ "Reveal in Files", nil, function()
			act.reveal(row.dir)
			act.focus_sidebar("files")
		end },
		"-",
		{ "Copy Path", "y", function() act.copy_path(row.dir, false) end },
	}, at)
end

-- The menu of a row.
function changes_list.menu(path, at)
	act.menu({
		{ "Open File", nil, function()
			compare.exit(path)
			act.open_file(path, { focus = true })
		end },
		{ "Reveal in Files", nil, function()
			act.reveal(path)
			act.focus_sidebar("files")
		end },
		"-",
		{ "Copy Path", "y", function() act.copy_path(path, false) end },
	}, at)
end

-- A key while the panel has the keyboard: whether it was the panel's.
function changes_list.key(key, mods)
	if mods.command or mods.alt then
		return false
	end
	if mods.control and key ~= "n" and key ~= "p" then
		return false
	end
	local all = stops()
	local page = math.max(math.floor(((layout.scroll_extent(changes_list.LIST) or { view = 240 }).view) / ROW) - 1, 1)
	local at = index_of(S.changes_selection)
	local row = at and changes_list.rows()[at] or nil
	local path = row and row.path
	if key == "t" then
		act.toggle_changes_tree()
	elseif row and row.dir and (key == "enter" or key == "o" or key == " ") then
		fold(row, not row.open)
	elseif row and row.dir and (key == "right" or key == "l") then
		if not row.open then
			fold(row, true)
		else
			move(1)
		end
	elseif row and row.dir and (key == "left" or key == "h") and row.open then
		fold(row, false)
	elseif row and row.depth and (key == "left" or key == "h") then
		-- The folder it is in, as the tree of the project goes.
		for index = at - 1, 1, -1 do
			local above = changes_list.rows()[index]
			if above.dir and above.depth == row.depth - 1 then
				choose(above.key)
				break
			end
		end
	elseif row and row.dir and (key == "y" or key == "Y") then
		act.copy_path(row.dir, key == "Y")
	elseif row and row.dir and key == "." then
		local bounds = layout.bounds("change-dir:" .. row.key)
		folder_menu(row, bounds and { x = bounds.x + 40, y = bounds.y + bounds.h } or { x = 60, y = 120 })
	elseif key == "down" or key == "j" or (mods.control and key == "n") then
		move(1)
	elseif key == "up" or key == "k" or (mods.control and key == "p") then
		move(-1)
	elseif key == "home" or key == "g" then
		move(0, 1)
	elseif key == "end" or key == "G" then
		move(0, #all)
	elseif key == "page_down" then
		move(page)
	elseif key == "page_up" then
		move(-page)
	elseif key == "enter" or key == "o" then
		if path then
			changes_list.open(path, true)
		end
	elseif key == "right" or key == "l" or key == " " then
		if path then
			changes_list.open(path, false)
		end
	elseif key == "y" or key == "Y" then
		if path then
			act.copy_path(path, key == "Y")
		end
	elseif key == "." then
		if path then
			local bounds = layout.bounds("change:" .. path)
			changes_list.menu(path, bounds and { x = bounds.x + 40, y = bounds.y + bounds.h } or { x = 60, y = 120 })
		end
	elseif key == "escape" then
		act.focus_editor()
	else
		return false
	end
	kit.gather(true)
	return true
end

-- The row of a folder a press was on.
local function folder_of(id)
	local key = string.match(id or "", "^change%-dir:(.*)$")
	local at = key and index_of(key)
	return at and changes_list.rows()[at] or nil
end

-- A press on a row: the file is shown beside the last commit, a folder folds or unfolds, and the panel
-- has the keys. The button at the head of the panel stands the files the other way.
function changes_list.press(id)
	if id == "changes-tree" then
		act.toggle_changes_tree()
		return true
	end
	local folder = folder_of(id)
	if folder then
		fold(folder, not folder.open)
		act.focus_sidebar("changes")
		return true
	end
	local path = string.match(id, "^change:(.*)$")
	if not path then
		return false
	end
	changes_list.open(path, false)
	return true
end

-- The right button on a row: its menu.
function changes_list.context(id, event)
	local folder = folder_of(id)
	if folder then
		S.changes_selection = folder.key
		folder_menu(folder, { x = event.x, y = event.y })
		return true
	end
	local path = string.match(id or "", "^change:(.*)$")
	if not path then
		return false
	end
	S.changes_selection = path
	changes_list.menu(path, { x = event.x, y = event.y })
	return true
end

return changes_list
