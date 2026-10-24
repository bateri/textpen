-- What the view may call and subscribe to: textpen's side of the six messages. Whatever writes to
-- the disk is the view's word, and the person's.
--
-- Nothing in this file knows about a terminal beyond the shapes of `pencere.serve`'s handlers;
-- app.lua is the only place that asks a terminal to show the view.
--
-- Calls: `boot`, `list`, `files`, `search`, `create`, `rename`, `delete`, `open`, `edit`, `slice`,
-- `save`, `buffer`, `ui`, `settings`, `discard` and `handoff`. Topic: `changes`. A call that fails
-- ends with a code the view can act on: `bad-params` for a call that is not what the program
-- asks, `out-of-sync` for an edit made on a version the program no longer holds, `failed` for what
-- the disk or the project would not do (the words are for the person).

local pencere = require("pencere")
local json, time = pencere.json, pencere.time

local buffers_module = require("lib.buffers")
local check = require("lib.check")
local wire = require("lib.wire")

local M = {}

-- How often the `changes` topic looks at the disk and at git.
M.CHANGES_EVERY = 2

local place = check.object({ anchor = check.number(), head = check.number(), top = check.number() })

local ui_state = check.object({
	tabs = check.array(check.string()),
	active = check.nullable(check.string()),
	expanded = check.array(check.string()),
	places = check.record(place),
	recent = check.array(check.string()),
	preview = check.array(check.string()),
})

local settings_schema = check.object({
	vim = check.boolean(),
	wrap = check.boolean(),
	fontSize = check.number(),
	lineNumbers = check.enum("on", "relative", "off"),
	tabSize = check.number(),
	insertSpaces = check.boolean(),
	whitespace = check.boolean(),
	autoSave = check.boolean(),
	sidebar = check.boolean(),
	sidebarWidth = check.number(),
})

local nothing = check.object({})
local one_path = check.object({ path = check.string(1) })

-- What a failure of the disk or of the project tells the view: the words are the person's. An error
-- that is a table with a code (out-of-sync, bad-params) is the call's own answer.
local function failure(err)
	if type(err) == "table" and err.code then
		return err
	end
	local words = string.gsub(tostring(err), "\nstack traceback:.*$", "")
	return { code = "failed", message = words }
end

-- A call that takes the params `schema` describes: the view is not trusted to send them, so what
-- does not fit is answered `bad-params`, and what the work raises is `failure`.
local function call(schema, work)
	return function(params, context)
		local parsed, why = check.parse(schema, params)
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

function M.create(textpen)
	local calls = {}

	-- What the view shows first: the project, its settings, and what is open.
	calls.boot = call(nothing, function()
		return textpen:boot()
	end)

	-- ---- files ----------------------------------------------------------------------------

	calls.list = call(check.object({ dir = check.string() }), function(args)
		local listing = textpen.project:list(args.dir)
		-- A message to the view is at most 1 MiB: a directory too big for one is cut, and says so.
		local count = wire.fitting(listing.entries, function(entry)
			return wire.wire_size(entry.name) + wire.wire_size(entry.path) + 80
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
	calls.files = call(check.object({ from = check.optional(check.integer(0)) }), function(args)
		local from = args.from or 0
		local index = textpen.project:files()
		local rest = {}
		for position = from + 1, #index.files do
			rest[#rest + 1] = index.files[position]
		end
		local count = wire.fitting(rest, function(path)
			return wire.wire_size(path) + 4
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

	calls.search = call(
		check.object({
			query = check.string(1),
			regex = check.optional(check.boolean()),
			caseSensitive = check.optional(check.boolean()),
			wholeWord = check.optional(check.boolean()),
		}),
		function(args)
			local result = textpen.project:search({
				query = args.query,
				regex = args.regex == true,
				caseSensitive = args.caseSensitive == true,
				wholeWord = args.wholeWord == true,
			})
			-- A message to the view is at most 1 MiB: a search too long for one is cut, and says so.
			local count = wire.fitting(result.files, function(file)
				return wire.wire_size(json.encode(file))
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

	calls.create = call(
		check.object({ path = check.string(1), kind = check.enum("file", "dir") }),
		function(args)
			return { path = textpen:create(args.path, args.kind) }
		end
	)

	calls.rename = call(
		check.object({ from = check.string(1), to = check.string(1) }),
		function(args)
			return { path = textpen:rename(args.from, args.to), buffers = textpen.buffers:all() }
		end
	)

	calls.delete = call(one_path, function(args)
		textpen:remove(args.path)
		return { path = args.path }
	end)

	-- ---- buffers --------------------------------------------------------------------------

	-- A file, or the buffer of one already open, with its text.
	calls.open = call(one_path, function(args)
		return buffers_module.sendable(textpen.buffers:open(args.path))
	end)

	-- The view's edits to a buffer, made on a version the program checks.
	calls.edit = call(
		check.object({
			path = check.string(),
			base = check.integer(),
			edits = check.array(check.tuple({ check.integer(), check.integer(), check.string() })),
		}),
		function(args)
			return textpen.buffers:edit(args.path, args.base, args.edits)
		end
	)

	-- The piece of a text too long for one message that begins at `from`, and where the next begins.
	calls.slice = call(
		check.object({ path = check.string(), version = check.integer(), from = check.integer(0) }),
		function(args)
			return textpen.buffers:slice(args.path, args.version, args.from)
		end
	)

	calls.save = call(
		check.object({ path = check.string(), version = check.integer(), force = check.optional(check.boolean()) }),
		function(args)
			return textpen.buffers:save(args.path, args.version, args.force == true)
		end
	)

	-- Read a buffer again from disk, keep it over a changed file, take or drop a draft, change its
	-- line ends, close it.
	calls.buffer = call(
		check.object({
			path = check.string(),
			action = check.enum("text", "revert", "keep", "restore", "drop", "lf", "crlf", "close"),
			discard = check.optional(check.boolean()),
		}),
		function(args)
			local buffers, action = textpen.buffers, args.action
			if action == "text" then
				return buffers_module.sendable(buffers:text(args.path))
			elseif action == "revert" then
				return buffers_module.sendable(buffers:revert(args.path))
			elseif action == "keep" then
				return buffers:keep(args.path)
			elseif action == "restore" or action == "drop" then
				return buffers_module.sendable(buffers:draft(args.path, action == "restore"))
			elseif action == "lf" or action == "crlf" then
				return buffers:set_eol(args.path, action)
			else
				buffers:close(args.path, args.discard == true)
				return { path = args.path }
			end
		end
	)

	-- Quitting without saving: what is not saved is not kept as a draft either.
	calls.discard = call(nothing, function()
		textpen.buffers:discard_all()
		return json.object()
	end)

	-- ---- the view's state -----------------------------------------------------------------

	-- Where the view is (tabs, folders, cursors), kept for the next view and the next run.
	calls.ui = call(check.object({ ui = ui_state }), function(args)
		textpen:keep(args.ui)
		return json.object()
	end)

	calls.settings = call(check.object({ settings = settings_schema }), function(args)
		return { settings = textpen:save_settings(args.settings) }
	end)

	-- Hands the pane to a shell in the project, which gives it back when it ends.
	calls.handoff = call(check.object({ kind = check.literal("shell") }), function()
		local desk = textpen.desk
		return { state = desk == nil and "unavailable" or desk.shell() }
	end)

	local topics = {}

	-- The disk and git, as they change: one value at once and then one only when something differs
	-- from what the view was told. Git's own state comes along when it changed.
	topics.changes = function(_, subscription)
		local timer = nil
		local told = nil
		local function tick()
			timer = nil
			local ok, found = pcall(function()
				return textpen:changes()
			end)
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

	return { calls = calls, topics = topics }
end

return M
