-- Markdown read into blocks and runs, for the preview: headings, paragraphs, fenced and indented
-- code, quotes, lists (with tasks), tables, rules; inside a block, code, strong and emphasised words,
-- struck words, links, images (named, as the page cannot show them) and addresses. GitHub's flavour,
-- read simply: what CommonMark says of the corners (a list in a list's lazy line, a link's title,
-- references) is not all here. Raw HTML is shown as text, never run.

local markdown = {}

-- The most blocks read from one text: a preview of a very long file shows its start.
local MAX_BLOCKS = 4000

local function trim(s)
	return (string.gsub(string.gsub(s, "^%s+", ""), "%s+$", ""))
end

local function indent_of(line)
	local spaces = string.match(line, "^[ \t]*")
	return #(string.gsub(spaces, "\t", "    "))
end

-- Takes `count` columns of indent off a line.
local function dedent(line, count)
	local at, columns = 1, 0
	while columns < count and at <= #line do
		local c = string.sub(line, at, at)
		if c == " " then
			columns += 1
		elseif c == "\t" then
			columns += 4
		else
			break
		end
		at += 1
	end
	return string.sub(line, at)
end

local function is_blank(line)
	return string.match(line, "^%s*$") ~= nil
end

local function fence_of(line)
	local indent, marks, info = string.match(line, "^( ? ? ?)(```+)%s*([^`]*)$")
	if not marks then
		indent, marks, info = string.match(line, "^( ? ? ?)(~~~+)%s*(.*)$")
	end
	if marks then
		return marks, trim(info or ""), #indent
	end
	return nil
end

local function is_rule(line)
	local stripped = string.gsub(line, "%s", "")
	return #stripped >= 3 and (string.match(stripped, "^%-+$") or string.match(stripped, "^%*+$") or string.match(stripped, "^_+$")) ~= nil
end

local function list_marker(line)
	local indent, bullet, rest = string.match(line, "^(%s*)([%-%*%+])%s+(.*)$")
	if bullet then
		return { indent = #indent, ordered = false, content = #indent + 2, rest = rest }
	end
	local number, punct
	indent, number, punct, rest = string.match(line, "^(%s*)(%d+)([%.%)])%s+(.*)$")
	if number and #number <= 9 then
		return { indent = #indent, ordered = true, start = tonumber(number), content = #indent + #number + 2, rest = rest }
	end
	-- A marker with nothing after it.
	indent, bullet = string.match(line, "^(%s*)([%-%*%+])$")
	if bullet then
		return { indent = #indent, ordered = false, content = #indent + 2, rest = "" }
	end
	return nil
end

local function split_row(line)
	local inner = trim(line)
	inner = string.gsub(inner, "^|", "")
	inner = string.gsub(inner, "|$", "")
	local cells = {}
	local cell = {}
	local at = 1
	while at <= #inner do
		local c = string.sub(inner, at, at)
		if c == "\\" and string.sub(inner, at + 1, at + 1) == "|" then
			cell[#cell + 1] = "|"
			at += 2
		elseif c == "|" then
			cells[#cells + 1] = trim(table.concat(cell))
			cell = {}
			at += 1
		else
			cell[#cell + 1] = c
			at += 1
		end
	end
	cells[#cells + 1] = trim(table.concat(cell))
	return cells
end

local function is_delimiter_row(line)
	if not string.find(line, "-", 1, true) then
		return false
	end
	for _, cell in split_row(line) do
		if not string.match(cell, "^:?%-+:?$") then
			return false
		end
	end
	return true
end

-- Whether a line starts a block of its own (and so ends a paragraph).
local function starts_block(line)
	return fence_of(line) ~= nil
		or string.match(line, "^ ? ? ?#+%s") ~= nil
		or string.match(line, "^ ? ? ?#+$") ~= nil
		or string.match(line, "^%s*>") ~= nil
		or is_rule(line)
		or list_marker(line) ~= nil
end

-- --- Inline ---------------------------------------------------------------------------------------

-- The runs of a block's text: { text =, strong =, em =, code =, strike =, link =, image = }.
function markdown.inline(source)
	local runs = {}
	local style = { strong = false, em = false, strike = false }
	local buffer = {}
	local function flush(link)
		if #buffer > 0 then
			runs[#runs + 1] = { text = table.concat(buffer), strong = style.strong, em = style.em, strike = style.strike, link = link }
			buffer = {}
		end
	end
	local function push(words, extra)
		flush()
		local run = { text = words, strong = style.strong, em = style.em, strike = style.strike }
		for key, value in extra do
			run[key] = value
		end
		runs[#runs + 1] = run
	end
	local at = 1
	local length = #source
	while at <= length do
		local c = string.sub(source, at, at)
		if c == "\\" and string.match(string.sub(source, at + 1, at + 1), "%p") then
			buffer[#buffer + 1] = string.sub(source, at + 1, at + 1)
			at += 2
		elseif c == "\\" and at == length then
			buffer[#buffer + 1] = "\n"
			at += 1
		elseif c == "\n" then
			-- A soft break is a space; two spaces before it a hard one.
			if #buffer > 0 and string.sub(buffer[#buffer], -1) == " " and at > 2 and string.sub(source, at - 2, at - 1) == "  " then
				buffer[#buffer + 1] = "\n"
			else
				buffer[#buffer + 1] = " "
			end
			at += 1
		elseif c == "`" then
			local ticks = string.match(source, "^`+", at)
			local close = string.find(source, ticks, at + #ticks, true)
			if close then
				local code = string.sub(source, at + #ticks, close - 1)
				if string.match(code, "^ .* $") and not string.match(code, "^%s+$") then
					code = string.sub(code, 2, -2)
				end
				push(code, { code = true })
				at = close + #ticks
			else
				buffer[#buffer + 1] = ticks
				at += #ticks
			end
		elseif c == "!" and string.sub(source, at + 1, at + 1) == "[" then
			local alt, url, stop = string.match(source, "^!%[([^%]]*)%]%(%s*<?([^%s%)>]*)>?[^%)]*%)()", at)
			if alt then
				push("▣ " .. (alt ~= "" and alt or url), { image = url })
				at = stop
			else
				buffer[#buffer + 1] = c
				at += 1
			end
		elseif c == "[" then
			local words, url, stop = string.match(source, "^%[([^%]]*)%]%(%s*<?([^%s%)>]*)>?[^%)]*%)()", at)
			if words then
				flush()
				for _, run in markdown.inline(words) do
					run.link = url
					run.strong = run.strong or style.strong
					run.em = run.em or style.em
					runs[#runs + 1] = run
				end
				at = stop
			else
				buffer[#buffer + 1] = c
				at += 1
			end
		elseif c == "<" and string.match(source, "^<https?://[^>%s]+>", at) then
			local url = string.match(source, "^<(https?://[^>%s]+)>", at)
			push(url, { link = url })
			at += #url + 2
		elseif c == "h" and string.match(source, "^https?://", at) and (at == 1 or string.match(string.sub(source, at - 1, at - 1), "[%s%(]")) then
			local url = string.match(source, "^https?://[%w%-%._~:/%?#%[%]@!%$&'%*%+,;=%%]+", at)
			url = string.gsub(url, "[%.,;:!%?%)]+$", "")
			push(url, { link = url })
			at += #url
		elseif c == "~" and string.sub(source, at, at + 1) == "~~" and (style.strike or string.find(source, "~~", at + 2, true)) then
			flush()
			style.strike = not style.strike
			at += 2
		elseif (c == "*" or c == "_") and string.sub(source, at + 1, at + 1) == c
			and (style.strong or string.find(source, c .. c, at + 2, true)) then
			flush()
			style.strong = not style.strong
			at += 2
		elseif (c == "*" or c == "_") then
			local before = at > 1 and string.sub(source, at - 1, at - 1) or " "
			local after = string.sub(source, at + 1, at + 1)
			local opens = not style.em and after ~= "" and not string.match(after, "%s")
				and string.find(source, c, at + 1, true) ~= nil and not (c == "_" and string.match(before, "[%w]"))
			local closes = style.em and not string.match(before, "%s") and not (c == "_" and string.match(after, "[%w]"))
			if opens or closes then
				flush()
				style.em = not style.em
			else
				buffer[#buffer + 1] = c
			end
			at += 1
		else
			local stop = string.find(source, "[\\\n`!%[<h~%*_]", at + 1) or (length + 1)
			buffer[#buffer + 1] = string.sub(source, at, stop - 1)
			at = stop
		end
	end
	flush()
	return runs
end

-- --- Blocks ---------------------------------------------------------------------------------------

local parse

local function paragraph_text(lines)
	local out = {}
	for index, line in lines do
		out[index] = trim(line)
		if string.match(line, "  $") then
			out[index] ..= "  "
		end
	end
	return table.concat(out, "\n")
end

parse = function(lines, depth)
	local blocks = {}
	local at = 1
	local count = #lines
	local paragraph = nil
	local function close_paragraph()
		if paragraph then
			blocks[#blocks + 1] = { kind = "paragraph", inline = markdown.inline(paragraph_text(paragraph)) }
			paragraph = nil
		end
	end
	while at <= count and #blocks < MAX_BLOCKS do
		local line = lines[at]
		local marks, info, fence_indent = fence_of(line)
		if is_blank(line) then
			close_paragraph()
			at += 1
		elseif marks then
			close_paragraph()
			local body = {}
			at += 1
			while at <= count do
				local closing = string.match(lines[at], "^%s*(" .. string.gsub(marks, ".", "%%%0") .. "+)%s*$")
				if closing then
					at += 1
					break
				end
				body[#body + 1] = dedent(lines[at], fence_indent)
				at += 1
			end
			blocks[#blocks + 1] = { kind = "code", lang = string.match(info, "^[^%s{]*"), lines = body }
		elseif string.match(line, "^ ? ? ?#+%s") or string.match(line, "^ ? ? ?#+$") then
			close_paragraph()
			local hashes, words = string.match(line, "^%s*(#+)%s*(.-)%s*$")
			if #hashes <= 6 then
				words = string.gsub(words, "%s+#+$", "")
				words = string.gsub(words, "^#+$", "")
				blocks[#blocks + 1] = { kind = "heading", level = #hashes, inline = markdown.inline(words) }
			else
				paragraph = paragraph or {}
				paragraph[#paragraph + 1] = line
			end
			at += 1
		elseif paragraph and (string.match(line, "^ ? ? ?=+%s*$") or string.match(line, "^ ? ? ?%-+%s*$")) then
			-- A setext heading: the paragraph above, underlined.
			local level = string.match(line, "=") and 1 or 2
			blocks[#blocks + 1] = { kind = "heading", level = level, inline = markdown.inline(paragraph_text(paragraph)) }
			paragraph = nil
			at += 1
		elseif is_rule(line) then
			close_paragraph()
			blocks[#blocks + 1] = { kind = "rule" }
			at += 1
		elseif string.match(line, "^%s*>") then
			close_paragraph()
			local inner = {}
			while at <= count do
				local this = lines[at]
				if string.match(this, "^%s*>") then
					inner[#inner + 1] = string.gsub(this, "^%s*> ?", "", 1)
				elseif not is_blank(this) and #inner > 0 and not is_blank(inner[#inner]) and not starts_block(this) then
					inner[#inner + 1] = this
				else
					break
				end
				at += 1
			end
			blocks[#blocks + 1] = { kind = "quote", blocks = depth < 8 and parse(inner, depth + 1) or {} }
		elseif list_marker(line) and (not paragraph or list_marker(line).rest ~= "") then
			close_paragraph()
			local first = list_marker(line)
			local list = { kind = "list", ordered = first.ordered, start = first.start or 1, items = {} }
			while at <= count do
				local marker = list_marker(lines[at])
				if not marker or marker.ordered ~= first.ordered or marker.indent > first.indent + 3 then
					break
				end
				local body = { marker.rest }
				at += 1
				local blank_before = false
				while at <= count do
					local this = lines[at]
					local next_marker = list_marker(this)
					if is_blank(this) then
						blank_before = true
						body[#body + 1] = ""
						at += 1
					elseif indent_of(this) >= marker.content then
						body[#body + 1] = dedent(this, marker.content)
						blank_before = false
						at += 1
					elseif next_marker and next_marker.indent <= first.indent + 3 then
						break
					elseif not blank_before and not starts_block(this) then
						body[#body + 1] = this
						at += 1
					else
						break
					end
				end
				while #body > 0 and is_blank(body[#body]) do
					table.remove(body)
				end
				local task = nil
				local mark, rest = string.match(body[1] or "", "^%[([ xX])%]%s+(.*)$")
				if mark then
					task = mark ~= " "
					body[1] = rest
				end
				list.items[#list.items + 1] = { task = task, blocks = depth < 8 and parse(body, depth + 1) or {} }
			end
			blocks[#blocks + 1] = list
		elseif string.find(line, "|", 1, true) and at < count and is_delimiter_row(lines[at + 1]) then
			close_paragraph()
			local head = split_row(line)
			local align = {}
			for index, cell in split_row(lines[at + 1]) do
				align[index] = (string.match(cell, "^:.*:$") and "center") or (string.match(cell, ":$") and "end") or "start"
			end
			local rows = {}
			at += 2
			while at <= count and not is_blank(lines[at]) and string.find(lines[at], "|", 1, true) do
				rows[#rows + 1] = split_row(lines[at])
				at += 1
			end
			local function cells_of(row)
				local out = {}
				for index = 1, #head do
					out[index] = markdown.inline(row[index] or "")
				end
				return out
			end
			local table_rows = {}
			for index, row in rows do
				table_rows[index] = cells_of(row)
			end
			blocks[#blocks + 1] = { kind = "table", head = cells_of(head), align = align, rows = table_rows }
		elseif not paragraph and indent_of(line) >= 4 then
			local body = {}
			while at <= count and (indent_of(lines[at]) >= 4 or is_blank(lines[at])) do
				body[#body + 1] = dedent(lines[at], 4)
				at += 1
			end
			while #body > 0 and is_blank(body[#body]) do
				table.remove(body)
			end
			blocks[#blocks + 1] = { kind = "code", lang = "", lines = body }
		elseif not paragraph and string.match(line, "^%s*<[%a!/]") then
			local body = {}
			while at <= count and not is_blank(lines[at]) do
				body[#body + 1] = lines[at]
				at += 1
			end
			blocks[#blocks + 1] = { kind = "html", text = table.concat(body, "\n") }
		else
			paragraph = paragraph or {}
			paragraph[#paragraph + 1] = line
			at += 1
		end
	end
	close_paragraph()
	return blocks
end

-- The blocks of a text; and whether some were left out (a text past the most read).
function markdown.parse(source)
	local lines = {}
	for line in string.gmatch(source .. "\n", "([^\n]*)\n") do
		lines[#lines + 1] = (string.gsub(line, "\r$", ""))
	end
	local blocks = parse(lines, 0)
	return blocks, #blocks >= MAX_BLOCKS
end

return markdown
