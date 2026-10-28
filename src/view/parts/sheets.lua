-- The sheets that come over the page: the keyboard's (every command with its keys, filtered as it
-- is typed) and the settings (each a line changed with Space or the arrows).

local kit = require("../lib/kit")
local text = require("../lib/text")
local fuzzy = require("@pencere/fuzzy")
local commands = require("./commands")

local b, S, act = kit.b, kit.S, kit.act
local layout = b.layout

local sheets = {}

local FILTER = "help-filter"
local HELP_BODY = "help-body"

local TREE_KEYS = {
	{ "↑ ↓  j k", "Move" },
	{ "← →  h l", "Fold, unfold, go to the parent" },
	{ "⏎  o", "Open and go to the editor" },
	{ "Space", "Open, staying in the tree" },
	{ "a  ⇧A", "New file, new folder" },
	{ "r  F2", "Rename or move" },
	{ "d  ⌫", "Delete" },
	{ "y  ⇧Y", "Copy the path, the absolute path" },
	{ ".", "Menu of the row" },
	{ "⇧R", "Read the folders again" },
	{ "Esc", "Back to the editor" },
}

-- --- The keyboard's sheet ----------------------------------------------------------------------------------

function act.open_help()
	act.set_overlay({ kind = "help", query = "" })
	S.field = FILTER
	kit.gather({ redraw = true, fill = { [FILTER] = "" }, keyboard = { field = FILTER } })
end

local function close()
	act.set_overlay(nil)
	act.regain()
	kit.gather(true)
end

local function sheet(width, size)
	return b.div()
		:press("sheet")
		:col()
		:w(math.min(width, size.w - 24))
		:max_h(size.h - 32)
		:rounded(10)
		:bg("raised2")
		:border(1, "border")
		:shadow(0, 18, 48, b.alpha("shadow", 0.5))
		:clip()
		:motion({ enter = { opacity = 0, dy = -6, seconds = 0.12 } })
		:id("sheet-box")
end

local function scrim(box, size)
	return b.div()
		:press("sheet-scrim")
		:overlay()
		:inset(0, 0, 0, 0)
		:bg(b.alpha("shadow", 0.22))
		:col()
		:items_center()
		:pt(math.max(math.floor(size.h * 0.08), 12))
		:child(box)
end

local function help_view(size)
	local query = S.overlay.query
	local head = b.div():row():items_center():gap(10):h(44):px(14):no_shrink()
		:child(b.text("Keyboard"):weight(700))
		:child(b.div():text_field(FILTER):label("Filter the keys"):grow(1):h(28):px(8):rounded(6):bg("background"):border(1, "border")
			:child(b.text("Filter…")))
		:child(kit.icon_button("sheet-close", "x", "Close", { keys = "escape" }))
	local body = b.div():id(HELP_BODY):col():gap(14):px(16):py(12):grow(1):min_h(0):scroll()
	local groups, order = {}, {}
	for _, command in commands.list do
		local keys = command.keys or command.shown
		local listed = keys and (not string.match(command.id, "^tab%-%d$") or command.id == "tab-1")
		if listed and (query == "" or fuzzy.match(query, command.category .. " " .. command.title) ~= nil) then
			if not groups[command.category] then
				groups[command.category] = {}
				order[#order + 1] = command.category
			end
			table.insert(groups[command.category], command)
		end
	end
	local function line(words, keys)
		return b.div():row():items_center():gap(10):h(24)
			:child(b.text(words):truncate():min_w(0):grow(1))
			:child(b.text(keys):color("dim"):no_shrink())
	end
	for _, category in order do
		local group = b.div():col():gap(2):child(b.text(string.upper(category)):text_size(11):weight(700):color("quiet"))
		for _, command in groups[category] do
			local keys = kit.keys_all(command.keys or command.shown, "   ")
			local words = command.title
			if command.id == "tab-1" then
				words, keys = "Go to Tab 1–9", kit.keys_of("ctrl+1") .. " … " .. kit.keys_of("ctrl+9")
			end
			group:child(line(words, keys))
		end
		body:child(group)
	end
	if query == "" then
		local group = b.div():col():gap(2)
			:child(b.text("FILES (A CLICK ON A ROW, OR " .. kit.keys_of("cmd+shift+e") .. " / F6)"):text_size(11):weight(700):color("quiet"))
		for _, entry in TREE_KEYS do
			group:child(line(entry[2], entry[1]))
		end
		body:child(group)
		body:child(b.text("⌃C always reaches textpen and ends it; what is not saved is kept as a draft and offered the next "
			.. "time. The terminal keeps its own keys (⌘T, ⌘W, ⌘P, ⌘1–9 in bateri): everything they would do here is in "
			.. "the palette, F1."):color("dim"):wrap():text_size(12))
	end
	return scrim(sheet(760, size):child(head):child(kit.rule_x()):child(body), size)
end

-- --- The settings' sheet ------------------------------------------------------------------------------------

local function cycle(values, value, by)
	local at = text.index_of(values, value) or 1
	return values[(at - 1 + by) % #values + 1]
end

local ROWS = {
	{
		key = "wrap",
		title = "Word wrap",
		detail = "Long lines wrap at the edge",
		value = function(s) return s.wrap and "On" or "Off" end,
		step = function(s) return { wrap = not s.wrap } end,
	},
	{
		key = "lineNumbers",
		title = "Line numbers",
		detail = "Relative counts from the caret",
		value = function(s) return s.lineNumbers end,
		step = function(s, by) return { lineNumbers = cycle({ "on", "relative", "off" }, s.lineNumbers, by) } end,
	},
	{
		key = "tabSize",
		title = "Tab size",
		detail = "Columns a tab and an indent take",
		value = function(s) return tostring(s.tabSize) end,
		step = function(s, by) return { tabSize = cycle({ 2, 4, 8 }, s.tabSize, by) } end,
	},
	{
		key = "insertSpaces",
		title = "Indent with",
		detail = "What ⇥ puts in",
		value = function(s) return s.insertSpaces and "Spaces" or "Tabs" end,
		step = function(s) return { insertSpaces = not s.insertSpaces } end,
	},
	{
		key = "whitespace",
		title = "Show whitespace",
		detail = "Spaces as dots, tabs as arrows",
		value = function(s) return s.whitespace end,
		step = function(s, by) return { whitespace = cycle({ "none", "selection", "boundary", "trailing", "all" }, s.whitespace, by) } end,
	},
	{
		key = "indentGuides",
		title = "Indent guides",
		detail = "A line down each level of indent",
		value = function(s) return s.indentGuides and "On" or "Off" end,
		step = function(s) return { indentGuides = not s.indentGuides } end,
	},
	{
		key = "autoSave",
		title = "Save when leaving",
		detail = "Before the shell, on another tab",
		value = function(s) return s.autoSave and "On" or "Off" end,
		step = function(s) return { autoSave = not s.autoSave } end,
	},
}

function act.open_settings()
	act.set_overlay({ kind = "settings", chosen = 1 })
	S.field = nil
	kit.keyboard("page")
end

local function settings_view(size)
	local overlay = S.overlay
	local head = b.div():row():items_center():gap(10):h(44):px(14):no_shrink()
		:child(b.text("Settings"):weight(700))
	if S.layout ~= "narrow" then
		head:child(b.text("kept in ~/.config/textpen"):color("quiet"))
	end
	head:child(b.spacer()):child(kit.icon_button("sheet-close", "x", "Close", { keys = "escape" }))
	local body = b.div():id("settings-body"):col():gap(2):p(6):grow(1):min_h(0):scroll()
	for index, row in ROWS do
		local value = row.value(S.settings)
		local chosen = index == overlay.chosen
		body:child(b.div()
			:press("setting:" .. index)
			:row()
			:items_center()
			:gap(12)
			:h(42)
			:px(10)
			:rounded(6)
			:bg(chosen and b.alpha("accent", 0.16) or nil)
			:role("button")
			:label(row.title .. ": " .. value)
			:child(b.div():col():grow(1):min_w(0)
				:child(b.text(row.title))
				:child(b.text(row.detail):color("quiet"):text_size(11.5):truncate()))
			:child(b.div():px(8):h(22):rounded(5):row():items_center():no_shrink()
				:bg(value == "On" and b.alpha("accent", 0.2) or "raised3")
				:child(b.text(value):color(value == "On" and "accent" or "foreground"):weight(600))))
	end
	local foot = b.div():row():items_center():gap(14):h(30):px(14):no_shrink():color("quiet"):text_size(11.5)
		:child(b.text("↑ ↓ choose"))
		:child(b.text("Space ← → change"))
		:child(b.text("Esc close"))
	return scrim(sheet(560, size):child(head):child(kit.rule_x()):child(body):child(kit.rule_x()):child(foot), size)
end

-- --- The page's hooks --------------------------------------------------------------------------------------

function act.overlay_view(overlay, size)
	if overlay.kind == "help" then
		return help_view(size)
	elseif overlay.kind == "settings" then
		return settings_view(size)
	end
	return nil
end

function act.overlay_press(id)
	if id == "sheet-scrim" or id == "sheet-close" then
		close()
		return true
	end
	local index = tonumber(string.match(id, "^setting:(%d+)$") or "")
	if index and S.overlay.kind == "settings" then
		S.overlay.chosen = index
		act.change_settings(ROWS[index].step(S.settings, 1))
		return true
	end
	return id == "sheet"
end

function act.overlay_key(key, mods)
	local overlay = S.overlay
	if overlay.kind == "help" then
		if key == "escape" then
			if overlay.query ~= "" then
				overlay.query = ""
				kit.gather({ fill = { [FILTER] = "" } })
			else
				close()
			end
		elseif key == "down" or key == "up" or key == "page_down" or key == "page_up" then
			local step = (key == "down" and 40) or (key == "up" and -40) or (key == "page_down" and 300) or -300
			layout.scroll_to(HELP_BODY, { y = layout.scroll_offset(HELP_BODY) + step })
		end
		kit.gather(true)
		return true
	elseif overlay.kind == "settings" then
		local row = ROWS[overlay.chosen]
		if key == "down" or key == "j" or (mods.control and key == "n") then
			overlay.chosen = math.min(overlay.chosen + 1, #ROWS)
		elseif key == "up" or key == "k" or (mods.control and key == "p") then
			overlay.chosen = math.max(overlay.chosen - 1, 1)
		elseif key == " " or key == "enter" or key == "right" or key == "l" then
			act.change_settings(row.step(S.settings, 1))
		elseif key == "left" or key == "h" then
			act.change_settings(row.step(S.settings, -1))
		elseif key == "escape" then
			close()
		end
		kit.gather(true)
		return true
	end
	return false
end

-- A command while a sheet is open: the sheet goes first.
function act.overlay_command(id)
	if id == "help" and S.overlay.kind == "help" then
		close()
		return true
	elseif id == "settings" and S.overlay.kind == "settings" then
		close()
		return true
	end
	close()
	return false
end

function sheets.text(field, words)
	if field == FILTER and S.overlay and S.overlay.kind == "help" then
		S.overlay.query = words
		return true
	end
	return false
end

return sheets
