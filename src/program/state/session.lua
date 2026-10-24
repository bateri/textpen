-- The state of one textpen program: its project, its open files, what it remembers, and who can
-- take the pane from the view. The view's own state outlives the view: a view that comes back from
-- a shell, or that the terminal starts again, finds it here.
--
-- Plain values in and out: this file knows nothing of the page. What the page is told of it is
-- made from it by api/.

local pencere = require("pencere")
local json = pencere.json

local tree_module = require("project.tree")
local buffers_module = require("state.buffers")
local store_module = require("state.store")

local M = {}

local Session = {}
Session.__index = Session

function M.new(root)
	local self = setmetatable({}, Session)
	self.project = tree_module.new(root)
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
	-- How the shell the pane was handed to ended, `{ status =, ms =, error = }`; told to the next
	-- view that boots, once.
	self.returned = nil
	-- A hand-off the terminal could not make ("unsupported" or "closing"); told to the view with
	-- the next changes, once.
	self.handoff_failed = nil
	return self
end

-- Starts on the files the command line names, in place of the tabs of the last run.
function Session:start_with(files, focus)
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

-- The view's own state, kept for the next view and the next run.
function Session:keep(ui)
	if ui.active == nil then
		ui.active = json.null
	end
	self.ui = ui
	self.store:save_ui(self.project.root, ui)
end

function Session:save_settings(settings)
	self.settings = self.store:save_settings(settings)
	return self.settings
end

-- The program ends: what is not saved is kept as drafts, and the view's state for next time.
function Session:ending()
	self.buffers:flush_drafts()
	self.store:flush()
end

return M
