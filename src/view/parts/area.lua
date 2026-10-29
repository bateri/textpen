-- Where the file is: its editor, with a strip above it when something needs a word (a draft from an
-- earlier run, a file changed or removed on disk, one that cannot be written), a page for a file
-- that is not text, and the start page with no file.

local kit = require("../lib/kit")
local text = require("../lib/text")
local docs = require("./docs")
local changes = require("./changes")
local terminal = require("@pencere/terminal")

local b, S, act = kit.b, kit.S, kit.act

local area = {}

-- What the editor is told, from the settings and the file's language; its caret and its selection
-- as the person's terminal draws them, where the terminal says how (textpen writes none of them).
function area.options(tab, doc)
	local settings = S.settings
	local config = doc.lang.config
	local numbers = "absolute"
	if settings.lineNumbers == "relative" then
		numbers = "relative"
	elseif settings.lineNumbers == "off" then
		numbers = "hidden"
	end
	return terminal.editor({
		wrap = settings.wrap,
		tab_width = math.clamp(settings.tabSize, 1, 16),
		hard_tabs = config.hard_tabs == true or not settings.insertSpaces,
		numbers = numbers,
		gutters = changes.gutters(),
		read_only = (tab.info and tab.info.readonly) or false,
		whitespace = settings.whitespace,
		indent_guides = settings.indentGuides,
		comment = config.comment,
		indent = config.indent,
		folds = config.folds or "page",
		pairs = config.pairs,
		line_height = 1.6,
		padding = 6,
		scroll_margin = 3,
		current_line = "all",
		matches = "selection",
		colors = {
			current_line = b.alpha("foreground", 0.045),
			selection = b.alpha("accent", 0.3),
			selection_blurred = b.alpha("foreground", 0.12),
			found = b.alpha("warning", 0.24),
			matches = b.alpha("accent", 0.16),
			brackets = b.alpha("accent", 0.55),
			numbers = "quiet",
			current_number = "foreground",
			whitespace = b.alpha("foreground", 0.24),
		},
	})
end

local function banner(tone, icon_name, words, buttons)
	local color = kit.TONES[tone] or "dim"
	local row = b.div()
		:row()
		:items_center()
		:gap(9)
		:px(12)
		:py(6)
		:min_h(34)
		:no_shrink()
		:bg(b.alpha(color, 0.1))
		:child(kit.icon(icon_name, 15, color))
		:child(b.rich(words):wrap():grow(1):min_w(0))
	for _, button in buttons or {} do
		row:child(button)
	end
	return b.div():col():no_shrink():child(row):child(kit.rule_x(b.alpha(color, 0.3)))
end

-- The strip above a file that needs a word, or nil.
local function banner_of(tab)
	local info = tab.info
	if not info then
		return nil
	end
	local name = text.basename(tab.path)
	if info.draft then
		-- `draft.at` is the program's time of day, milliseconds; the page's is `pencere.time.wall()`.
		local wall = b.time.wall()
		local when = info.draft.at and info.draft.at > 0 and wall
			and (" " .. text.ago(math.max(wall - info.draft.at / 1000, 0)) .. ", ") or " "
		return banner("info", "history", {
			"Unsaved changes to ",
			b.span(name):bold(),
			" were kept" .. when .. "when textpen last ended.",
		}, {
			kit.button("banner-draft-restore", "Restore", { tone = "primary", h = 24 }),
			kit.button("banner-draft-drop", "Discard", { h = 24 }),
		})
	elseif info.conflict then
		return banner("warn", "alert-triangle", { b.span(name):bold(), " changed on disk while it had changes here." }, {
			kit.button("banner-conflict-reload", "Reload from Disk", { h = 24 }),
			kit.button("banner-conflict-keep", "Keep Mine", { tone = "primary", h = 24 }),
		})
	elseif info.missing and info.mtime ~= 0 then
		return banner("bad", "trash", { b.span(name):bold(), " was deleted on disk. Saving writes it again." }, {
			kit.button("banner-missing-close", "Close", { h = 24 }),
		})
	elseif info.readonly then
		return banner("quiet", "lock", { "Read-only: textpen cannot write to " .. name .. "." })
	end
	return nil
end

-- A page for a tab that shows no text: its file is being read, or is not text.
local function placeholder(tab)
	local box = b.div():col():grow(1):items_center():justify_center():gap(8):p(20)
	if tab.status == "loading" then
		return box:child(kit.spinner(22, "quiet"))
	end
	local reason = tab.unopenable
	box:child(kit.icon(reason and reason.reason == "binary" and "binary" or "file-alert", 34, "quiet"))
	box:child(b.text(text.basename(tab.path)):weight(650))
	local words = reason and reason.message or (tab.error or "This file could not be opened.")
	if reason and reason.size and reason.size > 0 then
		words ..= " · " .. text.bytes(reason.size)
	end
	box:child(b.text(words):color("dim"):wrap():align("center"):max_w(480))
	box:child(kit.button("placeholder-close", "Close Tab", { h = 26 }))
	return box
end

function area.view(size)
	local tab = docs.tab_of(S.active)
	local column = b.div():col():grow(1):min_w(0):min_h(0)
	if tab and tab.doc and tab.doc.loaded then
		local strip = banner_of(tab)
		if strip then
			column:child(strip)
		end
	end
	if tab and tab.doc and tab.doc.loaded then
		local bar = act.find_bar(tab.doc)
		if bar then
			column:child(bar)
		end
	end
	local row = b.div():row():grow(1):min_h(0)
	local markdown = tab and act.is_markdown(tab.path)
	local previewing = markdown and tab.doc and tab.doc.loaded and text.index_of(S.preview, tab.path) ~= nil
	local only = previewing and S.layout ~= "wide"
	local shown = false
	for _, entry in docs.editors(area.options) do
		local editor = entry.element:grow(1):min_w(0):h_full():bg("background")
		local front = entry.tab == tab and entry.doc.loaded and not only
		if front then
			shown = true
			local peek = changes.peek(entry.doc)
			if peek then
				editor:child(peek)
			end
			if S.focus == "editor" and S.overlay == nil and S.field == entry.doc.id then
				editor:focused(true)
			end
		else
			editor:hidden()
		end
		row:child(editor)
	end
	if previewing then
		if shown then
			row:child(kit.rule_y())
		end
		row:child(act.preview_view(tab, size))
	end
	if not tab then
		row:child(act.welcome_view(size))
	elseif not (tab.doc and tab.doc.loaded) then
		row:child(placeholder(tab))
	end
	column:child(row)
	return column
end

-- A press in the area: whether it was one of its own.
function area.press(id)
	local path = S.active
	if id == "banner-draft-restore" then
		act.draft(path, true)
	elseif id == "banner-draft-drop" then
		act.draft(path, false)
	elseif id == "banner-conflict-reload" then
		act.revert(path, false)
	elseif id == "banner-conflict-keep" then
		act.keep_mine(path)
	elseif id == "banner-missing-close" or id == "placeholder-close" then
		act.close_tab(path)
	else
		return false
	end
	return true
end

return area
