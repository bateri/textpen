-- The size of the page's text, by points: how many larger than the terminal's own text the page is drawn
-- (a reply's `text_size`: the terminal draws this page that much larger, and its own text stays as it was,
-- so nothing of the person's terminal is larger when textpen is gone). It is kept in the settings and said
-- when the page opens. The keys (⌘+ ⌘- ⌘0), a pinch the boxes that zoom did not take, and the settings'
-- sheet make it.

local kit = require("../lib/kit")

local S, act = kit.S, kit.act

local textsize = {}

-- As the program keeps it (`TEXT_SIZES`): below it the text is too small to read, above it a pane holds
-- too few columns to be a pane.
textsize.MIN = -4
textsize.MAX = 24

-- How far a pinch goes for a step: the log of how many times larger the fingers made it, which is added
-- over the gesture, so a pinch of 1.1 and one of 1.1 again are a pinch of 1.21.
local STEP = 0.08

-- What a pinch has come to that is not a step yet.
local pinch = 0

function textsize.current()
	return S.settings and S.settings.textSize or 0
end

function textsize.clamp(size)
	return math.max(textsize.MIN, math.min(textsize.MAX, size))
end

-- The size to `size`, kept and asked of the terminal. Whether it changed.
function textsize.set(size)
	size = textsize.clamp(size)
	if size == textsize.current() then
		return false
	end
	act.change_settings({ textSize = size })
	return true
end

-- `by` points more (or fewer).
function textsize.step(by)
	return textsize.set(textsize.current() + by)
end

-- What the terminal is told when the page opens: the size that was kept.
function textsize.say()
	kit.gather({ text_size = textsize.current() })
end

-- What the status line and the settings say of the size.
function textsize.words(size)
	size = size or textsize.current()
	if size == 0 then
		return "As the terminal's"
	end
	return string.format("%+d pt", size)
end

-- A pinch no box that zooms took: a step at every `STEP` it comes to, the way it goes.
function textsize.magnify(event)
	if event.phase == "begin" then
		pinch = 0
	end
	pinch += math.log(math.max(1 + event.by, 0.01))
	while pinch >= STEP do
		pinch -= STEP
		if not textsize.step(1) then
			pinch = 0
		end
	end
	while pinch <= -STEP do
		pinch += STEP
		if not textsize.step(-1) then
			pinch = 0
		end
	end
	if event.phase == "end" then
		pinch = 0
	end
end

return textsize
