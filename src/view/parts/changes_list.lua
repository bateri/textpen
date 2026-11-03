-- The files git says are changed, as a panel of the sidebar beside the files and the search: those
-- with changes (`Changes`) and those git does not keep (`Unversioned Files`), each a row with its
-- icon, its name, the folder it is in and the letter git says of it. A press on a row, or a key that
-- moves to it, shows the file beside the last commit (`compare`); the keys stay in the panel, so that
-- it is gone through file after file, and ⏎ goes into the editor.
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
-- Kept while what git said is the same.
local cached = nil

function changes_list.rows()
	local git = S.git
	if cached and cached.git == git then
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
			for _, entry in group[2] do
				rows[#rows + 1] = entry
			end
		end
	end
	cached = { git = git, rows = rows }
	return rows
end

local function index_of(path)
	for index, row in changes_list.rows() do
		if row.path == path then
			return index
		end
	end
	return nil
end

-- --- Drawing -----------------------------------------------------------------------------------------------

local function row_view(row)
	if row.heading then
		return b.div():row():items_center():gap(6):h(ROW):px(12):no_shrink()
			:child(b.text(row.heading):weight(700):text_size(11.5):color("dim"))
			:child(b.text(tostring(row.count)):text_size(11.5):color("quiet"))
	end
	local path = row.path
	local id = "change:" .. path
	local selected = S.changes_selection == path
	local focused = selected and S.focus == "sidebar" and S.sidebar == "changes"
	local ground = nil
	if focused then
		ground = b.alpha("accent", 0.2)
	elseif selected or path == S.active then
		ground = b.alpha("foreground", 0.07)
	elseif kit.hovered(id) then
		ground = b.alpha("foreground", 0.045)
	end
	local name = text.basename(path)
	local folder = text.dirname(path)
	local kind = files.kind(path)
	local element = b.div():press(id):row():items_center():gap(6):h(ROW):px(10):no_shrink():bg(ground)
		:role("button"):label(name):selected(selected):tip(path)
		:child(kit.icon(kind.icon, 15, kind.color))
		:child(b.text(name):color(files.GIT_COLOR[row.state] or "foreground"):no_shrink())
	if folder ~= "" then
		element:child(b.text(folder):color("quiet"):text_size(11.5):truncate():min_w(0))
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

-- The files of the rows, in order.
local function paths()
	local out = {}
	for _, row in changes_list.rows() do
		if row.path then
			out[#out + 1] = row.path
		end
	end
	return out
end

-- The panel has the keys: a row is chosen where none is (the file shown, or the first).
function changes_list.focused()
	local all = paths()
	if S.changes_selection == nil or index_of(S.changes_selection) == nil then
		S.changes_selection = (S.active and index_of(S.active) and S.active) or all[1]
	end
end

local function move(step, to)
	local all = paths()
	if #all == 0 then
		return
	end
	local at = 0
	for index, path in all do
		if path == S.changes_selection then
			at = index
		end
	end
	local index = to or math.clamp(at + step, 1, #all)
	if index ~= at or to then
		changes_list.open(all[index], false)
	end
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
	local all = paths()
	local page = math.max(math.floor(((layout.scroll_extent(changes_list.LIST) or { view = 240 }).view) / ROW) - 1, 1)
	local path = S.changes_selection
	if key == "down" or key == "j" or (mods.control and key == "n") then
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

-- A press on a row: the file is shown beside the last commit, and the panel has the keys.
function changes_list.press(id)
	local path = string.match(id, "^change:(.*)$")
	if not path then
		return false
	end
	changes_list.open(path, false)
	return true
end

-- The right button on a row: its menu.
function changes_list.context(id, event)
	local path = string.match(id or "", "^change:(.*)$")
	if not path then
		return false
	end
	S.changes_selection = path
	changes_list.menu(path, { x = event.x, y = event.y })
	return true
end

return changes_list
