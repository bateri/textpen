-- What textpen remembers between runs, where the XDG directories say: the settings
-- (`~/.config/textpen/settings.json`) and, for each project, the files that were open and where
-- (`~/.local/state/textpen/projects/`). The view's own storage does not last: a terminal gives
-- every view a new origin.

local pencere = require("pencere")
local fs, json, time, env, hash = pencere.fs, pencere.json, pencere.time, pencere.env, pencere.hash

local paths = require("lib.paths")
local types = require("lib.types")

local M = {}

-- A key for a path that can name a file: the drafts and the projects are kept by it.
function M.key_of(path)
	return string.sub(hash.sha256(path), 1, 24)
end

local function home()
	return pencere.os.home() or ""
end

local function directory(variable, fallback)
	local base = env.get(variable)
	if base == nil or base == "" then
		base = paths.join(home(), fallback)
	end
	return paths.join(base, "textpen")
end

function M.config_directory()
	return directory("XDG_CONFIG_HOME", ".config")
end

function M.state_directory()
	return directory("XDG_STATE_HOME", ".local/state")
end

local function read_json(path)
	local ok, text = pcall(fs.read, path)
	if not ok then
		return nil
	end
	local decoded, value = pcall(json.decode, text)
	if not decoded then
		return nil
	end
	return value
end

-- A home that cannot be written to: textpen works, and forgets.
local function write_json(path, value)
	pcall(function()
		fs.mkdir(paths.dirname(path))
		fs.write(path, json.encode(value, true) .. "\n")
	end)
end

local function number(value, minimum, maximum, fallback)
	if type(value) == "number" and value == value and value ~= math.huge and value ~= -math.huge then
		return math.min(maximum, math.max(minimum, math.floor(value + 0.5)))
	end
	return fallback
end

local function flag(value, fallback)
	if type(value) == "boolean" then
		return value
	end
	return fallback
end

local function strings(value)
	local out = {}
	if type(value) == "table" then
		for _, item in ipairs(value) do
			if type(item) == "string" then
				out[#out + 1] = item
			end
		end
	end
	return out
end

local function sanitize(settings)
	local defaults = types.DEFAULT_SETTINGS
	local numbering = settings.lineNumbers
	if numbering ~= "on" and numbering ~= "relative" and numbering ~= "off" then
		numbering = defaults.lineNumbers
	end
	return {
		vim = flag(settings.vim, defaults.vim),
		wrap = flag(settings.wrap, defaults.wrap),
		fontSize = number(settings.fontSize, 9, 28, defaults.fontSize),
		lineNumbers = numbering,
		tabSize = number(settings.tabSize, 1, 8, defaults.tabSize),
		insertSpaces = flag(settings.insertSpaces, defaults.insertSpaces),
		whitespace = flag(settings.whitespace, defaults.whitespace),
		autoSave = flag(settings.autoSave, defaults.autoSave),
		sidebar = flag(settings.sidebar, defaults.sidebar),
		sidebarWidth = number(settings.sidebarWidth, 160, 520, defaults.sidebarWidth),
	}
end

local Store = {}
Store.__index = Store

function M.new()
	local self = setmetatable({}, Store)
	self.config = M.config_directory()
	self.state = M.state_directory()
	self.pending = nil
	self.timer = nil
	return self
end

function Store:settings()
	local merged = {}
	for key, value in pairs(types.DEFAULT_SETTINGS) do
		merged[key] = value
	end
	local read = read_json(paths.join(self.config, "settings.json"))
	if type(read) == "table" then
		for key, value in pairs(read) do
			if type(key) == "string" then
				merged[key] = value
			end
		end
	end
	return sanitize(merged)
end

function Store:save_settings(settings)
	local clean = sanitize(settings)
	write_json(paths.join(self.config, "settings.json"), clean)
	return clean
end

function Store:ui_file(root)
	return paths.join(self.state, "projects", M.key_of(root) .. ".json")
end

function Store:ui(root)
	local read = read_json(self:ui_file(root))
	if type(read) ~= "table" or read.root ~= root then
		return types.empty_ui()
	end
	local places = read.places
	if type(places) ~= "table" or (getmetatable(places) ~= "json.object" and #places > 0) then
		places = json.object()
	end
	local recent = strings(read.recent)
	while #recent > 50 do
		recent[#recent] = nil
	end
	return {
		tabs = strings(read.tabs),
		active = type(read.active) == "string" and read.active or json.null,
		expanded = strings(read.expanded),
		places = places,
		recent = recent,
		preview = strings(read.preview),
	}
end

-- Keeps the view's state for the next run, a moment after it stops changing.
function Store:save_ui(root, ui)
	self.pending = { root = root, ui = ui }
	if self.timer then
		self.timer:cancel()
	end
	self.timer = time.after(1, function()
		self:flush()
	end)
end

function Store:flush()
	if self.timer then
		self.timer:cancel()
		self.timer = nil
	end
	local pending = self.pending
	self.pending = nil
	if pending then
		local record = { root = pending.root }
		for key, value in pairs(pending.ui) do
			record[key] = value
		end
		write_json(self:ui_file(pending.root), record)
	end
end

return M
