-- The calls about the open files: `open`, `edit`, `slice`, `save`, `buffer` and `discard`. The
-- program holds each file's text (state/buffers); what the view is sent of it is cut to fit one
-- message: a text too long for one is announced (`partial`) and read in pieces with `slice`.
--
-- This is api/'s: the files of api/ know the page, and project/, state/ and terminal/ do not.

local pencere = require("pencere")
local json, text = pencere.json, pencere.text

local call = require("api.call")
local params = require("api.params")
local pieces = require("api.pieces")

local M = {}

-- A buffer as it can go to the view in one message: whole, or announced, to be read in pieces.
local function sendable(opened)
	local content = opened.text
	if content == nil then
		return opened
	end
	local units = text.units(content)
	if units <= pieces.WIRE_BUDGET / 6 then
		return opened
	end
	if pieces.wire_size(content) <= pieces.WIRE_BUDGET then
		return opened
	end
	opened.text = ""
	opened.partial = { length = units }
	return opened
end

function M.calls(session)
	local buffers = session.buffers
	local calls = {}

	-- A file, or the buffer of one already open, with its text.
	calls.open = call.new(params.one_path, function(args)
		return sendable(buffers:open(args.path))
	end)

	-- The view's edits to a buffer, made on a version the program checks.
	calls.edit = call.new(
		params.object({
			path = params.string(),
			base = params.integer(),
			edits = params.array(params.tuple({ params.integer(), params.integer(), params.string() })),
		}),
		function(args)
			return buffers:edit(args.path, args.base, args.edits)
		end
	)

	-- The piece of a text too long for one message that begins at `from`, and where the next begins.
	calls.slice = call.new(
		params.object({ path = params.string(), version = params.integer(), from = params.integer(0) }),
		function(args)
			return buffers:slice(args.path, args.version, args.from, pieces.WIRE_BUDGET)
		end
	)

	calls.save = call.new(
		params.object({
			path = params.string(),
			version = params.integer(),
			force = params.optional(params.boolean()),
		}),
		function(args)
			return buffers:save(args.path, args.version, args.force == true)
		end
	)

	-- Read a buffer again from disk, keep it over a changed file, take or drop a draft, change its
	-- line ends, close it.
	calls.buffer = call.new(
		params.object({
			path = params.string(),
			action = params.enum("text", "revert", "keep", "restore", "drop", "lf", "crlf", "close"),
			discard = params.optional(params.boolean()),
		}),
		function(args)
			local action = args.action
			if action == "text" then
				return sendable(buffers:text(args.path))
			elseif action == "revert" then
				return sendable(buffers:revert(args.path))
			elseif action == "keep" then
				return buffers:keep(args.path)
			elseif action == "restore" or action == "drop" then
				return sendable(buffers:draft(args.path, action == "restore"))
			elseif action == "lf" or action == "crlf" then
				return buffers:set_eol(args.path, action)
			else
				buffers:close(args.path, args.discard == true)
				return { path = args.path }
			end
		end
	)

	-- Quitting without saving: what is not saved is not kept as a draft either.
	calls.discard = call.new(params.nothing, function()
		buffers:discard_all()
		return json.object()
	end)

	return calls
end

return M
