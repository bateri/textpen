-- The menu of the editor, for the right button over its text (and Shift+F10 at the caret): the
-- library's (`@pencere/ui`: Cut, Copy, Paste, Select All, Toggle Line Comment, Select All Occurrences,
-- each with the editor's keys for it) with textpen's own after it: Find in Project, Revert Change (on
-- a line git has changed) and Command Palette. It is the host's menu, shown at the press, as the
-- tree's and the tabs' are (`act.menu`); only the lines are the library's, and a pick of one of
-- them is told to the library's context, which asks the editor for it.

local kit = require("../lib/kit")
local docs = require("./docs")
local changes = require("./changes")
local compare = require("./compare")
local library = require("@pencere/ui")

local b, S, act = kit.b, kit.S, kit.act
local layout = b.layout

local editor_menu = {}

local ID = "editor-menu"
local context = library.new({ pointer = kit.ui })

-- The hunk of the line the menu was opened on, if it was a changed one: { doc =, index =, hunk = }.
local reverting = nil

-- The command that opens the menu at the caret, as the library names it.
editor_menu.COMMAND = library.EDITOR_MENU_COMMAND

-- Opens the menu of the file `doc` at `at` ({ x =, y = }), or under its caret without `at`. `line` is
-- the line the menu is of (from 0, as the program counts them): the one Revert Change is offered for.
local function open(doc, at, line)
	local tab = docs.tab_of(doc.path)
	-- Beside the last commit the marks of the index are not shown, and no change is taken back by them.
	local index = not (tab and tab.compare) and changes.hunk_of_line(doc, line) or nil
	reverting = index and { doc = doc, index = index, hunk = doc.hunks[index] } or nil
	local after = {
		{ key = "find_in_project", title = "Find in Project", keys = "cmd+shift+f" },
	}
	if reverting then
		after[#after + 1] = { key = "revert_change", title = "Revert Change", keys = "ctrl+shift+backspace" }
	end
	after[#after + 1] = {
		key = "compare", title = (tab and tab.compare) and "Close Comparison" or "Compare with Last Commit", keys = "cmd+shift+d",
	}
	after[#after + 1] = { key = "palette", title = "Command Palette", keys = "f1" }
	kit.gather(context:editor_menu(ID, {
		editor = doc.id,
		at = at,
		read_only = (tab and tab.info and tab.info.readonly) or false,
		after = after,
	}))
end

-- A right press over the text of an open file (`event.place`): its menu at the pointer. Whether it
-- took the press.
function editor_menu.pressed(event)
	local place = event.place
	local doc = place and docs.by_id(place.editor)
	if not doc or event.button ~= "right" or not doc.loaded then
		return false
	end
	open(doc, { x = event.x, y = event.y }, (layout.line_of(doc.id, place.at) or 1) - 1)
	return true
end

-- Shift+F10: the menu of the file shown, under its caret.
function editor_menu.at_caret()
	local doc = docs.active()
	if not doc or not doc.loaded then
		return
	end
	act.focus_editor()
	open(doc, nil, docs.where(doc).line - 1)
end

local OWN = {
	find_in_project = function()
		act.search_selection(true)
	end,
	revert_change = function()
		local was = reverting
		if was and was.doc.hunks and was.doc.hunks[was.index] == was.hunk then
			changes.revert(was.doc, was.index)
		end
	end,
	compare = function()
		compare.toggle(S.active)
	end,
	palette = function()
		act.open_palette("commands")
	end,
}

-- The pick of a line (`event`: the host's `menu` event): whether it was this menu's.
function editor_menu.pick(event)
	local _, actions, taken = context:handle(event)
	if not taken then
		return false
	end
	for _, action in actions do
		if action.from == ID then
			local own = OWN[action.entry]
			if own then
				own()
			else
				kit.gather(context:editor_menu_pick(action))
				act.focus_editor()
			end
		end
	end
	return true
end

return editor_menu
