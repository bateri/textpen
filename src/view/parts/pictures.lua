-- The pictures the page draws: a tab of a PNG, a JPEG or an SVG file shows the picture, fitted into
-- the tab with its size under it, a tab of a PDF shows its pages (`pdf.lua`), and a Markdown file's
-- preview draws the pictures it names that are in the project. The program reads the file, measures it and gives it to the terminal to hold
-- (`picture`); the page draws it by the name it is told (`sha256:` and the file's digest) and never
-- sees its bytes. When the tab closes the program lets the file go (`release`).
--
-- What is not a picture the terminal draws (a file of the name that is text, one that says no size,
-- a terminal that holds no files for the view) is what it was: text to edit, or the placeholder of
-- a file that is not text.

local kit = require("../lib/kit")
local text = require("../lib/text")

local b, S = kit.b, kit.S

local pictures = {}

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

-- The size a picture of `width` by `height` is drawn at in a room of `room_w` by `room_h`: smaller
-- in its proportions where the room is smaller, never larger.
function pictures.fitted(width, height, room_w, room_h)
	local scale = math.min(1, math.max(room_w, 1) / width, math.max(room_h, 1) / height)
	return math.max(1, math.floor(width * scale + 0.5)), math.max(1, math.floor(height * scale + 0.5))
end

-- --- A tab of a picture --------------------------------------------------------------------------

-- Asks for the picture of the tab's file. `plain()` is what is done where the file is none, or the
-- terminal holds nothing for it: it opens as any file does. With `options.keep` the tab goes on
-- showing what it showed (a picture on its way into the project) until the answer comes.
function pictures.open(tab, plain, options)
	if text.extension(tab.path) == "pdf" and b.div().page == nil then
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
-- and the line under it that says its size.
function pictures.room(size)
	local side = 0
	if kit.act.sidebar_docked() then
		side = math.clamp(S.settings.sidebarWidth, 160, math.max(size.w - 280, 160)) + 1
	end
	local margin = 2 * 24
	return math.max(size.w - side - margin, 40), math.max(size.h - kit.SIZE.tabs - kit.SIZE.status - margin - 36, 40)
end

-- The tab of a picture the person gave that is on its way into the project: the file they gave,
-- fitted into the room, until the program holds the picture.
local function uploading(tab, size)
	local row = tab.upload
	local room_w, room_h = pictures.room(size)
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

-- The tab's page: the picture, and under it how large the file is and how many pixels.
function pictures.view(tab, size)
	local picture = tab.picture
	if tab.upload and not picture then
		return uploading(tab, size)
	end
	local room_w, room_h = pictures.room(size)
	local w, h = pictures.fitted(picture.width, picture.height, room_w, room_h)
	local shown = b.image(picture.name)
		:size(w, h)
		:fit("contain")
		:no_shrink()
		:role("image")
		:label(text.basename(tab.path))
	local frame = b.div():no_shrink():border(1, "border"):bg("raised1"):child(shown)
	local words = text.bytes_fine(picture.size) .. " · " .. picture.width .. " × " .. picture.height
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
		:child(frame)
		:child(b.text(words):color("dim"):text_size(12))
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
