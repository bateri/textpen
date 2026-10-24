-- Which of the directories that were listed have changed since. A directory is noted with its
-- modification time when it is listed; asking tells those whose time is another now (or that went),
-- and does not tell of them twice.
--
-- Plain values in and out: this file knows nothing of the page.

local M = {}

local Listed = {}
Listed.__index = Listed

function M.new()
	local self = setmetatable({}, Listed)
	-- Directories listed, with their modification time then.
	self.times = {}
	return self
end

function Listed:note(dir, modified)
	if modified then
		self.times[dir] = modified
	end
end

-- The listed directories whose entries changed since they were listed (or that went), in order.
-- `modified_of(dir)` is the directory's modification time now, or nil when it is gone.
function Listed:changed(modified_of)
	local changed = {}
	local names = {}
	for dir in pairs(self.times) do
		names[#names + 1] = dir
	end
	table.sort(names)
	for _, dir in ipairs(names) do
		local now = modified_of(dir)
		if now == nil then
			self.times[dir] = nil
			changed[#changed + 1] = dir
		elseif now ~= self.times[dir] then
			changed[#changed + 1] = dir
		end
	end
	return changed
end

return M
