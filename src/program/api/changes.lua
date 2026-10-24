-- The `changes` topic: the disk and git, as they change. The view is told one value at once and
-- then one only when something differs from what it was told: the open files that changed on disk,
-- the listed directories that did, git's own state when it changed, and a hand-off the terminal
-- could not make. It looks every two seconds while it is subscribed.
--
-- This is api/'s: the files of api/ know the page, and project/, state/ and terminal/ do not.

local pencere = require("pencere")
local json, time = pencere.json, pencere.time

local M = {}

-- How often the `changes` topic looks at the disk and at git.
M.CHANGES_EVERY = 2

-- What the person is told when the terminal could not hand its pane to a shell (the program says
-- why, as a short name: terminal/handoff).
local HANDOFF_FAILED = {
	unsupported = "This terminal cannot hand its pane to a command and back.",
	closing = "The view could not be closed just now.",
}

-- What happened to the files since the view was last told: git, the open files that changed on
-- disk, the listed directories that did. Asking marks them as told, so one subscription asks.
local function look(session)
	local handoff_failed = session.handoff_failed
	session.handoff_failed = nil
	local git = session.project.git:status()
	local buffers = session.buffers:changed_on_disk()
	local dirs = session.project:changed_dirs()
	return { git = git, buffers = buffers, dirs = dirs, handoff_failed = HANDOFF_FAILED[handoff_failed] }
end

-- The disk and git, as they change: one value at once and then one only when something differs
-- from what the view was told. Git's own state comes along when it changed.
function M.topic(session)
	return function(_, subscription)
		local timer = nil
		local told = nil
		local function tick()
			timer = nil
			local ok, found = pcall(look, session)
			if subscription.cancelled then
				return
			end
			if ok then
				local git = json.encode(found.git == nil and json.null or found.git)
				local value = { buffers = found.buffers, dirs = found.dirs }
				if git ~= told then
					value.git = found.git == nil and json.null or found.git
				end
				if found.handoff_failed then
					value.handoffFailed = found.handoff_failed
				end
				local quiet = git == told
					and #found.buffers == 0
					and #found.dirs == 0
					and found.handoff_failed == nil
				if not quiet and subscription:push(value) then
					told = git
				end
			end
			-- A look at the disk that failed is made again at the next turn.
			if not subscription.cancelled then
				timer = time.after(M.CHANGES_EVERY, tick)
			end
		end
		timer = time.after(0, tick)
		subscription:on_end(function()
			if timer then
				timer:cancel()
				timer = nil
			end
		end)
	end
end

return M
