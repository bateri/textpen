-- The files that change hands: ones the person gives the page (dropped on the tree or on the editor,
-- pasted into a Markdown file, chosen in the Open panel), which the program receives into a folder
-- of the project, and one of the project's that the person saves where they choose in the Save panel.
--
-- A file changes hands only by the person's move, the person sees at every moment what goes where,
-- and can give it up; and no file is written over another unasked. The page holds the person's files
-- by handle (it never sees their bytes) and passes the handle on to the program (`upload`,
-- `download`); an upload is a row of the tree that is faint and says how far it has got, until the
-- file is whole and the tree lists it.

local kit = require("../lib/kit")
local text = require("../lib/text")
local docs = require("./docs")
local pictures = require("./pictures")

local b, S, act = kit.b, kit.S, kit.act
local layout = b.layout

local uploads = {}

-- How long an upload may go without a byte before its row says so, seconds. The pauses measured in
-- a terminal so far are 4.4 s at the longest: a few seconds' pause only keeps the percent where it is.
uploads.STALL = 30

local made = 0

-- --- The rows ----------------------------------------------------------------------------------------

-- Whether the row is still in the list: it can be given up while the program is at it.
local function alive(row)
	return text.index_of(S.uploads, row) ~= nil
end

local function remove(row)
	S.uploads = text.without(S.uploads, row)
	if row.timer then
		row.timer:cancel()
		row.timer = nil
	end
end

-- The upload that will make the file at `path`, if one is under way.
function uploads.of(path)
	for _, row in S.uploads do
		if row.path == path then
			return row
		end
	end
	return nil
end

-- The uploads into `dir`, in the order they began.
function uploads.in_dir(dir)
	local out = {}
	for _, row in S.uploads do
		if row.dir == dir then
			out[#out + 1] = row
		end
	end
	return out
end

-- How far the file has got, 0 to 100.
function uploads.percent(row)
	if row.size <= 0 then
		return row.state == "done" and 100 or 0
	end
	return math.clamp(math.floor(row.done * 100 / row.size), 0, 100)
end

-- What a screen reader says of the row.
function uploads.spoken(row)
	if row.state == "stalled" then
		return "Upload stalled"
	end
	return "Uploading, " .. uploads.percent(row) .. " percent"
end

-- The row's end, in words: how far it has got, or why it does not go on.
function uploads.words(row)
	if row.state == "stalled" then
		return "Upload stalled"
	end
	return uploads.percent(row) .. "%"
end

-- The pictures a Markdown file names that this upload is about to make: for the preview to tell.
local generation = 0
function uploads.generation()
	return generation
end

-- The upload under way for the file a Markdown file `tab` names as `src`, if there is one.
function uploads.pending(tab, src)
	if #S.uploads == 0 or string.find(src, "^[%a][%w+.%-]*:") ~= nil then
		return nil
	end
	local name = string.match(src, "^[^?#]*") or src
	name = string.gsub(name, "%%(%x%x)", function(hex)
		return string.char(tonumber(hex, 16))
	end)
	local dir = text.dirname(tab.path)
	local parts = string.split(dir == "" and name or (dir .. "/" .. name), "/")
	local kept = {}
	for _, part in parts do
		if part == ".." then
			table.remove(kept)
		elseif part ~= "." and part ~= "" then
			kept[#kept + 1] = part
		end
	end
	local row = uploads.of(table.concat(kept, "/"))
	if row and row.state ~= "done" then
		return row
	end
	return nil
end

-- What a Markdown preview draws for a picture that is on its way into the project: its place, with
-- its name and how far it has got.
function uploads.placeholder(row)
	return b.div()
		:id("upload-placeholder:" .. row.id)
		:row()
		:items_center()
		:gap(8)
		:h(44)
		:px(12)
		:rounded(8)
		:bg("raised1")
		:border(1, "border")
		:child(kit.spinner(14, "quiet"))
		:child(b.text(row.name):color("dim"):truncate():min_w(0))
		:child(b.text(uploads.words(row)):color("quiet"):text_size(11):no_shrink())
end

-- The folder is shown in the tree, open as far down as it is.
local function show(dir)
	for _, each in text.ancestors(dir .. "/_") do
		act.set_expanded(each, true)
	end
end

-- Begins the row's upload: the program receives the file, and tells how far it has got.
local arm
local function go(row)
	row.state = "going"
	row.done = 0
	arm(row)
	if row.open then
		-- The one picture the person gave is shown, from the file they gave while it is on its way.
		row.open = nil
		act.open_file(row.path, { focus = false })
	end
	row.call = kit.call("upload", {
		dir = row.dir,
		name = row.name,
		handle = row.handle,
		size = row.size,
		replace = row.replace or nil,
	}, function(result, err)
		if not alive(row) then
			return nil
		end
		row.call = nil
		return act.upload_over(row, result, err)
	end, function(report)
		if not alive(row) then
			return nil
		end
		row.done = report.done or row.done
		if row.state == "stalled" then
			row.state = "going"
		end
		if row.md then
			generation += 1
		end
		arm(row)
		return true
	end)
end

-- The row says it has stalled when it hears nothing for a while; each word from the program starts
-- the wait again.
function arm(row)
	if row.timer then
		row.timer:cancel()
	end
	row.timer = kit.after(uploads.STALL, function()
		row.timer = nil
		if alive(row) and row.state == "going" then
			row.state = "stalled"
			generation += 1
		end
		return true
	end)
end

-- The preview of a picture tab that was waiting for the file is over: the file is the program's.
local function settle_tab(row)
	local tab = docs.tab_of(row.path)
	if tab and tab.upload == row then
		if row.failed then
			tab.upload = nil
			act.close_tab(row.path)
		else
			pictures.open(tab, function()
				tab.status = "loading"
				tab.plain = true
				act.open_file(row.path, { focus = false })
			end, { keep = true })
		end
	end
end

-- The program answered: the file is whole, or why it is not.
function act.upload_over(row, result, err)
	if row.timer then
		row.timer:cancel()
		row.timer = nil
	end
	if err and err.code == "exists" then
		-- Something came to be there that the listing did not know of, or while the file was on its
		-- way: nothing was written over it; the person is asked, as for any file that is there.
		row.state = "asking"
		uploads.check(row)
		return true
	end
	if err then
		row.failed = true
		remove(row)
		settle_tab(row)
		kit.toast(err.message or ("Could not upload " .. row.name), "bad", 6)
		generation += 1
		return true
	end
	row.state = "done"
	row.path = result.path or row.path
	row.done = row.size
	generation += 1
	-- The row stays as the file until the tree lists it.
	act.list_dir(row.dir, function()
		remove(row)
		pictures.changed()
		generation += 1
		kit.gather(true)
	end)
	settle_tab(row)
	return true
end

-- --- Asking where the name is taken --------------------------------------------------------------------

local asking = {}
local pumping = false

local function other_names(row)
	local names = {}
	for _, each in S.uploads do
		if each ~= row and each.dir == row.dir and each.state ~= "asking" then
			names[#names + 1] = each.name
		end
	end
	return names
end

local function pump()
	if pumping then
		return
	end
	local item = table.remove(asking, 1)
	if not item then
		return
	end
	pumping = true
	local row, place = item.row, item.place
	if not alive(row) then
		pumping = false
		return pump()
	end
	-- The buttons say it all, and what Keep Both would name the file is in its tip.
	act.ask({
		title = "Replace " .. place.name .. "?",
		actions = {
			{ id = "replace", label = "Replace", danger = true },
			{ id = "keep", label = "Keep Both", primary = true, tip = "Uploads this one as " .. place.free },
		},
	}, function(chosen)
		pumping = false
		if not alive(row) then
			-- Given up meanwhile.
		elseif chosen == "replace" then
			row.name, row.path, row.replace = place.name, text.join(row.dir, place.name), true
			go(row)
		elseif chosen == "keep" then
			row.name, row.path, row.replace = place.free, text.join(row.dir, place.free), false
			go(row)
		else
			-- Esc: the file is not uploaded.
			remove(row)
		end
		kit.gather(true)
		pump()
	end)
end

-- Finds out whether the row's name is taken in its folder, and goes on, or asks.
function uploads.check(row)
	local also = other_names(row)
	-- An empty table is no list to JSON: none is left out.
	kit.call("place", { dir = row.dir, name = row.name, also = #also > 0 and also or nil }, function(place, err)
		if not alive(row) then
			return nil
		end
		if err then
			remove(row)
			kit.toast(err.message or ("Could not upload " .. row.name), "bad", 6)
			return true
		end
		if not place.exists then
			row.name, row.path = place.name, text.join(row.dir, place.name)
			go(row)
		else
			table.insert(asking, { row = row, place = place })
			pump()
		end
		return true
	end)
end

-- --- Beginning ---------------------------------------------------------------------------------------------

-- A row for the person's file `file` (as the page was told of it) to be named `name` in `dir`; its
-- name is known to be free where `checked`.
local function begin(file, dir, name, checked)
	made += 1
	local row = {
		id = made,
		dir = dir,
		name = name,
		path = text.join(dir, name),
		handle = file.handle,
		size = file.size or 0,
		kind = file.type,
		done = 0,
		state = "asking",
		image = pictures.named(name),
	}
	S.uploads[#S.uploads + 1] = row
	show(dir)
	if checked then
		go(row)
	else
		uploads.check(row)
	end
	return row
end

-- The person's files go into `dir`. Folders are not uploaded (yet): the person is told, once.
function uploads.give(files, dir)
	local folders = 0
	local sent = {}
	for _, file in files do
		if file.kind == "folder" or not file.handle then
			folders += 1
		else
			sent[#sent + 1] = begin(file, dir, file.name)
		end
	end
	if folders > 0 then
		kit.toast("Folders can't be uploaded yet", "warn")
	end
	-- One picture alone is shown, as the file the person gave.
	if #sent == 1 and #files == 1 and sent[1].image then
		sent[1].open = true
	end
	kit.gather(true)
end

-- --- Giving up, and trying again -----------------------------------------------------------------------

function uploads.cancel(row)
	if not alive(row) then
		return
	end
	if row.call then
		b.cancel(row.call)
		row.call = nil
	end
	row.failed = true
	remove(row)
	settle_tab(row)
	generation += 1
	kit.gather(true)
end

-- Tries again a file that stalled: the terminal is asked for its bytes from the start (a file the
-- person gave can be read again), and the old call is given up.
function uploads.retry(row)
	if not alive(row) then
		return
	end
	if row.call then
		b.cancel(row.call)
		row.call = nil
	end
	go(row)
	kit.gather(true)
end

-- A press on one of the row's own buttons: whether it was.
function uploads.press(id)
	local kind, number = string.match(id or "", "^upload%-(%a+):(%d+)$")
	if not kind then
		return false
	end
	for _, row in S.uploads do
		if row.id == tonumber(number) then
			if kind == "cancel" then
				uploads.cancel(row)
			elseif kind == "retry" then
				uploads.retry(row)
			end
			return true
		end
	end
	return true
end

-- --- Where files dragged over the page would go ------------------------------------------------------------

-- Where files put down at `x`, `y` go: { kind = "tree" | "editor", dir = }. A row is its folder (or a
-- file's), the room under the rows is the top of the project, and anywhere else is the editor's.
function uploads.target_at(x, y)
	local hit = layout.hit({ x = x, y = y })
	local path = hit and string.match(hit, "^row:(.*)$")
	if path then
		local entry = act.tree_entry(path)
		if entry and entry.kind == "dir" then
			return { kind = "tree", dir = path }
		end
		return { kind = "tree", dir = text.dirname(path) }
	end
	local number = hit and string.match(hit, "^upload%-%a+:(%d+)$")
	if number then
		for _, row in S.uploads do
			if row.id == tonumber(number) then
				return { kind = "tree", dir = row.dir }
			end
		end
	end
	local side = S.sidebar == "files" and layout.bounds("sidebar")
	if side and x >= side.x and x < side.x + side.w and y >= side.y and y < side.y + side.h then
		return { kind = "tree", dir = "" }
	end
	local active = S.active
	if active and string.sub(active, 1, 1) ~= "/" then
		return { kind = "editor", dir = text.dirname(active) }
	end
	return { kind = "editor", dir = "" }
end

-- Files are over the page: the place they would go to is marked, and named. Over an editor that is
-- the folder of the file shown, wherever the program runs.
function uploads.over(event)
	local target = uploads.target_at(event.x, event.y)
	local before = S.drop
	if target then
		target.count = event.count
	end
	S.drop = target
	return (before == nil) ~= (target == nil)
		or (before and target and (before.kind ~= target.kind or before.dir ~= target.dir)) or false
end

function uploads.left()
	local had = S.drop ~= nil
	S.drop = nil
	return had
end

-- The place named: "docs/", or the project's own name for the top.
local function folder_name(dir)
	if dir == "" then
		return (S.boot and S.boot.rootName or "") .. "/"
	end
	return dir .. "/"
end

-- The ring around the place the files would go to, and its name.
function uploads.mark(kind)
	local drop = S.drop
	if not (drop and drop.kind == kind) then
		return nil
	end
	local chip = b.div()
		:row()
		:items_center()
		:h(28)
		:px(12)
		:rounded(8)
		:bg("raised3")
		:border(1, "accent")
		:shadow(0, 8, 22, b.alpha("shadow", 0.35))
		:child(b.text("Upload to " .. folder_name(drop.dir)):color("foreground"):weight(600):truncate():min_w(0))
	local mark = b.div()
		:id("drop-mark-" .. kind)
		:absolute()
		:inset(0, 0, 0, 0)
		:col()
		:items_center()
		:justify_end()
		:pb(14)
		:px(12)
		:child(chip)
	if kind == "editor" or drop.dir == "" then
		-- The whole place is where they would go; a folder is marked by its own row.
		mark:border(2, "accent"):bg(b.alpha("accent", 0.08))
	end
	return mark
end

-- Whether `path` (a folder) is the place the files would go to: its row is marked.
function uploads.dropping(path)
	return S.drop ~= nil and S.drop.kind == "tree" and S.drop.dir == path and path ~= ""
end

-- --- What the person gave -------------------------------------------------------------------------------------

local IMAGE_TYPES = { png = "png", jpeg = "jpg", jpg = "jpg", gif = "gif", webp = "webp", svg = "svg" }

-- The name a pasted picture is given: "image" and the extension its name or its type says.
local function pasted_name(file)
	local extension = text.extension(file.name or "")
	if extension == "" then
		extension = IMAGE_TYPES[string.match(file.type or "", "^image/([%w]+)") or ""] or "png"
	end
	return "image." .. extension
end

-- Pasted pictures into a Markdown file being edited: each goes beside the file as `image.png` (the
-- next free `image-1.png`, `image-2.png`), and `![](image.png)` is written where the caret is.
local function paste(event)
	local tab = docs.tab_of(S.active)
	local doc = docs.active()
	if S.focus ~= "editor" or not (tab and doc and act.is_markdown(tab.path)) or string.sub(tab.path, 1, 1) == "/" then
		return
	end
	local dir = text.dirname(tab.path)
	local files = {}
	for _, file in event.files or {} do
		if file.handle and file.kind ~= "folder" and string.match(file.type or "", "^image/") then
			files[#files + 1] = file
		end
	end
	local taken = {}
	local function next_one(index)
		local file = files[index]
		if not file then
			return
		end
		local also = {}
		for _, name in taken do
			also[#also + 1] = name
		end
		for _, each in S.uploads do
			if each.dir == dir then
				also[#also + 1] = each.name
			end
		end
		-- An empty table is no list to JSON: none is left out.
		kit.call("place", { dir = dir, name = pasted_name(file), also = #also > 0 and also or nil, dash = true }, function(place, err)
			if err then
				kit.toast(err.message or "Could not paste the picture", "bad", 6)
				return true
			end
			local name = place.exists and place.free or place.name
			taken[#taken + 1] = name
			-- The name is free, so that nothing is asked.
			local row = begin(file, dir, name, true)
			row.md = tab.path
			local words = "![](" .. string.gsub(name, " ", "%%20") .. ")"
			if index > 1 then
				words = "\n" .. words
			end
			kit.ask(doc.id, { command = "insert", text = words })
			next_one(index + 1)
			return true
		end)
	end
	next_one(1)
end

-- Files were given to the page: dropped on it, pasted, or chosen in the Open panel.
function uploads.files(event)
	S.drop = nil
	if event.files == nil then
		-- A terminal that gives no handles: the files are opened where they are, as they were.
		for _, path in event.paths or {} do
			act.open_file(path, { focus = true })
		end
		return
	end
	if event.source == "paste" then
		paste(event)
		return
	end
	if event.source == "pick" then
		local dir = S.picking
		S.picking = nil
		if dir and #event.files > 0 then
			uploads.give(event.files, dir)
		end
		return
	end
	if event.source ~= "drop" then
		return
	end
	-- On the tree into the folder under the pointer, on an editor beside the file shown: uploaded
	-- the same wherever the program runs, by the handles the terminal made.
	uploads.give(event.files, uploads.target_at(event.x, event.y).dir)
end

-- --- The panels -----------------------------------------------------------------------------------------

-- The Open panel, for files to go into the folder `dir`. Asked for in the reply to a command, a key
-- or a press, which the terminal counts as the person's move.
function uploads.choose(dir)
	S.picking = dir
	kit.gather({ pick = { multiple = true } })
end

-- The Save panel, for the project's file `path`, proposing its name.
function uploads.save_as(path)
	S.saving = path
	kit.gather({ save = { name = text.basename(path) } })
end

-- The person chose a place in the Save panel (or turned it away): the file is written there.
function uploads.saving(event)
	local path = S.saving
	S.saving = nil
	if not (event.handle and path) then
		return
	end
	local name = event.name or text.basename(path)
	S.busy += 1
	kit.call("download", { path = path, handle = event.handle }, function(_, err)
		S.busy -= 1
		if err then
			kit.toast(err.message or ("Could not download " .. name), "bad", 6)
		else
			kit.toast("Downloaded " .. name, "ok")
		end
		return true
	end)
end

-- The folder a command uploads into: the folder chosen in the tree.
function uploads.chosen_dir()
	local entry = S.tree_selection and act.tree_entry(S.tree_selection)
	if entry and entry.kind == "dir" then
		return entry.path
	end
	return nil
end

-- The file a command downloads: the file chosen in the tree where the keys are there, else the one
-- shown; one that is in the project and on its disk.
function uploads.chosen_file()
	local path = S.focus == "sidebar" and S.tree_selection or S.active
	if not path or string.sub(path, 1, 1) == "/" then
		return nil
	end
	local entry = act.tree_entry(path)
	if entry and entry.kind == "file" and not entry.upload then
		return path
	end
	if not entry and docs.tab_of(path) then
		return path
	end
	return nil
end

return uploads
