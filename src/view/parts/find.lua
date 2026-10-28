-- Find and replace in the file shown: the editor's own search (it marks what it finds, looks again
-- as the text changes and says how many and which one the caret is on), with a bar of the page's
-- that is worked with keys: ⏎ and ⇧⏎ go through the matches, ⌥⌘C ⌥⌘W ⌥⌘R switch case, whole words and
-- regular expressions, ⌥⌘F shows the replacement, ⌘⏎ selects every match (in the replacement:
-- replaces them all), Esc goes back to the text.

local kit = require("../lib/kit")
local docs = require("./docs")

local b, S, act = kit.b, kit.S, kit.act
local layout = b.layout

local find = {}

local QUERY = "find-query"
local REPLACE = "find-replace"

-- What is looked for, and what the editor last said of it.
S.find = {
	open = false,
	replacing = false,
	query = "",
	replacement = "",
	case = false,
	word = false,
	regex = false,
	-- The editor the search is in, and what it found there: how many, which the caret is on, and
	-- whether there may be more; `invalid` where the pattern is none.
	doc = nil,
	count = 0,
	current = nil,
	more = false,
	invalid = false,
}

local F = S.find

-- Tells the editor what to look for (or to stop, for nothing).
local function search(doc)
	if not doc then
		return
	end
	F.doc = doc.id
	F.invalid = false
	if F.query == "" then
		F.count, F.current, F.more = 0, nil, false
		kit.ask(doc.id, { command = "stop_finding" })
		return
	end
	kit.ask(doc.id, { command = "find", pattern = F.query, regex = F.regex, case = F.case, word = F.word })
end

local function to_field(field)
	S.field = field
	kit.gather({ redraw = true, keyboard = { field = field } })
end

-- ⌘F (and ⌥⌘F, `replacing`): the bar over the file shown, the words chosen in it what is looked
-- for; with no file, the search in the project's files.
function act.find(replacing)
	local doc = docs.active()
	if not doc then
		act.focus_sidebar("search")
		return
	end
	S.focus = "editor"
	S.drawer = false
	F.open = true
	if replacing then
		F.replacing = true
	end
	local range = doc.ranges[doc.main]
	if range and range.anchor ~= range.head then
		local from, to = math.min(range.anchor, range.head), math.max(range.anchor, range.head)
		if to - from <= 500 then
			local chosen = layout.editor_text(doc.id, from, to) or ""
			if chosen ~= "" and not string.find(chosen, "\n", 1, true) then
				F.query = chosen
			end
		end
	end
	search(doc)
	kit.gather({ fill = { [QUERY] = F.query, [REPLACE] = F.replacement } })
	to_field(replacing and REPLACE or QUERY)
end

local function close()
	local doc = F.doc and docs.by_id(F.doc)
	F.open = false
	if doc then
		kit.ask(doc.id, { command = "stop_finding" })
	end
	F.count, F.current, F.more = 0, nil, false
	act.focus_editor()
end

-- The file shown changed while the bar is open: the search goes with it.
local function follow(doc)
	if F.doc == doc.id then
		return
	end
	local before = F.doc and docs.by_id(F.doc)
	if before then
		kit.ask(before.id, { command = "stop_finding" })
	end
	F.doc = doc.id
	kit.after(0, function()
		if F.open and F.doc == doc.id then
			search(doc)
		end
		return nil
	end)
end

local function toggle(id, on, words, keys)
	local over = kit.hovered(id)
	return b.div()
		:press(id)
		:size(24, 22)
		:rounded(4)
		:row()
		:items_center()
		:justify_center()
		:no_shrink()
		:bg(on and b.alpha("accent", 0.22) or (over and b.alpha("foreground", 0.08) or nil))
		:role("checkbox")
		:checked(on)
		:label(words)
		:tip(words, kit.keys_of(keys))
		:child(b.text(id == "find-word" and "ab" or (id == "find-case" and "Aa" or ".*"))
			:text_size(11.5):weight(on and 700 or 500):color(on and "accent" or "dim"))
end

local function field(id, hint, width, toggles)
	local box = b.div()
		:row()
		:items_center()
		:gap(2)
		:h(28)
		:pl(8)
		:pr(3)
		:rounded(6)
		:bg("background")
		:border(1, S.field == id and b.alpha("accent", 0.8) or "border")
		:grow(1)
		:min_w(math.min(width, 120))
		:max_w(width)
		:child(b.div():text_field(id):label(hint):grow(1):h_full():child(b.text(hint)))
	if toggles then
		box:child(toggle("find-case", F.case, "Match case", "cmd+alt+c"))
		box:child(toggle("find-word", F.word, "Whole words", "cmd+alt+w"))
		box:child(toggle("find-regex", F.regex, "Regular expression", "cmd+alt+r"))
	end
	if id == QUERY and F.invalid then
		box:border(1, "error")
	end
	return box
end

-- What the count says: "3 of 12", "No results", "Not a pattern".
local function status_words()
	if F.query == "" then
		return ""
	elseif F.invalid then
		return "Not a pattern"
	elseif F.count == 0 then
		return "No results"
	end
	return string.format("%s of %d%s", F.current and tostring(F.current) or "?", F.count, F.more and "+" or "")
end

-- The bar over the file shown, or nil where it is not open.
function act.find_bar(doc)
	if not F.open then
		return nil
	end
	follow(doc)
	local narrow = S.layout == "narrow"
	local width = narrow and 260 or 380
	local none = F.count == 0
	local top = b.div():row():items_center():gap(4):h(36):px(6)
	top:child(kit.icon_button("find-expand", F.replacing and "chevron-down" or "chevron-right",
		F.replacing and "Hide replace" or "Show replace", { keys = "cmd+alt+f", size = 24, icon = 14 }))
	top:child(field(QUERY, "Find", width, true))
	top:child(b.text(status_words()):color((F.invalid or (none and F.query ~= "")) and "error" or "dim")
		:text_size(11.5):min_w(narrow and 54 or 84):no_shrink())
	top:child(kit.icon_button("find-previous", "arrow-up", "Previous match", { keys = "shift+enter", size = 24, icon = 15, disabled = none }))
	top:child(kit.icon_button("find-next", "arrow-down", "Next match", { keys = "enter", size = 24, icon = 15, disabled = none }))
	if not narrow then
		top:child(kit.icon_button("find-all", "select-all", "Select all matches", { keys = "cmd+enter", size = 24, icon = 15, disabled = none }))
	end
	top:child(b.spacer())
	top:child(kit.icon_button("find-close", "x", "Close", { keys = "escape", size = 24, icon = 15 }))
	local bar = b.div():id("find-bar"):col():no_shrink():bg("raised1"):child(top)
	if F.replacing then
		local tab = docs.tab_of(S.active)
		local readonly = tab and tab.info and tab.info.readonly
		local row = b.div():row():items_center():gap(4):h(36):px(6):pl(36)
		row:child(field(REPLACE, "Replace", width, false))
		row:child(kit.button("find-replace-one", "Replace", { icon = "replace", h = 26, keys = "enter", disabled = none or readonly }))
		row:child(kit.button("find-replace-all", "All", { h = 26, keys = "cmd+enter", tip = "Replace all matches", disabled = none or readonly }))
		bar:child(row)
	end
	bar:child(kit.rule_x())
	return bar
end

-- --- What the person does ------------------------------------------------------------------------------

local function flip(key)
	F[key] = not F[key]
	search(docs.active())
	kit.gather(true)
end

local function editor_command(name, extra)
	local doc = docs.active()
	if not doc then
		return
	end
	local request = { command = name }
	for key, value in extra or {} do
		request[key] = value
	end
	kit.ask(doc.id, request)
end

function find.press(id)
	if id == "find-expand" then
		F.replacing = not F.replacing
		if F.replacing then
			kit.gather({ fill = { [REPLACE] = F.replacement } })
			to_field(REPLACE)
		end
	elseif id == "find-case" then
		flip("case")
	elseif id == "find-word" then
		flip("word")
	elseif id == "find-regex" then
		flip("regex")
	elseif id == "find-previous" then
		editor_command("find_previous")
	elseif id == "find-next" then
		editor_command("find_next")
	elseif id == "find-all" then
		editor_command("select_found")
		act.focus_editor()
	elseif id == "find-close" then
		close()
	elseif id == "find-replace-one" then
		editor_command("replace", { text = F.replacement })
	elseif id == "find-replace-all" then
		editor_command("replace_all", { text = F.replacement })
	else
		return false
	end
	kit.gather(true)
	return true
end

function find.text(field_id, words)
	if field_id == QUERY then
		F.query = words
		search(docs.active())
		return true
	elseif field_id == REPLACE then
		F.replacement = words
		return true
	end
	return false
end

-- A key while one of the bar's fields has the keyboard: whether it was the bar's.
function find.key(key, mods)
	if not F.open then
		return false
	end
	if S.field ~= QUERY and S.field ~= REPLACE then
		-- Esc in the text closes the bar too.
		if key == "escape" and S.focus == "editor" and not (mods.command or mods.shift) then
			close()
			kit.gather(true)
			return true
		end
		return false
	end
	local in_replace = S.field == REPLACE
	if mods.command and mods.alt and (key == "c" or key == "w" or key == "r") then
		flip(key == "c" and "case" or (key == "w" and "word" or "regex"))
	elseif key == "escape" then
		close()
	elseif key == "enter" then
		if in_replace then
			editor_command(mods.command and "replace_all" or "replace", { text = F.replacement })
		elseif mods.command then
			editor_command("select_found")
			act.focus_editor()
		else
			editor_command(mods.shift and "find_previous" or "find_next")
		end
	elseif key == "tab" then
		if in_replace and mods.shift then
			to_field(QUERY)
		elseif not in_replace and not mods.shift and F.replacing then
			to_field(REPLACE)
		end
	elseif key == "down" and not in_replace then
		editor_command("find_next")
	elseif key == "up" and not in_replace then
		editor_command("find_previous")
	else
		return false
	end
	kit.gather(true)
	return true
end

-- What the editors say of the search.
function find.editor_event(event)
	if event.editor ~= F.doc then
		return
	end
	if event.type == "found" then
		F.count, F.current, F.more = event.count or 0, event.current, event.more == true
		F.invalid = false
	elseif event.type == "editor_refused" and event.reason == "bad-pattern" then
		F.invalid = true
		F.count, F.current, F.more = 0, nil, false
	end
end

return find
