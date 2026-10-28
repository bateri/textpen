-- The languages of tags and of marked-up prose: HTML (its scripts and styles read in their own
-- languages), XML, and Markdown (the code in its fences read in the fence's language).

local clike = require("./clike")

local set, literal = clike.set, clike.literal

local markup = {}

local VOID = set("area base br col embed hr img input link meta param source track wbr")

-- --- HTML and XML ----------------------------------------------------------------------------------

-- A reader of tags; `embed(name)` is the reader of what is inside `<script>` and `<style>` (HTML).
local function tags(embed)
	local function start()
		return { mode = "text", tag = nil, closing = false, quote = nil, inner = nil, inner_state = nil }
	end
	local function token(stream, state)
		if state.mode == "comment" then
			if stream:skip_to("-->") then
				stream:match("%-%->")
				state.mode = "text"
			else
				stream:skip_to_end()
			end
			return "comment"
		elseif state.mode == "cdata" then
			if stream:skip_to("]]>") then
				stream:match("%]%]>")
				state.mode = "text"
			else
				stream:skip_to_end()
			end
			return "string"
		elseif state.mode == "value" then
			local close = literal(state.quote)
			while not stream:eol() do
				if stream:match(close) then
					state.mode = "tag"
					return "string"
				end
				stream:next()
			end
			return "string"
		elseif state.mode == "embedded" then
			if stream:match("^</" .. state.tag, false) then
				state.mode = "text"
				state.inner, state.inner_state = nil, nil
			else
				return state.inner.token(stream, state.inner_state)
			end
		end
		if state.mode == "tag" then
			if stream:eat_space() then
				return nil
			end
			if stream:match("/>") then
				state.mode = "text"
				return "tag"
			elseif stream:match(">") or stream:match("?>") then
				local name = state.tag and string.lower(state.tag) or ""
				state.mode = "text"
				local inner = embed and (name == "script" or name == "style") and embed(name)
				if inner and not state.closing then
					state.mode = "embedded"
					state.inner, state.inner_state = inner, inner.start()
				end
				return "tag"
			elseif stream:match("^[%a_:][%w_:%.%-]*") then
				return "attribute"
			elseif stream:match("=") then
				return "operator"
			elseif stream:match('"') or stream:match("'") then
				state.quote = stream:current()
				state.mode = "value"
				return token(stream, state)
			end
			stream:next()
			return "string"
		end
		if stream:match("<!%-%-") then
			state.mode = "comment"
			return token(stream, state)
		elseif stream:match("<!%[CDATA%[") then
			state.mode = "cdata"
			return token(stream, state)
		elseif stream:match("^<![%a]+") or stream:match("^<%?[%w%-]*") then
			state.mode, state.tag, state.closing = "tag", nil, false
			return "meta"
		end
		local name = stream:match("^</([%a_][%w_:%.%-]*)")
		if name then
			state.mode, state.tag, state.closing = "tag", name, true
			return "tag"
		end
		name = stream:match("^<([%a_][%w_:%.%-]*)")
		if name then
			state.mode, state.tag, state.closing = "tag", name, false
			return "tag"
		end
		if stream:match("^&[#%w]+;") then
			return "atom"
		end
		stream:next()
		while not stream:eol() do
			local c = stream:peek()
			if c == "<" or c == "&" then
				break
			end
			stream:next()
		end
		return nil
	end
	return { start = start, token = token }
end

-- --- Markdown ----------------------------------------------------------------------------------------

local function markdown(lookup)
	local function start()
		return { fence = nil, inner = nil, inner_state = nil }
	end
	-- What is inside a line of prose: code, strong, emphasis, struck words, links, escapes.
	local function inline(stream)
		if stream:match("^\\[%p]") then
			return "escape"
		end
		local ticks = stream:match("^`+")
		if ticks then
			if stream:skip_to(ticks) then
				stream:match(literal(ticks))
			else
				stream:skip_to_end()
			end
			return "code"
		end
		if stream:match("^%*%*[^%*]+%*%*") or stream:match("^__[^_]+__") then
			return "strong"
		end
		if stream:match("^%*[^%*%s][^%*]*%*") or stream:match("^_[^_%s][^_]*_") then
			return "emphasis"
		end
		if stream:match("^~~[^~]+~~") then
			return "strike"
		end
		if stream:match("^!?%[[^%]]*%]") then
			return "link"
		end
		if stream:match("^%([^%)%s]*%)") and stream.start > 1 and string.sub(stream.string, stream.start - 1, stream.start - 1) == "]" then
			return "url"
		end
		if stream:match("^<https?://[^>]+>") or stream:match("^https?://[%w%-%._~:/%?#%[%]@!%$&'%*%+,;=%%]+") then
			return "url"
		end
		if stream:match("^[%w%s]+") then
			return nil
		end
		stream:next()
		return nil
	end
	local function token(stream, state)
		local fence = state.fence
		if fence then
			if stream:sol() and stream:match("^%s*" .. literal(fence) .. "%s*$") then
				state.fence, state.inner, state.inner_state = nil, nil, nil
				return "meta"
			end
			if state.inner then
				return state.inner.token(stream, state.inner_state)
			end
			stream:skip_to_end()
			return "code"
		end
		if stream:sol() then
			local open, info = stream:match("^%s*(```+)%s*([%w_%+%-#%.]*)")
			if not open then
				open, info = stream:match("^%s*(~~~+)%s*([%w_%+%-#%.]*)")
			end
			if open then
				state.fence = open
				local inner = info ~= "" and lookup(info) or nil
				if inner then
					state.inner, state.inner_state = inner, inner.start()
				end
				stream:skip_to_end()
				return "meta"
			end
			if stream:match("^%s*#+%s") or stream:match("^%s*#+$") then
				stream:skip_to_end()
				return "heading"
			end
			if stream:match("^%s*[%-%*_]%s*[%-%*_]%s*[%-%*_][%s%-%*_]*$") then
				return "meta"
			end
			if stream:match("^%s*>") then
				stream:skip_to_end()
				return "quote"
			end
			if stream:match("^%s*[%-%*%+]%s") or stream:match("^%s*%d+[%.%)]%s") then
				if stream:match("^%[[ xX]%]") then
					return "list atom"
				end
				return "list"
			end
			if stream:match("^%s*|") then
				return "punctuation"
			end
		end
		if stream:match("|") then
			return "punctuation"
		end
		return inline(stream)
	end
	return { start = start, token = token, blank = nil }
end

function markup.languages(lookup)
	local function embed(name)
		return lookup(name == "script" and "javascript" or "css")
	end
	return {
		{
			name = "HTML",
			aliases = { "html", "htm", "xhtml" },
			extensions = { "html", "htm", "xhtml", "shtml", "vue", "svelte" },
			first_line = "^%s*<![dD][oO][cC][tT][yY][pP][eE]%s+[hH][tT][mM][lL]",
			reader = tags(embed),
			comment = { block = { "<!--", "-->" } },
			indent = { increase = "<([a-zA-Z][\\w.-]*)(\\s[^<>]*)?>[^<]*$", decrease = "^\\s*</" },
			pairs = "()[]{}\"\"''",
			void = VOID,
		},
		{
			name = "XML",
			aliases = { "xml", "svg", "xsd", "xsl", "plist" },
			extensions = { "xml", "svg", "xsd", "xsl", "xslt", "plist", "csproj", "fsproj", "vcxproj", "props", "targets",
				"xaml", "rss", "atom", "wsdl", "storyboard", "xib" },
			first_line = "^%s*<%?xml",
			reader = tags(nil),
			comment = { block = { "<!--", "-->" } },
			indent = { increase = "<([a-zA-Z][\\w.:-]*)(\\s[^<>]*)?>[^<]*$", decrease = "^\\s*</" },
			pairs = "()[]{}\"\"''",
		},
		{
			name = "Markdown",
			aliases = { "md", "markdown" },
			extensions = { "md", "markdown", "mdx", "mkd", "mdown" },
			reader = markdown(lookup),
			comment = { block = { "<!--", "-->" } },
			pairs = "()[]{}``",
			prose = true,
		},
	}
end

return markup
