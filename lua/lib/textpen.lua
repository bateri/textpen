-- The state of one textpen program: its project, its open files, what it remembers, and who can
-- take the pane from the view. The view's own state outlives the view: a view that comes back from
-- a shell, or that the terminal starts again, finds it here.

local pencere = require("pencere")
local fs, json, env = pencere.fs, pencere.json, pencere.env

local buffers_module = require("lib.buffers")
local paths = require("lib.paths")
local project_module = require("lib.project")
local store_module = require("lib.store")
local version = require("lib.version")

local M = {}

local Textpen = {}
Textpen.__index = Textpen

function M.new(root)
	local self = setmetatable({}, Textpen)
	self.project = project_module.new(root)
	self.store = store_module.new()
	self.buffers = buffers_module.new(self.project, self.store.state)
	self.settings = self.store:settings()
	self.ui = self.store:ui(self.project.root)
	-- Who can take the pane from the view: `desk.shell()`, which closes the view and runs a shell
	-- in the terminal. Absent where no terminal shows the view.
	self.desk = nil
	-- What the command line asked to open; told to the first view only.
	self.focus = nil
	self.problems = {}
	self.close_quits = false
	-- How the shell the pane was handed to ended; told to the next view that boots, once.
	self.returned = nil
	-- A hand-off the terminal could not make; told to the view with the next changes, once.
	self.handoff_failed = nil
	return self
end

-- Starts on the files the command line names, in place of the tabs of the last run.
function Textpen:start_with(files, focus)
	local tabs = {}
	for index, path in ipairs(files) do
		tabs[index] = self.project:name(self.project:absolute(path))
	end
	self.close_quits = true
	local active = json.null
	if focus then
		active = self.project:name(self.project:absolute(focus.path))
	elseif tabs[1] then
		active = tabs[1]
	end
	self.ui = {
		tabs = tabs,
		active = active,
		expanded = self.ui.expanded,
		places = self.ui.places,
		recent = self.ui.recent,
		preview = self.ui.preview,
	}
	if focus then
		self.focus = {
			path = self.project:name(self.project:absolute(focus.path)),
			line = focus.line,
			column = focus.column,
		}
	end
end

local function present(value)
	return value ~= nil and value ~= ""
end

function Textpen:boot()
	local returned, focus = self.returned, self.focus
	self.returned, self.focus = nil, nil
	local git = self.project:git(0)
	local boot = {
		root = self.project.root,
		rootName = paths.basename(self.project.root) ~= "" and paths.basename(self.project.root) or self.project.root,
		home = pencere.os.home() or "",
		user = pencere.os.username(),
		hostname = pencere.os.hostname(),
		remote = present(env.get("SSH_CONNECTION")) or present(env.get("SSH_TTY")),
		version = version,
		git = git or json.null,
		settings = self.settings,
		ui = self.ui,
		buffers = self.buffers:all(),
		handoff = self.desk ~= nil,
		closeQuits = self.close_quits and self.desk ~= nil,
		problems = self.problems,
	}
	if returned then
		boot.returned = returned
	end
	if focus then
		boot.focus = focus
	end
	return boot
end

-- The view's own state, kept for the next view and the next run.
function Textpen:keep(ui)
	if ui.active == nil then
		ui.active = json.null
	end
	self.ui = ui
	self.store:save_ui(self.project.root, ui)
end

-- What happened to the files since the view was last told: git, the open files that changed on
-- disk, the listed directories that did. Asking marks them as told, so one subscription asks.
function Textpen:changes()
	local handoff_failed = self.handoff_failed
	self.handoff_failed = nil
	local git = self.project:git()
	local buffers = self.buffers:changed_on_disk()
	local dirs = self.project:changed_dirs()
	return { git = git, buffers = buffers, dirs = dirs, handoff_failed = handoff_failed }
end

function Textpen:save_settings(settings)
	self.settings = self.store:save_settings(settings)
	return self.settings
end

function Textpen:create(path, kind)
	local absolute = self.project:absolute(path)
	if fs.exists(absolute) then
		error(path .. " already exists", 0)
	end
	if kind == "dir" then
		fs.mkdir(absolute)
	else
		fs.mkdir(paths.dirname(absolute))
		-- Made with no other file there: an existing file is not truncated, and a link that points
		-- at nothing is not written through to make its target.
		local ok, why = pcall(fs.create, absolute)
		if not ok then
			if string.find(tostring(why), "File exists", 1, true) then
				error(path .. " already exists", 0)
			end
			error(why, 0)
		end
	end
	self.project:forget()
	return self.project:name(absolute)
end

function Textpen:rename(from, to)
	local source = self.project:absolute(from)
	local target = self.project:absolute(to)
	if source == target then
		return self.project:name(target)
	end
	-- A change of case alone is the same file on a disk that ignores case, and allowed.
	if string.lower(source) ~= string.lower(target) and fs.exists(target) then
		error(to .. " already exists", 0)
	end
	fs.mkdir(paths.dirname(target))
	fs.rename(source, target)
	self.buffers:moved(self.project:name(source), self.project:name(target))
	self.project:forget()
	return self.project:name(target)
end

function Textpen:remove(path)
	local absolute = self.project:absolute(path)
	if absolute == self.project.root then
		error("The project itself is not removed", 0)
	end
	fs.remove(absolute, true)
	self.project:forget()
end

-- The program ends: what is not saved is kept as drafts, and the view's state for next time.
function Textpen:ending()
	self.buffers:flush_drafts()
	self.store:flush()
end

return M
