-- The scripting languages: Python, Ruby, the shell and Lua, each a reader of its own (their comments,
-- strings and blocks have little in common), with the words, the indent and the brackets each says.

local clike = require("./clike")

local set, literal = clike.set, clike.literal

local scripting = {}

local IDENT = "^[%a_\128-\255][%w_\128-\255]*"

local function number(stream)
	return stream:match("^0[xX][%x_]+") or stream:match("^0[bBoO][%x_]+")
		or stream:match("^%d[%d_]*%.?[%d_]*[eE][%+%-]?%d+") or stream:match("^%d[%d_]*%.%d[%d_]*")
		or stream:match("^%d[%d_]*") or stream:match("^%.%d[%d_]*")
end

-- A string to `close` (escapes with `\` where `escapes`); one that goes past the line is kept open in
-- `state.str` where `many`.
local function read_string(stream, state, close, escapes, many, style)
	local pattern = literal(close)
	while not stream:eol() do
		if escapes and stream:eat("\\") then
			stream:next()
		elseif stream:match(pattern) then
			state.str = nil
			return style or "string"
		else
			stream:next()
		end
	end
	state.str = many and { close = close, escapes = escapes, style = style } or nil
	return style or "string"
end

local function operator_or_punctuation(stream)
	if stream:match("^[%(%)%[%]{};,%.]") then
		return "punctuation"
	elseif stream:match("^[%+%-%*/%%=&|<>!%?:~%^@]+") then
		return "operator"
	end
	stream:next()
	return nil
end

local function word_kind(stream, word, o, state)
	local def = state.def
	state.def = nil
	if o.keywords[word] then
		state.def = o.definers and o.definers[word]
		return "keyword"
	elseif o.atoms and o.atoms[word] then
		return "atom"
	elseif o.special and o.special[word] then
		return "variable.special"
	elseif o.builtins and o.builtins[word] then
		return "function"
	elseif def then
		return def
	elseif stream:match("^%s*%(", false) then
		return "function"
	elseif state.dot then
		return "property"
	end
	local first = string.sub(word, 1, 1)
	if o.capitals and first >= "A" and first <= "Z" then
		return (#word > 1 and not string.find(word, "%l")) and "constant" or "type"
	end
	return nil
end

-- --- Python ------------------------------------------------------------------------------------------

local PYTHON = {
	keywords = set("and as assert async await break class continue def del elif else except finally for from global "
		.. "if import in is lambda nonlocal not or pass raise return try while with yield match case type"),
	atoms = set("True False None Ellipsis NotImplemented"),
	special = set("self cls"),
	builtins = set("print len range enumerate zip map filter sorted reversed open isinstance super int str float "
		.. "list dict set tuple bool bytes type repr abs min max sum any all iter next hasattr getattr setattr"),
	definers = { def = "function", class = "type" },
	capitals = true,
}

local function python()
	return {
		start = function()
			return { str = nil, def = nil, dot = false }
		end,
		token = function(stream, state)
			if state.str then
				return read_string(stream, state, state.str.close, state.str.escapes, true)
			end
			if stream:eat_space() then
				return nil
			end
			local dot = state.dot
			state.dot = false
			if stream:match("#") then
				stream:skip_to_end()
				return "comment"
			end
			if string.match(string.sub(stream.string, 1, stream.pos - 1), "^%s*$") and stream:match("^@[%w_%.]+") then
				return "meta"
			end
			-- A string, with its prefix (r, b, f, u and their pairs): three quotes go over lines.
			local prefix = stream:match("^[rRbBfFuU][rRbBfF]?", false)
			local at = stream.pos + (prefix and #prefix or 0)
			local q = string.sub(stream.string, at, at)
			if q == '"' or q == "'" then
				if prefix then
					stream:match("^[rRbBfFuU][rRbBfF]?")
				end
				local raw = prefix and string.find(prefix, "[rR]") ~= nil
				if stream:match(q .. q .. q) then
					return read_string(stream, state, q .. q .. q, not raw, true)
				end
				stream:next()
				return read_string(stream, state, q, not raw, false)
			end
			if number(stream) then
				stream:match("^[jJlL]")
				return "number"
			end
			local word = stream:match(IDENT)
			if word then
				local def = state.def
				state.def = PYTHON.keywords[word] and PYTHON.definers[word] or nil
				return word_kind(stream, word, PYTHON, { def = def, dot = dot })
			end
			if stream:match("^%.") then
				state.dot = true
				return "punctuation"
			end
			return operator_or_punctuation(stream)
		end,
	}
end

-- --- Ruby --------------------------------------------------------------------------------------------

local RUBY = {
	keywords = set("BEGIN END alias and begin break case class def defined? do else elsif end ensure for if in module "
		.. "next not or redo rescue retry return super then undef unless until when while yield require "
		.. "require_relative include extend attr_reader attr_writer attr_accessor private protected public raise lambda proc"),
	atoms = set("true false nil"),
	special = set("self"),
	definers = { def = "function", class = "type", module = "type" },
	capitals = true,
}

local function ruby()
	return {
		start = function()
			return { str = nil, doc = false, def = nil }
		end,
		token = function(stream, state)
			if state.doc then
				if stream:sol() and stream:match("^=end") then
					state.doc = false
				end
				stream:skip_to_end()
				return "comment"
			end
			if state.str then
				return read_string(stream, state, state.str.close, state.str.escapes, true)
			end
			if stream:sol() and stream:match("^=begin") then
				state.doc = true
				stream:skip_to_end()
				return "comment"
			end
			if stream:eat_space() then
				return nil
			end
			if stream:match("#") then
				stream:skip_to_end()
				return "comment"
			end
			local q = stream:peek()
			if q == '"' or q == "'" or q == "`" then
				stream:next()
				return read_string(stream, state, q, q ~= "'", true)
			end
			if stream:match("^:[%a_][%w_]*[%?!]?") then
				return "atom"
			end
			if stream:match("^@@?[%a_][%w_]*") or stream:match("^%$[%a_][%w_]*") then
				return "variable.special"
			end
			if number(stream) then
				return "number"
			end
			local word = stream:match("^[%a_][%w_]*[%?!]?")
			if word then
				return word_kind(stream, word, RUBY, state)
			end
			return operator_or_punctuation(stream)
		end,
	}
end

-- --- The shell -----------------------------------------------------------------------------------------

local SHELL = {
	keywords = set("if then else elif fi case esac for select while until do done in function time coproc return "
		.. "break continue exit export local readonly declare typeset unset shift source alias"),
	builtins = set("echo printf cd pwd read test eval exec set trap wait kill command type which"),
}

local function shell()
	return {
		start = function()
			return { str = nil }
		end,
		token = function(stream, state)
			if state.str then
				return read_string(stream, state, state.str.close, state.str.escapes, true)
			end
			if stream:eat_space() then
				return nil
			end
			if stream:match("#") then
				stream:skip_to_end()
				return "comment"
			end
			local q = stream:peek()
			if q == '"' or q == "'" or q == "`" then
				stream:next()
				return read_string(stream, state, q, q ~= "'", true)
			end
			if stream:match("^%${[^}]*}?") or stream:match("^%$%(%(?") or stream:match("^%$[%w_@%*#%?%$!%-]") or stream:match("^%$[%a_][%w_]*") then
				return "variable.special"
			end
			if stream:match("^%-%-?[%a][%w%-]*") then
				return "attribute"
			end
			if stream:match("^%d+") then
				return "number"
			end
			local word = stream:match("^[%a_][%w_%-]*")
			if word then
				if stream:match("^=", false) then
					return "property"
				elseif SHELL.keywords[word] then
					return "keyword"
				elseif SHELL.builtins[word] then
					return "function"
				end
				return nil
			end
			if stream:match("^[|&;<>]+") then
				return "operator"
			end
			stream:next()
			return nil
		end,
	}
end

-- --- Lua ---------------------------------------------------------------------------------------------------

local LUA = {
	keywords = set("and break do else elseif end for function goto if in local not or repeat return then until while continue export type"),
	atoms = set("true false nil"),
	special = set("self"),
	builtins = set("print pairs ipairs next type tostring tonumber require error assert pcall xpcall select setmetatable getmetatable rawget rawset rawequal unpack"),
	definers = { ["function"] = "function" },
}

local function lua()
	return {
		start = function()
			return { long = nil, comment = false, def = nil, dot = false }
		end,
		token = function(stream, state)
			if state.long then
				local close = "]" .. state.long .. "]"
				local style = state.comment and "comment" or "string"
				if stream:skip_to(close) then
					stream:match(literal(close))
					state.long = nil
				else
					stream:skip_to_end()
				end
				return style
			end
			if stream:eat_space() then
				return nil
			end
			local dot = state.dot
			state.dot = false
			local level = stream:match("^%-%-%[(=*)%[")
			if level then
				state.long, state.comment = level, true
				return "comment"
			end
			if stream:match("^%-%-") then
				stream:skip_to_end()
				return "comment"
			end
			level = stream:match("^%[(=*)%[")
			if level then
				state.long, state.comment = level, false
				local close = "]" .. level .. "]"
				if stream:skip_to(close) then
					stream:match(literal(close))
					state.long = nil
				else
					stream:skip_to_end()
				end
				return "string"
			end
			local q = stream:peek()
			if q == '"' or q == "'" or q == "`" then
				stream:next()
				local scratch = {}
				return read_string(stream, scratch, q, true, false)
			end
			if number(stream) then
				return "number"
			end
			local word = stream:match(IDENT)
			if word then
				local def = state.def
				state.def = LUA.keywords[word] and LUA.definers[word] or nil
				return word_kind(stream, word, LUA, { def = def, dot = dot })
			end
			if stream:match("^%.%.%.?") then
				return "operator"
			end
			if stream:match("^[%.:]") then
				state.dot = true
				return "punctuation"
			end
			return operator_or_punctuation(stream)
		end,
	}
end

function scripting.languages()
	return {
		{
			name = "Python",
			aliases = { "py", "python", "python3" },
			extensions = { "py", "pyi", "pyw" },
			names = { "SConstruct", "SConscript" },
			shebang = "python",
			reader = python(),
			comment = { line = "#" },
			indent = { increase = ":\\s*(#.*)?$", decrease = "^\\s*(elif|else|except|finally|case)\\b.*:\\s*(#.*)?$" },
			pairs = "()[]{}''\"\"",
		},
		{
			name = "Ruby",
			aliases = { "rb", "ruby" },
			extensions = { "rb", "rake", "gemspec", "ru" },
			names = { "Gemfile", "Rakefile", "Guardfile", "Podfile", "Vagrantfile" },
			shebang = "ruby",
			reader = ruby(),
			comment = { line = "#", block = { "=begin", "=end" } },
			indent = {
				increase = "^\\s*(def|class|module|if|unless|while|until|for|begin|case|else|elsif|when|rescue|ensure)\\b|\\bdo(\\s*\\|[^|]*\\|)?\\s*$",
				decrease = "^\\s*(end|else|elsif|when|rescue|ensure)\\b",
			},
			pairs = "()[]{}''\"\"",
		},
		{
			name = "Shell",
			aliases = { "sh", "bash", "zsh", "shell", "shellscript" },
			extensions = { "sh", "bash", "zsh", "ksh", "fish", "command" },
			names = { ".bashrc", ".bash_profile", ".bash_aliases", ".bash_logout", ".zshrc", ".zprofile", ".zshenv",
				".zlogin", ".profile", ".envrc", "PKGBUILD" },
			shebang = "sh",
			reader = shell(),
			comment = { line = "#" },
			indent = { increase = "\\b(then|do|else|in)\\s*$|\\{\\s*$|\\(\\s*$", decrease = "^\\s*(fi|done|esac|else|elif)\\b|^\\s*[\\}\\)]" },
			pairs = "()[]{}''\"\"``",
		},
		{
			name = "Lua",
			aliases = { "lua", "luau" },
			extensions = { "lua", "luau", "rockspec" },
			shebang = "lua",
			reader = lua(),
			comment = { line = "--", block = { "--[[", "]]" } },
			indent = {
				increase = "\\b(then|do|else|repeat)\\s*$|\\bfunction\\b[^)]*\\)\\s*$|\\{\\s*$",
				decrease = "^\\s*(end|else|elseif|until)\\b|^\\s*\\}",
			},
			pairs = "()[]{}''\"\"",
		},
	}
end

return scripting
