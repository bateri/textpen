-- The pictures the page draws: a tab of a PNG, a JPEG or an SVG file shows the picture, fitted into
-- the tab with its size under it, and a Markdown file's preview draws the pictures it names that are
-- in the project. The program reads the file, measures it and gives it to the terminal to hold
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

local NAMED = { png = true, jpg = true, jpeg = true, svg = true }

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
-- terminal holds nothing for it: it opens as any file does.
function pictures.open(tab, plain)
	tab.status = "loading"
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
		if err or answer.refused == "not-a-picture" then
			-- The program holds nothing for it.
			tab.owner = nil
			plain()
		elseif answer.refused == "too-large" then
			tab.status = "unopenable"
			tab.unopenable = { path = tab.path, reason = "too-large", size = answer.size, message = "Too large to preview" }
		else
			tab.status = "picture"
			tab.picture = answer
		end
		return nil
	end)
end

-- The tab is closed: what the program holds for it goes.
function pictures.closed(tab)
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

-- The tab's page: the picture, and under it how large the file is and how many pixels.
function pictures.view(tab, size)
	local picture = tab.picture
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

return pictures
