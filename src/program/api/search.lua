-- The `search` call: the matches of a pattern in the files of the project, as many as fit one
-- message; a search too long for one is cut, and says so (`truncated`).
--
-- This is api/'s: the files of api/ know the page, and project/, state/ and terminal/ do not.

local pencere = require("pencere")
local json = pencere.json

local call = require("api.call")
local params = require("api.params")
local pieces = require("api.pieces")
local search = require("project.search")

local M = {}

function M.calls(session)
	local calls = {}

	calls.search = call.new(
		params.object({
			query = params.string(1),
			regex = params.optional(params.boolean()),
			caseSensitive = params.optional(params.boolean()),
			wholeWord = params.optional(params.boolean()),
		}),
		function(args)
			local result = search.run(session.project, {
				query = args.query,
				regex = args.regex == true,
				caseSensitive = args.caseSensitive == true,
				wholeWord = args.wholeWord == true,
			})
			-- A message to the view is at most 1 MiB: a search too long for one is cut, and says so.
			local count = pieces.fitting(result.files, function(file)
				return pieces.wire_size(json.encode(file))
			end)
			if count == #result.files then
				return result
			end
			local files = {}
			for index = 1, count do
				files[index] = result.files[index]
			end
			result.files = files
			result.truncated = true
			return result
		end
	)

	return calls
end

return M
