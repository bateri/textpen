-- Every command, with its keys: the palette lists them, the help shows them, the host lists them
-- with its own, and their keys run them. A key the terminal keeps for itself (bateri's ⌘T, ⌘W, ⌘P,
-- ⌘D, ⌘[ ⌘], ⌘1–9) never reaches the page, so no command depends on one: each is in the palette
-- too, and the editor's own that the terminal takes have another key here (⌃G for the next
-- occurrence, ⌃⇧↑ ⌃⇧↓ for a caret above or below).
--
-- A command whose keys are the editor's own (`shown`, not `keys`) is listed with them and runs
-- the same command on the editor; its keys stay the editor's. `keys` is the words of one set of
-- keys or a list of them, each doing the command, the first the one shown.

local kit = require("../lib/kit")
local text = require("../lib/text")
local docs = require("./docs")

local S, act = kit.S, kit.act

local commands = {}

local function has_file()
	return docs.active() ~= nil
end

local function tab_count()
	return #S.tabs
end

-- An editor command on the file shown, the keyboard back in it.
local function on_editor(request)
	return function()
		local doc = docs.active()
		if not doc then
			return
		end
		act.focus_editor()
		kit.ask(doc.id, request)
	end
end

local list = {}

local function add(command)
	list[#list + 1] = command
end

-- --- Going places -------------------------------------------------------------------------------------

add({ id = "files", title = "Go to File…", category = "Go", keys = "cmd+k", run = function() act.open_palette("files") end })
add({ id = "commands", title = "Show All Commands", category = "Go", keys = "f1", run = function() act.open_palette("commands") end })
add({ id = "switch", title = "Switch to Open File…", category = "Go", keys = "cmd+e", when = function() return tab_count() > 0 end,
	run = function() act.open_palette("tabs") end })
add({ id = "line", title = "Go to Line…", category = "Go", keys = "cmd+l", when = has_file, run = function() act.open_palette("line") end })
add({ id = "language", title = "Change Language…", category = "View", when = has_file, run = function() act.open_palette("language") end })

-- --- Files ----------------------------------------------------------------------------------------------

add({ id = "save", title = "Save", category = "File", keys = "cmd+s", when = has_file, run = function() act.save() end })
add({ id = "save-all", title = "Save All", category = "File", keys = "cmd+alt+s",
	when = function()
		for _, tab in S.tabs do
			if tab.info and tab.info.dirty then
				return true
			end
		end
		return false
	end,
	run = function() act.save_all() end })
add({ id = "new-file", title = "New File…", category = "File", keys = "cmd+alt+n", run = function() act.create("file") end })
add({ id = "new-folder", title = "New Folder…", category = "File", run = function() act.create("dir") end })
add({ id = "rename", title = "Rename…", category = "File", keys = "f2",
	when = function() return (S.focus == "sidebar" and S.tree_selection or S.active) ~= nil end,
	run = function()
		local path = S.focus == "sidebar" and S.tree_selection or S.active
		if path then
			act.rename(path)
		end
	end })
add({ id = "delete", title = "Delete File…", category = "File", when = function() return S.active ~= nil end,
	run = function() act.remove(S.active, false) end })
add({ id = "revert", title = "Revert File", category = "File",
	when = function()
		local tab = docs.tab_of(S.active)
		return tab ~= nil and tab.info ~= nil and tab.info.dirty == true
	end,
	run = function() act.revert(S.active) end })
add({ id = "draft-restore", title = "Restore Unsaved Changes from Last Time", category = "File",
	when = function()
		local tab = docs.tab_of(S.active)
		return tab ~= nil and tab.info ~= nil and tab.info.draft ~= nil
	end,
	run = function() act.draft(S.active, true) end })
add({ id = "draft-drop", title = "Discard Unsaved Changes from Last Time", category = "File",
	when = function()
		local tab = docs.tab_of(S.active)
		return tab ~= nil and tab.info ~= nil and tab.info.draft ~= nil
	end,
	run = function() act.draft(S.active, false) end })
add({ id = "conflict-reload", title = "Reload from Disk (Drop My Changes)", category = "File",
	when = function()
		local tab = docs.tab_of(S.active)
		return tab ~= nil and tab.info ~= nil and tab.info.conflict == true
	end,
	run = function() act.revert(S.active, false) end })
add({ id = "conflict-keep", title = "Keep My Changes over the Disk", category = "File",
	when = function()
		local tab = docs.tab_of(S.active)
		return tab ~= nil and tab.info ~= nil and tab.info.conflict == true
	end,
	run = function() act.keep_mine(S.active) end })
add({ id = "copy-path", title = "Copy Path", category = "File", when = function() return S.active ~= nil end,
	run = function() act.copy_path(S.active, false) end })
add({ id = "copy-absolute-path", title = "Copy Absolute Path", category = "File", when = function() return S.active ~= nil end,
	run = function() act.copy_path(S.active, true) end })
add({ id = "reveal", title = "Reveal in Files", category = "File", when = function() return S.active ~= nil end,
	run = function()
		act.reveal(S.active)
		act.focus_sidebar("files")
	end })
add({ id = "eol", title = "Change Line Endings (LF / CRLF)", category = "File", when = has_file,
	run = function()
		local tab = docs.tab_of(S.active)
		if tab and tab.info then
			act.set_eol(tab.path, tab.info.eol == "lf" and "crlf" or "lf")
		end
	end })

-- --- Tabs -----------------------------------------------------------------------------------------------

add({ id = "close", title = "Close Tab", category = "Tabs", keys = "ctrl+w", when = function() return S.active ~= nil end,
	run = function() act.close_tab(S.active) end })
add({ id = "close-others", title = "Close Other Tabs", category = "Tabs", when = function() return tab_count() > 1 end,
	run = function() act.close_others(S.active) end })
add({ id = "close-all", title = "Close All Tabs", category = "Tabs", when = function() return tab_count() > 0 end,
	run = function() act.close_others(nil) end })
add({ id = "reopen", title = "Reopen Closed Tab", category = "Tabs", keys = "cmd+shift+t", when = function() return act.can_reopen() end,
	run = function() act.reopen_closed() end })
add({ id = "next-tab", title = "Next Tab", category = "Tabs", keys = { "ctrl+page_down", "ctrl+tab" }, when = function() return tab_count() > 1 end,
	run = function() act.cycle_tab(1) end })
add({ id = "previous-tab", title = "Previous Tab", category = "Tabs", keys = { "ctrl+page_up", "ctrl+shift+tab" }, when = function() return tab_count() > 1 end,
	run = function() act.cycle_tab(-1) end })
add({ id = "move-tab-right", title = "Move Tab Right", category = "Tabs", keys = "ctrl+shift+page_down",
	when = function() return tab_count() > 1 end, run = function() act.move_tab(1) end })
add({ id = "move-tab-left", title = "Move Tab Left", category = "Tabs", keys = "ctrl+shift+page_up",
	when = function() return tab_count() > 1 end, run = function() act.move_tab(-1) end })
for index = 1, 9 do
	add({
		id = "tab-" .. index,
		title = "Go to Tab " .. index,
		category = "Tabs",
		keys = "ctrl+" .. index,
		hidden = index > 1,
		when = function() return tab_count() >= index or (index == 9 and tab_count() > 0) end,
		run = function()
			-- ⌃9 is the last tab, as in a browser.
			local tab = index == 9 and S.tabs[#S.tabs] or S.tabs[index]
			if tab then
				act.activate(tab.path)
			end
		end,
	})
end

-- --- View -------------------------------------------------------------------------------------------------

add({ id = "sidebar", title = "Toggle Sidebar", category = "View", keys = "cmd+b", run = function() act.toggle_sidebar() end })
add({ id = "show-files", title = "Show Files", category = "View", keys = "cmd+shift+e", run = function() act.focus_sidebar("files") end })
add({ id = "focus", title = "Move Focus Between Sidebar and Editor", category = "View", keys = "f6", run = function() act.cycle_focus() end })
add({ id = "wrap", title = "Toggle Word Wrap", category = "View",
	run = function() act.change_settings({ wrap = not S.settings.wrap }) end })
add({ id = "whitespace", title = "Show Whitespace: None, Selection, Boundary, Trailing, All", category = "View",
	run = function()
		local order = { "none", "selection", "boundary", "trailing", "all" }
		local at = text.index_of(order, S.settings.whitespace) or 1
		local next_one = order[at % #order + 1]
		act.change_settings({ whitespace = next_one })
		kit.toast("Whitespace shown: " .. next_one, "info", 1.6)
	end })
add({ id = "indent-guides", title = "Toggle Indent Guides", category = "View",
	run = function() act.change_settings({ indentGuides = not S.settings.indentGuides }) end })
add({ id = "line-numbers", title = "Line Numbers: On, Relative, Off", category = "View",
	run = function()
		local order = { "on", "relative", "off" }
		local at = text.index_of(order, S.settings.lineNumbers) or 1
		local next_one = order[at % #order + 1]
		act.change_settings({ lineNumbers = next_one })
		kit.toast("Line numbers: " .. next_one, "info", 1.6)
	end })
add({ id = "preview", title = "Toggle Markdown Preview", category = "View", keys = "cmd+shift+v",
	when = function() return S.active ~= nil and act.is_markdown(S.active) end,
	run = function() act.toggle_preview() end })
add({ id = "indentation", title = "Indentation…", category = "View", run = function() act.open_palette("indent") end })
add({ id = "settings", title = "Settings", category = "textpen", keys = "cmd+,", run = function() act.open_settings() end })
add({ id = "help", title = "Keyboard Shortcuts", category = "textpen", keys = "cmd+shift+/", run = function() act.open_help() end })
add({ id = "shell", title = "Open a Shell Here", category = "textpen", keys = "cmd+j",
	when = function() return S.boot ~= nil and S.boot.handoff == true end, run = function() act.shell() end })
add({ id = "quit", title = "Quit textpen", category = "textpen", keys = "ctrl+q", run = function() act.quit() end })

-- --- Search -------------------------------------------------------------------------------------------------

add({ id = "find", title = "Find", category = "Search", keys = "cmd+f", run = function() act.find(false) end })
add({ id = "replace", title = "Replace", category = "Search", keys = "cmd+alt+f", when = has_file, run = function() act.find(true) end })
add({ id = "search", title = "Search in Files", category = "Search", keys = "cmd+shift+f", run = function() act.search_selection() end })
add({ id = "next-match", title = "Next Search Result", category = "Search", keys = "f4",
	when = function() return S.search.result ~= nil and S.search.result.matches > 0 end,
	run = function() act.open_match(1) end })
add({ id = "previous-match", title = "Previous Search Result", category = "Search", keys = "shift+f4",
	when = function() return S.search.result ~= nil and S.search.result.matches > 0 end,
	run = function() act.open_match(-1) end })

-- --- Editing: the editor's own keys ------------------------------------------------------------------------

add({ id = "undo", title = "Undo", category = "Edit", shown = "cmd+z", when = has_file, run = on_editor({ command = "undo" }) })
add({ id = "redo", title = "Redo", category = "Edit", shown = "cmd+shift+z", when = has_file, run = on_editor({ command = "redo" }) })
add({ id = "comment", title = "Toggle Comment", category = "Edit", shown = "cmd+/", when = has_file,
	run = on_editor({ command = "toggle_comment" }) })
add({ id = "block-comment", title = "Toggle Block Comment", category = "Edit", when = has_file,
	run = on_editor({ command = "toggle_block_comment" }) })
add({ id = "delete-line", title = "Delete Line", category = "Edit", keys = { "cmd+shift+k", "ctrl+shift+k" }, when = has_file,
	run = on_editor({ command = "delete_lines" }) })
add({ id = "select-line", title = "Select Line", category = "Selection", keys = "cmd+i", when = has_file,
	run = on_editor({ command = "select_line" }) })
add({ id = "duplicate-line-up", title = "Duplicate Line Up", category = "Edit", keys = "alt+shift+up", when = has_file,
	run = on_editor({ command = "duplicate_lines_up" }) })
add({ id = "join-lines", title = "Join Lines", category = "Edit", keys = "ctrl+j", when = has_file,
	run = on_editor({ command = "join_lines" }) })
add({ id = "insert-line", title = "Insert Line Below", category = "Edit", when = has_file,
	run = on_editor({ command = "insert_blank_line" }) })
add({ id = "split-line", title = "Split Line at the Caret", category = "Edit", when = has_file,
	run = on_editor({ command = "split_line" }) })
add({ id = "transpose", title = "Transpose Characters", category = "Edit", when = has_file,
	run = on_editor({ command = "transpose_chars" }) })
add({ id = "delete-to-start", title = "Delete to Start of Line", category = "Edit", when = has_file,
	run = on_editor({ command = "delete_to_line_start" }) })
add({ id = "delete-to-end", title = "Delete to End of Line", category = "Edit", when = has_file,
	run = on_editor({ command = "delete_to_line_end" }) })
add({ id = "move-line-up", title = "Move Line Up", category = "Edit", shown = "alt+up", when = has_file,
	run = on_editor({ command = "move_lines_up" }) })
add({ id = "move-line-down", title = "Move Line Down", category = "Edit", shown = "alt+down", when = has_file,
	run = on_editor({ command = "move_lines_down" }) })
add({ id = "duplicate-line", title = "Duplicate Line", category = "Edit", shown = "alt+shift+down", when = has_file,
	run = on_editor({ command = "duplicate_lines" }) })
add({ id = "indent", title = "Indent Line", category = "Edit", shown = "tab", when = has_file, run = on_editor({ command = "indent" }) })
add({ id = "outdent", title = "Outdent Line", category = "Edit", shown = "shift+tab", when = has_file, run = on_editor({ command = "outdent" }) })
add({ id = "fold", title = "Fold", category = "Edit", shown = "cmd+alt+[", when = has_file, run = on_editor({ command = "fold" }) })
add({ id = "unfold", title = "Unfold", category = "Edit", shown = "cmd+alt+]", when = has_file, run = on_editor({ command = "unfold" }) })
add({ id = "fold-all", title = "Fold All", category = "Edit", keys = "ctrl+alt+[", when = has_file,
	run = on_editor({ command = "fold_all" }) })
add({ id = "unfold-all", title = "Unfold All", category = "Edit", keys = "ctrl+alt+]", when = has_file,
	run = on_editor({ command = "unfold_all" }) })
add({ id = "select-all", title = "Select All", category = "Selection", shown = "cmd+a", when = has_file,
	run = on_editor({ command = "select_all" }) })
add({ id = "select-next", title = "Add Next Occurrence", category = "Selection", keys = "ctrl+g", when = has_file,
	run = on_editor({ command = "select_next" }) })
add({ id = "select-all-matches", title = "Select All Occurrences", category = "Selection", keys = "cmd+shift+l", when = has_file,
	run = on_editor({ command = "select_occurrences" }) })
add({ id = "cursor-above", title = "Add Cursor Above", category = "Selection", keys = "ctrl+shift+up", when = has_file,
	run = on_editor({ command = "add_caret_above" }) })
add({ id = "cursor-below", title = "Add Cursor Below", category = "Selection", keys = "ctrl+shift+down", when = has_file,
	run = on_editor({ command = "add_caret_below" }) })

commands.list = list

local by_id = {}
for _, command in list do
	by_id[command.id] = command
end

function commands.get(id)
	return by_id[id]
end

-- Whether a command means anything now.
function commands.enabled(command)
	return command.when == nil or command.when() == true
end

-- The commands that mean something now.
function commands.available()
	local out = {}
	for _, command in list do
		if commands.enabled(command) and not command.hidden then
			out[#out + 1] = command
		end
	end
	return out
end

-- Runs a command where it means something now; whether it did.
function commands.run(id)
	local command = by_id[id]
	if command and commands.enabled(command) then
		command.run()
		return true
	end
	return false
end

-- The page's outermost box with every command on it: the host lists them and runs them by their
-- keys, whatever has the keyboard.
function commands.on(root)
	for _, command in list do
		root:command(command.id, command.category .. ": " .. command.title, command.keys, commands.enabled(command))
	end
	return root
end

return commands
