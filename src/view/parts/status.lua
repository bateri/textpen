-- The status line: the branch, the keys that work where the keyboard is, and the file's place,
-- indentation, line ends and language, each a button that changes it. Items give way, the least
-- needed first, as the pane narrows.

local kit = require("../lib/kit")
local docs = require("./docs")
local search = require("./search")

local b, S, SIZE, act = kit.b, kit.S, kit.SIZE, kit.act
local layout = b.layout

local status = {}

-- How long the word that a file was saved stays, seconds.
local SAVED_SHOWN = 1.4

-- The keys that work where the keyboard is, each a button that does what its key does:
-- { keys, words, id }.
local function hints(has_file)
	if S.focus == "sidebar" and S.sidebar == "files" then
		return {
			{ "enter", "open", "hint:tree-open" },
			{ "a", "new", "hint:tree-new" },
			{ "r", "rename", "hint:tree-rename" },
			{ "d", "delete", "hint:tree-delete" },
			{ ".", "menu", "hint:tree-menu" },
			{ "escape", "editor", "hint:editor" },
		}
	elseif S.focus == "sidebar" and search.in_results() then
		return {
			{ "enter", "open", "hint:search-open" },
			{ "space", "show", "hint:search-show" },
			{ "escape", "search", "hint:search-field" },
		}
	elseif S.focus == "sidebar" then
		return {
			{ "enter", "search", "hint:search-run" },
			{ "down", "results", "hint:search-results" },
			{ "cmd+alt+r", "regex", "hint:search-regex" },
			{ "escape", "editor", "hint:editor" },
		}
	end
	local out = {
		{ "cmd+k", "files", "hint:files" },
		{ "f1", "commands", "hint:commands" },
	}
	if has_file then
		out[#out + 1] = { "cmd+s", "save", "hint:save" }
		out[#out + 1] = { "cmd+f", "find", "hint:find" }
	end
	out[#out + 1] = { "ctrl+w", "close", "hint:close" }
	return out
end

local function hint_view(hint)
	local id = hint[3]
	return b.div()
		:press(id)
		:row()
		:items_center()
		:gap(5)
		:px(5)
		:h(18)
		:rounded(4)
		:bg(kit.hovered(id) and b.alpha("foreground", 0.07) or nil)
		:no_shrink()
		:child(kit.kbd(hint[1], true))
		:child(b.text(hint[2]):color("quiet"))
end

local function item(id, words, tip)
	local element = b.div():row():items_center():h(20):px(6):rounded(4):no_shrink():child(b.text(words))
	if id then
		element:press(id):bg(kit.hovered(id) and b.alpha("foreground", 0.07) or nil)
		if tip then
			element:tip(tip)
		end
	end
	return element
end

-- Where the main caret is: its line and column (from 1), how much is selected and how many carets.
function status.cursor(doc)
	local range = doc.ranges[doc.main] or { anchor = 0, head = 0 }
	local line = layout.line_of(doc.id, range.head) or 0
	local column = range.head - (layout.line_start(doc.id, line) or 0)
	local selected = 0
	for _, each in doc.ranges do
		selected += math.abs(each.head - each.anchor)
	end
	return line + 1, column + 1, selected, #doc.ranges
end

function status.view()
	local tab = docs.tab_of(S.active)
	local doc = docs.active()
	local info = tab and tab.info
	local wide = S.layout == "wide"
	local row = b.div():row():items_center():gap(4):h(SIZE.status):px(8):no_shrink():bg("raised1"):color("dim"):text_size(11.5)
	local git = S.git
	if git then
		local branch = b.div():row():items_center():gap(4):px(4):min_w(0)
			:child(kit.icon("git-branch", 13, "dim"))
			:child(b.text(git.branch):truncate():min_w(0))
			:tip("Branch " .. git.branch)
		if git.ahead and git.ahead > 0 then
			branch:child(kit.icon("arrow-up", 11, "dim")):child(b.text(tostring(git.ahead)))
		end
		if git.behind and git.behind > 0 then
			branch:child(kit.icon("arrow-down", 11, "dim")):child(b.text(tostring(git.behind)))
		end
		row:child(branch:max_w(180))
	end
	if S.busy > 0 then
		row:child(kit.icon("loader-2", 12, "quiet"))
	end
	local saved = S.active and S.saved_at[S.active]
	if saved and S.now - saved < SAVED_SHOWN then
		row:child(b.div():row():items_center():gap(3):px(4)
			:child(kit.icon("check", 12, "success")):child(b.text("Saved"):color("success")))
	end
	row:child(b.spacer())
	if wide then
		local keys = b.div():row():items_center():gap(6):no_shrink()
		for _, hint in hints(doc ~= nil) do
			keys:child(hint_view(hint))
		end
		row:child(keys)
		row:child(b.spacer())
	end
	if doc and info then
		local line, column, selected, carets = status.cursor(doc)
		local words = string.format("Ln %d, Col %d", line, column)
		local place = item("status-line", words, "Go to line " .. kit.keys_of("cmd+l"))
		if selected > 0 then
			place:child(b.text(string.format(" (%d selected%s)", selected, carets > 1 and (", " .. carets .. " carets") or ""))
				:color("quiet"))
		end
		row:child(place)
		if S.layout ~= "narrow" then
			local settings = S.settings
			local spaces = settings.insertSpaces and not doc.lang.tabs
			row:child(item("status-indent", (spaces and "Spaces: " or "Tab: ") .. settings.tabSize, "Indentation"))
			row:child(item(nil, info.bom and "UTF-8 BOM" or "UTF-8"))
			row:child(item("status-eol", string.upper(info.eol), "Line endings: click to change"))
		end
		row:child(item("status-language", doc.lang.name, "Change language"))
	end
	if S.boot and S.boot.handoff and S.layout ~= "narrow" then
		row:child(kit.icon_button("status-shell", "terminal-2", "Shell here", { keys = "cmd+j", size = 20, icon = 13 }))
	end
	row:child(kit.icon_button("status-help", "help-circle", "Keyboard shortcuts", { keys = "cmd+shift+/", size = 20, icon = 13 }))
	return b.div():col():no_shrink():child(kit.rule_x()):child(row)
end

-- When the page is to be drawn again for the word that a file was saved to go.
function status.wake(now)
	local saved = S.active and S.saved_at[S.active]
	if saved and now - saved < SAVED_SHOWN then
		return saved + SAVED_SHOWN
	end
	return nil
end

-- A press on the line: whether it was one of its own.
function status.press(id)
	if id == "status-line" then
		act.open_palette("line")
	elseif id == "status-indent" then
		act.open_palette("indent")
	elseif id == "status-language" then
		act.open_palette("language")
	elseif id == "status-eol" then
		local tab = docs.tab_of(S.active)
		if tab and tab.info then
			act.set_eol(tab.path, tab.info.eol == "lf" and "crlf" or "lf")
		end
	elseif id == "status-shell" then
		act.shell()
	elseif id == "status-help" then
		act.open_help()
	elseif string.sub(id, 1, 5) == "hint:" then
		act.hint(string.sub(id, 6))
	else
		return false
	end
	return true
end

return status
