-- Files as the page shows them: an icon by kind, in one of the terminal's colours, and what git
-- says of them in a letter and a colour.

local text = require("./text")

local files = {}

-- An icon of src/assets/icons and the role it is drawn in.
local function kind(icon, color)
	return { icon = icon, color = color }
end

local BY_NAME = {
	dockerfile = kind("brand-docker", "info"),
	containerfile = kind("brand-docker", "info"),
	["docker-compose.yml"] = kind("brand-docker", "info"),
	["docker-compose.yaml"] = kind("brand-docker", "info"),
	["compose.yaml"] = kind("brand-docker", "info"),
	["compose.yml"] = kind("brand-docker", "info"),
	[".gitignore"] = kind("brand-git", "error"),
	[".gitattributes"] = kind("brand-git", "error"),
	[".gitmodules"] = kind("brand-git", "error"),
	["package.json"] = kind("brand-npm", "error"),
	["package-lock.json"] = kind("lock", "dim"),
	["cargo.lock"] = kind("lock", "dim"),
	["yarn.lock"] = kind("lock", "dim"),
	["pnpm-lock.yaml"] = kind("lock", "dim"),
	["poetry.lock"] = kind("lock", "dim"),
	license = kind("license", "warning"),
	["license.md"] = kind("license", "warning"),
	["license.txt"] = kind("license", "warning"),
	copying = kind("license", "warning"),
	makefile = kind("file-settings", "warning"),
	gnumakefile = kind("file-settings", "warning"),
	[".editorconfig"] = kind("file-settings", "dim"),
	[".env"] = kind("file-settings", "warning"),
}

local BY_EXTENSION = {
	ts = kind("brand-typescript", "info"),
	mts = kind("brand-typescript", "info"),
	cts = kind("brand-typescript", "info"),
	tsx = kind("brand-react", "info"),
	jsx = kind("brand-react", "warning"),
	js = kind("brand-javascript", "warning"),
	mjs = kind("brand-javascript", "warning"),
	cjs = kind("brand-javascript", "warning"),
	json = kind("json", "warning"),
	jsonc = kind("json", "warning"),
	json5 = kind("json", "warning"),
	md = kind("markdown", "accent"),
	mdx = kind("markdown", "accent"),
	markdown = kind("markdown", "accent"),
	py = kind("brand-python", "warning"),
	pyi = kind("brand-python", "warning"),
	rs = kind("brand-rust", "error"),
	go = kind("brand-golang", "info"),
	html = kind("brand-html5", "error"),
	htm = kind("brand-html5", "error"),
	css = kind("brand-css3", "accent"),
	scss = kind("brand-sass", "error"),
	sass = kind("brand-sass", "error"),
	less = kind("brand-css3", "accent"),
	vue = kind("brand-vue", "success"),
	svelte = kind("brand-svelte", "error"),
	swift = kind("brand-swift", "error"),
	kt = kind("brand-kotlin", "accent"),
	kts = kind("brand-kotlin", "accent"),
	java = kind("coffee", "error"),
	c = kind("letter-c", "info"),
	h = kind("letter-c", "accent"),
	cc = kind("brand-cpp", "info"),
	cpp = kind("brand-cpp", "info"),
	cxx = kind("brand-cpp", "info"),
	hpp = kind("brand-cpp", "accent"),
	hh = kind("brand-cpp", "accent"),
	cs = kind("brand-c-sharp", "accent"),
	php = kind("brand-php", "accent"),
	rb = kind("diamond", "error"),
	graphql = kind("brand-graphql", "error"),
	gql = kind("brand-graphql", "error"),
	sh = kind("terminal-2", "success"),
	bash = kind("terminal-2", "success"),
	zsh = kind("terminal-2", "success"),
	fish = kind("terminal-2", "success"),
	lua = kind("file-code", "info"),
	luau = kind("file-code", "info"),
	sql = kind("database", "warning"),
	toml = kind("toml", "dim"),
	yml = kind("file-settings", "error"),
	yaml = kind("file-settings", "error"),
	ini = kind("file-settings", "dim"),
	conf = kind("file-settings", "dim"),
	cfg = kind("file-settings", "dim"),
	properties = kind("file-settings", "dim"),
	env = kind("file-settings", "warning"),
	lock = kind("lock", "dim"),
	csv = kind("table", "success"),
	tsv = kind("table", "success"),
	diff = kind("file-diff", "warning"),
	patch = kind("file-diff", "warning"),
	txt = kind("file-text", "dim"),
	log = kind("file-text", "dim"),
	png = kind("photo", "accent"),
	jpg = kind("photo", "accent"),
	jpeg = kind("photo", "accent"),
	gif = kind("photo", "accent"),
	webp = kind("photo", "accent"),
	ico = kind("photo", "accent"),
	svg = kind("photo", "warning"),
	pdf = kind("file-type-pdf", "error"),
	zip = kind("file-zip", "dim"),
	gz = kind("file-zip", "dim"),
	tar = kind("file-zip", "dim"),
	wasm = kind("binary", "accent"),
	xml = kind("file-code", "warning"),
	zig = kind("file-code", "warning"),
	ex = kind("file-code", "accent"),
	exs = kind("file-code", "accent"),
	hs = kind("file-code", "accent"),
	mk = kind("file-settings", "warning"),
	proto = kind("braces", "info"),
}

local FILE = kind("file", "dim")

-- The icon of a file and its colour, by its name.
function files.kind(path)
	local name = string.lower(text.basename(path))
	return BY_NAME[name]
		or (text.starts(name, ".env") and BY_NAME[".env"])
		or (text.starts(name, "dockerfile") and BY_NAME.dockerfile)
		or BY_EXTENSION[text.extension(path)]
		or FILE
end

-- What git says of a file, in a letter, and the colour it is said in.
files.GIT_LETTER = {
	modified = "M",
	added = "A",
	untracked = "U",
	deleted = "D",
	renamed = "R",
	conflict = "!",
}

files.GIT_COLOR = {
	modified = "warning",
	renamed = "warning",
	added = "success",
	untracked = "success",
	deleted = "error",
	conflict = "error",
}

-- Which of two states a directory shows, of the files in it: the one that needs the eye most.
files.GIT_RANK = { conflict = 5, modified = 4, deleted = 3, renamed = 2, added = 1, untracked = 1 }

return files
