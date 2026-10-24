-- What the view sends is not trusted: each call says what it asks (a schema), and what does not
-- fit it is answered `bad-params`, never run. A schema is a function from a value to the value
-- as the program takes it (extra keys of an object are dropped), or to nil and the words for what
-- is wrong, led by where (`tabs.0`). Lua has no `undefined`: a value that is nil is absent or null.

local pencere = require("pencere")
local json = pencere.json

local M = {}

-- What a value from JSON is. A table that is empty and has no mark is a list, as a page's `[]` is;
-- `{}` arrives marked (`json.object()`).
local function kind(value)
	local t = type(value)
	if t ~= "table" then
		return t
	end
	if getmetatable(value) == "json.object" then
		return "object"
	end
	local count = 0
	for _ in pairs(value) do
		count += 1
	end
	if count == 0 then
		return "array"
	end
	local length = #value
	if length == count then
		return "array"
	end
	for key in pairs(value) do
		if type(key) ~= "string" then
			return "mixed"
		end
	end
	return "object"
end
M.kind = kind

local function where(path)
	if #path == 0 then
		return ""
	end
	return table.concat(path, ".") .. ": "
end

local function wrong(path, words)
	return nil, where(path) .. words
end

local function describe(value)
	local k = kind(value)
	if k == "nil" then
		return "nothing"
	end
	return k
end

function M.string(minimum)
	return function(value, path)
		if type(value) ~= "string" then
			return wrong(path, "expected a string, got " .. describe(value))
		end
		if minimum and #value < minimum then
			return wrong(path, "expected at least " .. minimum .. " character")
		end
		return value
	end
end

function M.number()
	return function(value, path)
		if type(value) ~= "number" or value ~= value or value == math.huge or value == -math.huge then
			return wrong(path, "expected a number, got " .. describe(value))
		end
		return value
	end
end

function M.integer(minimum)
	return function(value, path)
		if type(value) ~= "number" or value ~= value or value == math.huge or value == -math.huge or math.floor(value) ~= value then
			return wrong(path, "expected an integer, got " .. describe(value))
		end
		if minimum and value < minimum then
			return wrong(path, "expected at least " .. minimum)
		end
		return value
	end
end

function M.boolean()
	return function(value, path)
		if type(value) ~= "boolean" then
			return wrong(path, "expected true or false, got " .. describe(value))
		end
		return value
	end
end

function M.enum(...)
	local names = { ... }
	return function(value, path)
		for _, name in ipairs(names) do
			if value == name then
				return value
			end
		end
		return wrong(path, "expected one of " .. table.concat(names, ", "))
	end
end

function M.literal(expected)
	return function(value, path)
		if value ~= expected then
			return wrong(path, "expected " .. tostring(expected))
		end
		return value
	end
end

-- Not there is as good as there: for a key a call may leave out.
function M.optional(schema)
	return function(value, path)
		if value == nil then
			return nil, nil, true
		end
		return schema(value, path)
	end
end

-- A value or null. (Lua cannot tell null from absent, so absent is allowed.)
function M.nullable(schema)
	return function(value, path)
		if value == nil then
			return nil, nil, true
		end
		return schema(value, path)
	end
end

function M.array(schema)
	return function(value, path)
		if kind(value) ~= "array" then
			return wrong(path, "expected a list, got " .. describe(value))
		end
		local out = {}
		for index = 1, #value do
			path[#path + 1] = tostring(index - 1)
			local item, why = schema(value[index], path)
			path[#path] = nil
			if why then
				return nil, why
			end
			out[index] = item
		end
		return out
	end
end

-- A list of exactly these, one schema for each place.
function M.tuple(schemas)
	return function(value, path)
		if kind(value) ~= "array" or #value ~= #schemas then
			return wrong(path, "expected a list of " .. #schemas)
		end
		local out = {}
		for index, schema in ipairs(schemas) do
			path[#path + 1] = tostring(index - 1)
			local item, why = schema(value[index], path)
			path[#path] = nil
			if why then
				return nil, why
			end
			out[index] = item
		end
		return out
	end
end

-- An object whose every member is one schema, under any key. Its keys are text; a list is not it.
-- The result is a plain table: a key such as `__index` or `constructor` is a key and nothing more.
function M.record(schema)
	return function(value, path)
		if kind(value) ~= "object" then
			return wrong(path, "expected an object, got " .. describe(value))
		end
		local out = json.object()
		for key, item in pairs(value) do
			if type(key) ~= "string" then
				return wrong(path, "expected keys that are text")
			end
			path[#path + 1] = key
			local clean, why = schema(item, path)
			path[#path] = nil
			if why then
				return nil, why
			end
			out[key] = clean
		end
		return out
	end
end

-- An object with these members; others it was sent are left out of what it gives.
function M.object(shape)
	local keys = {}
	for key in pairs(shape) do
		keys[#keys + 1] = key
	end
	table.sort(keys)
	return function(value, path)
		if kind(value) ~= "object" then
			return wrong(path, "expected an object, got " .. describe(value))
		end
		local out = {}
		for _, key in ipairs(keys) do
			path[#path + 1] = key
			local clean, why, absent = shape[key](value[key], path)
			path[#path] = nil
			if why then
				return nil, why
			end
			if not absent then
				out[key] = clean
			end
		end
		return out
	end
end

-- `params` as `schema` has it, or nil and the words for what is wrong. No params at all is `{}`.
function M.parse(schema, params)
	if params == nil then
		params = json.object()
	end
	return schema(params, {})
end

return M
