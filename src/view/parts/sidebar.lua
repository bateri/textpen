-- The sidebar: the files, or the search in them. Beside the editor in a page wide enough for both
-- (its edge dragged to make it wider), over it as a drawer in a narrow one.

local kit = require("../lib/kit")
local text = require("../lib/text")
local docs = require("./docs")
local tree = require("./tree")

local b, S, SIZE, act = kit.b, kit.S, kit.SIZE, kit.act

local sidebar = {}

local MIN_W, MAX_W = 160, 520

-- One of the two panels' buttons, an icon alone (its name in its tooltip).
local function segment(id, icon_name, words, on, keys)
	local over = kit.hovered(id)
	return b.div()
		:press(id)
		:row()
		:items_center()
		:justify_center()
		:size(30, 24)
		:rounded(5)
		:bg(on and "raised3" or (over and b.alpha("foreground", 0.06) or nil))
		:role("tab")
		:selected(on)
		:label(words)
		:tip(words, kit.keys_of(keys))
		:child(kit.icon(icon_name, 15, on and "foreground" or "dim"))
end

local function head(drawer)
	local row = b.div():row():items_center():gap(2):h(SIZE.tabs):px(8):no_shrink()
	local switch = b.div():row():gap(2):p(2):rounded(7):bg("raised1")
		:child(segment("sidebar-files", "files", "Files", S.sidebar == "files", "cmd+shift+e"))
		:child(segment("sidebar-search", "search", "Search in files", S.sidebar == "search", "cmd+shift+f"))
	row:child(switch)
	row:child(b.spacer())
	if S.sidebar == "files" then
		row:child(kit.icon_button("sidebar-new-file", "file-plus", "New file", { keys = "cmd+alt+n" }))
		row:child(kit.icon_button("sidebar-new-folder", "folder-plus", "New folder"))
		row:child(kit.icon_button("sidebar-collapse", "list-tree", "Collapse folders"))
	end
	row:child(kit.icon_button("sidebar-hide", "layout-sidebar-left-collapse", drawer and "Close" or "Hide sidebar", { keys = "cmd+b" }))
	return b.div():col():no_shrink():child(row):child(kit.rule_x())
end

local function project_line()
	local boot = S.boot
	local line = b.div():row():items_center():gap(8):h(28):px(12):no_shrink():tip(boot.root)
	line:child(b.text(string.upper(boot.rootName)):weight(700):text_size(11.5):color("dim"):truncate():min_w(0):no_shrink())
	local git = S.git
	if git then
		local changed = 0
		for _ in git.changes or {} do
			changed += 1
		end
		line:child(b.text(git.branch .. (changed > 0 and (" · " .. changed .. " changed") or "")):color("quiet")
			:text_size(11.5):truncate():min_w(0))
	end
	return line
end

function sidebar.view(width, drawer)
	local panel = b.div():col():w(width):h_full():no_shrink():bg("raised1"):child(head(drawer))
	if S.sidebar == "files" then
		panel:child(project_line()):child(tree.view())
	else
		panel:child(act.search_view())
	end
	return panel
end

function act.sidebar_view(width)
	return sidebar.view(width, false)
end

-- The edge between the sidebar and the editor, dragged.
function act.resizer_view()
	local on = kit.hovered("resizer") or S.resizing ~= nil
	return b.div()
		:press("resizer")
		:w(5)
		:ml(-2)
		:mr(-3)
		:h_full()
		:no_shrink()
		:row()
		:justify_center()
		:label("Sidebar width")
		:child(b.div():w(on and 2 or 1):h_full():bg(on and "accent" or "border"):motion({ seconds = 0.12, color = true }))
end

-- The sidebar over the editor, in a narrow page.
function act.drawer_view(size)
	local width = math.min(math.max(size.w - 64, MIN_W), 320)
	return b.div()
		:overlay()
		:inset(0, 0, SIZE.status, 0)
		:row()
		:child(sidebar.view(width, true):shadow(8, 0, 24, b.alpha("shadow", 0.4)))
		:child(b.div():press("drawer-scrim"):grow(1):h_full():bg(b.alpha("shadow", 0.25)))
end

-- --- The resizer's drag --------------------------------------------------------------------------------

function sidebar.pressed(hit, event)
	if hit == "resizer" then
		S.resizing = { x = event.x, width = S.settings.sidebarWidth }
		kit.gather({ cursor = "resize_horizontal" })
		return true
	end
	return false
end

function sidebar.dragged(event)
	local drag = S.resizing
	if not drag then
		return false
	end
	local width = math.clamp(math.floor(drag.width + event.x - drag.x + 0.5), MIN_W, math.min(MAX_W, S.width - 280))
	S.settings = table.clone(S.settings)
	S.settings.sidebarWidth = width
	kit.gather({ redraw = true, cursor = "resize_horizontal" })
	return true
end

function sidebar.released()
	if not S.resizing then
		return false
	end
	S.resizing = nil
	act.change_settings({ sidebarWidth = S.settings.sidebarWidth })
	return true
end

-- A press in the sidebar: whether it was one of its own.
function sidebar.press(id, event)
	if id == "sidebar-files" then
		act.focus_sidebar("files")
	elseif id == "sidebar-search" then
		act.focus_sidebar("search")
	elseif id == "sidebar-new-file" then
		act.create("file")
	elseif id == "sidebar-new-folder" then
		act.create("dir")
	elseif id == "sidebar-collapse" then
		S.expanded = {}
		act.list_dir("")
	elseif id == "sidebar-hide" then
		if S.drawer then
			S.drawer = false
		else
			act.change_settings({ sidebar = false })
		end
		act.focus_editor()
	elseif id == "drawer-scrim" then
		S.drawer = false
		act.focus_editor()
	elseif id == "resizer" then
		return true
	else
		return tree.press(id, event)
	end
	kit.gather(true)
	return true
end

-- --- Where the keys are --------------------------------------------------------------------------------

-- The keyboard goes to the sidebar's panel: the tree's rows (the page's own keys), or the search's
-- field.
function act.focus_sidebar(panel)
	S.sidebar = panel or S.sidebar
	S.focus = "sidebar"
	if S.layout == "narrow" then
		S.drawer = true
	elseif not S.settings.sidebar then
		act.change_settings({ sidebar = true })
	end
	if S.sidebar == "files" then
		if not S.tree_selection then
			local rows = tree.rows()
			S.tree_selection = rows[1] and rows[1].entry.path or nil
		end
		S.field = nil
		kit.keyboard("page")
	else
		act.focus_search()
	end
	kit.gather(true)
end

function act.toggle_sidebar()
	if S.layout == "narrow" then
		if S.drawer then
			S.drawer = false
			act.focus_editor()
		else
			act.focus_sidebar(S.sidebar)
		end
		return
	end
	local shown = not S.settings.sidebar
	act.change_settings({ sidebar = shown })
	if not shown and S.focus == "sidebar" then
		act.focus_editor()
	end
end

function act.show_sidebar()
	act.focus_sidebar("files")
end

-- F6: the sidebar and the editor, in turn.
function act.cycle_focus()
	if S.focus == "sidebar" then
		act.focus_editor()
	else
		act.focus_sidebar(S.sidebar)
	end
end

-- A key with the keyboard on the page's own keys while the sidebar has it.
function sidebar.key(key, mods)
	if S.focus ~= "sidebar" then
		return false
	end
	if S.sidebar == "files" then
		return tree.key(key, mods)
	end
	return act.search_key(key, mods)
end

-- --- The tree's listings and the files ----------------------------------------------------------------

function act.list_dir(dir)
	kit.call("list", { dir = dir }, function(listing, err)
		if err then
			S.listing_errors[dir] = err.message
		else
			S.listings[dir] = listing
			S.listing_errors[dir] = nil
		end
		return true
	end)
end

function act.set_expanded(dir, open)
	local is_open = text.index_of(S.expanded, dir) ~= nil
	if open == is_open then
		return
	end
	if open then
		S.expanded[#S.expanded + 1] = dir
		if not S.listings[dir] then
			act.list_dir(dir)
		end
	else
		-- A folded directory folds what is in it too: it opens as it was first seen.
		local kept = {}
		for _, each in S.expanded do
			if not text.within(each, dir) then
				kept[#kept + 1] = each
			end
		end
		S.expanded = kept
	end
	kit.gather(true)
end

-- Shows a file in the tree: its directories open, its row chosen.
function act.reveal(path)
	for _, dir in text.ancestors(path) do
		if not text.index_of(S.expanded, dir) then
			S.expanded[#S.expanded + 1] = dir
		end
		if not S.listings[dir] then
			act.list_dir(dir)
		end
	end
	S.tree_selection = path
	tree.pending_show = path
end

-- The size of a file as its listing says, bytes (0 where none does).
function act.size_of(path)
	local entry = tree.entry_of(path)
	return entry and entry.size or 0
end

-- The directory a new file goes in: the chosen directory of the tree, or the chosen file's.
local function target_dir()
	local chosen = S.focus == "sidebar" and S.tree_selection or S.active
	if not chosen then
		return ""
	end
	local entry = tree.entry_of(chosen)
	if entry and entry.kind == "dir" then
		return chosen
	end
	return text.dirname(chosen)
end

local function clean(path)
	return (string.gsub(text.trim(path), "/+$", ""))
end

function act.create(kind, dir)
	dir = dir or target_dir()
	act.ask({
		title = kind == "file" and "New file" or "New folder",
		input = {
			value = dir == "" and "" or (dir .. "/"),
			placeholder = kind == "file" and "src/name.ts" or "src/folder",
			hint = "A path from the project's root; missing folders are made.",
		},
		actions = {
			{ id = "create", label = "Create", primary = true },
			{ id = "cancel", label = "Cancel" },
		},
	}, function(chosen, value)
		local path = chosen == "create" and clean(value or "") or ""
		if path == "" or path == dir then
			return
		end
		S.busy += 1
		kit.call("create", { path = path, kind = kind }, function(created, err)
			S.busy -= 1
			if err then
				kit.toast(err.message, "bad", 6)
				return true
			end
			act.list_dir(text.dirname(created.path))
			act.reveal(created.path)
			if kind == "file" then
				act.open_file(created.path, { focus = true })
			else
				act.set_expanded(created.path, true)
			end
			return true
		end)
	end)
end

function act.rename(path)
	local name = text.basename(path)
	act.ask({
		title = "Rename " .. name,
		input = { value = path, hint = "Change the folders too, to move it." },
		actions = {
			{ id = "rename", label = "Rename", primary = true },
			{ id = "cancel", label = "Cancel" },
		},
	}, function(chosen, value)
		local to = chosen == "rename" and clean(value or "") or ""
		if to == "" or to == path then
			return
		end
		local function go()
			S.busy += 1
			kit.call("rename", { from = path, to = to }, function(result, err)
				S.busy -= 1
				if err then
					kit.toast(err.message, "bad", 6)
					return true
				end
				local target = result.path
				local function follow(p)
					if text.within(p, path) then
						return target .. string.sub(p, #path + 1)
					end
					return p
				end
				for _, tab in S.tabs do
					if text.within(tab.path, path) then
						local new_path = follow(tab.path)
						tab.path = new_path
						if tab.doc then
							docs.renamed(tab.doc, new_path)
						end
						for _, info in result.buffers or {} do
							if info.path == new_path then
								tab.info = info
							end
						end
					end
				end
				if S.active then
					S.active = follow(S.active)
				end
				for index, each in S.recent do
					S.recent[index] = follow(each)
				end
				for index, each in S.expanded do
					S.expanded[index] = follow(each)
				end
				for index, each in S.preview do
					S.preview[index] = follow(each)
				end
				local places = {}
				for each, place in S.places do
					places[follow(each)] = place
				end
				S.places = places
				S.tree_selection = target
				act.list_dir(text.dirname(path))
				if text.dirname(target) ~= text.dirname(path) then
					act.list_dir(text.dirname(target))
				end
				kit.toast("Renamed to " .. target, "ok")
				return true
			end)
		end
		-- What the person changed goes to the program before the files move.
		docs.flush_all(go)
	end)
end

function act.remove(path, is_dir)
	if not path then
		return
	end
	act.ask({
		title = "Delete " .. text.basename(path) .. "?",
		message = is_dir and ("The folder " .. path .. " and everything in it are deleted. This cannot be undone.")
			or (path .. " is deleted. This cannot be undone."),
		actions = {
			{ id = "delete", label = "Delete", danger = true },
			{ id = "cancel", label = "Cancel", primary = true },
		},
	}, function(chosen)
		if chosen ~= "delete" then
			return
		end
		S.busy += 1
		kit.call("delete", { path = path }, function(_, err)
			S.busy -= 1
			if err then
				kit.toast(err.message, "bad", 6)
				return true
			end
			-- The tabs of what is gone close, their changes with them.
			local gone = {}
			for _, tab in S.tabs do
				if text.within(tab.path, path) then
					gone[#gone + 1] = tab
				end
			end
			for _, tab in gone do
				docs.close(tab, true, function() end)
				local at = text.index_of(S.tabs, tab)
				if at then
					table.remove(S.tabs, at)
				end
				S.preview = text.without(S.preview, tab.path)
			end
			if S.active and not docs.tab_of(S.active) then
				S.active = nil
				for _, each in S.recent do
					if docs.tab_of(each) then
						act.open_file(each, { focus = S.focus == "editor" })
						break
					end
				end
				if not S.active and S.tabs[1] then
					act.open_file(S.tabs[1].path, { focus = S.focus == "editor" })
				end
			end
			local kept = {}
			for _, each in S.expanded do
				if not text.within(each, path) then
					kept[#kept + 1] = each
				end
			end
			S.expanded = kept
			S.tree_selection = text.dirname(path) ~= "" and text.dirname(path) or nil
			act.list_dir(text.dirname(path))
			kit.toast("Deleted " .. path, "ok")
			return true
		end)
	end)
end

-- After the page was laid out: the row revealed is brought into view.
function sidebar.settle()
	if tree.pending_show and S.listings[text.dirname(tree.pending_show)] then
		local path = tree.pending_show
		tree.pending_show = nil
		tree.current = tree.rows()
		tree.show(path)
	end
end

sidebar.context = tree.context

return sidebar
