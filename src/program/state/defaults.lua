-- What is a default, and what is empty, in the state the program keeps for the view, in the shapes
-- the view knows (src/view/lib/types.ts). A list is a table with no mark, an object with no
-- members is made with `json.object()`, a null is `json.null`.

local pencere = require("pencere")
local json = pencere.json

local M = {}

M.DEFAULT_SETTINGS = {
	vim = false,
	wrap = false,
	fontSize = 13,
	lineNumbers = "on",
	tabSize = 2,
	insertSpaces = true,
	whitespace = false,
	autoSave = false,
	sidebar = true,
	sidebarWidth = 240,
}

function M.empty_ui()
	return {
		tabs = {},
		active = json.null,
		expanded = {},
		places = json.object(),
		recent = {},
		preview = {},
	}
end

return M
