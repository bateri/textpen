-- The pictures the page draws: a tab of a PNG, a JPEG or an SVG file shows the picture, fitted into
-- the tab and made larger by the person (its size is in the status line), a tab of a PDF shows its
-- pages (`pdf.lua`), and a Markdown file's
-- preview draws the pictures it names that are in the project. The program reads the file, measures it and gives it to the terminal to hold
-- (`picture`); the page draws it by the name it is told (`sha256:` and the file's digest) and never
-- sees its bytes. When the tab closes the program lets the file go (`release`).
--
-- What is not a picture the terminal draws (a file of the name that is text, one that says no size,
-- a terminal that holds no files for the view) is what it was: text to edit, or the placeholder of
-- a file that is not text.

local kit = require("../lib/kit")
local text = require("../lib/text")
local docs = require("./docs")

local b, S = kit.b, kit.S
local layout = b.layout

local pictures = {}

-- Whether the terminal draws a page of a PDF: the kit that does gave the elements `page`
-- (`Element:page`), an older terminal's have none. A test of an older terminal sets it false.
pictures.draws_pdf = b.div().page ~= nil

-- The scales the zoom keys go by (1 is a point of the picture or of the page to a point of the tab), and
-- the longest side a picture or a page is shown with, points: the terminal draws what is seen, in
-- parts of at most 4096 pixels, so what bounds one is the 65536 pixels its whole may come to (16384
-- points at four to the point); a picture or a page of the sizes people make is sharp at every step
-- to 400%.
pictures.STEPS = { 0.25, 0.33, 0.5, 0.67, 0.75, 1, 1.25, 1.5, 2, 3, 4 }
pictures.LONGEST = 16384

-- How many times its own size a picture is shown at, at most; the room around it, and what the line
-- under it was (its size, now in the status line).
local MOST = 8
local MARGIN = 24

-- How many answers have come, for the preview to know it is to be drawn again.
local generation = 0

local NAMED = { png = true, jpg = true, jpeg = true, svg = true, pdf = true }

-- Whether the file is named as a picture the terminal draws. The program says whether it is one.
function pictures.named(path)
	return NAMED[text.extension(path)] == true
end

-- What a tab holds the program's pictures by: its path when it first asked, which a rename of the
-- file does not change.
function pictures.owner(tab)
	tab.owner = tab.owner or tab.path
	return tab.owner
end

-- The scale a picture of `width` by `height` is drawn at in a room of `room_w` by `room_h`: smaller
-- in its proportions where the room is smaller, never larger (1).
function pictures.fit(width, height, room_w, room_h)
	return math.min(1, math.max(room_w, 1) / width, math.max(room_h, 1) / height)
end

-- The size it is drawn at.
function pictures.fitted(width, height, room_w, room_h)
	local scale = pictures.fit(width, height, room_w, room_h)
	return math.max(1, math.floor(width * scale + 0.5)), math.max(1, math.floor(height * scale + 0.5))
end

-- The tab's picture that is zoomed (a PDF's pages are `pdf.lua`'s), or nil.
function pictures.of(tab)
	if tab and tab.status == "picture" and tab.picture and not tab.picture.pdf then
		return tab
	end
	return nil
end

-- The picture in front, or nil.
function pictures.shown()
	return pictures.of(docs.tab_of(S.active))
end

-- The name of the box the picture is zoomed in.
function pictures.box(tab)
	return "picture:" .. pictures.owner(tab)
end

-- --- A tab of a picture --------------------------------------------------------------------------

-- Asks for the picture of the tab's file. `plain()` is what is done where the file is none, or the
-- terminal holds nothing for it: it opens as any file does. With `options.keep` the tab goes on
-- showing what it showed (a picture on its way into the project) until the answer comes.
function pictures.open(tab, plain, options)
	if text.extension(tab.path) == "pdf" and not pictures.draws_pdf then
		-- A terminal older than the kit that draws a page of a PDF: the file is not sent to it.
		tab.status = "unopenable"
		tab.unopenable = { path = tab.path, reason = "no-pdf", message = "This terminal does not show PDFs" }
		return
	end
	if not (options and options.keep) then
		tab.status = "loading"
	end
	tab.asking = true
	local owner = pictures.owner(tab)
	S.busy += 1
	kit.call("picture", { path = tab.path, owner = owner }, function(answer, err)
		S.busy -= 1
		tab.asking = nil
		if not text.index_of(S.tabs, tab) then
			-- The tab was closed while the program was at it.
			pictures.release(owner)
			return nil
		end
		tab.upload = nil
		if err or answer.refused == "not-a-picture" then
			-- The program holds nothing for it.
			tab.owner = nil
			plain()
		elseif answer.refused == "too-large" then
			tab.status = "unopenable"
			tab.unopenable = { path = tab.path, reason = "too-large", size = answer.size, message = "Too large to preview" }
		else
			tab.status = "picture"
			-- A file that comes in place of the one shown is another: its pages are not the old ones.
			pictures.drop_sheet(tab)
			tab.picture = answer
		end
		return nil
	end)
end

-- What the page kept of the pages of a PDF (`pdf.lua`) goes: the search out is given up, and the words
-- not yet asked for are not.
function pictures.drop_sheet(tab)
	local v = tab.sheet
	if v then
		if v.finding then
			b.cancel(v.finding)
		end
		v.queue = {}
		tab.sheet = nil
	end
end

-- The tab is closed: what the program holds for it goes.
function pictures.closed(tab)
	pictures.drop_sheet(tab)
	if tab.owner then
		pictures.release(tab.owner)
		tab.owner = nil
	end
end

function pictures.release(owner)
	kit.call("release", { owner = owner }, function()
		return nil
	end)
end

-- The files of the project changed under the pictures a Markdown file names: those that were not
-- there are asked for again.
function pictures.changed()
	for _, tab in S.tabs do
		for src, known in tab.pictures or {} do
			if known.gone then
				tab.pictures[src] = nil
			end
		end
	end
	generation += 1
end

-- The room a picture has in the tab, points: what the page leaves the tab, less a margin around it
-- and `below`, the height of what is drawn under it.
function pictures.room(size, below)
	local side = 0
	if kit.act.sidebar_docked() then
		side = math.clamp(S.settings.sidebarWidth, 160, math.max(size.w - 280, 160)) + 1
	end
	local margin = 2 * MARGIN
	return math.max(size.w - side - margin - 2, 40),
		math.max(size.h - kit.SIZE.tabs - kit.SIZE.status - margin - 2 - (below or 0), 40)
end

-- The tab of a picture the person gave that is on its way into the project: the file they gave,
-- fitted into the room, until the program holds the picture.
local function uploading(tab, size)
	local row = tab.upload
	local room_w, room_h = pictures.room(size, 36)
	local shown = b.image("file:" .. row.handle)
		:size(room_w, room_h)
		:fit("contain")
		:no_shrink()
		:role("image")
		:label(text.basename(tab.path))
	return b.div()
		:col()
		:grow(1)
		:min_w(0)
		:min_h(0)
		:items_center()
		:justify_center()
		:gap(12)
		:p(24)
		:bg("background")
		:child(shown)
		:child(b.text(text.bytes_fine(row.size)):color("dim"):text_size(12))
end

-- What the zoom of a picture comes to, as the tab and its status line show it: `shot` is what the page
-- knows of the picture in its room (`pictures.view`), `z` the scale of its box. `pct` is how much of its
-- own size the picture is shown at, and `fit` that it is not larger than the fit (and has its frame).
local function looks(shot, z)
	return { fit = z <= 1.001, pct = math.floor(shot.f * z * 100 + 0.5) }
end

local function same(a, b_)
	return a.fit == b_.fit and a.pct == b_.pct
end

-- The scale of the picture's box now: the terminal's last word, else what was asked.
local function scale_of(tab)
	return layout.zoom_of(pictures.box(tab)) or 1
end

-- The tab's page: the picture in a box that scrolls and that the person zooms (the terminal does it,
-- by a pinch, a double tap or the wheel with the command key; the scale 1 is the picture fitted into
-- the tab, and the keys step to the others). What the box holds is laid out at the scale, so it is as
-- large as the picture and its margin, and the room that is left is the box's whichever way it is
-- scrolled: the picture is in the middle of what grows to fill the box, and does not go out of reach.
function pictures.view(tab, size)
	local picture = tab.picture
	if tab.upload and not picture then
		return uploading(tab, size)
	end
	local room_w, room_h = pictures.room(size)
	local f = pictures.fit(picture.width, picture.height, room_w, room_h)
	local w, h = pictures.fitted(picture.width, picture.height, room_w, room_h)
	-- Larger than the picture's own size by `MOST`, and not past what the terminal draws of one.
	local most = math.max(math.min(MOST, pictures.LONGEST / math.max(picture.width, picture.height)), 1)
	local max = math.max(most / f, 1)
	-- A double tap goes to the picture's own size, or to twice the fit where it is already about it.
	local smart = 1 / f
	if smart < 1.25 then
		smart = 2
	end
	local shot = { f = f, max = max }
	local look = looks(shot, scale_of(tab))
	shot.look = look
	tab.zoomed = shot
	local shown = b.image(picture.name)
		:size(w, h)
		:fit("contain")
		:no_shrink()
		:role("image")
		:label(text.basename(tab.path))
	-- The frame is there while the picture is fitted into the room; larger, the picture is the room.
	local frame = b.div()
		:no_shrink()
		:border(1, look.fit and "border" or b.alpha("border", 0))
		:bg("raised1")
		:child(shown)
	local held = b.div():col():items_center():justify_center():p(MARGIN):child(frame)
	-- One cell that is as large as what is in it, and as large as the box where that is less.
	return b.div()
		:id(pictures.box(tab))
		:grid({ "auto" })
		:grid_rows({ "auto" })
		:grow(1)
		:min_w(0)
		:min_h(0)
		:scroll()
		:zoom({ min = 1, max = max, smart = math.min(smart, max) })
		:bg("background")
		:child(held)
end

-- The size of the file and the pixels, as the status line says them.
function pictures.caption(tab)
	local picture = tab.picture
	return text.bytes_fine(picture.size) .. " · " .. picture.width .. " × " .. picture.height
end

-- What kind of file it is.
function pictures.kind(tab)
	local extension = text.extension(tab.path)
	return extension == "jpg" and "JPEG" or string.upper(extension)
end

-- What the status line says of the zoom: the fit, or how much of its own size the picture is shown at.
function pictures.zoom_words(tab)
	local shot = tab.zoomed
	if not shot then
		return "Fit"
	end
	local look = looks(shot, scale_of(tab))
	return look.fit and "Fit" or string.format("%d%%", look.pct)
end

-- The terminal zoomed or scrolled the boxes: whether the picture's room or its words are not what
-- they were.
function pictures.scrolled(event)
	local tab = pictures.shown()
	local shot = tab and tab.zoomed
	if not shot then
		return false
	end
	local id = pictures.box(tab)
	for _, box in event.boxes do
		if box.id == id and not (shot.look and same(looks(shot, box.zoom or 1), shot.look)) then
			return true
		end
	end
	return false
end

-- Zooms: a step `direction` (1, -1) of the picture's own size from where it is, or to fit (0).
function pictures.zoom(tab, direction)
	local shot = tab and tab.zoomed
	if not shot then
		return
	end
	local id = pictures.box(tab)
	local now = scale_of(tab)
	local to = 1
	if direction > 0 then
		to = shot.max
		for _, step in pictures.STEPS do
			if step > shot.f * now * 1.001 then
				to = math.min(step / shot.f, shot.max)
				break
			end
		end
	elseif direction < 0 then
		for at = #pictures.STEPS, 1, -1 do
			local step = pictures.STEPS[at]
			if step < shot.f * now * 0.999 then
				to = math.max(step / shot.f, 1)
				break
			end
		end
	end
	if math.abs(to - now) > 0.0005 then
		layout.zoom_to(id, to, { animate = false })
	end
	kit.gather(true)
end

-- --- The pictures of a Markdown file ------------------------------------------------------------------
--
-- A preview is drawn while the page is made, which asks nothing of the program: a picture it names is
-- asked for a moment later (once for each, by what the file wrote), and the preview is drawn again
-- when the answer comes (`generation` is what tells it). Until then, and for a picture the program
-- does not find, is not in the project or cannot show, it is its words, as it was.

local queue = {}
local queued = false

function pictures.generation()
	return generation
end

-- What a name is, where it is no file of the project's disk: an address, a path from the top of the
-- disk, a part of the page.
local function is_file_name(src)
	if src == "" or string.sub(src, 1, 1) == "/" or string.sub(src, 1, 1) == "#" then
		return false
	end
	return string.find(src, "^[%a][%w+.%-]*:") == nil
end

local function ask_queued()
	queued = false
	local jobs = queue
	queue = {}
	for _, job in jobs do
		local tab, src = job.tab, job.src
		local owner = pictures.owner(tab)
		kit.call("picture", { path = src, owner = owner, from = tab.path }, function(answer, err)
			if not text.index_of(S.tabs, tab) then
				-- The file was closed while the program was at it.
				pictures.release(owner)
				return nil
			end
			if err or answer.refused then
				tab.pictures[src] = { gone = true }
			else
				tab.pictures[src] = { picture = answer }
			end
			generation += 1
			return true
		end)
	end
	return nil
end

-- The picture a Markdown file `tab` names as `src`, once the program has said: its answer
-- (`Picture`), or nil where there is none (yet).
function pictures.named_in(tab, src)
	local path = string.match(src, "^[^?#]*") or src
	-- A Markdown file shows images: the program refuses a PDF it names.
	if not (is_file_name(src) and pictures.named(path)) or text.extension(path) == "pdf" then
		return nil
	end
	tab.pictures = tab.pictures or {}
	local known = tab.pictures[src]
	if known == nil then
		tab.pictures[src] = { asking = true }
		queue[#queue + 1] = { tab = tab, src = src }
		if not queued then
			queued = true
			kit.after(0, ask_queued)
		end
		return nil
	end
	return known.picture
end

return pictures
