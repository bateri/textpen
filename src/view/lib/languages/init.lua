-- The languages the editor reads files in: how each is told from a file's name or first line, what
-- colours it (a language of the kit's, `pencere.Syntax`) and what the editor is told of it (its
-- comments, the rules of its indent, folding, the brackets that close themselves).

local languages = {}

-- A file in no language the page knows: prose, as the editor is with nothing said.
languages.plain = { name = "Plain Text", pairs = "()[]{}" }

local list = { languages.plain }
local by_name = { [string.lower(languages.plain.name)] = languages.plain }

-- The language named `name` (or one of its other names), any case; nil for none.
function languages.find(name)
	return by_name[string.lower(name)]
end

-- Every language, by name.
function languages.all()
	return list
end

-- The language of a file, by its name and its first line; plain text for one the page does not
-- know.
function languages.detect(path, first_line)
	return languages.plain
end

return languages
