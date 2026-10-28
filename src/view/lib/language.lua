-- What textpen decides about the languages of the kit's library (`@pencere/languages`): the one it
-- adds (a file in none is Plain Text) and the names it lists. How a file is told, how each is
-- coloured and what the editor is told of it is the kit's.

local languages = require("@pencere/languages")

local language = {}

-- A file in none of the kit's languages: prose, as the editor is with nothing said, but for the
-- brackets and quotes that close themselves.
language.plain = { name = "Plain Text", config = { pairs = "()[]{}" } }

local PLAIN_NAMES = { ["plain text"] = true, plaintext = true, text = true, txt = true }

-- The language `which` names (a name, or one of its other names, in any case), built; nil for none.
function language.find(name)
	if PLAIN_NAMES[string.lower(name)] then
		return language.plain
	end
	local found = languages.find(name)
	return found and languages.load(found) or nil
end

-- The language of the file at `path` whose first line is `first_line`.
function language.detect(path, first_line)
	local found = languages.detect(path, first_line)
	return found and languages.load(found) or language.plain
end

-- The names of the languages a person can choose, Plain Text first.
function language.names()
	local names = { language.plain.name }
	for _, description in languages.list() do
		names[#names + 1] = description.name
	end
	return names
end

return language
