-- The looks of code, in the terminal's colours: every language names its tokens from this one table
-- (a dotted name falls back to what is before its last dot), so that a keyword is a keyword in any
-- of them. Code needs about seven kinds told apart; each takes one of the terminal's colours as it
-- is — chosen by the theme's author to read on its ground — and the few in between are mixed from
-- two.

local b = pencere

local comment = b.mix("dim", "quiet", 0.3)
local punctuation = b.mix("foreground", "background", 0.38)

return {
	keyword = { color = "accent" },
	["keyword.modifier"] = { color = "accent", italic = true },
	["variable.special"] = { color = "accent", italic = true },
	string = { color = "success" },
	["string.special"] = { color = b.mix("error", "warning", 0.4) },
	escape = { color = b.mix("error", "warning", 0.4) },
	regexp = { color = b.mix("error", "warning", 0.4) },
	number = { color = "warning" },
	atom = { color = "warning" },
	constant = { color = "warning" },
	type = { color = "info" },
	["function"] = { color = b.mix("accent", "info", 0.6) },
	property = { color = b.mix("foreground", "info", 0.22) },
	variable = { color = "foreground" },
	comment = { color = comment, italic = true },
	punctuation = { color = punctuation },
	operator = { color = punctuation },
	meta = { color = punctuation },
	tag = { color = "error" },
	attribute = { color = "warning" },
	-- Prose and the like.
	heading = { color = "accent", weight = 700 },
	emphasis = { italic = true },
	strong = { weight = 700 },
	strike = { strike = true },
	link = { color = "info", underline = "info" },
	url = { color = "info" },
	quote = { color = comment, italic = true },
	code = { color = "success" },
	list = { color = "warning" },
	-- Diffs.
	inserted = { color = "success" },
	deleted = { color = "error" },
	changed = { color = "warning" },
	invalid = { color = "error", underline = { wavy = "error" } },
}
