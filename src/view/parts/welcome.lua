-- The page with no file open: where the project is, what to do first and the keys for it, and the
-- files opened lately.

local kit = require("../lib/kit")
local text = require("../lib/text")
local files = require("../lib/files")
local commands = require("./commands")

local b, S, act = kit.b, kit.S, kit.act

local welcome = {}

local ACTIONS = {
	{ "files", "file-search" },
	{ "new-file", "file-plus" },
	{ "search", "search" },
	{ "commands", "command" },
	{ "shell", "terminal-2" },
	{ "help", "keyboard" },
}

-- textpen's mark, which its icon is drawn from too: lines of text, the last one being written, with
-- the caret at its end.
local function mark()
	local paths = {}
	-- A bar with round ends: its middle and a circle at each end.
	local function bar(x, y, w, h, alpha)
		local color = b.alpha("accent", alpha)
		if w >= h then
			local r = h / 2
			paths[#paths + 1] = b.path():rect(x + r, y, w - h, h):fill(color)
			paths[#paths + 1] = b.path():circle(x + r, y + r, r):fill(color)
			paths[#paths + 1] = b.path():circle(x + w - r, y + r, r):fill(color)
		else
			local r = w / 2
			paths[#paths + 1] = b.path():rect(x, y + r, w, h - w):fill(color)
			paths[#paths + 1] = b.path():circle(x + r, y + r, r):fill(color)
			paths[#paths + 1] = b.path():circle(x + r, y + h - r, r):fill(color)
		end
	end
	bar(3, 3.8, 18, 3, 0.4)
	bar(6.5, 9.8, 11.5, 3, 0.7)
	bar(3, 15.8, 6, 3, 1)
	bar(10.6, 14.3, 2.2, 6, 1)
	return b.canvas(paths):size(24, 24)
end

function act.welcome_view(size)
	local boot = S.boot
	local inner = b.div():col():gap(14):w(math.min(520, size.w - 48))
	inner:child(b.div():size(40, 40):rounded(10):bg(b.alpha("accent", 0.12)):row():items_center():justify_center():child(mark()))
	inner:child(b.text(boot.rootName):text_size(20):weight(700))
	local where = b.div():row():items_center():gap(10):color("dim")
		:child(b.text(text.friendly(boot.root, boot.home)):truncate():min_w(0))
	if S.git then
		where:child(b.div():row():items_center():gap(4):no_shrink()
			:child(kit.icon("git-branch", 13, "dim")):child(b.text(S.git.branch)))
	end
	if boot.remote then
		where:child(b.text("on " .. boot.hostname):color("quiet"):no_shrink())
	end
	inner:child(where)
	local actions = b.div():col():gap(2):mt(6)
	for _, entry in ACTIONS do
		local command = commands.get(entry[1])
		if command and commands.enabled(command) then
			local id = "welcome:" .. entry[1]
			local over = kit.hovered(id)
			local row = b.div()
				:press(id)
				:row()
				:items_center()
				:gap(10)
				:h(30)
				:px(10)
				:rounded(6)
				:bg(over and b.alpha("foreground", 0.06) or nil)
				:role("button")
				:child(kit.icon(entry[2], 16, over and "foreground" or "dim"))
				:child(b.text((string.gsub(command.title, "…$", ""))):grow(1))
			if command.keys then
				row:child(b.text(kit.keys_of(command.keys)):color("quiet"))
			end
			actions:child(row)
		end
	end
	inner:child(actions)
	if #S.recent > 0 and not S.short then
		local recent = b.div():col():gap(2):mt(6)
			:child(b.text("RECENT"):text_size(11):weight(700):color("quiet"):px(10))
		for index = 1, math.min(#S.recent, 6) do
			local path = S.recent[index]
			local id = "welcome-file:" .. path
			local kind = files.kind(path)
			recent:child(b.div()
				:press(id)
				:row()
				:items_center()
				:gap(9)
				:h(26)
				:px(10)
				:rounded(6)
				:bg(kit.hovered(id) and b.alpha("foreground", 0.06) or nil)
				:child(kit.icon(kind.icon, 14, kind.color))
				:child(b.rich({ b.span(text.basename(path)), b.span(" " .. text.dirname(path)):color("quiet") }):truncate():min_w(0)))
		end
		inner:child(recent)
	end
	return b.div():id("welcome"):col():grow(1):min_h(0):scroll():items_center():pt(math.max(math.floor(size.h * 0.12), 16)):pb(24)
		:child(inner)
end

function welcome.press(id)
	local command = string.match(id, "^welcome:(.*)$")
	if command then
		commands.run(command)
		return true
	end
	local path = string.match(id, "^welcome%-file:(.*)$")
	if path then
		act.open_file(path, { focus = true })
		return true
	end
	return false
end

return welcome
