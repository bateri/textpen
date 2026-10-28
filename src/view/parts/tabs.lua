-- The open files, as tabs: dragged to reorder, closed with their × or the middle button, with a menu
-- on the right button. ⌃PgUp ⌃PgDn go through them, ⌃1–⌃9 go to one, ⌘E lists them by when they
-- were seen, ⌃W closes one.

local kit = require("../lib/kit")
local text = require("../lib/text")
local files = require("../lib/files")
local docs = require("./docs")

local b, S, SIZE, act = kit.b, kit.S, kit.SIZE, kit.act

local tabs = {}

tabs.STRIP = "tabs"

-- Each tab's name, with as much of its directory as tells two of the same name apart.
local function hints()
	local by_name = {}
	for _, tab in S.tabs do
		local name = text.basename(tab.path)
		by_name[name] = by_name[name] or {}
		table.insert(by_name[name], tab)
	end
	local out = {}
	local function hint(count, path)
		local parts = string.split(text.dirname(path), "/")
		local from = math.max(#parts - count + 1, 1)
		return table.concat(parts, "/", from, #parts)
	end
	for _, group in by_name do
		if #group > 1 then
			for _, tab in group do
				local depth = #string.split(text.dirname(tab.path), "/")
				local take = 1
				local function shared()
					for _, other in group do
						if other ~= tab and hint(take, other.path) == hint(take, tab.path) then
							return true
						end
					end
					return false
				end
				while take < depth and shared() do
					take += 1
				end
				local words = hint(take, tab.path)
				out[tab.path] = words ~= "" and words or "./"
			end
		end
	end
	return out
end

local function tab_view(tab, hint)
	local path = tab.path
	local id = "tab:" .. path
	local active = path == S.active
	local info = tab.info
	local dirty = info and info.dirty
	local over = kit.hovered(id) or kit.hovered("tab-close:" .. path)
	local git = S.git and S.git.changes and S.git.changes[path]
	local dragging = kit.ui:dragging()
	local target = dragging and dragging.target == id
	local name_color = active and "foreground" or "dim"
	if git and files.GIT_COLOR[git] then
		name_color = active and files.GIT_COLOR[git] or b.mix(files.GIT_COLOR[git], "background", 0.25)
	end
	if info and info.missing and info.mtime ~= 0 then
		name_color = "error"
	end
	local kind = files.kind(path)
	local element = b.div()
		:press(id)
		:draggable(id)
		:drop_target(id)
		:row()
		:items_center()
		:gap(7)
		:h(SIZE.tabs)
		:pl(12)
		:pr(6)
		:no_shrink()
		:max_w(260)
		:bg(active and "background" or (over and b.alpha("foreground", 0.04) or nil))
		:motion({ seconds = 0.11, color = true })
		:role("tab")
		:selected(active)
		:label(text.basename(path) .. (dirty and ", not saved" or ""))
		:tip(path .. (dirty and " · not saved" or "") .. (info and info.readonly and " · read-only" or ""))
	element:child(b.div():absolute():inset(0, 0, nil, 0):h(2):bg(active and "accent" or (target and b.alpha("accent", 0.6) or nil)))
	element:child(b.div():absolute():inset(0, 0, 0, nil):w(1):bg("border"))
	element:child(kit.icon(kind.icon, 14, kind.color))
	local name = b.rich({
		b.span(text.basename(path)):color(name_color),
		hint and b.span(" " .. hint):color("quiet") or nil,
	}):truncate():min_w(0)
	element:child(name)
	if info and info.readonly then
		element:child(kit.icon("lock", 12, "quiet"))
	end
	if info and info.conflict then
		element:child(b.text("!"):weight(800):color("error"))
	end
	local close_id = "tab-close:" .. path
	local close = b.div():press(close_id):size(20, 20):rounded(4):row():items_center():justify_center():no_shrink()
		:label("Close " .. text.basename(path)):tip("Close", kit.keys_of("ctrl+w"))
	if kit.hovered(close_id) then
		close:bg(b.alpha("foreground", 0.1))
	end
	if dirty and not over then
		close:child(b.div():size(8, 8):rounded(4):bg(active and "foreground" or "dim"))
	elseif over or active then
		close:child(kit.icon("x", 13, kit.hovered(close_id) and "foreground" or "dim"))
	end
	element:child(close)
	return element
end

function tabs.view(docked)
	local bar = b.div():row():items_center():h(SIZE.tabs):no_shrink():bg("raised1")
	if not docked then
		if S.layout == "narrow" then
			bar:child(b.div():px(5):child(kit.icon_button("show-sidebar", "menu-2", "Files", { keys = "cmd+b" })))
		else
			bar:child(b.div():px(5):child(kit.icon_button("show-sidebar", "layout-sidebar", "Show sidebar", { keys = "cmd+b" })))
		end
		bar:child(kit.rule_y())
	end
	local strip = b.div():id(tabs.STRIP):row():grow(1):min_w(0):h_full():scroll()
	local names = hints()
	for _, tab in S.tabs do
		strip:child(tab_view(tab, names[tab.path]))
	end
	bar:child(strip)
	local right = b.div():row():items_center():gap(2):px(5):no_shrink()
	local active = S.active
	if active and act.is_markdown(active) then
		local on = text.index_of(S.preview, active) ~= nil
		right:child(kit.icon_button("toggle-preview", on and "eye-off" or "eye", on and "Hide preview" or "Preview",
			{ keys = "cmd+shift+v", pressed = on }))
	end
	if #S.tabs > 1 then
		right:child(kit.icon_button("open-tabs", "chevron-down", "Open files", { keys = "cmd+e" }))
	end
	bar:child(right)
	return b.div():col():no_shrink():child(bar):child(kit.rule_x())
end

-- The menu of a tab, for the host to show at the pointer: { entries =, run = }.
function tabs.menu(path)
	local items = {
		{ "Close", "ctrl+w", function() act.close_tab(path) end },
		{ "Close Others", nil, function() act.close_others(path) end },
		{ "Close All", nil, function() act.close_others(nil) end },
		"-",
		{ "Move Left", "ctrl+shift+page_up", function()
			act.activate(path)
			act.move_tab(-1)
		end },
		{ "Move Right", "ctrl+shift+page_down", function()
			act.activate(path)
			act.move_tab(1)
		end },
		"-",
		{ "Rename…", "f2", function() act.rename(path) end },
		{ "Copy Path", nil, function() act.copy_path(path, false) end },
		{ "Reveal in Files", nil, function()
			act.reveal(path)
			act.focus_sidebar("files")
		end },
	}
	return items
end

-- A press on the strip: whether it was a tab's.
function tabs.press(id, event)
	local path = string.match(id, "^tab%-close:(.*)$")
	if path then
		act.close_tab(path)
		return true
	end
	path = string.match(id, "^tab:(.*)$")
	if path then
		if event.button == "middle" then
			act.close_tab(path)
		elseif event.button == "right" then
			act.menu(tabs.menu(path), event)
		else
			act.activate(path)
		end
		return true
	end
	if id == "open-tabs" then
		act.open_palette("tabs")
		return true
	elseif id == "toggle-preview" then
		act.toggle_preview()
		return true
	elseif id == "show-sidebar" then
		act.show_sidebar()
		return true
	end
	return false
end

-- A tab dragged onto another: it goes before it, or after it where it came from before it.
function tabs.drop(dropped)
	local from = string.match(dropped.source or "", "^tab:(.*)$")
	local onto = string.match(dropped.target or "", "^tab:(.*)$")
	if not from or not onto then
		return false
	end
	local a = text.index_of(S.tabs, docs.tab_of(from))
	local c = text.index_of(S.tabs, docs.tab_of(onto))
	act.drop_tab(from, onto, a and c and a < c)
	return true
end

-- Keeps the tab in front in view in the strip.
function tabs.reveal()
	local active = S.active and ("tab:" .. S.active)
	if not active then
		return
	end
	local strip = b.layout.bounds(tabs.STRIP)
	local rect = b.layout.bounds(active)
	if not (strip and rect) then
		return
	end
	local offset = select(2, b.layout.scroll_offset(tabs.STRIP)) or 0
	if rect.x < strip.x then
		b.layout.scroll_to(tabs.STRIP, { x = offset - (strip.x - rect.x) })
	elseif rect.x + rect.w > strip.x + strip.w then
		b.layout.scroll_to(tabs.STRIP, { x = offset + (rect.x + rect.w - strip.x - strip.w) })
	end
end

return tabs
