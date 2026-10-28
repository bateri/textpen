-- What every file of the page shares: the kit's builders, the one `Ui` that remembers the pointer,
-- the page's state, the reply that what the page does is gathered into, and the few pieces the
-- page is made of more than once (an icon, a button, the keys of a command, a note that comes and
-- goes).

local keys = require("@pencere/keys")

local b = pencere
local ui = b.Ui.new()

-- The page's state: what the program said and what the person did, in one place, so that `render`
-- is a function of it. The text of the files is not here: it is in the terminal's editors, one for
-- each open file, and, as the source of truth, in the program.
local S = {
	now = 0,
	width = 1000,
	height = 700,
	-- "wide", "medium" or "narrow", by the page's width; `short` for a page with little height.
	layout = "wide",
	short = false,
	-- What `boot` said, and why there is none.
	boot = nil,
	boot_error = nil,
	settings = nil,
	git = nil,
	-- The tree: each listed directory's listing ({ dir =, entries =, truncated = }) and why one
	-- could not be read, the directories open, the row the keys act on.
	listings = {},
	listing_errors = {},
	expanded = {},
	tree_selection = nil,
	-- The sidebar's panel ("files" or "search"); in a narrow page it is a drawer over the editor.
	sidebar = "files",
	drawer = false,
	-- Where the keys are, as the page has it: "editor" or "sidebar".
	focus = "editor",
	-- The open files, in their order ({ path =, status =, info =, unopenable =, error =, doc = }),
	-- the one shown, the ones opened lately (the latest first) and where each was left.
	tabs = {},
	active = nil,
	recent = {},
	places = {},
	-- The Markdown files shown as their preview.
	preview = {},
	-- The peek of a change of a file ({ path =, index = }), or none.
	peek = nil,
	-- What is over the page: { kind = "palette" | "dialog" | "help" | "settings", ... }.
	overlay = nil,
	toasts = {},
	-- The search in the project's files.
	search = {
		query = "",
		regex = false,
		case = false,
		word = false,
		running = false,
		result = nil,
		error = nil,
		chosen = nil,
		collapsed = {},
	},
	-- The pane is being given to a shell ("shell") or textpen is ending ("quit").
	handing_off = nil,
	-- The project's files for the quick open, once read: { files =, lower =, truncated = }.
	file_index = nil,
	-- Calls out that the person waits on: a quiet spinner.
	busy = 0,
	-- When each file was last saved, for the status line's word.
	saved_at = {},
	-- The field the keyboard was last sent to (nil: the page's own keys).
	field = nil,
}

local SIZE = {
	tabs = 36,
	status = 24,
	row = 24,
	sidebar_head = 36,
	-- The widths the page changes its layout at, points.
	narrow = 760,
	medium = 1080,
	-- A page this short shows less around the editor.
	short = 320,
}

-- --- What the page does is gathered into one reply ---------------------------------------------
--
-- What an action asks of the host (an editor's load, the keyboard, the pasteboard, a link) is put
-- here as it is decided, wherever in the page that is, and every way into the page (an event, an
-- answer of the program, a timer) hands it out with its own reply. The languages' colours go with
-- it (`syntaxes`).

local gathered = nil
local syntaxes = {}

local function gather(reply)
	gathered = b.merge(gathered, reply)
end

-- An ask of the editor named `editor`, after the ones asked before it.
local function ask(editor, request)
	gather({ editor = { [editor] = { request } } })
end

local function keyboard(where)
	gather({ redraw = true, keyboard = where })
end

-- The reply of a way into the page: `reply` with what was gathered and what the languages have
-- for their editors (theirs first: they were made against the text as it was).
local function take(reply)
	local out = nil
	for _, syntax in syntaxes do
		local asks = syntax:asks()
		if #asks > 0 then
			out = b.merge(out, { editor = { [syntax.editor] = asks } })
		end
	end
	out = b.merge(out, gathered)
	gathered = nil
	return b.merge(out, reply)
end

-- Asks the program to do `method`; `done(result, err)` is called with its answer, and what it
-- gathered goes out with that answer's reply. `err` is { code =, message = }.
local function call(method, params, done)
	return b.call(method, params, function(result, err)
		local reply = nil
		if done then
			reply = done(result, err)
		end
		return take(reply == nil and true or reply)
	end)
end

-- Runs `fn` `seconds` from now; what it gathers goes out with the wake's reply.
local function after(seconds, fn)
	return b.time.after(seconds, function()
		local reply = fn()
		return take(reply == nil and true or reply)
	end)
end

-- --- Words and pieces ------------------------------------------------------------------------------

-- Whether the pointer is over the element named `id`.
local function hovered(id)
	return ui.hover == id
end

-- One of the pictures of src/assets/icons (Tabler's, MIT), `size` points, in a colour of the theme.
local function icon(name, size, color)
	size = size or 15
	return b.image("assets/icons/" .. name .. ".svg"):size(size, size):tint(color or "dim"):no_shrink()
end

-- The keys of a command as the host's keyboard writes them (`@pencere/keys`: "cmd+shift+k" is
-- "⇧⌘K" on a Mac, "Super+Shift+K" elsewhere). `spec` is the words of a command's keys, or a list of
-- them: the first is the one shown.
local function first_keys(spec)
	if type(spec) == "table" then
		return spec[1]
	end
	return spec
end

local function keys_of(spec)
	spec = first_keys(spec)
	if not spec then
		return nil
	end
	return keys.label(spec)
end

-- Every way a command is done, one after the other.
local function keys_all(spec, between)
	if type(spec) ~= "table" then
		return keys_of(spec)
	end
	local out = {}
	for _, one in spec do
		out[#out + 1] = keys.label(one)
	end
	return table.concat(out, between or "  ")
end

-- Keys as keycaps, small: the signs and the key apart, as a person reads them on a keyboard.
local function kbd(spec, quiet)
	local row = b.div():row():items_center():gap(2):no_shrink()
	local words = first_keys(spec)
	for _, cap in words and keys.caps(words) or {} do
		row:child(
			b.div()
				:px(4)
				:min_w(16)
				:h(16)
				:rounded(4)
				:border(1, quiet and b.alpha("border", 0.8) or "border")
				:row()
				:items_center()
				:justify_center()
				:no_shrink()
				:child(b.text(cap):text_size(10.5):color(quiet and "quiet" or "dim"))
		)
	end
	return row
end

-- A picture turning round, for what is being waited for: the host turns it, a frame at a time.
local function spinner(size, color)
	return icon("loader-2", size, color):motion({ spin = 1 })
end

-- A button that is an icon alone: its name (the tooltip and the screen reader's word) and the keys
-- that do the same.
local function icon_button(id, name, words, options)
	options = options or {}
	local on = options.pressed == true
	local over = hovered(id)
	local ground = on and b.alpha("accent", 0.16) or (over and b.alpha("foreground", 0.08) or nil)
	local element = b.div()
		:press(id)
		:size(options.size or 26, options.size or 26)
		:rounded(5)
		:row()
		:items_center()
		:justify_center()
		:no_shrink()
		:bg(ground)
		:motion({ seconds = 0.11, color = true })
		:label(words)
		:role("button")
		:tip(words, options.keys and keys_of(options.keys) or nil)
		:child(icon(name, options.icon or 15, on and "accent" or (over and "foreground" or "dim")))
	if options.disabled then
		element:disabled(true):opacity(0.4)
	end
	return element
end

-- A button of words: `tone` "primary" for the one that answers ⏎, "danger" for one that loses
-- something.
local function button(id, words, options)
	options = options or {}
	local tone = options.tone
	local over = hovered(id)
	local ground, ink = over and b.alpha("foreground", 0.1) or b.alpha("foreground", 0.05), "foreground"
	if tone == "primary" then
		ground, ink = over and b.mix("accent", "foreground", 0.12) or "accent", "background"
	elseif tone == "danger" then
		ground, ink = over and b.alpha("error", 0.24) or b.alpha("error", 0.14), "error"
	end
	local element = b.div()
		:press(id)
		:row()
		:items_center()
		:gap(6)
		:h(options.h or 26)
		:px(options.px or 10)
		:rounded(6)
		:bg(ground)
		:no_shrink()
		:motion({ seconds = 0.11, color = true })
		:role("button")
		:label(options.label or words)
	if options.icon then
		element:child(icon(options.icon, 14, ink == "background" and "background" or "dim"))
	end
	if options.underline_first then
		element:child(b.rich({
			b.span(utf8.char(utf8.codepoint(words, 1))):underline():color(ink),
			b.span(string.sub(words, utf8.offset(words, 2) or #words + 1)):color(ink),
		}))
	else
		element:child(b.text(words):color(ink):weight(tone == "primary" and 650 or 500))
	end
	if options.keys then
		element:tip(options.tip or words, keys_of(options.keys))
	elseif options.tip then
		element:tip(options.tip)
	end
	if options.disabled then
		element:disabled(true):opacity(0.45)
	end
	return element
end

-- A line across or down, in the theme's colour of rules.
local function rule_x(color)
	return b.div():h(1):w_full():no_shrink():bg(color or "border")
end

local function rule_y(color)
	return b.div():w(1):h_full():no_shrink():bg(color or "border")
end

-- A note at the bottom of the page for a while: `tone` "info", "ok", "warn" or "bad".
local toast_id = 0
local function toast(words, tone, seconds)
	toast_id += 1
	local note = { id = toast_id, words = words, tone = tone or "info", until_ = S.now + (seconds or 4.2) }
	table.insert(S.toasts, note)
	while #S.toasts > 4 do
		table.remove(S.toasts, 1)
	end
	gather(true)
	return note
end

-- The few colours the page speaks in, by tone.
local TONES = { info = "accent", ok = "success", warn = "warning", bad = "error", quiet = "dim" }

-- The actions of the page, by name, filled by the parts that do them: a part calls another's
-- action through here, so that no two parts require each other.
local act = {}

return {
	b = b,
	ui = ui,
	S = S,
	SIZE = SIZE,
	act = act,
	gather = gather,
	ask = ask,
	keyboard = keyboard,
	take = take,
	call = call,
	after = after,
	syntaxes = syntaxes,
	hovered = hovered,
	icon = icon,
	keys_of = keys_of,
	keys_all = keys_all,
	spinner = spinner,
	kbd = kbd,
	icon_button = icon_button,
	button = button,
	rule_x = rule_x,
	rule_y = rule_y,
	toast = toast,
	TONES = TONES,
}
