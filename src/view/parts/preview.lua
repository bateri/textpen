-- A Markdown file as it reads: beside its text in a wide page, in its place in a narrow one. The
-- reading and the drawing are the kit's (`@pencere/markdown`): a document kept for the file's text,
-- read again where a change touched it (a moment after the typing stops), and drawn as a long list,
-- only the blocks near what is in view made, so a long file is shown to its end. It goes where the
-- caret goes. Links open in the person's browser through the terminal (an http or https address
-- alone); images, which the page cannot reach, are named; raw HTML is shown as text, never run.

local kit = require("../lib/kit")
local text = require("../lib/text")
local markdown = require("@pencere/markdown")
local docs = require("./docs")

local b, S, act = kit.b, kit.S, kit.act
local layout = b.layout

local preview = {}

local BOX = "preview"
-- The most text read for the preview, characters: a file longer than this shows its start.
local MOST = 4 * 1024 * 1024
-- How many characters are read from an editor at a time (a read answers 1 MiB at most).
local PIECE = 128 * 1024
-- How wide the text of the preview is, at the most, points.
local COLUMN = 780

local document = markdown.document()
-- The editor `document` is read from, the version of its text it has and whether the file is longer
-- than what is read; whether a reading of it waits for the typing to stop.
local held = { id = nil, version = nil, cut = false, waiting = false }

local function read(doc)
	local length = layout.editor_length(doc.id) or 0
	local pieces = {}
	local at = 0
	while at < math.min(length, MOST) do
		pieces[#pieces + 1] = layout.editor_text(doc.id, at, math.min(at + PIECE, length, MOST)) or ""
		at += PIECE
	end
	held.id, held.version, held.cut = doc.id, doc.edited, length > MOST
	document:set(table.concat(pieces))
end

-- The document is of the file shown: read at once the first time, and again a moment after the
-- typing stops (what was read before is shown meanwhile), so that a keystroke in a long file costs no
-- reading.
local function keep(doc)
	if held.id ~= doc.id then
		read(doc)
	elseif held.version ~= doc.edited and not held.waiting then
		held.waiting = true
		kit.after(0.2, function()
			held.waiting = false
			local now = docs.by_id(held.id)
			if now and now.edited ~= held.version then
				read(now)
			end
			return true
		end)
	end
end

-- A link of the preview was pressed: an address of the web opens in the browser.
local function link(href)
	if string.match(href, "^https?://") then
		return { link = href }
	end
	kit.toast("Only a web address opens from the preview: " .. href, "info")
	return nil
end

local OPTIONS = {
	id = "markdown",
	headings = { 23, 19, 16, 14.5, 13.5, 13 },
	colors = { link = "info", code = "raised1", head = "raised1" },
	link = link,
}

-- The preview of the tab's file. What it draws is made again only where the file's blocks changed
-- or the room it has did.
function act.preview_view(tab, size)
	local doc = tab.doc
	keep(doc)
	local only = S.layout ~= "wide"
	local width = only and size.w or math.floor((size.w - (act.sidebar_docked() and S.settings.sidebarWidth or 0)) / 2)
	local side = math.max(20, math.floor((width - COLUMN) / 2))
	-- The list is a box that scrolls and has no room of its own around its rows: the box around it has.
	local box = b.div()
		:id("preview-room")
		:min_w(0)
		:h_full()
		:family("system")
		:text_size(13.5)
		:bg("background")
		:px(side)
		:pt(22)
		:child(document:list(BOX, OPTIONS):size_full())
	if only then
		box:grow(1)
	else
		-- Beside the text, half of what the editor had.
		box:w(width):no_shrink()
	end
	return box
end

-- The page was laid out: the blocks made are as tall as the layout says. Whether one was not as
-- tall as it stood.
function preview.settle()
	return document:settle()
end

-- What the page is told, for a press on a link of the preview (`markdown.handle`): its reply.
function preview.handle(event)
	if S.active and text.index_of(S.preview, S.active) then
		return markdown.handle(event)
	end
	return nil
end

-- The caret moved in a file shown with its preview: the preview goes to about the same place.
local last_line = nil
function preview.follow(event)
	if event.type ~= "selection" then
		return
	end
	local doc = docs.by_id(event.editor)
	if not (doc and doc.path == S.active and text.index_of(S.preview, doc.path) and held.id == doc.id) then
		return
	end
	local main = event.ranges[event.main]
	local line = main and layout.line_of(doc.id, main.head)
	if not line or line == last_line then
		return
	end
	last_line = line
	document:reveal(line + 1)
end

return preview
