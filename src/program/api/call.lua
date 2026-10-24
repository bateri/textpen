-- How a call of the view is answered. The params it arrives with are checked against the schema it
-- names (what does not fit is answered `bad-params`, and the work never runs), and what the work
-- raises is told to the view with a code it can act on: `out-of-sync` for an edit made on a version
-- the program no longer holds, `failed` for what the disk or the project would not do (the words
-- are for the person).
--
-- This is api/'s: the files of api/ know the page, and project/, state/ and terminal/ do not.

local params = require("api.params")

local M = {}

-- What a failure tells the view. An error that is a table with a code (`bad-params`) is the call's
-- own answer. One from state/buffers is a table with `out_of_sync` and its words: the view tells
-- it from any other failure by the code.
local function failure(err)
	if type(err) == "table" and err.code then
		return err
	end
	if type(err) == "table" and err.out_of_sync then
		return { code = "out-of-sync", message = err.out_of_sync }
	end
	local words = string.gsub(tostring(err), "\nstack traceback:.*$", "")
	return { code = "failed", message = words }
end

-- A call that takes the params `schema` describes: the view is not trusted to send them, so what
-- does not fit is answered `bad-params`, and what the work raises is `failure`.
function M.new(schema, work)
	return function(args, context)
		local parsed, why = params.parse(schema, args)
		if why then
			error({ code = "bad-params", message = why }, 0)
		end
		local results = table.pack(pcall(work, parsed, context))
		if not results[1] then
			error(failure(results[2]), 0)
		end
		return table.unpack(results, 2, results.n)
	end
end

return M
