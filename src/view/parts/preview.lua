-- A Markdown file as it reads: beside its text in a wide page, in its place in a narrow one. The
-- reading and the drawing are the kit's (`@pencere/markdown`): a document kept for the file's text,
-- read again where a change touched it (a moment after the typing stops), and drawn as a long list,
-- only the blocks near what is in view made, so a long file is shown to its end. It goes where the
-- caret goes. Links open in the person's browser through the terminal (an http or https address
-- alone); a picture is drawn where it is in the project, fitted to the width of the text, and its
-- words are shown where it is not; raw HTML is shown as text, never run.

local kit = require("../lib/kit")
local text = require("../lib/text")
local markdown = require("@pencere/markdown")
local docs = require("./docs")
local pictures = require("./pictures")
local uploads = require("./uploads")

local b, S, act = kit.b, kit.S, kit.act

local preview = {}

local BOX = "preview"
-- The most text read for the preview, bytes: a file longer than this shows its start.
local MOST = 4 * 1024 * 1024
-- How wide the text of the preview is, at the most, points.
local COLUMN = 780

local document = markdown.document()
-- The editor `document` is read from, the version of its text it has and whether the file is longer
-- than what is read; whether a reading of it waits for the typing to stop.
local held = { id = nil, version = nil, cut = false, waiting = false }

local function read(doc)
	local words, cut = docs.text(doc, MOST)
	held.id, held.version, held.cut = doc.id, doc.edited, cut
	document:set(words)
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

-- The tallest a picture of the preview is, points.
local PICTURE_HEIGHT = 520

-- What draws a picture of the file (`image` of the markdown module): the picture where the program
-- has one, else the words the file gave it. A new function when the answers or the room changed,
-- which is what makes the module draw its blocks again.
local drawing = { key = nil, image = nil }

local function image_for(tab, room)
	local key = tab.path .. ":" .. room .. ":" .. pictures.generation() .. ":" .. uploads.generation()
	if drawing.key ~= key then
		drawing.key = key
		drawing.image = function(src, alt, title)
			local coming = uploads.pending(tab, src)
			if coming then
				return uploads.placeholder(coming)
			end
			local picture = pictures.named_in(tab, src)
			if not picture then
				local words = (alt ~= nil and alt ~= "") and alt or src or ""
				return b.text(words):color("dim"):wrap()
			end
			local w, h = pictures.fitted(picture.width, picture.height, room, PICTURE_HEIGHT)
			local element = b.image(picture.name):size(w, h):fit("contain"):no_shrink():role("image")
			if alt and alt ~= "" then
				element:label(alt)
			end
			if title and title ~= "" then
				element:tip(title)
			end
			return element
		end
	end
	return drawing.image
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
	local options = table.clone(OPTIONS)
	options.image = image_for(tab, math.max(width - 2 * side, 40))
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
		:child(document:list(BOX, options):size_full())
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
	-- The line the caret is on, from 1, as the docs worked it out from the text the selection is of.
	local line = docs.where(doc).line
	if line == last_line then
		return
	end
	last_line = line
	document:reveal(line)
end

return preview
