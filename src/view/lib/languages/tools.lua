-- The languages of the tools around code: SQL, the Dockerfile, the Makefile, and a diff.

local clike = require("./clike")

local set = clike.set

local tools = {}

-- --- SQL ---------------------------------------------------------------------------------------------

local SQL_KEYWORDS = set("select from where group by order having limit offset insert into values update set delete "
	.. "create table drop alter add column index view join left right inner outer full cross on using and or not "
	.. "null is in as distinct case when then else end begin commit rollback transaction union all exists between "
	.. "like ilike primary key foreign references unique check default constraint with recursive returning "
	.. "if replace temporary temp trigger function procedure return returns language declare cascade restrict "
	.. "asc desc nulls first last over partition window filter grant revoke schema database explain analyze "
	.. "vacuum pragma upsert conflict do nothing lateral natural except intersect fetch only rows row")
local SQL_TYPES = set("int integer bigint smallint tinyint serial bigserial real double precision float decimal "
	.. "numeric boolean bool char varchar text blob bytea date time timestamp timestamptz interval json jsonb uuid "
	.. "money")
local SQL_ATOMS = set("true false null")

local function sql()
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
			if stream:match("%-%-") or stream:match("#") then
				stream:skip_to_end()
				return "comment"
			end
			if stream:match("/%*") then
				state.comment = true
				return "comment"
			end
			local c = stream:peek()
			if c == "'" then
				stream:next()
				while not stream:eol() do
					if stream:next() == "'" and not stream:eat("'") then
						break
					end
				end
				return "string"
			elseif c == '"' or c == "`" then
				stream:next()
				stream:skip_to(c)
				stream:eat(c)
				return "property"
			end
			if stream:match("^%d+%.?%d*") then
				return "number"
			end
			if stream:match("^[:@%$][%w_]+") or stream:match("^%?%d*") then
				return "variable.special"
			end
			local word = stream:match("^[%a_][%w_]*")
			if word then
				local lower = string.lower(word)
				if SQL_KEYWORDS[lower] then
					return "keyword"
				elseif SQL_TYPES[lower] then
					return "type"
				elseif SQL_ATOMS[lower] then
					return "atom"
				elseif stream:match("^%s*%(", false) then
					return "function"
				end
				return nil
			end
			if stream:match("^[%(%)%.,;]") then
				return "punctuation"
			end
			if stream:match("^[%+%-%*/<>=!|%%]+") then
				return "operator"
			end
			stream:next()
			return nil
		end,
	}
end

-- --- Dockerfile -----------------------------------------------------------------------------------------

local DOCKER = set("FROM RUN CMD LABEL MAINTAINER EXPOSE ENV ADD COPY ENTRYPOINT VOLUME USER WORKDIR ARG ONBUILD "
	.. "STOPSIGNAL HEALTHCHECK SHELL")

local function dockerfile()
	return {
		start = function()
			return {}
		end,
		token = function(stream, state)
			if stream:sol() and stream:match("^%s*#") then
				stream:skip_to_end()
				return "comment"
			end
			if stream:eat_space() then
				return nil
			end
			local before = string.sub(stream.string, 1, stream.pos - 1)
			local word = stream:match("^[%a]+", false)
			if word and string.match(before, "^%s*$") and DOCKER[string.upper(word)] then
				stream:match("^[%a]+")
				return "keyword"
			end
			if word and string.upper(word) == "AS" and stream:match("^[%a]+%s", false) then
				stream:match("^[%a]+")
				return "keyword"
			end
			if stream:match("^%-%-[%w%-]+") then
				return "attribute"
			end
			if stream:match("^%${[^}]*}") or stream:match("^%$[%a_][%w_]*") then
				return "variable.special"
			end
			local c = stream:peek()
			if c == '"' or c == "'" then
				stream:next()
				while not stream:eol() do
					local next_char = stream:next()
					if next_char == "\\" then
						stream:next()
					elseif next_char == c then
						break
					end
				end
				return "string"
			end
			if stream:match("^[%[%],=]") then
				return "punctuation"
			end
			stream:match("^[^%s%$\"'=%[%],]+")
			if stream.pos == stream.start then
				stream:next()
			end
			return nil
		end,
	}
end

-- --- Makefile --------------------------------------------------------------------------------------------

local MAKE = set("include -include sinclude ifeq ifneq ifdef ifndef else endif define endef export unexport override vpath")

local function makefile()
	return {
		start = function()
			return {}
		end,
		token = function(stream, state)
			if stream:sol() and stream:match("^\t") then
				-- A recipe: the shell's, its variables marked.
				return nil
			end
			if stream:match("#") then
				stream:skip_to_end()
				return "comment"
			end
			if stream:eat_space() then
				return nil
			end
			if stream:match("^%$%([^%)]*%)") or stream:match("^%${[^}]*}") or stream:match("^%$[%w@<%^%*%?%%+]") then
				return "variable.special"
			end
			local before = string.sub(stream.string, 1, stream.pos - 1)
			if string.match(before, "^%s*$") then
				local word = stream:match("^%-?[%a]+", false)
				if word and MAKE[word] then
					stream:match("^%-?[%a]+")
					return "keyword"
				end
				if stream:match("^[%w_%.%-]+%s*[:%?%+!]?=", false) then
					stream:match("^[%w_%.%-]+")
					return "property"
				end
				if stream:match("^[^:#=%s][^:#=]-:", false) and not stream:match("^[^:#=]-:=", false) then
					stream:match("^[^:#=]-:")
					return "function"
				end
			end
			local c = stream:peek()
			if c == '"' or c == "'" then
				stream:next()
				stream:skip_to(c)
				stream:eat(c)
				return "string"
			end
			stream:match("^[^%s%$#\"']+")
			if stream.pos == stream.start then
				stream:next()
			end
			return nil
		end,
	}
end

-- --- Diff ----------------------------------------------------------------------------------------------

local function diff()
	return {
		start = function()
			return {}
		end,
		token = function(stream)
			local style = nil
			if stream:match("^%+%+%+") or stream:match("^%-%-%-") or stream:match("^diff ") or stream:match("^index ")
				or stream:match("^new file") or stream:match("^deleted file") or stream:match("^similarity") or stream:match("^rename ") then
				style = "meta"
			elseif stream:match("^@@") then
				style = "changed"
			elseif stream:match("^%+") then
				style = "inserted"
			elseif stream:match("^%-") then
				style = "deleted"
			end
			stream:skip_to_end()
			return style
		end,
	}
end

function tools.languages()
	return {
		{
			name = "SQL",
			aliases = { "sql", "postgresql", "pgsql", "mysql", "sqlite", "plsql" },
			extensions = { "sql", "psql", "pgsql", "ddl", "dml" },
			reader = sql(),
			comment = { line = "--", block = { "/*", "*/" } },
			indent = { increase = "\\(\\s*$", decrease = "^\\s*\\)" },
			pairs = "()[]''\"\"",
		},
		{
			name = "Dockerfile",
			aliases = { "dockerfile", "docker", "containerfile" },
			extensions = { "dockerfile", "containerfile" },
			names = { "Dockerfile", "Containerfile" },
			reader = dockerfile(),
			comment = { line = "#" },
			pairs = "()[]{}''\"\"",
		},
		{
			name = "Makefile",
			aliases = { "make", "makefile", "mk" },
			extensions = { "mk", "mak", "make" },
			names = { "Makefile", "makefile", "GNUmakefile", "BSDmakefile" },
			reader = makefile(),
			comment = { line = "#" },
			tabs = true,
			pairs = "()[]{}''\"\"",
		},
		{
			name = "Diff",
			aliases = { "diff", "patch" },
			extensions = { "diff", "patch", "rej" },
			reader = diff(),
		},
	}
end

return tools
