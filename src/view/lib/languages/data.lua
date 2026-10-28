-- The languages of data and settings: JSON, YAML, TOML, and INI with its kin (properties, .env).
-- A key is coloured as a property, its value as what it is.

local clike = require("./clike")

local literal = clike.literal

local data = {}

local NUMBER = { "^%-?0[xX]%x+", "^[%-%+]?%d[%d_]*%.?[%d_]*[eE][%+%-]?%d+", "^[%-%+]?%d[%d_]*%.%d[%d_]*", "^[%-%+]?%d[%d_]*" }

local function number(stream)
	for _, pattern in NUMBER do
		if stream:match(pattern) then
			return true
		end
	end
	return false
end

local function quoted(stream, close, escapes)
	local pattern = literal(close)
	while not stream:eol() do
		if escapes and stream:eat("\\") then
			stream:next()
		elseif stream:match(pattern) then
			return true
		else
			stream:next()
		end
	end
	return false
end

-- --- JSON -----------------------------------------------------------------------------------------------

local function json()
	return {
		start = function()
			return { comment = false }
		end,
		token = function(stream, state)
			if state.comment then
				if stream:skip_to("*/") then
					stream:match("%*/")
					state.comment = false
				else
					stream:skip_to_end()
				end
				return "comment"
			end
			if stream:eat_space() then
				return nil
			end
			if stream:match("//") then
				stream:skip_to_end()
				return "comment"
			elseif stream:match("/%*") then
				state.comment = true
				return "comment"
			end
			if stream:eat('"') then
				quoted(stream, '"', true)
				return stream:match("^%s*:", false) and "property" or "string"
			end
			if number(stream) then
				return "number"
			end
			if stream:match("^true") or stream:match("^false") or stream:match("^null") then
				return "atom"
			end
			if stream:match("^[{}%[%],:]") then
				return "punctuation"
			end
			stream:next()
			return "invalid"
		end,
	}
end

-- --- YAML -----------------------------------------------------------------------------------------------

local YAML_ATOMS = clike.set("true false yes no on off null True False Yes No Null TRUE FALSE NULL ~")

local function yaml()
	return {
		start = function()
			return { block = nil }
		end,
		token = function(stream, state)
			-- A block scalar (`|`, `>`): the lines further in than the key that opened it.
			if state.block and stream:sol() then
				local indent = stream:indentation()
				if string.match(stream.string, "^%s*$") or indent > state.block then
					stream:skip_to_end()
					return "string"
				end
				state.block = nil
			end
			if stream:sol() and (stream:match("^%-%-%-") or stream:match("^%.%.%.")) then
				return "meta"
			end
			if stream:eat_space() then
				return nil
			end
			local before = string.sub(stream.string, 1, stream.pos - 1)
			if stream:match("#") and (before == "" or string.match(before, "%s$")) then
				stream:skip_to_end()
				return "comment"
			end
			-- A key: at the start of the line (after a list's `-`), up to its colon.
			if string.match(before, "^%s*$") or string.match(before, "^%s*%-%s+$") then
				if stream:match("^[^#:%s%-%[{][^#:]-%s*:%s", false) or stream:match("^[^#:%s%-%[{][^#:]-%s*:$", false)
					or stream:match('^"[^"]*"%s*:', false) or stream:match("^'[^']*'%s*:", false) then
					stream:match("^[^:]+")
					return "property"
				end
				if stream:match("^%-%s") or stream:match("^%-$") then
					return "punctuation"
				end
			end
			if stream:match("^[|>][%-%+]?%d*%s*$") then
				state.block = string.len(string.match(stream.string, "^%s*"))
				return "meta"
			end
			if stream:eat('"') then
				quoted(stream, '"', true)
				return "string"
			elseif stream:eat("'") then
				quoted(stream, "'", false)
				return "string"
			end
			if stream:match("^[&%*][%w_%-]+") then
				return "variable.special"
			end
			if stream:match("^!!?[%w_%-/]*") then
				return "meta"
			end
			if stream:match("^[:,%[%]{}]") then
				return "punctuation"
			end
			local word = stream:match("^[%w~]+", false)
			if word and YAML_ATOMS[word] then
				local after = string.sub(stream.string, stream.pos + #word, stream.pos + #word)
				if after == "" or string.find(after, "^[%s,%]}]") then
					stream:match("^[%w~]+")
					return "atom"
				end
			end
			if number(stream) and (stream:eol() or stream:match("^%s", false) or stream:match("^[,%]}]", false)) then
				return "number"
			end
			stream:match("^[^#,%[%]{}]+")
			if stream.pos == stream.start then
				stream:next()
			end
			local words = stream:current()
			-- A plain value runs to a comment.
			local hash = string.find(words, "%s#")
			if hash then
				stream:backup(utf8.len(string.sub(words, hash)) or (#words - hash + 1))
			end
			return "string"
		end,
	}
end

-- --- TOML -----------------------------------------------------------------------------------------------

local function toml()
	return {
		start = function()
			return { str = nil }
		end,
		token = function(stream, state)
			if state.str then
				if quoted(stream, state.str, state.str == '"""') then
					state.str = nil
				end
				return "string"
			end
			if stream:eat_space() then
				return nil
			end
			if stream:match("#") then
				stream:skip_to_end()
				return "comment"
			end
			local before = string.sub(stream.string, 1, stream.pos - 1)
			if string.match(before, "^%s*$") and stream:match("^%[%[?[^%]]*%]%]?") then
				return "type"
			end
			if string.match(before, "^%s*$") and (stream:match("^[%w_%-%.\"' ]-%s*=", false)) then
				local key = stream:match("^[^=]+")
				local trailing = #string.match(key, "%s*$")
				if trailing > 0 then
					stream:backup(trailing)
				end
				return "property"
			end
			for _, close in { '"""', "'''" } do
				if stream:match(close) then
					if not quoted(stream, close, close == '"""') then
						state.str = close
					end
					return "string"
				end
			end
			if stream:eat('"') then
				quoted(stream, '"', true)
				return "string"
			elseif stream:eat("'") then
				quoted(stream, "'", false)
				return "string"
			end
			if stream:match("^%d%d%d%d%-%d%d%-%d%d[T %d:%.Z%+%-]*") or stream:match("^%d%d:%d%d:%d%d[%.%d]*") then
				return "number"
			end
			if number(stream) then
				return "number"
			end
			if stream:match("^true") or stream:match("^false") or stream:match("^[%+%-]?inf") or stream:match("^[%+%-]?nan") then
				return "atom"
			end
			if stream:match("^[=,%[%]{}%.]") then
				return "punctuation"
			end
			stream:next()
			return nil
		end,
	}
end

-- --- INI, properties, .env ----------------------------------------------------------------------------------

local function ini()
	return {
		start = function()
			return {}
		end,
		token = function(stream, state)
			if stream:sol() then
				if stream:match("^%s*[;#!]") then
					stream:skip_to_end()
					return "comment"
				end
				if stream:match("^%s*%[[^%]]*%]") then
					return "type"
				end
				if stream:match("^%s*export%s+") then
					return "keyword"
				end
			end
			if stream:eat_space() then
				return nil
			end
			local before = string.sub(stream.string, 1, stream.pos - 1)
			if (string.match(before, "^%s*$") or string.match(before, "^%s*export%s+$")) and stream:match("^[^=:%s]+%s*[=:]", false) then
				stream:match("^[^=:%s]+")
				return "property"
			end
			if stream:match("^[=:]") then
				return "operator"
			end
			if stream:eat('"') then
				quoted(stream, '"', true)
				return "string"
			elseif stream:eat("'") then
				quoted(stream, "'", false)
				return "string"
			end
			if stream:match("^%${[^}]*}") or stream:match("^%$[%a_][%w_]*") then
				return "variable.special"
			end
			if (stream:match("^true", false) or stream:match("^false", false)) and stream:match("^%a+%s*$", false) then
				stream:match("^%a+")
				return "atom"
			end
			if number(stream) and (stream:eol() or stream:match("^%s", false)) then
				return "number"
			end
			stream:skip_to_end()
			return "string"
		end,
	}
end

function data.languages()
	return {
		{
			name = "JSON",
			aliases = { "json", "jsonc", "json5" },
			extensions = { "json", "jsonc", "json5", "jsonl", "geojson", "webmanifest", "har", "babelrc", "eslintrc" },
			names = { ".babelrc", ".eslintrc", ".prettierrc", ".jshintrc", "composer.lock", "flake.lock" },
			reader = json(),
			comment = { line = "//", block = { "/*", "*/" } },
			indent = { decrease = "^\\s*[\\}\\]]" },
			pairs = '()[]{}""',
		},
		{
			name = "YAML",
			aliases = { "yaml", "yml" },
			extensions = { "yaml", "yml" },
			names = { ".clang-format", ".clangd" },
			reader = yaml(),
			comment = { line = "#" },
			indent = { increase = ":\\s*(#.*)?$|^\\s*-\\s*$|[|>][-+]?\\s*$" },
			pairs = "()[]{}''\"\"",
		},
		{
			name = "TOML",
			aliases = { "toml" },
			extensions = { "toml" },
			names = { "Cargo.lock", "Pipfile", "poetry.lock" },
			reader = toml(),
			comment = { line = "#" },
			pairs = "()[]{}''\"\"",
		},
		{
			name = "INI",
			aliases = { "ini", "properties", "env", "dotenv", "conf", "cfg", "editorconfig", "gitconfig" },
			extensions = { "ini", "cfg", "conf", "properties", "env", "editorconfig", "gitconfig", "desktop", "service", "npmrc" },
			names = { ".editorconfig", ".gitconfig", ".npmrc", ".env", ".flake8", "setup.cfg", "tox.ini", ".pylintrc" },
			reader = ini(),
			comment = { line = "#" },
			pairs = "()[]{}\"\"",
		},
	}
end

return data
