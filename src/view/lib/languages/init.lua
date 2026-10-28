-- The languages the editor reads files in: how each is told from a file's name or first line, what
-- colours it (a language of the kit's, `pencere.Syntax`, made from the reader of its family) and
-- what the editor is told of it (its comments, the rules of its indent, folding by the indent, the
-- brackets and quotes that close themselves).
--
-- The families: the languages of braces (clike: JavaScript, TypeScript, JSX, TSX, Rust, Go, C, C++,
-- C#, Java, Kotlin, Swift, PHP), the scripting ones (Python, Ruby, the shell, Lua), markup (HTML, XML,
-- Markdown), data (JSON, YAML, TOML, INI), style sheets (CSS, SCSS, Less) and the tools' (SQL, the
-- Dockerfile, the Makefile, a diff).

local clike = require("@self/clike")
local scripting = require("@self/scripting")
local markup = require("@self/markup")
local data = require("@self/data")
local sheets = require("@self/sheets")
local tools = require("@self/tools")
local STYLES = require("@self/styles")

local b = pencere

local languages = {}

languages.STYLES = STYLES

-- A file in no language the page knows: prose, as the editor is with nothing said.
languages.plain = { name = "Plain Text", family = "prose", pairs = "()[]{}" }

local list = { languages.plain }
local by_alias = { ["plain text"] = languages.plain, plaintext = languages.plain, text = languages.plain, txt = languages.plain }
local by_extension = { txt = languages.plain, text = languages.plain, log = languages.plain }
local by_name = {}

-- The reader of the language named `name` (any of its names): for the code in a Markdown fence and
-- the scripts and styles of a page of HTML. Nil for one the page does not know.
local function reader_of(name)
	local lang = by_alias[string.lower(name)]
	return lang and lang.reader or nil
end

local function add(family, langs)
	for _, lang in langs do
		lang.family = lang.family or family
		if not lang.reader.token then
			lang.reader = clike.reader(lang.reader)
		end
		lang.syntax = b.Syntax.define({
			name = lang.name,
			start = lang.reader.start,
			token = lang.reader.token,
			blank = lang.reader.blank,
			styles = STYLES,
		})
		if lang.family ~= "prose" and lang.folds == nil then
			lang.folds = "indent"
		end
		list[#list + 1] = lang
		by_alias[string.lower(lang.name)] = lang
		for _, alias in lang.aliases or {} do
			by_alias[string.lower(alias)] = lang
		end
		for _, ext in lang.extensions or {} do
			by_extension[ext] = by_extension[ext] or lang
		end
		for _, name in lang.names or {} do
			by_name[string.lower(name)] = lang
		end
	end
end

add("C-like", clike.languages())
add("scripting", scripting.languages())
add("markup", markup.languages(reader_of))
add("data", data.languages())
add("style sheets", sheets.languages())
add("tools", tools.languages())

table.sort(list, function(a, c)
	if a == languages.plain or c == languages.plain then
		return a == languages.plain and c ~= languages.plain
	end
	return string.lower(a.name) < string.lower(c.name)
end)

-- The language named `name` (or one of its other names), any case; nil for none.
function languages.find(name)
	return by_alias[string.lower(name)]
end

-- Every language, Plain Text first and the rest by name.
function languages.all()
	return list
end

-- Names that say a language without an extension of their own.
local NAME_PATTERNS = {
	{ "^%.env", "INI" },
	{ "^dockerfile[%.%-]", "Dockerfile" },
	{ "%.dockerfile$", "Dockerfile" },
	{ "^containerfile[%.%-]", "Dockerfile" },
	{ "^makefile[%.%-]", "Makefile" },
	{ "^[tj]sconfig.*%.json$", "JSON" },
	{ "^%.?eslintrc", "JSON" },
	{ "^%.?prettierrc", "JSON" },
	{ "^%.bash", "Shell" },
	{ "^%.zsh", "Shell" },
	{ "^%.git", "INI" },
}

-- What a first line says: a script's interpreter, an XML or HTML document, a page of PHP.
local function by_first_line(line)
	local interpreter = string.match(line, "^#!%s*%S*/env%s+[%-%w=%s]-([%w%.%-_]+)%s*$")
		or string.match(line, "^#!%s*%S*/env%s+([%w%.%-_]+)")
		or string.match(line, "^#!%s*%S-([%w%.%-_]+)%s*$")
		or string.match(line, "^#!%s*%S-([%w%.%-_]+)%s")
	if interpreter then
		local name = string.lower(interpreter)
		if string.match(name, "^node") or name == "deno" or name == "bun" then
			return languages.find("JavaScript")
		elseif string.match(name, "^python") then
			return languages.find("Python")
		elseif string.match(name, "^ruby") then
			return languages.find("Ruby")
		elseif string.match(name, "^lua") or name == "luau" then
			return languages.find("Lua")
		elseif name == "php" then
			return languages.find("PHP")
		elseif name == "bash" or name == "sh" or name == "zsh" or name == "ksh" or name == "dash" or name == "fish" then
			return languages.find("Shell")
		end
	end
	for _, lang in list do
		if lang.first_line and string.find(line, lang.first_line) then
			return lang
		end
	end
	return nil
end

-- The language of a file, by its name and its first line; plain text for one the page does not
-- know.
function languages.detect(path, first_line)
	local name = string.lower(string.match(path, "([^/]*)$") or path)
	if by_name[name] then
		return by_name[name]
	end
	for _, rule in NAME_PATTERNS do
		if string.find(name, rule[1]) then
			return languages.find(rule[2])
		end
	end
	local ext = string.match(name, "^.+%.([^%.]+)$")
	local found = ext and by_extension[ext]
	if found then
		return found
	end
	return by_first_line(first_line or "") or languages.plain
end

return languages
