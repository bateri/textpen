-- The calls about the files of the project: `list` (one directory), `files` (every file, for the
-- quick open), `create`, `rename` and `delete`. Whatever writes to the disk is the view's word, and
-- the person's.
--
-- This is api/'s: the files of api/ know the page, and project/, state/ and terminal/ do not. What
-- is answered is cut to fit one message; a directory too big for one says so (`truncated`).

local pencere = require("pencere")
local json = pencere.json

local call = require("api.call")
local params = require("api.params")
local pieces = require("api.pieces")

local M = {}

function M.calls(session)
	local project, buffers = session.project, session.buffers
	local calls = {}

	calls.list = call.new(params.object({ dir = params.string() }), function(args)
		local listing = project:list(args.dir)
		-- A message to the view is at most 1 MiB: a directory too big for one is cut, and says so.
		local count = pieces.fitting(listing.entries, function(entry)
			return pieces.wire_size(entry.name) + pieces.wire_size(entry.path) + 80
		end)
		if count == #listing.entries then
			return listing
		end
		local entries = {}
		for index = 1, count do
			entries[index] = listing.entries[index]
		end
		return { dir = listing.dir, entries = entries, truncated = true }
	end)

	-- The files of the project for the quick open, a message's worth at a time from `from`.
	calls.files = call.new(params.object({ from = params.optional(params.integer(0)) }), function(args)
		local from = args.from or 0
		local index = project:files()
		local rest = {}
		for position = from + 1, #index.files do
			rest[#rest + 1] = index.files[position]
		end
		local count = pieces.fitting(rest, function(path)
			return pieces.wire_size(path) + 4
		end)
		local finish = from + count
		local files = {}
		for position = 1, count do
			files[position] = rest[position]
		end
		return {
			files = files,
			next = finish < #index.files and finish or json.null,
			truncated = index.truncated,
		}
	end)

	calls.create = call.new(
		params.object({ path = params.string(1), kind = params.enum("file", "dir") }),
		function(args)
			return { path = project:create(args.path, args.kind) }
		end
	)

	calls.rename = call.new(
		params.object({ from = params.string(1), to = params.string(1) }),
		function(args)
			local renamed = project:rename(args.from, args.to)
			if renamed.moved then
				buffers:moved(renamed.from, renamed.to)
			end
			return { path = renamed.to, buffers = buffers:all() }
		end
	)

	calls.delete = call.new(params.one_path, function(args)
		project:remove(args.path)
		return { path = args.path }
	end)

	return calls
end

return M
