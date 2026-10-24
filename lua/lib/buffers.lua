-- The open files. The program holds each one's text, not the view: what the view sends its program
-- travels through the terminal's input queue in frames of 511 bytes, so it sends what changed, a
-- few hundred milliseconds of typing at a time, and saving is a word to the program rather than
-- the whole file. The text the program holds is also what survives the view — a shell, a restart
-- of the terminal — and, written to a draft, a ⌃C or a dropped connection.
--
-- Places in a text are UTF-16 units, as the view's editor counts them (`edit`, `slice`, the length
-- of a draft); the text itself is UTF-8 bytes, and `pencere.text` does the counting.

local pencere = require("pencere")
local fs, json, time, text = pencere.fs, pencere.json, pencere.time, pencere.text

local paths = require("lib.paths")
local project_module = require("lib.project")
local store = require("lib.store")
local wire = require("lib.wire")

local M = {}

local BOM = "\239\187\191"

-- An edit made on a version the program no longer holds, a buffer that is not open: the view
-- tells it from any other failure by the code.
local function out_of_sync(words)
	error({ code = "out-of-sync", message = words }, 0)
end
M.out_of_sync = out_of_sync

local function now_ms()
	return math.floor(time.now() * 1000)
end

local function message_of(failure)
	return (string.gsub(tostring(failure), "\nstack traceback:.*$", ""))
end

local function mtime_of(path)
	return project_module.modified_ms(path)
end

local function is_dirty(buffer)
	return buffer.text ~= buffer.saved or buffer.eol ~= buffer.saved_eol
end

local function info_of(buffer)
	local info = {
		path = buffer.path,
		version = buffer.version,
		dirty = is_dirty(buffer),
		eol = buffer.eol,
		bom = buffer.bom,
		mtime = buffer.mtime,
		missing = buffer.missing,
		conflict = buffer.conflict,
		readonly = buffer.readonly,
	}
	if buffer.draft then
		info.draft = { at = buffer.draft.at, length = text.units(buffer.draft.text) }
	end
	return info
end

local function with_text(info, content)
	info.text = content
	return info
end

local function with_eol(content, eol)
	if eol == "crlf" then
		return (string.gsub(content, "\n", "\r\n"))
	end
	return content
end

local function unopenable(path, reason, size, message)
	return { path = path, reason = reason, size = size, message = message }
end

-- A file as text, with its line ends made `\n` and what they were remembered.
local function read_file_text(absolute, name)
	local stated, info = pcall(fs.stat, absolute)
	if not stated then
		return unopenable(name, "unreadable", 0, message_of(info))
	end
	if info == nil then
		-- A file that is not there yet: it opens empty and comes to be when it is saved.
		return { text = "", eol = "lf", bom = false, mtime = 0, missing = true, readonly = false }
	end
	if info.kind == "dir" then
		return unopenable(name, "directory", 0, name .. " is a directory")
	end
	if info.size > project_module.TEXT_LIMIT then
		return unopenable(
			name,
			"too-large",
			info.size,
			string.format(
				"%s is %.1f MB; textpen opens files up to %d MB",
				name,
				info.size / 1024 / 1024,
				project_module.TEXT_LIMIT / 1024 / 1024
			)
		)
	end
	local read, bytes = pcall(fs.read, absolute)
	if not read then
		return unopenable(name, "unreadable", info.size, message_of(bytes))
	end
	if project_module.is_binary(bytes) then
		return unopenable(name, "binary", info.size, name .. " is not text")
	end
	if not text.valid(bytes) then
		return unopenable(name, "binary", info.size, name .. " is not UTF-8 text")
	end
	-- The decoder takes a byte order mark off by itself; it is put back when the file is written.
	local bom = string.sub(bytes, 1, 3) == BOM
	local raw = bom and string.sub(bytes, 4) or bytes
	local _, crlf = string.gsub(raw, "\r\n", "")
	local _, newlines = string.gsub(raw, "\n", "")
	local lf = newlines - crlf
	return {
		text = (string.gsub(raw, "\r\n?", "\n")),
		eol = crlf > lf and "crlf" or "lf",
		bom = bom,
		mtime = info.modified * 1000,
		missing = false,
		readonly = not fs.writable(absolute),
	}
end

-- A buffer as it can go to the view in one message: whole, or announced, to be read in pieces.
function M.sendable(opened)
	local content = opened.text
	if content == nil then
		return opened
	end
	local units = text.units(content)
	if units <= wire.WIRE_BUDGET / 6 then
		return opened
	end
	if wire.wire_size(content) <= wire.WIRE_BUDGET then
		return opened
	end
	opened.text = ""
	opened.partial = { length = units }
	return opened
end

local Buffers = {}
Buffers.__index = Buffers

function M.new(project, state_directory)
	local self = setmetatable({}, Buffers)
	self.project = project
	self.drafts = paths.join(state_directory, "drafts")
	-- In the order they were opened.
	self.names = {}
	self.by_name = {}
	self.timers = {}
	return self
end

function Buffers:add(name, buffer)
	if self.by_name[name] == nil then
		self.names[#self.names + 1] = name
	end
	self.by_name[name] = buffer
end

function Buffers:delete(name)
	self.by_name[name] = nil
	for index, known in ipairs(self.names) do
		if known == name then
			table.remove(self.names, index)
			break
		end
	end
end

function Buffers:each()
	local list = {}
	for _, name in ipairs(self.names) do
		list[#list + 1] = self.by_name[name]
	end
	return list
end

function Buffers:info(path)
	local buffer = self.by_name[path]
	return buffer and info_of(buffer)
end

function Buffers:all()
	local infos = {}
	for _, buffer in ipairs(self:each()) do
		infos[#infos + 1] = info_of(buffer)
	end
	return infos
end

function Buffers:get(path)
	local buffer = self.by_name[path]
	if not buffer then
		out_of_sync(path .. " is not open")
	end
	return buffer
end

function Buffers:draft_file(absolute)
	return paths.join(self.drafts, store.key_of(absolute) .. ".json")
end

function Buffers:read_draft(absolute)
	local read, content = pcall(fs.read, self:draft_file(absolute))
	if not read then
		return nil
	end
	local decoded, draft = pcall(json.decode, content)
	if decoded and type(draft) == "table" and draft.path == absolute and type(draft.text) == "string" then
		return draft
	end
	return nil
end

function Buffers:remove_draft(absolute)
	pcall(fs.remove, self:draft_file(absolute))
end

function Buffers:cancel_draft(buffer)
	local timer = self.timers[buffer.absolute]
	if timer then
		timer:cancel()
		self.timers[buffer.absolute] = nil
	end
end

function Buffers:write_draft(buffer)
	pcall(function()
		fs.mkdir(self.drafts)
		fs.write(
			self:draft_file(buffer.absolute),
			json.encode({ path = buffer.absolute, text = buffer.text, at = now_ms() })
		)
	end)
end

function Buffers:schedule_draft(buffer)
	self:cancel_draft(buffer)
	self.timers[buffer.absolute] = time.after(1.5, function()
		self.timers[buffer.absolute] = nil
		if buffer.text == buffer.saved then
			self:remove_draft(buffer.absolute)
			return
		end
		self:write_draft(buffer)
	end)
end

-- The buffer of a file, read from the disk if it is not open; a missing file opens empty.
function Buffers:open(path)
	local absolute = self.project:absolute(path)
	local name = self.project:name(absolute)
	local open = self.by_name[name]
	if open then
		return with_text(info_of(open), open.text)
	end
	local read = read_file_text(absolute, name)
	if read.reason then
		return read
	end
	local buffer = {
		path = name,
		absolute = absolute,
		text = read.text,
		saved = read.text,
		version = 1,
		eol = read.eol,
		saved_eol = read.eol,
		bom = read.bom,
		mtime = read.mtime,
		missing = read.missing,
		conflict = false,
		readonly = read.readonly,
	}
	local draft = self:read_draft(absolute)
	if draft and draft.text ~= buffer.text then
		buffer.draft = { at = draft.at, text = draft.text }
	elseif draft then
		self:remove_draft(absolute)
	end
	self:add(name, buffer)
	return with_text(info_of(buffer), buffer.text)
end

-- The piece of a buffer's text that begins at `from` and fits one message to the view, for a view
-- that reads a long one in pieces: the next piece begins at `end`.
function Buffers:slice(path, version, from)
	local buffer = self:get(path)
	if version ~= buffer.version then
		out_of_sync(string.format("%s is at version %d, not %d", path, buffer.version, version))
	end
	local content = buffer.text
	local start = text.index(content, from)
	if start == nil then
		if from >= text.units(content) then
			return { text = "", ["end"] = from }
		end
		error(path .. ": a piece cannot begin inside a character", 0)
	end
	-- A piece always has a character in it, so that reading goes on.
	local last, units = text.wire_fit(content, start, wire.WIRE_BUDGET)
	return { text = string.sub(content, start, last), ["end"] = from + units }
end

-- The current text of an open buffer.
function Buffers:text(path)
	local buffer = self:get(path)
	return with_text(info_of(buffer), buffer.text)
end

-- Applies the view's edits, made on version `base`, in order.
function Buffers:edit(path, base, edits)
	local buffer = self:get(path)
	if base ~= buffer.version then
		out_of_sync(string.format("%s is at version %d, not %d", path, buffer.version, base))
	end
	local content = buffer.text
	local length = text.units(content)
	for _, edit in ipairs(edits) do
		local from, to, insert = edit[1], edit[2], edit[3]
		if not (from >= 0 and from <= to and to <= length) then
			out_of_sync("An edit of " .. path .. " reaches outside it")
		end
		local first = text.index(content, from)
		local after = first and text.index(content, to - from, first)
		if first == nil or after == nil then
			out_of_sync("An edit of " .. path .. " reaches outside it")
		end
		content = string.sub(content, 1, first - 1) .. insert .. string.sub(content, after)
		length = length - (to - from) + text.units(insert)
	end
	buffer.text = content
	buffer.version += 1
	self:schedule_draft(buffer)
	return info_of(buffer)
end

-- Writes the buffer to its file. A file that changed on disk since it was read is not overwritten
-- unless `force`: the answer says so, and the view asks.
function Buffers:save(path, version, force)
	local buffer = self:get(path)
	if version ~= buffer.version then
		out_of_sync(string.format("%s is at version %d, not %d", path, buffer.version, version))
	end
	if not force and not buffer.missing then
		local now = mtime_of(buffer.absolute)
		if now ~= nil and now ~= buffer.mtime then
			buffer.conflict = true
			return { saved = false, info = info_of(buffer) }
		end
	end
	local body = (buffer.bom and BOM or "") .. with_eol(buffer.text, buffer.eol)
	fs.mkdir(paths.dirname(buffer.absolute))
	fs.write(buffer.absolute, body)
	buffer.mtime = mtime_of(buffer.absolute) or now_ms()
	buffer.saved = buffer.text
	buffer.saved_eol = buffer.eol
	buffer.missing = false
	buffer.conflict = false
	buffer.readonly = false
	self:cancel_draft(buffer)
	self:remove_draft(buffer.absolute)
	self.project:forget()
	return { saved = true, info = info_of(buffer) }
end

local function reset_from(buffer, read)
	buffer.text = read.text
	buffer.saved = read.text
	buffer.eol = read.eol
	buffer.saved_eol = read.eol
	buffer.bom = read.bom
	buffer.mtime = read.mtime
	buffer.missing = read.missing
	buffer.conflict = false
end

-- Reads the file again, dropping the buffer's changes.
function Buffers:revert(path)
	local buffer = self:get(path)
	local read = read_file_text(buffer.absolute, buffer.path)
	if read.reason then
		return read
	end
	buffer.version += 1
	reset_from(buffer, read)
	buffer.readonly = read.readonly
	buffer.draft = nil
	self:cancel_draft(buffer)
	self:remove_draft(buffer.absolute)
	return with_text(info_of(buffer), buffer.text)
end

-- Keeps the buffer's changes over the file that changed on disk; the next save writes them.
function Buffers:keep(path)
	local buffer = self:get(path)
	buffer.mtime = mtime_of(buffer.absolute) or buffer.mtime
	buffer.conflict = false
	return info_of(buffer)
end

-- Takes the draft an earlier textpen left, or drops it.
function Buffers:draft(path, take)
	local buffer = self:get(path)
	local draft = buffer.draft
	buffer.draft = nil
	if take and draft then
		buffer.text = draft.text
		buffer.version += 1
	else
		self:remove_draft(buffer.absolute)
	end
	return with_text(info_of(buffer), buffer.text)
end

-- Changes the line ends the buffer is written with.
function Buffers:set_eol(path, eol)
	local buffer = self:get(path)
	buffer.eol = eol
	return info_of(buffer)
end

function Buffers:close(path, discard)
	local buffer = self.by_name[path]
	if not buffer then
		return
	end
	if is_dirty(buffer) and not discard then
		error(path .. " has changes that are not saved", 0)
	end
	self:cancel_draft(buffer)
	if discard then
		self:remove_draft(buffer.absolute)
	end
	self:delete(path)
end

-- After a rename on disk: the buffers of the file, or of the files under the directory, follow.
function Buffers:moved(from, to)
	for _, path in ipairs({ table.unpack(self.names) }) do
		local buffer = self.by_name[path]
		local rest = nil
		if path == from then
			rest = ""
		elseif string.sub(path, 1, #from + 1) == from .. "/" then
			rest = string.sub(path, #from + 1)
		end
		if rest ~= nil then
			self:delete(path)
			buffer.absolute = self.project:absolute(to .. rest)
			buffer.path = self.project:name(buffer.absolute)
			self:add(buffer.path, buffer)
		end
	end
end

-- The buffers whose file changed on disk since it was read. One without changes of its own is read
-- again; one with changes is marked as in conflict, for the view to ask.
function Buffers:changed_on_disk()
	local changed = {}
	for _, buffer in ipairs(self:each()) do
		local now = mtime_of(buffer.absolute)
		if now == nil then
			if not buffer.missing and buffer.mtime ~= 0 then
				buffer.missing = true
				changed[#changed + 1] = info_of(buffer)
			end
		elseif not (now == buffer.mtime and not buffer.missing) then
			local report = true
			if not is_dirty(buffer) then
				local read = read_file_text(buffer.absolute, buffer.path)
				if read.reason then
					report = false
				else
					if read.text ~= buffer.text then
						buffer.version += 1
					end
					reset_from(buffer, read)
					buffer.missing = false
				end
			elseif not buffer.conflict then
				buffer.conflict = true
				buffer.missing = false
			else
				report = false
			end
			if report then
				changed[#changed + 1] = info_of(buffer)
			end
		end
	end
	return changed
end

-- Writes a draft of every buffer with changes, now: the program is about to end.
function Buffers:flush_drafts()
	for _, buffer in ipairs(self:each()) do
		self:cancel_draft(buffer)
		if buffer.text ~= buffer.saved then
			self:write_draft(buffer)
		end
	end
end

-- Drops every change that is not saved, and its draft: the person quit without saving.
function Buffers:discard_all()
	for _, buffer in ipairs(self:each()) do
		self:cancel_draft(buffer)
		buffer.saved = buffer.text
		buffer.saved_eol = buffer.eol
		self:remove_draft(buffer.absolute)
	end
end

return M
