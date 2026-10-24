-- The calls about the program and the view's own state: `boot` (what the view shows first: the
-- project, its settings, and what is open), `ui` (where the view is: tabs, folders, cursors, kept
-- for the next view and the next run), `settings`, and `handoff` (the pane given to a shell in the
-- project, which gives it back when it ends).
--
-- This is api/'s: the files of api/ know the page, and project/, state/ and terminal/ do not. The
-- shapes the page knows are src/view/lib/types.ts.

local pencere = require("pencere")
local env, json = pencere.env, pencere.json

local call = require("api.call")
local params = require("api.params")
local paths = require("project.paths")
local version = require("version")

local M = {}

local place = params.object({ anchor = params.number(), head = params.number(), top = params.number() })

local ui_state = params.object({
	tabs = params.array(params.string()),
	active = params.nullable(params.string()),
	expanded = params.array(params.string()),
	places = params.record(place),
	recent = params.array(params.string()),
	preview = params.array(params.string()),
})

local settings_schema = params.object({
	vim = params.boolean(),
	wrap = params.boolean(),
	fontSize = params.number(),
	lineNumbers = params.enum("on", "relative", "off"),
	tabSize = params.number(),
	insertSpaces = params.boolean(),
	whitespace = params.boolean(),
	autoSave = params.boolean(),
	sidebar = params.boolean(),
	sidebarWidth = params.number(),
})

local function present(value)
	return value ~= nil and value ~= ""
end

function M.calls(session)
	local calls = {}

	-- What the view shows first: the project, its settings, and what is open.
	calls.boot = call.new(params.nothing, function()
		local returned, focus = session.returned, session.focus
		session.returned, session.focus = nil, nil
		local root = session.project.root
		local git = session.project.git:status(0)
		local boot = {
			root = root,
			rootName = paths.basename(root) ~= "" and paths.basename(root) or root,
			home = pencere.os.home() or "",
			user = pencere.os.username(),
			hostname = pencere.os.hostname(),
			remote = present(env.get("SSH_CONNECTION")) or present(env.get("SSH_TTY")),
			version = version,
			git = git or json.null,
			settings = session.settings,
			ui = session.ui,
			buffers = session.buffers:all(),
			handoff = session.desk ~= nil,
			closeQuits = session.close_quits and session.desk ~= nil,
			problems = session.problems,
		}
		if returned then
			-- How the shell ended, as the view is told: the status (null when it was a signal's),
			-- how long it ran, and why it could not start.
			boot.returned = {
				exitCode = returned.status == nil and json.null or returned.status,
				duration = returned.ms,
				error = returned.error,
			}
		end
		if focus then
			boot.focus = focus
		end
		return boot
	end)

	-- Where the view is (tabs, folders, cursors), kept for the next view and the next run.
	calls.ui = call.new(params.object({ ui = ui_state }), function(args)
		session:keep(args.ui)
		return json.object()
	end)

	calls.settings = call.new(params.object({ settings = settings_schema }), function(args)
		return { settings = session:save_settings(args.settings) }
	end)

	-- Hands the pane to a shell in the project, which gives it back when it ends.
	calls.handoff = call.new(params.object({ kind = params.literal("shell") }), function()
		local desk = session.desk
		return { state = desk == nil and "unavailable" or desk.shell() }
	end)

	return calls
end

return M
