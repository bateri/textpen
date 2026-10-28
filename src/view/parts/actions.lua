-- What the page does: start from what the program says, open, show, save and close files, and stay
-- in step with the program. Each function is what one command or one click asks for; the commands,
-- the keys and the pointer all come here (through `kit.act`, where the other parts find them).

local kit = require("../lib/kit")
local text = require("../lib/text")
local docs = require("./docs")

local b, S, act = kit.b, kit.S, kit.act
local json = b.json

local actions = {}

local RECENT_KEPT = 50

-- --- The keyboard --------------------------------------------------------------------------------

-- The keyboard goes to the file shown, or to the page's own keys where none is.
function act.focus_editor()
	S.focus = "editor"
	S.drawer = false
	local doc = docs.active()
	if doc then
		S.field = doc.id
		kit.keyboard({ field = doc.id })
	else
		S.field = nil
		kit.keyboard("page")
	end
end

-- Where the keyboard goes back to when what was over the page goes.
function act.regain()
	if S.overlay ~= nil or S.handing_off ~= nil then
		return
	end
	if S.focus == "sidebar" then
		act.focus_sidebar(S.sidebar)
	else
		act.focus_editor()
	end
end

-- --- Tabs ------------------------------------------------------------------------------------------

local function remember(path)
	S.recent = text.without(S.recent, path)
	table.insert(S.recent, 1, path)
	while #S.recent > RECENT_KEPT do
		table.remove(S.recent)
	end
end

-- Shows a file: its tab (made after the one shown where it has none), its text (read where it is
-- not yet), the caret where `options` says (`line`, `column`, `select` = { line =, from =, to = }
-- in characters of the line), and the keyboard in it where `options.focus`.
function act.open_file(path, options)
	options = options or {}
	if S.layout == "narrow" and S.drawer then
		options.focus = true
	end
	local tab = docs.tab_of(path)
	if not tab then
		tab = { path = path, status = "loading" }
		local at = text.index_of(S.tabs, docs.tab_of(S.active)) or #S.tabs
		table.insert(S.tabs, at + 1, tab)
	end
	local before = docs.tab_of(S.active)
	if before and before ~= tab and S.settings.autoSave then
		act.save_quietly(before)
	end
	S.active = path
	remember(path)
	if S.focus ~= "sidebar" and string.sub(path, 1, 1) ~= "/" then
		act.reveal(path)
	end
	if S.layout == "narrow" then
		S.drawer = false
	end
	if options.focus then
		S.focus = "editor"
	end
	local wanted = options.line ~= nil or options.select ~= nil
	local arrive = nil
	if wanted then
		arrive = { line = options.line, column = options.column, select = options.select }
	end
	local doc = tab.doc
	if not doc and tab.status ~= "unopenable" then
		if not arrive and S.places[path] then
			arrive = { place = S.places[path] }
		end
		local size = act.size_of(path)
		docs.open(tab, arrive, size)
	elseif doc and arrive then
		if doc.loaded then
			doc.arrive = arrive
			-- As if it had just come in: the caret goes there now.
			docs.arrive_now(doc)
		else
			doc.arrive = arrive
		end
	end
	if S.focus == "editor" and S.overlay == nil then
		act.focus_editor()
	end
	kit.gather(true)
end

-- The tab of `path` in front; the keyboard in it where `focus`.
function act.activate(path, focus)
	if not docs.tab_of(path) then
		return
	end
	if S.active == path then
		if focus ~= false then
			act.focus_editor()
		end
		return
	end
	act.open_file(path, { focus = focus ~= false })
end

-- The tab `step` places from the one in front, around the ends.
function act.cycle_tab(step)
	local count = #S.tabs
	if count == 0 then
		return
	end
	local at = text.index_of(S.tabs, docs.tab_of(S.active)) or 1
	act.activate(S.tabs[(at - 1 + step) % count + 1].path)
end

function act.move_tab(step)
	local at = text.index_of(S.tabs, docs.tab_of(S.active))
	if not at then
		return
	end
	local to = at + step
	if to < 1 or to > #S.tabs then
		return
	end
	local tab = table.remove(S.tabs, at)
	table.insert(S.tabs, to, tab)
	kit.gather(true)
end

-- Puts the tab of `path` before (or after, `after`) the tab of `target`: a tab dragged.
function act.drop_tab(path, target, after)
	local tab = docs.tab_of(path)
	local onto = docs.tab_of(target)
	if not tab or not onto or tab == onto then
		return
	end
	table.remove(S.tabs, text.index_of(S.tabs, tab))
	local at = text.index_of(S.tabs, onto)
	table.insert(S.tabs, after and at + 1 or at, tab)
	kit.gather(true)
end

local recently_closed = {}

-- The tab is gone: the one seen last before it comes to the front.
local function drop_tab(tab)
	local at = text.index_of(S.tabs, tab)
	if at then
		table.remove(S.tabs, at)
	end
	S.places[tab.path] = nil
	S.preview = text.without(S.preview, tab.path)
	if S.active == tab.path then
		local next_path = nil
		for _, path in S.recent do
			if path ~= tab.path and docs.tab_of(path) then
				next_path = path
				break
			end
		end
		if not next_path and S.tabs[1] then
			next_path = S.tabs[1].path
		end
		S.active = nil
		if next_path then
			act.open_file(next_path, { focus = S.focus == "editor" })
		else
			act.focus_editor()
		end
	end
	-- Opened on files, as $EDITOR is: the last one closed is the end of the edit.
	if #S.tabs == 0 and S.boot and S.boot.closeQuits then
		act.end_view(false)
	end
	kit.gather(true)
end

-- Closes a tab, asking first when its file has changes that are not saved. `done(closed)`.
function act.close_tab(path, done)
	local tab = docs.tab_of(path)
	local function finish(closed)
		if closed then
			recently_closed = text.without(recently_closed, path)
			table.insert(recently_closed, 1, path)
		end
		if done then
			done(closed)
		end
	end
	if not tab then
		finish(true)
		return
	end
	local function close(discard)
		docs.close(tab, discard, function(ok)
			if ok then
				drop_tab(tab)
			end
			finish(ok)
		end)
	end
	local function decide()
		if not (tab.info and tab.info.dirty) then
			close(false)
			return
		end
		act.ask({
			title = "Save the changes to " .. text.basename(path) .. "?",
			message = "They are lost if the file closes without saving.",
			actions = {
				{ id = "save", label = "Save", primary = true },
				{ id = "discard", label = "Don’t Save", danger = true },
				{ id = "cancel", label = "Cancel" },
			},
		}, function(chosen)
			if chosen == "save" then
				docs.save(tab, false, function(saved)
					if saved then
						close(false)
					else
						finish(false)
					end
				end)
			elseif chosen == "discard" then
				close(true)
			else
				finish(false)
			end
		end)
	end
	if tab.doc and tab.doc.loaded then
		docs.flush(tab.doc, decide)
	else
		decide()
	end
end

-- Closes every tab but `keep` (nil: all), one after another, stopping where one is not closed.
function act.close_others(keep)
	local paths = {}
	for _, tab in S.tabs do
		if tab.path ~= keep then
			paths[#paths + 1] = tab.path
		end
	end
	local function next_one(index)
		if index > #paths then
			return
		end
		act.close_tab(paths[index], function(closed)
			if closed then
				next_one(index + 1)
			end
		end)
	end
	next_one(1)
end

function act.reopen_closed()
	local path = table.remove(recently_closed, 1)
	if path then
		act.open_file(path, { focus = true })
	end
end

function act.can_reopen()
	return #recently_closed > 0
end

-- --- Saving ------------------------------------------------------------------------------------------

function act.save(path, done)
	local tab = docs.tab_of(path or S.active)
	if not tab then
		if done then
			done(false)
		end
		return
	end
	docs.save(tab, false, done)
end

-- Saves where something is not saved, saying nothing where all is well (another tab, the shell).
function act.save_quietly(tab)
	if tab.info and tab.info.dirty and tab.doc and tab.doc.loaded then
		docs.save(tab, false)
	end
end

-- Saves every file with changes; `done(all)`.
function act.save_all(done)
	docs.flush_all(function()
		local dirty = {}
		for _, tab in S.tabs do
			if tab.info and tab.info.dirty then
				dirty[#dirty + 1] = tab
			end
		end
		local all = true
		local function next_one(index)
			if index > #dirty then
				if done then
					done(all)
				end
				return
			end
			docs.save(dirty[index], false, function(saved)
				all = all and saved
				next_one(index + 1)
			end)
		end
		next_one(1)
	end)
end

-- Reads the file again from disk; asks first when that drops changes.
function act.revert(path, confirm)
	local tab = docs.tab_of(path or S.active)
	if not (tab and tab.doc) then
		return
	end
	local function revert()
		docs.buffer(tab, "revert")
	end
	if confirm ~= false and tab.info and tab.info.dirty then
		act.ask({
			title = "Drop the changes to " .. text.basename(tab.path) .. "?",
			message = "The file is read again from disk, as it was last saved.",
			actions = {
				{ id = "revert", label = "Drop Changes", danger = true },
				{ id = "cancel", label = "Cancel", primary = true },
			},
		}, function(chosen)
			if chosen == "revert" then
				revert()
			end
		end)
	else
		revert()
	end
end

-- Keeps the buffer's changes over a file that changed on disk.
function act.keep_mine(path)
	local tab = docs.tab_of(path)
	if tab then
		docs.buffer(tab, "keep")
	end
end

-- Takes or drops the draft an earlier textpen left of this file.
function act.draft(path, take)
	local tab = docs.tab_of(path)
	if not tab then
		return
	end
	docs.buffer(tab, take and "restore" or "drop", function(ok)
		if ok and take then
			kit.toast("The unsaved changes to " .. text.basename(path) .. " are back", "ok")
		end
	end)
end

function act.set_eol(path, eol)
	local tab = docs.tab_of(path)
	if tab then
		docs.buffer(tab, eol)
	end
end

-- --- Small things ----------------------------------------------------------------------------------------

local MARKDOWN = { md = true, markdown = true, mdx = true }

function act.is_markdown(path)
	return MARKDOWN[text.extension(path)] == true
end

function act.toggle_preview(path)
	path = path or S.active
	if not path then
		return
	end
	if text.index_of(S.preview, path) then
		S.preview = text.without(S.preview, path)
	else
		S.preview[#S.preview + 1] = path
	end
	kit.gather(true)
end

-- The path on the pasteboard: as the project names it, or from the root of the machine.
function act.copy_path(path, absolute)
	local root = S.boot and S.boot.root or ""
	local words = (absolute and string.sub(path, 1, 1) ~= "/") and text.join(root, path) or path
	kit.gather({ copy = words })
	kit.toast("Copied " .. words, "info", 2)
end

-- A menu at the pointer for the host to show: `items` are { title, keys, run } (`keys` written as a
-- command's are: "ctrl+w", "f2", "shift+a") and "-" for a line; the one picked runs (`act.menu_pick`).
local menu_items = nil

function act.menu(items, at)
	menu_items = items
	local entries = {}
	for _, item in items do
		if item == "-" then
			entries[#entries + 1] = "-"
		else
			entries[#entries + 1] = { title = item[1], keys = item[2], enabled = item[4] ~= false }
		end
	end
	kit.gather({ menu = { at = { x = at.x, y = at.y }, entries = entries } })
end

function act.menu_pick(index)
	local item = menu_items and menu_items[index]
	menu_items = nil
	if type(item) == "table" then
		item[3]()
	end
end

-- --- Settings -----------------------------------------------------------------------------------------

local settings_timer = nil

-- Changes the settings at once, and tells the program once the person stops changing them.
function act.change_settings(change)
	local settings = table.clone(S.settings)
	for key, value in change do
		settings[key] = value
	end
	S.settings = settings
	if settings_timer then
		settings_timer:cancel()
	end
	settings_timer = kit.after(0.4, function()
		settings_timer = nil
		kit.call("settings", { settings = S.settings }, function(answer, err)
			if err then
				kit.toast("The settings were not kept: " .. err.message, "warn")
			end
			return nil
		end)
		return nil
	end)
	kit.gather(true)
end

-- --- Staying in step with the program --------------------------------------------------------------------

-- The view's state as the program keeps it for the next view and the next run (`UiState`).
local function ui_state()
	local tabs = setmetatable({}, json.array)
	for _, tab in S.tabs do
		tabs[#tabs + 1] = tab.path
	end
	for _, tab in S.tabs do
		local doc = tab.doc
		if doc and doc.loaded then
			S.places[tab.path] = docs.place(doc)
		end
	end
	local places = {}
	for path, place in S.places do
		if docs.tab_of(path) then
			places[path] = place
		end
	end
	local function list(values)
		local out = setmetatable({}, json.array)
		for _, value in values do
			out[#out + 1] = value
		end
		return out
	end
	return {
		tabs = tabs,
		active = S.active or json.null,
		expanded = list(S.expanded),
		places = places,
		recent = list(S.recent),
		preview = list(S.preview),
	}
end

local last_ui = nil
local keeping = false

-- Tells the program where the view is, where that is not what it was last told; `done()` after.
function act.keep_ui(done)
	local ui = ui_state()
	local said = json.encode(ui)
	if said == last_ui or keeping then
		if done then
			done()
		end
		return
	end
	keeping = true
	kit.call("ui", { ui = ui }, function(_, err)
		keeping = false
		if not err then
			last_ui = said
		end
		if done then
			done()
		end
		return nil
	end)
end

-- What the program says happens to the files (`changes`).
local function apply(changes)
	-- `git: null` is the repository gone, no `git` that nothing changed there.
	if json.has(changes, "git") then
		S.git = changes.git
	end
	if changes.handoffFailed then
		S.handing_off = nil
		kit.toast(changes.handoffFailed, "bad", 6)
	end
	for _, info in changes.buffers or {} do
		docs.changed(info)
	end
	for _, dir in changes.dirs or {} do
		if S.listings[dir] and act.list_dir then
			act.list_dir(dir)
		end
	end
end

local function start_sync()
	b.subscribe("changes", {}, function(changes)
		apply(changes)
		return kit.take(true)
	end)
	b.time.every(2, function()
		if S.handing_off == nil then
			act.keep_ui()
		end
		return kit.take(nil)
	end)
end

-- --- Starting --------------------------------------------------------------------------------------------

function act.start(boot)
	local ui = boot.ui
	S.boot = boot
	S.settings = boot.settings
	S.git = boot.git
	S.expanded = ui.expanded or {}
	S.recent = ui.recent or {}
	S.places = ui.places or {}
	S.preview = ui.preview or {}
	S.tabs = {}
	for _, path in ui.tabs or {} do
		local tab = { path = path, status = "loading" }
		for _, info in boot.buffers or {} do
			if info.path == path then
				tab.info = info
			end
		end
		S.tabs[#S.tabs + 1] = tab
	end
	S.active = nil
	act.list_dir("")
	for _, dir in S.expanded do
		act.list_dir(dir)
	end
	-- The tab in front is opened again; the others come up as they are shown.
	local focus = boot.focus
	local active = focus and focus.path or ui.active or (ui.tabs and ui.tabs[1])
	if active then
		act.open_file(active, { line = focus and focus.line, column = focus and focus.column, focus = true })
	else
		S.focus = act.sidebar_docked() and "sidebar" or "editor"
		act.regain()
	end
	for _, problem in boot.problems or {} do
		kit.toast(problem, "warn", 6)
	end
	if boot.returned then
		local returned = boot.returned
		if returned.error then
			kit.toast("The shell did not start: " .. returned.error, "bad", 6)
		else
			local code = returned.exitCode
			kit.toast("Back from the shell" .. ((code ~= nil and code ~= 0) and (" (it ended with " .. code .. ")") or ""), "info")
		end
	end
	start_sync()
end

-- --- The pane -----------------------------------------------------------------------------------------------

-- Ends textpen: the page's state kept, what is not saved dropped first where the person asked to
-- quit without saving (else the program keeps it as drafts), then the program asked to end (`quit`):
-- a page of the kit has no close of its own, its view is the program's to end (docs/calls.md).
function act.end_view(discard)
	local function quit()
		kit.call("quit", {}, function(_, err)
			if err then
				S.handing_off = nil
				kit.toast(err.message, "bad", 8)
			end
			return nil
		end)
	end
	act.keep_ui(function()
		S.handing_off = "quit"
		act.set_overlay(nil)
		if not discard then
			quit()
			return
		end
		kit.call("discard", {}, function(_, err)
			if err then
				S.handing_off = nil
				kit.toast(err.message, "bad")
			else
				quit()
			end
			return nil
		end)
	end)
end

-- Ends textpen, asking first about files that are not saved (`tried`: saving them was tried).
function act.quit(tried)
	docs.flush_all(function()
		local dirty = {}
		for _, tab in S.tabs do
			if tab.info and tab.info.dirty then
				dirty[#dirty + 1] = tab
			end
		end
		if #dirty == 0 then
			act.end_view(false)
			return
		end
		-- Saving when a file is left saves it when textpen is left, too; what could not be saved is
		-- asked about.
		if S.settings.autoSave and not tried then
			act.save_all(function()
				act.quit(true)
			end)
			return
		end
		local names = {}
		for _, tab in dirty do
			names[#names + 1] = text.basename(tab.path)
		end
		act.ask({
			title = #dirty == 1 and ("Save the changes to " .. names[1] .. " before quitting?")
				or (#dirty .. " files have changes that are not saved"),
			message = #dirty > 1 and table.concat(names, ", ") or nil,
			actions = {
				{ id = "save", label = #dirty == 1 and "Save and Quit" or "Save All and Quit", primary = true },
				{ id = "discard", label = "Quit Without Saving", danger = true },
				{ id = "cancel", label = "Cancel" },
			},
		}, function(chosen)
			if chosen == "save" then
				act.save_all(function(all)
					if all then
						act.end_view(false)
					end
				end)
			elseif chosen == "discard" then
				act.end_view(true)
			end
		end)
	end)
end

-- Hands the pane to a shell in the project; the editor comes back when it ends.
function act.shell()
	if not (S.boot and S.boot.handoff) then
		kit.toast("There is no terminal to hand over here.", "warn")
		return
	end
	docs.flush_all(function()
		local function hand()
			act.keep_ui(function()
				kit.call("handoff", { kind = "shell" }, function(answer, err)
					if err then
						kit.toast(err.message, "bad")
					elseif answer.state == "scheduled" then
						S.handing_off = "shell"
						act.set_overlay(nil)
					elseif answer.state == "busy" then
						kit.toast("The pane is already being handed over.", "warn")
					else
						kit.toast("This terminal cannot give the pane to a shell yet.", "warn")
					end
					return nil
				end)
			end)
		end
		if S.settings.autoSave then
			act.save_all(function()
				hand()
			end)
		else
			hand()
		end
	end)
end

return actions
