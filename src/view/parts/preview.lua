-- A Markdown file as it reads: beside its text in a wide page, in its place in a narrow one. It is
-- read from the editor's text as it is, again once a change was told; it scrolls with the caret,
-- roughly where the caret is in the text. Links open in the person's browser through the terminal
-- (an http or https address alone); images, which the page cannot reach, are named; raw HTML is
-- shown as text, never run.

local kit = require("../lib/kit")
local text = require("../lib/text")
local markdown = require("../lib/markdown")
local languages = require("../lib/languages")
local docs = require("./docs")

local b, S, act = kit.b, kit.S, kit.act
local layout = b.layout

local preview = {}

local BOX = "preview"
-- The most text read for the preview, characters: a reading of it fits in what a call of the page may
-- take, with the rest of the page's work beside it.
local READ = 64000
-- The most elements a preview draws, and so the most a call makes for it (a page has 20 000 to a
-- frame and a call 250 ms): a long file's preview shows its start.
local ELEMENTS = 2500

local cache = { id = nil, version = nil, blocks = nil, cut = false, links = {}, waiting = false }

local STYLES = languages.STYLES

-- The look of a token's name in the code of a fence (its parts, each by itself or by what is before
-- its last dot).
local function look_of(name)
	local merged = nil
	for part in string.gmatch(name, "%S+") do
		local found = STYLES[part]
		while found == nil do
			part = string.match(part, "^(.*)%.[^%.]*$")
			if not part then
				break
			end
			found = STYLES[part]
		end
		if found then
			merged = merged or {}
			for key, value in found do
				merged[key] = value
			end
		end
	end
	return merged
end

local function parse(doc)
	local length = layout.editor_length(doc.id) or 0
	local source = layout.editor_text(doc.id, 0, math.min(length, READ)) or ""
	local blocks, cut = markdown.parse(source)
	cache.id, cache.version, cache.blocks, cache.cut = doc.id, doc.edited, blocks, cut or length > READ
end

-- The blocks of the file: read at once the first time, and again a moment after the typing stops
-- (what was read before is shown meanwhile), so that a keystroke in a long file costs no reading.
local function blocks_of(doc)
	if cache.id ~= doc.id or cache.blocks == nil then
		parse(doc)
	elseif cache.version ~= doc.edited and not cache.waiting then
		cache.waiting = true
		kit.after(0.2, function()
			cache.waiting = false
			local now = docs.by_id(cache.id)
			if now and now.edited ~= cache.version then
				parse(now)
			end
			return true
		end)
	end
	return cache.blocks, cache.cut
end

-- --- Drawing ------------------------------------------------------------------------------------------

local budget = 0

local function spans_of(runs, size)
	local spans = {}
	for _, run in runs do
		budget -= 1
		local span = b.span(run.text)
		if run.code then
			span:family("terminal"):bg("raised2"):color("foreground")
		end
		if run.strong then
			span:bold()
		end
		if run.em then
			span:italic()
		end
		if run.strike then
			span:strike()
		end
		if run.image then
			span:color("dim")
		end
		if run.link then
			local index = #cache.links + 1
			cache.links[index] = run.link
			span:link("md-link:" .. index):color("info")
		end
		spans[#spans + 1] = span
	end
	if #spans == 0 then
		spans[1] = b.span("")
	end
	return b.rich(spans):wrap():text_size(size or 13.5)
end

local HEADING_SIZES = { 23, 19, 16, 14.5, 13.5, 13 }

local function code_view(block)
	local lang = block.lang ~= "" and languages.find(block.lang) or nil
	local syntax = lang and lang.syntax
	local state = nil
	local box = b.div():col():p(12):rounded(8):bg("raised1"):family("terminal"):text_size(12.5):clip()
	for _, line in block.lines do
		if budget <= 0 then
			box:child(b.text("…"):color("quiet"))
			break
		end
		budget -= 1
		if syntax and #line <= 2000 then
			local ok, tokens, after = pcall(syntax.tokens, syntax, line, state)
			if ok then
				state = after
				local spans = {}
				local at = 0
				for _, token in tokens do
					if token.from > at then
						spans[#spans + 1] = b.span(text.sub(line, at + 1, token.from))
					end
					local span = b.span(text.sub(line, token.from + 1, token.to))
					local look = look_of(token.name)
					if look then
						if look.color then
							span:color(look.color)
						end
						if look.weight and look.weight >= 600 then
							span:bold()
						end
						if look.italic then
							span:italic()
						end
					end
					spans[#spans + 1] = span
					at = token.to
				end
				local rest = text.sub(line, at + 1)
				if rest ~= "" or #spans == 0 then
					spans[#spans + 1] = b.span(rest == "" and " " or rest)
				end
				box:child(b.rich(spans))
			else
				box:child(b.text(line == "" and " " or line))
			end
		else
			box:child(b.text(line == "" and " " or line))
		end
	end
	return box
end

local blocks_view

local function block_view(block, depth)
	budget -= 1
	local kind = block.kind
	if kind == "heading" then
		local heading = b.div():col():gap(6):pt(block.level <= 2 and 10 or 6)
			:child(spans_of(block.inline, HEADING_SIZES[block.level]):weight(700):role("heading", block.level))
		if block.level <= 2 then
			heading:child(kit.rule_x())
		end
		return heading
	elseif kind == "paragraph" then
		return spans_of(block.inline)
	elseif kind == "code" then
		return code_view(block)
	elseif kind == "html" then
		return b.div():p(10):rounded(6):bg("raised1"):child(b.text(block.text):family("terminal"):text_size(12):color("dim"):wrap())
	elseif kind == "rule" then
		return kit.rule_x()
	elseif kind == "quote" then
		return b.div():row():gap(12)
			:child(b.div():w(3):rounded(2):bg("border"):no_shrink())
			:child(blocks_view(block.blocks, depth + 1):color("dim"):grow(1):min_w(0))
	elseif kind == "list" then
		local list = b.div():col():gap(4):pl(4)
		for index, item in block.items do
			if budget <= 0 then
				break
			end
			local marker
			if item.task ~= nil then
				marker = b.div():size(14, 14):mt(3):rounded(3):border(1, "border"):row():items_center():justify_center()
					:bg(item.task and b.alpha("accent", 0.2) or nil)
					:role("checkbox"):checked(item.task)
				if item.task then
					marker:child(kit.icon("check", 11, "accent"))
				end
			elseif block.ordered then
				marker = b.text((block.start + index - 1) .. "."):color("dim"):text_size(13.5)
			else
				marker = b.text(depth % 2 == 0 and "•" or "◦"):color("dim"):text_size(13.5)
			end
			list:child(b.div():row():gap(8):items_start()
				:child(b.div():w(22):row():justify_end():no_shrink():child(marker))
				:child(blocks_view(item.blocks, depth + 1):grow(1):min_w(0)))
		end
		return list
	elseif kind == "table" then
		local columns = #block.head
		local tracks = {}
		for index = 1, columns do
			tracks[index] = b.fr(1)
		end
		local grid = b.div():grid(tracks):border(1, "border"):rounded(4):clip()
		local function cell(runs, head, index)
			budget -= 1
			return b.div():px(10):py(6):border(1, b.alpha("border", 0.7)):bg(head and "raised1" or nil)
				:child(spans_of(runs, 13):weight(head and 700 or 400):align(block.align[index] or "start"))
		end
		for index, runs in block.head do
			grid:child(cell(runs, true, index))
		end
		for _, row in block.rows do
			if budget <= 0 then
				break
			end
			for index, runs in row do
				grid:child(cell(runs, false, index))
			end
		end
		return grid
	end
	return b.div()
end

blocks_view = function(blocks, depth)
	local column = b.div():col():gap(12)
	for _, block in blocks do
		if budget <= 0 then
			break
		end
		column:child(block_view(block, depth))
	end
	return column
end

-- The preview of the tab's file. What it draws is made again only when what it reads changed or
-- the room it has did: a frame of typing in the text beside it costs nothing here.
local built = { blocks = nil, width = nil, element = nil }

function act.preview_view(tab, size)
	local doc = tab.doc
	local blocks, cut = blocks_of(doc)
	local only = S.layout ~= "wide"
	local width = only and size.w or math.floor((size.w - (act.sidebar_docked() and S.settings.sidebarWidth or 0)) / 2)
	if built.blocks == blocks and built.width == width and built.only == only then
		return built.element
	end
	cache.links = {}
	budget = ELEMENTS
	local article = blocks_view(blocks, 0):w_full():max_w(780)
	if cut or budget <= 0 then
		article:child(b.text("The rest of the file is not shown in the preview."):color("quiet"):text_size(12))
	end
	local box = b.div()
		:id(BOX)
		:col()
		:items_center()
		:min_w(0)
		:h_full()
		:scroll()
		:family("system")
		:bg("background")
		:child(b.div():col():w_full():items_center():px(28):pt(22):pb(40):child(article))
	if only then
		box:grow(1)
	else
		-- Beside the text, half of what the editor had.
		box:w(width):no_shrink()
	end
	built.blocks, built.width, built.only, built.element = blocks, width, only, box
	return box
end

-- A press in the preview: a link opens in the person's browser.
function preview.press(id)
	local index = tonumber(string.match(id or "", "^md%-link:(%d+)$") or "")
	if not index then
		return false
	end
	local url = cache.links[index]
	if url and string.match(url, "^https?://") then
		kit.gather({ link = url })
	elseif url then
		kit.toast("Only a web address opens from the preview: " .. url, "info")
	end
	return true
end

-- The caret moved in a file shown with its preview: the preview goes to about the same place.
local last_line = nil
function preview.follow(event)
	if event.type ~= "selection" then
		return
	end
	local doc = docs.by_id(event.editor)
	if not (doc and doc.path == S.active and text.index_of(S.preview, doc.path)) then
		return
	end
	local main = event.ranges[event.main]
	local line = main and layout.line_of(doc.id, main.head)
	local lines = layout.editor_lines(doc.id) or 1
	if not line or line == last_line or lines <= 1 then
		return
	end
	last_line = line
	local extent = layout.scroll_extent(BOX)
	if extent and extent.max_offset > 0 then
		layout.scroll_to(BOX, { y = extent.max_offset * line / (lines - 1) })
	end
end

return preview
