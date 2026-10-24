-- What one message to the view may carry. The host passes a page's messages unread and drops one
-- over 1 MiB, in either direction, so nothing the program answers is allowed to grow with the
-- project: a long text, a directory of thousands of entries, the files of a big repository and a
-- long search all come in pieces that fit.
--
-- This is api/'s: the files of api/ know the page, and project/, state/ and terminal/ do not.

local pencere = require("pencere")
local text = pencere.text

local M = {}

-- The room a payload has in a message of 1 MiB: what is around it (the envelope, the other
-- members of the result) takes a few hundred bytes, and this leaves them 124 KiB.
M.WIRE_BUDGET = 900 * 1024

-- How many bytes a text takes in a message to the view, as pencere counts it against its limit:
-- the UTF-8 of each character, before the wire's escapes.
M.wire_size = text.wire_size

-- How many of the first `items` go in a message together, each taking `size(item)` bytes. At least
-- one.
function M.fitting(items, size)
	local used = 0
	for index = 1, #items do
		used += size(items[index])
		if used > M.WIRE_BUDGET then
			return math.max(index - 1, 1)
		end
	end
	return #items
end

return M
