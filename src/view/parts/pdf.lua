-- A tab of a PDF: its pages one under the other, as wide as the tab (or as large as the person has
-- asked), selectable and searchable. The terminal draws the pages (`pencere.image(name):page(n)`:
-- the file crossed to it once, and each page is drawn at the size it is shown, so a page made
-- larger is sharp); the program reads the PDF for what the terminal does not say: how many pages
-- there are and how large (`picture`), the words of a page and where each letter stands (`pdf-text`),
-- and where a pattern is found in the whole of it (`pdf-find`). What the person selects is worked
-- out here, on the page's own lines, so that nothing goes to the program as the pointer moves.
--
-- Only the pages in view and one beside them are made, in a box that scrolls (the host scrolls it and
-- tells the page, `scrolled`, which pages come into view); the others are room.

local kit = require("../lib/kit")
local text = require("../lib/text")
local docs = require("./docs")
local pictures = require("./pictures")

local b, S, SIZE, act = kit.b, kit.S, kit.SIZE, kit.act
local layout = b.layout

local pdf = {}

-- The room around the pages, points: between two pages and above the first and under the last, and
-- beside a page at the tab's width (and beside one wider than the tab, at the left of the content).
local GAP = 16
local SIDE = 40

-- The scales the zoom keys go by and the longest side a page is shown with are a picture's too
-- (`pictures.lua`).
local STEPS = pictures.STEPS
local LONGEST = pictures.LONGEST

-- How many pages' words are kept, and how many pages one selection may hold (what is copied is read
-- from the words kept, and all of a selection's pages are kept).
local KEPT = 40
local MOST_SELECTED = 100

-- How many pages' words are asked at once.
local ASKS = 4

-- The height of the bar of the search over the tab (its rule too).
local BAR = 37

local MATCH = b.alpha("warning", 0.38)
local CURRENT = b.alpha("warning", 0.55)
local CURRENT_EDGE = b.mix("warning", "#000000", 0.4)
local SELECTION = b.alpha("accent", 0.3)

-- The ground the pages lie on: the theme's own on a dark one, a step darker on a light one, so that a
-- white page does not blend into it.
local GROUND = { light = "#00000012", dark = "#00000000" }

-- --- The tab's pages --------------------------------------------------------------------------------

-- The tab's PDF, or nil where it is none.
function pdf.of(tab)
	if tab and tab.status == "picture" and tab.picture and tab.picture.pdf then
		return tab
	end
	return nil
end

-- The PDF tab in front, or nil.
function pdf.shown()
	return pdf.of(docs.tab_of(S.active))
end

-- What the page keeps for the tab: the pages' sizes (points), how they are laid out, where it is
-- scrolled, the words it has read of the pages, the selection and the search.
local function state(tab)
	local v = tab.sheet
	if v then
		return v
	end
	local widths, heights = {}, {}
	for _, run in tab.picture.pdf.sizes do
		for _ = 1, run[1] do
			widths[#widths + 1] = run[2]
			heights[#heights + 1] = run[3]
		end
	end
	v = {
		count = #widths,
		widths = widths,
		heights = heights,
		-- nil: as wide as the tab; else the scale.
		zoom = nil,
		-- The layout of the pages for the tab's width and the zoom (`geometry`).
		geometry = nil,
		-- Where it was scrolled last, and where it is to be scrolled at the next frame ({ y =, x = }).
		scroll_y = 0,
		scroll_x = 0,
		pending = nil,
		-- The pages made, { first, last }, and the one most in view.
		built = nil,
		current = 1,
		-- The room the box has, as it was made.
		avail = 600,
		view_h = 300,
		-- The words of the pages read ({ lines = }), those being asked, those to ask, and how many calls are out.
		text = {},
		loaded = 0,
		asked = {},
		queue = {},
		out = 0,
		pumping = false,
		-- The selection, { anchor =, head = } of places ({ page =, line =, k = }: k characters into the line),
		-- the drag making it, and what it was last counted to hold.
		sel = nil,
		drag = nil,
		counted = nil,
		-- The search: the call out, and what it found ({ matches =, by_page =, current =, truncated = }).
		finding = nil,
		found = nil,
	}
	tab.sheet = v
	return v
end

-- The name of the box the pages scroll in.
function pdf.box(tab)
	return "pdf:" .. pictures.owner(tab)
end

-- --- Layout ---------------------------------------------------------------------------------------------

-- The room the pages have in a page `size` points wide and tall: the tab's width and height.
function pdf.room(size)
	local side = 0
	if act.sidebar_docked() then
		side = math.clamp(S.settings.sidebarWidth, 160, math.max(size.w - 280, 160)) + 1
	end
	local bar = S.find.open and BAR or 0
	return math.max(size.w - side, 80), math.max(size.h - SIZE.tabs - SIZE.status - bar, 40)
end

-- The scale of page `index`: the tab's width less the sides, or the zoom, and never so large that the
-- terminal could not draw it (`LONGEST`).
local function scale_of(v, avail, index)
	local scale = v.zoom or (math.max(avail - 2 * SIDE, 40) / v.widths[index])
	local longest = math.max(v.widths[index], v.heights[index])
	return math.min(scale, LONGEST / longest)
end

-- Where a page `w` points wide stands across the content in a tab `avail` wide: one that fits with its
-- sides is in the middle of the tab (the content scrolled to its left, as it is unless the person
-- scrolls across a page wider than the tab), one that does not is the side's room from the left.
local function across_of(avail, w)
	if w + 2 * SIDE <= avail then
		return math.floor((avail - w) / 2)
	end
	return SIDE
end

-- Whether page `index` fits in the tab with its sides, as the layout `g` draws it.
local function fits(g, index)
	return g.pw[index] + 2 * SIDE <= g.avail
end

-- Where each page stands in the box (`tops`, from the top of the box, with a gap above the first, and
-- `xs`, from its left), how large it is drawn (`pw`, `ph`, whole points), the box's height and how wide
-- its content is.
local function compute(v, avail)
	local tops, xs, pw, ph = {}, {}, {}, {}
	local y, widest = GAP, 0
	for index = 1, v.count do
		local scale = scale_of(v, avail, index)
		local w = math.max(math.floor(v.widths[index] * scale + 0.5), 1)
		local h = math.max(math.floor(v.heights[index] * scale + 0.5), 1)
		tops[index], xs[index], pw[index], ph[index] = y, across_of(avail, w), w, h
		y += h + GAP
		widest = math.max(widest, w)
	end
	return {
		avail = avail,
		zoom = v.zoom,
		count = v.count,
		tops = tops,
		xs = xs,
		pw = pw,
		ph = ph,
		total = y,
		content_w = math.max(avail, widest + 2 * SIDE),
	}
end

-- How far the box is scrolled: where a scroll is asked to go, else where the host has it, else where it
-- was left (a box not laid out yet).
local function offset_of(tab, v)
	if v.pending then
		return v.pending.y, v.pending.x
	end
	local id = pdf.box(tab)
	if layout.scroll_extent(id) then
		local y, x = layout.scroll_offset(id)
		return y or 0, x or 0
	end
	return v.scroll_y, v.scroll_x
end

-- The first page whose bottom is below `y`.
local function locate(g, y)
	local low, high = 1, g.count
	while low < high do
		local mid = math.floor((low + high) / 2)
		if g.tops[mid] + g.ph[mid] > y then
			high = mid
		else
			low = mid + 1
		end
	end
	return low
end

-- The pages that touch the window from `y` down `view` points (first, last) and the one with most of it.
local function visible(g, y, view)
	local first = locate(g, y)
	local last = locate(g, y + view)
	if last > first and g.tops[last] >= y + view then
		last -= 1
	end
	local current, most = first, -1
	for index = first, last do
		local top = math.max(g.tops[index], y)
		local bottom = math.min(g.tops[index] + g.ph[index], y + view)
		if bottom - top > most then
			current, most = index, bottom - top
		end
	end
	return first, last, current
end

-- The pages made: those in view and one beside them.
local function built_of(g, first, last)
	return math.max(first - 1, 1), math.min(last + 1, g.count)
end

-- The window's height: what the box was laid out at, else what the page has left.
local function view_of(tab, v)
	local extent = layout.scroll_extent(pdf.box(tab))
	return extent and extent.view or v.view_h
end

-- The layout for the tab's width, made again where the width or the zoom changed; the page that was at the
-- top of the view stays there, and across, the page most in view keeps the point that was in the middle of
-- the tab there (one that fits in the tab is in its middle: the content is not scrolled across for it).
local function geometry(tab, v, avail)
	local g = v.geometry
	if g and g.avail == avail and g.zoom == v.zoom then
		return g
	end
	local old = g
	g = compute(v, avail)
	v.geometry = g
	if old then
		local y, x = offset_of(tab, v)
		local ny = y
		if y > old.tops[1] then
			local page = locate(old, y)
			local along = math.max(math.min((y - old.tops[page]) / old.ph[page], 1), 0)
			ny = g.tops[page] + along * g.ph[page]
		end
		local page = math.max(math.min(v.current, g.count), 1)
		local nx = 0
		if not fits(g, page) then
			local along = (x + old.avail / 2 - old.xs[page]) / old.pw[page]
			nx = g.xs[page] + along * g.pw[page] - avail / 2
		end
		v.pending = {
			y = math.max(math.min(ny, g.total - v.view_h), 0),
			x = math.max(math.min(nx, g.content_w - avail), 0),
		}
	end
	return g
end

-- Asks the host to scroll the pages to `y` (and `x`) at the next frame, within the content.
local function scroll_to(tab, v, y, x)
	local g = v.geometry
	if not g then
		return
	end
	local _, now_x = offset_of(tab, v)
	v.pending = {
		y = math.max(math.min(y, g.total - view_of(tab, v)), 0),
		x = math.max(math.min(x or now_x, g.content_w - v.avail), 0),
	}
	kit.gather(true)
end

-- Where the host scrolled the pages to has been done: the next frame is made at it.
function pdf.settle()
	local tab = pdf.shown()
	local v = tab and tab.sheet
	if not (v and v.pending) then
		return false
	end
	local id = pdf.box(tab)
	if not layout.scroll_extent(id) then
		return false
	end
	local pending = v.pending
	v.pending = nil
	layout.scroll_to(id, { y = pending.y, x = pending.x })
	v.scroll_y, v.scroll_x = pending.y, pending.x
	return true
end

-- The host scrolled the box: whether the pages made, or the page most in view, are not those that were.
function pdf.scrolled(event)
	local tab = pdf.shown()
	local v = tab and tab.sheet
	if not (v and v.geometry) then
		return false
	end
	local id = pdf.box(tab)
	for _, box in event.boxes do
		if box.id == id then
			v.scroll_y, v.scroll_x = box.y, box.x
			local first, last, current = visible(v.geometry, box.y, view_of(tab, v))
			local from, to = built_of(v.geometry, first, last)
			local built = v.built
			if current ~= v.current or not built or built[1] ~= from or built[2] ~= to then
				return true
			end
		end
	end
	return false
end

-- --- The words of the pages ---------------------------------------------------------------------------

-- The page's lines as the program said them, with the characters of each cut once.
local function prepared(answer)
	local lines = {}
	for _, line in answer.lines or {} do
		line.chars = text.chars(line.text)
		lines[#lines + 1] = line
	end
	return { lines = lines }
end

-- The pages that may not be let go: those in view and one beside them, and those a selection holds.
local function held(v, page)
	local built = v.built
	if built and page >= built[1] and page <= built[2] then
		return true
	end
	local sel = v.sel
	if sel then
		local from, to = math.min(sel.anchor.page, sel.head.page), math.max(sel.anchor.page, sel.head.page)
		return page >= from and page <= to
	end
	return false
end

-- Lets the pages go that are far from the one in view, the farthest first, past what is kept.
local function prune(v)
	while v.loaded > KEPT do
		local far, away = nil, -1
		for page in v.text do
			if not held(v, page) and math.abs(page - v.current) > away then
				far, away = page, math.abs(page - v.current)
			end
		end
		if not far then
			return
		end
		v.text[far] = nil
		v.loaded -= 1
	end
end

local pump

-- Asks the program for the words of the pages in the queue, a few at a time.
function pump(tab, v)
	local owner = pictures.owner(tab)
	while v.out < ASKS and #v.queue > 0 do
		local page = table.remove(v.queue, 1)
		-- A page that was scrolled past while it waited is not asked for: it is when it is in view again.
		if not v.text[page] and not v.asked[page] and held(v, page) then
			v.asked[page] = true
			v.out += 1
			kit.call("pdf-text", { owner = owner, page = page }, function(answer, err)
				v.out -= 1
				v.asked[page] = nil
				if not text.index_of(S.tabs, tab) then
					return nil
				end
				if v.text[page] == nil then
					v.loaded += 1
				end
				v.text[page] = (err or not answer) and { lines = {} } or prepared(answer)
				prune(v)
				pump(tab, v)
				return true
			end)
		end
	end
end

-- Wants the words of the pages `first` to `last`, those not read yet asked for in a moment.
local function want(tab, v, first, last)
	local queued = false
	for page = first, last do
		if not v.text[page] and not v.asked[page] and not text.index_of(v.queue, page) then
			v.queue[#v.queue + 1] = page
			queued = true
		end
	end
	if queued and not v.pumping then
		v.pumping = true
		kit.after(0, function()
			v.pumping = false
			pump(tab, v)
			return nil
		end)
	end
end

-- --- Places in the words -----------------------------------------------------------------------------------

local function before(a, z)
	if a.page ~= z.page then
		return a.page < z.page
	elseif a.line ~= z.line then
		return a.line < z.line
	end
	return a.k < z.k
end

local function same(a, z)
	return a.page == z.page and a.line == z.line and a.k == z.k
end

-- The two ends of the selection, the earlier first.
local function ordered(sel)
	if before(sel.head, sel.anchor) then
		return sel.head, sel.anchor
	end
	return sel.anchor, sel.head
end

local function selected(v)
	return v.sel ~= nil and not same(v.sel.anchor, v.sel.head)
end

-- The place nearest to `px`, `py` (points from the page's top left corner) in the words of `page`: the
-- line it is nearest, and how many of that line's characters come before it. Whether the point is on the
-- line.
local function place_at(v, page, px, py)
	local layer = v.text[page]
	if not layer or #layer.lines == 0 then
		return nil
	end
	local best, best_gap, on = nil, math.huge, false
	for index, line in layer.lines do
		local dy = 0
		if py < line.y then
			dy = line.y - py
		elseif py > line.y + line.h then
			dy = py - line.y - line.h
		end
		local dx = 0
		if px < line.x then
			dx = line.x - px
		elseif px > line.x + line.w then
			dx = px - line.x - line.w
		end
		local gap = dy * 8 + dx
		if gap < best_gap then
			best, best_gap, on = index, gap, dy == 0 and dx == 0
		end
	end
	local line = layer.lines[best]
	local across = px - line.x
	local k, nearest = 0, math.huge
	for at = 1, #line.xs do
		local distance = math.abs(line.xs[at] - across)
		if distance < nearest then
			k, nearest = at - 1, distance
		end
	end
	return { page = page, line = best, k = k }, on
end

-- The kind of a character, for the run a double click takes.
local function kind_of(c)
	if c == " " or c == "\t" or c == "\u{a0}" then
		return "space"
	elseif #c > 1 or string.find(c, "^[%w_]$") then
		return "word"
	end
	return "mark"
end

-- The places that bound the word at `place` (the run of letters, or of marks, or of spaces it is in).
local function word_at(v, place)
	local line = v.text[place.page].lines[place.line]
	local count = #line.chars
	if count == 0 then
		return place, place
	end
	local at = math.min(place.k + 1, count)
	local kind = kind_of(line.chars[at])
	local from, to = at, at
	while from > 1 and kind_of(line.chars[from - 1]) == kind do
		from -= 1
	end
	while to < count and kind_of(line.chars[to + 1]) == kind do
		to += 1
	end
	return { page = place.page, line = place.line, k = from - 1 }, { page = place.page, line = place.line, k = to }
end

local function line_at(v, place)
	local line = v.text[place.page].lines[place.line]
	return { page = place.page, line = place.line, k = 0 }, { page = place.page, line = place.line, k = #line.chars }
end

-- The pages from the one a selection starts on to the one it ends on.
local function span_of(sel)
	local a, z = ordered(sel)
	return a.page, z.page
end

-- The text a selection holds, and whether a page it spans has not been read yet.
local function selection_text(v)
	if not selected(v) then
		return "", false
	end
	local a, z = ordered(v.sel)
	local out, missing = {}, false
	for page = a.page, z.page do
		local layer = v.text[page]
		if not layer then
			missing = true
		else
			local lines = layer.lines
			local from_line = page == a.page and a.line or 1
			local to_line = page == z.page and math.min(z.line, #lines) or #lines
			for index = from_line, to_line do
				local line = lines[index]
				if line then
					local count = #line.chars
					local from = (page == a.page and index == a.line) and a.k or 0
					local to = (page == z.page and index == z.line) and math.min(z.k, count) or count
					if to > from then
						out[#out + 1] = text.sub(line.text, from + 1, to)
					end
				end
			end
		end
	end
	return table.concat(out, "\n"), missing
end

-- How many words the selection holds, counted again only when the selection or the words read change.
function pdf.selection_words(tab)
	local v = tab.sheet
	if not (v and selected(v)) then
		return 0
	end
	local a, z = ordered(v.sel)
	local key = string.format("%s:%s:%s:%s:%s:%s:%s", a.page, a.line, a.k, z.page, z.line, z.k, v.loaded)
	if v.counted and v.counted.key == key then
		return v.counted.words
	end
	local words = 0
	for _ in string.gmatch((selection_text(v)), "%S+") do
		words += 1
	end
	v.counted = { key = key, words = words }
	return words
end

-- The rectangles (points on the page) that the selection covers on `page`.
local function selection_rects(v, page)
	if not selected(v) then
		return {}
	end
	local a, z = ordered(v.sel)
	if page < a.page or page > z.page then
		return {}
	end
	local layer = v.text[page]
	if not layer then
		return {}
	end
	local out = {}
	local lines = layer.lines
	local from_line = page == a.page and a.line or 1
	local to_line = page == z.page and math.min(z.line, #lines) or #lines
	for index = from_line, to_line do
		local line = lines[index]
		if line then
			local count = #line.chars
			local from = (page == a.page and index == a.line) and a.k or 0
			local to = (page == z.page and index == z.line) and math.min(z.k, count) or count
			if to > from then
				out[#out + 1] = {
					x = line.x + line.xs[from + 1],
					y = line.y,
					w = line.xs[to + 1] - line.xs[from + 1],
					h = line.h,
					id = string.format("pdf-selection:%d:%d", page, index),
				}
			end
		end
	end
	return out
end

-- --- The pages ----------------------------------------------------------------------------------------------

local function mark(rect, sx, sy, ground, id)
	return b.div()
		:id(id)
		:absolute()
		:inset(rect.y * sy, nil, nil, rect.x * sx)
		:size(math.max(rect.w * sx, 1), math.max(rect.h * sy, 1))
		:rounded(2)
		:bg(ground)
end

local function page_view(tab, v, g, index)
	local w, h = g.pw[index], g.ph[index]
	local sx, sy = w / v.widths[index], h / v.heights[index]
	local element = b.div()
		:id("pdf-page:" .. index)
		:size(w, h)
		:no_shrink()
		:bg("#ffffff")
		:shadow(0, 1, 4, b.alpha("shadow", 0.4))
		:child(b.image(tab.picture.name):page(index):size(w, h):fit("contain"):no_shrink():role("image")
			:label(string.format("Page %d of %d", index, v.count)))
	local found = v.found
	if found then
		for _, at in found.by_page[index] or {} do
			local match = found.matches[at]
			local current = at == found.current
			local box = mark(match, sx, sy, current and CURRENT or MATCH, "pdf-match:" .. at)
			if current then
				box:border(1, CURRENT_EDGE)
			end
			element:child(box)
		end
	end
	for _, rect in selection_rects(v, index) do
		element:child(mark(rect, sx, sy, SELECTION, rect.id))
	end
	return element
end

-- The tab: the pages in view and the room of the rest, in a box that scrolls.
function pdf.view(tab, size)
	local v = state(tab)
	local avail, view_h = pdf.room(size)
	v.avail, v.view_h = avail, view_h
	local g = geometry(tab, v, avail)
	local id = pdf.box(tab)
	if not v.pending and not layout.scroll_extent(id) and (v.scroll_y ~= 0 or v.scroll_x ~= 0) then
		-- Back in front: where it was left.
		v.pending = { y = v.scroll_y, x = v.scroll_x }
	end
	local y = offset_of(tab, v)
	local view = view_of(tab, v)
	local first, last, current = visible(g, y, view)
	local from, to = built_of(g, first, last)
	v.built, v.current = { from, to }, current
	want(tab, v, from, to)
	local sheet = b.div():col():items_start():no_shrink()
	if g.content_w > avail then
		sheet:w(g.content_w)
	else
		sheet:w_full()
	end
	sheet:child(b.div():h(g.tops[from]):no_shrink())
	for index = from, to do
		if index > from then
			sheet:child(b.div():h(GAP):no_shrink())
		end
		sheet:child(page_view(tab, v, g, index):ml(g.xs[index]))
	end
	sheet:child(b.div():h(math.max(g.total - (g.tops[to] + g.ph[to]), 0)):no_shrink())
	return b.div():id(id):col():grow(1):min_w(0):min_h(0):scroll():bg(GROUND):child(sheet)
end

-- --- Where the pointer is -------------------------------------------------------------------------------------

local function inside(rect, x, y)
	return rect ~= nil and x >= rect.x and x < rect.x + rect.w and y >= rect.y and y < rect.y + rect.h
end

-- The page nearest to the point `x`, `y` (the page's points) among those made, and the point as
-- points from its top left corner; whether the point is on it.
local function page_at(v, x, y)
	local built = v.built
	if not built then
		return nil
	end
	local best, gap = nil, math.huge
	for index = built[1], built[2] do
		local r = layout.bounds("pdf-page:" .. index)
		if r then
			local d = 0
			if y < r.y then
				d = r.y - y
			elseif y > r.y + r.h then
				d = y - r.y - r.h
			end
			if d < gap then
				best, gap = index, d
			end
		end
	end
	local r = best and layout.bounds("pdf-page:" .. best)
	if not r then
		return nil
	end
	return best, (x - r.x) / r.w * v.widths[best], (y - r.y) / r.h * v.heights[best], gap == 0 and x >= r.x and x < r.x + r.w
end

-- Whether the pointer is over the words of a page (it is a text pointer there).
function pdf.over_text(event)
	local tab = pdf.shown()
	local v = tab and tab.sheet
	if not v or S.overlay or not inside(layout.bounds(pdf.box(tab)), event.x, event.y) then
		return false
	end
	local page, px, py, on = page_at(v, event.x, event.y)
	if not (page and on) then
		return false
	end
	local place, over = place_at(v, page, px, py)
	return place ~= nil and over
end

-- Cuts the selection short where it would hold the words of more than `MOST_SELECTED` pages: its head
-- stops at the last page one selection may hold.
local function clamp_selection(sel)
	local first, last = span_of(sel)
	if last - first >= MOST_SELECTED then
		if before(sel.anchor, sel.head) then
			sel.head = { page = first + MOST_SELECTED - 1, line = math.huge, k = math.huge }
		else
			sel.head = { page = last - MOST_SELECTED + 1, line = 1, k = 0 }
		end
	end
end

-- Sets the end of the selection the drag is making to the point `x`, `y`.
local function drag_to(v, x, y)
	local drag = v.drag
	local page, px, py = page_at(v, x, y)
	if not page then
		return false
	end
	local place = place_at(v, page, px, py)
	if not place then
		return false
	end
	local from, to = place, place
	if drag.mode == "word" then
		from, to = word_at(v, place)
	elseif drag.mode == "line" then
		from, to = line_at(v, place)
	end
	local base = drag.base
	if before(from, base.from) then
		v.sel = { anchor = base.to, head = from }
	else
		v.sel = { anchor = base.from, head = to }
	end
	clamp_selection(v.sel)
	return true
end

local function need_selection(tab, v)
	if v.sel then
		local first, last = span_of(v.sel)
		want(tab, v, first, last)
	end
end

-- A press on the pages: whether it was the pages' own (it starts a selection, or opens the menu).
function pdf.pressed(event)
	local tab = pdf.shown()
	local v = tab and tab.sheet
	if not v or S.overlay or not inside(layout.bounds(pdf.box(tab)), event.x, event.y) then
		return false
	end
	if event.button == "right" then
		pdf.menu(tab, event)
		return true
	elseif event.button ~= "left" then
		return false
	end
	-- The keys are the pages' again, with the search kept open.
	S.focus, S.field = "editor", nil
	kit.keyboard("page")
	local page, px, py = page_at(v, event.x, event.y)
	local place = page and place_at(v, page, px, py)
	if not place then
		v.sel, v.drag = nil, nil
		kit.gather(true)
		return true
	end
	local mods = event.mods or {}
	local count = event.count or 1
	local from, to, mode = place, place, "char"
	if count == 2 then
		from, to = word_at(v, place)
		mode = "word"
	elseif count >= 3 then
		from, to = line_at(v, place)
		mode = "line"
	end
	if mods.shift and v.sel and count < 2 then
		v.sel = { anchor = v.sel.anchor, head = place }
		clamp_selection(v.sel)
		v.drag = { mode = "char", base = { from = v.sel.anchor, to = v.sel.anchor } }
	else
		v.sel = { anchor = from, head = to }
		v.drag = { mode = mode, base = { from = from, to = to } }
	end
	need_selection(tab, v)
	kit.gather(true)
	return true
end

-- The pointer moved with the button held: the selection follows it; at the edge of the pages they scroll.
function pdf.dragged(event)
	local tab = pdf.shown()
	local v = tab and tab.sheet
	if not (v and v.drag) then
		return false
	end
	local box = layout.bounds(pdf.box(tab))
	if box then
		local y, x = offset_of(tab, v)
		if event.y < box.y + 24 then
			scroll_to(tab, v, y - (box.y + 24 - event.y) / 2 - 4, x)
		elseif event.y > box.y + box.h - 24 then
			scroll_to(tab, v, y + (event.y - (box.y + box.h - 24)) / 2 + 4, x)
		end
	end
	if drag_to(v, event.x, event.y) then
		need_selection(tab, v)
	end
	return true
end

-- The button went up: the selection is what it was.
function pdf.released(_event)
	local tab = pdf.shown()
	local v = tab and tab.sheet
	if v and v.drag then
		v.drag = nil
		if v.sel and same(v.sel.anchor, v.sel.head) then
			v.sel = nil
		end
		return true
	end
	return false
end

-- --- Selecting, copying -------------------------------------------------------------------------------------

-- Whether there is a selection to copy and the keys are the pages' (not a field's).
function pdf.can_copy()
	local tab = pdf.shown()
	return tab ~= nil and tab.sheet ~= nil and selected(tab.sheet) and S.field == nil and S.overlay == nil
end

-- Whether the pages can be selected from the keys.
function pdf.can_select()
	return pdf.shown() ~= nil and S.field == nil and S.overlay == nil
end

-- Puts the selection on the pasteboard.
function pdf.copy(tab)
	local v = tab and tab.sheet
	if not (v and selected(v)) then
		return
	end
	need_selection(tab, v)
	local words, missing = selection_text(v)
	if missing then
		kit.toast("Still reading the pages of the selection, try again in a moment", "info", 2.4)
		return
	end
	kit.gather({ copy = words })
end

-- Selects all of the words, to the most a selection may hold.
function pdf.select_all(tab)
	local v = tab and tab.sheet
	if not v then
		return
	end
	local last = math.min(v.count, MOST_SELECTED)
	v.sel = {
		anchor = { page = 1, line = 1, k = 0 },
		head = { page = last, line = math.huge, k = math.huge },
	}
	need_selection(tab, v)
	if last < v.count then
		kit.toast(string.format("A selection holds the words of %d pages at most", MOST_SELECTED), "info", 3)
	end
	kit.gather(true)
end

-- Drops the selection: whether there was one.
function pdf.clear_selection(tab)
	local v = tab and tab.sheet
	if v and v.sel then
		v.sel, v.drag = nil, nil
		kit.gather(true)
		return true
	end
	return false
end

-- The words a selection of one line holds, to look for them (nil for none, or for more than that).
function pdf.selected_words(tab)
	local v = tab and tab.sheet
	if not (v and selected(v)) then
		return nil
	end
	local a, z = ordered(v.sel)
	if a.page ~= z.page or a.line ~= z.line then
		return nil
	end
	local words = selection_text(v)
	if words == "" or #words > 500 then
		return nil
	end
	return words
end

-- The menu of the right button: Copy and Select All.
function pdf.menu(tab, event)
	local v = tab.sheet
	act.menu({
		{ "Copy", "cmd+c", function()
			pdf.copy(tab)
		end, selected(v) },
		{ "Select All", "cmd+a", function()
			pdf.select_all(tab)
		end },
	}, event)
end

-- --- Going places, zoom -----------------------------------------------------------------------------------------

-- The page most in view, and how many there are.
function pdf.page_of(tab)
	local v = tab.sheet
	return v and v.current or 1, tab.picture.pdf.pages
end

-- The scale the page most in view is shown at.
local function current_scale(v)
	local g = v.geometry
	return g and g.pw[v.current] / v.widths[v.current] or 1
end

-- What the status line says of the zoom.
function pdf.zoom_words(tab)
	local v = tab.sheet
	if not (v and v.zoom) then
		return "Fit Width"
	end
	-- The scale the page is drawn at, which is the zoom unless the terminal could not draw so large a page.
	return string.format("%d%%", math.floor(current_scale(v) * 100 + 0.5))
end

-- Scrolls to the top of page `n`.
function pdf.go_to(tab, n)
	local v = tab.sheet
	if not (v and v.geometry) then
		return
	end
	n = math.max(math.min(n, v.count), 1)
	scroll_to(tab, v, v.geometry.tops[n] - GAP)
end

-- Zooms: a step `direction` (1, -1) from the scale now, or back to the tab's width (0).
function pdf.zoom(tab, direction)
	local v = tab.sheet
	if not v then
		return
	end
	if direction == 0 then
		v.zoom = nil
	else
		local now = current_scale(v)
		-- Not past the scale at which the page most in view is as large as the terminal draws one
		-- (`LONGEST`: far past what the steps reach for a page of the sizes people make).
		local most = LONGEST / math.max(v.widths[v.current], v.heights[v.current])
		local to = nil
		if direction > 0 then
			for _, step in STEPS do
				if step > now * 1.001 and step <= most * 1.001 then
					to = step
					break
				end
			end
		else
			for at = #STEPS, 1, -1 do
				if STEPS[at] < now * 0.999 then
					to = STEPS[at]
					break
				end
			end
		end
		if not to then
			return
		end
		v.zoom = to
	end
	kit.gather(true)
end

-- Scrolls by `dy` down and `dx` across (points).
function pdf.scroll_by(tab, dy, dx)
	local v = tab.sheet
	if not v then
		return
	end
	local y, x = offset_of(tab, v)
	scroll_to(tab, v, y + dy, x + dx)
end

-- The keys of the pages while they have the keyboard: whether it was one of theirs.
function pdf.key(name, mods)
	local tab = pdf.shown()
	local v = tab and tab.sheet
	if not v or S.field ~= nil or S.focus == "sidebar" or mods.command or mods.control or mods.alt then
		return false
	end
	local view = view_of(tab, v)
	local step = 48
	if name == "down" then
		pdf.scroll_by(tab, step, 0)
	elseif name == "up" then
		pdf.scroll_by(tab, -step, 0)
	elseif name == "right" then
		pdf.scroll_by(tab, 0, step)
	elseif name == "left" then
		pdf.scroll_by(tab, 0, -step)
	elseif name == "page_down" or (name == " " and not mods.shift) then
		pdf.scroll_by(tab, view - 2 * GAP, 0)
	elseif name == "page_up" or (name == " " and mods.shift) then
		pdf.scroll_by(tab, -(view - 2 * GAP), 0)
	elseif name == "home" then
		scroll_to(tab, v, 0)
	elseif name == "end" then
		scroll_to(tab, v, math.huge)
	elseif name == "escape" and not mods.shift then
		return pdf.clear_selection(tab)
	else
		return false
	end
	return true
end

-- Scrolls to `y` (points from the top of the pages), for a view that goes back to where it was.
function pdf.scroll_to(tab, y)
	local v = tab.sheet
	if v then
		scroll_to(tab, v, y)
	end
end

-- --- The search ------------------------------------------------------------------------------------------------------

-- The matches of the search, and which one is the current.
function pdf.found(tab)
	return tab.sheet and tab.sheet.found
end

-- Scrolls so that match `at` is in view, where it is not.
local function reveal(tab, v, at)
	local g = v.geometry
	local match = v.found and v.found.matches[at]
	if not (g and match) then
		return
	end
	local scale_y = g.ph[match.page] / v.heights[match.page]
	local scale_x = g.pw[match.page] / v.widths[match.page]
	local top = g.tops[match.page] + match.y * scale_y
	local bottom = top + match.h * scale_y
	local y, x = offset_of(tab, v)
	local view = view_of(tab, v)
	local to_y, to_x = y, x
	if top - GAP < y or bottom + GAP > y + view then
		to_y = math.max(top - view / 3, 0)
	end
	local left = g.xs[match.page] + match.x * scale_x
	if fits(g, match.page) then
		-- A page that fits in the tab is in its middle, the content not scrolled across.
		to_x = 0
	elseif left < x or left + match.w * scale_x > x + v.avail then
		to_x = math.max(left - v.avail / 3, 0)
	end
	if to_y ~= y or to_x ~= x then
		scroll_to(tab, v, to_y, to_x)
	end
end

-- Looks for `query` in the words of every page (`options`: regex, case, word); `done(count, current,
-- more, invalid)` is told. The search before is given up.
function pdf.find(tab, query, options, done)
	local v = state(tab)
	if v.finding then
		b.cancel(v.finding)
		v.finding = nil
	end
	v.found = nil
	if query == "" or tab.picture.pdf.text == false then
		done(0, nil, false, false)
		return
	end
	v.finding = kit.call("pdf-find", {
		owner = pictures.owner(tab),
		query = query,
		regex = options.regex,
		caseSensitive = options.case,
		wholeWord = options.word,
	}, function(answer, err)
		v.finding = nil
		if err or not answer then
			done(0, nil, false, true)
			return true
		end
		local by_page = {}
		local first = nil
		for at, match in answer.matches do
			by_page[match.page] = by_page[match.page] or {}
			table.insert(by_page[match.page], at)
			if not first and match.page >= v.current then
				first = at
			end
		end
		local count = #answer.matches
		if count == 0 then
			done(0, nil, false, false)
			return true
		end
		v.found = { matches = answer.matches, by_page = by_page, current = first or 1, truncated = answer.truncated == true }
		reveal(tab, v, v.found.current)
		done(count, v.found.current, v.found.truncated, false)
		return true
	end)
end

-- Goes to the next match (1) or the one before (-1), around the ends; which one it is now.
function pdf.step(tab, direction)
	local v = tab.sheet
	local found = v and v.found
	if not found then
		return nil
	end
	local count = #found.matches
	found.current = (found.current - 1 + direction) % count + 1
	reveal(tab, v, found.current)
	kit.gather(true)
	return found.current
end

-- The search is over: the marks go.
function pdf.stop_finding(tab)
	local v = tab and tab.sheet
	if not v then
		return
	end
	if v.finding then
		b.cancel(v.finding)
		v.finding = nil
	end
	v.found = nil
	kit.gather(true)
end

-- --- The status line ------------------------------------------------------------------------------------------------

-- The keys that work on the pages, for the status line: { keys, words, id }.
function pdf.hints(tab)
	local F = S.find
	if F.open and F.pdf == tab then
		if S.field == "find-query" then
			return {
				{ "enter", "next", "hint:pdf-next" },
				{ "shift+enter", "previous", "hint:pdf-previous" },
				{ "escape", "close", "hint:pdf-close" },
			}
		end
		-- The keys are the pages' (the pointer was on them): Enter is the field's, the commands' keys are not.
		return {
			{ "cmd+g", "next", "hint:pdf-next" },
			{ "cmd+shift+g", "previous", "hint:pdf-previous" },
			{ "escape", "close", "hint:pdf-close" },
		}
	end
	local v = tab.sheet
	if v and selected(v) then
		return {
			{ "cmd+c", "copy", "hint:pdf-copy" },
			{ "cmd+f", "find", "hint:find" },
			{ "escape", "clear", "hint:pdf-clear" },
		}
	end
	return {
		{ "cmd+f", "find", "hint:find" },
		{ "ctrl+g", "go to page", "hint:pdf-page" },
		{ "cmd++", "zoom", "hint:pdf-zoom" },
	}
end

-- A key of the status line, by its name.
function act.pdf_hint(name)
	local tab = pdf.shown()
	if not tab then
		return
	end
	if name == "page" then
		act.open_palette("page")
	elseif name == "zoom" then
		pdf.zoom(tab, 1)
	elseif name == "copy" then
		pdf.copy(tab)
	elseif name == "clear" then
		pdf.clear_selection(tab)
	elseif name == "next" or name == "previous" then
		act.pdf_step(name == "next" and 1 or -1)
	elseif name == "close" then
		act.pdf_find_close()
	end
end

return pdf
