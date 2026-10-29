-- The page of textpen: an editor of a project's files, drawn by the terminal in its own font and
-- theme.
--
-- Each open file is an editor of the terminal's (`pencere.editor`), coloured by a language of the
-- kit's library (`@pencere/languages`, `@pencere/syntax`); around them the tabs, the project's tree and its search, a palette for
-- going anywhere, and the status line. The program reads and writes the files, keeps git and the
-- search, and remembers where the person was; the page talks to it only by the calls of
-- docs/calls.md.

local kit = require("./lib/kit")
local docs = require("./parts/docs")
local actions = require("./parts/actions")
local commands = require("./parts/commands")
local tabs = require("./parts/tabs")
local area = require("./parts/area")
local status = require("./parts/status")
local dialogs = require("./parts/dialogs")
local sidebar = require("./parts/sidebar")
local palette = require("./parts/palette")
local find = require("./parts/find")
local search = require("./parts/search")
local preview = require("./parts/preview")
local welcome = require("./parts/welcome")
local sheets = require("./parts/sheets")
local changes = require("./parts/changes")
local editor_menu = require("./parts/editor_menu")

local b, ui, S, SIZE, act = kit.b, kit.ui, kit.S, kit.SIZE, kit.act
local layout = b.layout

local _ = actions

-- --- The page -----------------------------------------------------------------------------------------

local function layout_of(width)
	if width < SIZE.narrow then
		return "narrow"
	elseif width < SIZE.medium then
		return "medium"
	end
	return "wide"
end

-- Whether the sidebar is beside the editor (a wide enough page, and shown).
function act.sidebar_docked()
	return S.layout ~= "narrow" and S.settings ~= nil and S.settings.sidebar == true
end

local function booting()
	local box = b.div():size_full():col():items_center():justify_center():gap(8):bg("background"):family("terminal")
	if S.boot_error then
		box:child(b.text("The program did not answer"):weight(650))
		box:child(b.text(S.boot_error):color("dim"):wrap():max_w(520):align("center"))
	else
		box:child(kit.spinner(20, "quiet"))
	end
	return box
end

local function handing_off()
	local words = S.handing_off == "shell" and "Handing the pane to a shell…" or "Closing textpen…"
	local box = b.div()
		:row()
		:items_center()
		:gap(10)
		:px(16)
		:py(12)
		:rounded(10)
		:bg("raised2")
		:border(1, "border")
		:child(kit.icon("terminal-2", 20, "dim"))
		:child(b.text(words))
	if S.handing_off == "shell" then
		box:child(b.text("exit or ⌃D comes back here"):color("quiet"))
	end
	return b.div():overlay():inset(0, 0, 0, 0):bg(b.alpha("background", 0.6)):col():items_center():justify_center():child(box)
end

local function render(size, now)
	S.now, S.width, S.height = now, size.w, size.h
	S.layout = layout_of(size.w)
	S.short = size.h < SIZE.short
	if S.layout ~= "narrow" then
		S.drawer = false
	end
	local toast_wake = dialogs.expire(now)
	if not S.boot then
		return booting(), false, nil
	end
	local docked = act.sidebar_docked()
	local main = b.div():col():grow(1):min_w(0):min_h(0):child(tabs.view(docked)):child(area.view(size))
	local body = b.div():row():grow(1):min_h(0)
	if docked then
		body:child(act.sidebar_view(math.clamp(S.settings.sidebarWidth, 160, math.max(size.w - 280, 160))))
		body:child(act.resizer_view())
	end
	body:child(main)
	local root = b.div():size_full():col():bg("background"):color("foreground"):family("terminal")
	commands.on(root)
	root:child(body):child(status.view())
	if S.layout == "narrow" and S.drawer and act.drawer_view then
		root:child(act.drawer_view(size))
	end
	local overlay = S.overlay
	if overlay then
		if overlay.kind == "dialog" then
			root:child(dialogs.view(size))
		elseif overlay.kind == "palette" then
			root:child(palette.view(size))
		else
			root:child(act.overlay_view(overlay, size))
		end
	end
	if #S.toasts > 0 then
		root:child(dialogs.toasts(SIZE.status))
	end
	if S.handing_off then
		root:child(handing_off())
	end
	local wake = toast_wake
	local saved = status.wake(now)
	if saved then
		wake = wake and math.min(wake, saved) or saved
	end
	return root, false, wake
end

local function settle()
	tabs.reveal()
	sidebar.settle()
	return preview.settle()
end

-- --- What the person does -------------------------------------------------------------------------------

-- A click (a press and a release on the same element, not dragged), by the element's name.
local function click(id, event)
	if S.overlay and S.overlay.kind == "dialog" then
		dialogs.press(id)
		return
	end
	if S.overlay and S.overlay.kind == "palette" then
		if palette.press(id) then
			return
		end
	elseif S.overlay and act.overlay_press(id) then
		return
	end
	if dialogs.dismiss(id) then
		return
	end
	local _ = tabs.press(id, event) or status.press(id) or area.press(id) or changes.press(id) or find.press(id)
		or search.press(id, event) or welcome.press(id) or sidebar.press(id, event)
end

local press_event = nil

local function pressed(event)
	local at = { x = event.x, y = event.y }
	local hit = ui:press(layout, at)
	press_event = event
	if event.button == "right" and not S.overlay and editor_menu.pressed(event) then
		return true
	end
	if hit and (event.button == "right" or event.button == "middle") then
		-- The right button opens a row's menu, the middle one closes a tab: at once, no click.
		if event.button == "middle" or not sidebar.context(hit, event) then
			tabs.press(hit, event)
		end
		return true
	end
	if sidebar.pressed(hit, event) then
		return true
	end
	return true
end

local function released(event)
	local at = { x = event.x, y = event.y }
	if sidebar.released(event) then
		ui:release(layout, at)
		return true
	end
	local clicked = ui:release(layout, at)
	local dropped = ui:take_drop()
	if dropped then
		tabs.drop(dropped)
		return true
	end
	if clicked and press_event and (press_event.button or "left") == "left" then
		click(clicked, press_event)
	end
	return true
end

-- The pointer's shape where it is. Every reply to the pointer says it: one that says none is the
-- arrow, so a press or a release would otherwise turn the hand over a row into the arrow.
local function cursor_at(event)
	local over = ui.hover
	if event.place then
		return "text"
	elseif over == "resizer" then
		return "resize_horizontal"
	elseif ui:dragging() then
		return "grabbing"
	elseif over then
		return "pointer"
	end
	return "arrow"
end

local function moved(event)
	if sidebar.dragged(event) then
		return true
	end
	local changed = ui:point(layout, { x = event.x, y = event.y })
	if S.overlay and S.overlay.kind == "palette" and palette.hover(ui.hover) then
		changed = true
	end
	return { redraw = changed, cursor = cursor_at(event) }
end

local function key(event)
	local name, mods = event.key, event.mods or {}
	local overlay = S.overlay
	if overlay and overlay.kind == "dialog" then
		return dialogs.key(name, mods)
	end
	if overlay and overlay.kind == "palette" then
		return palette.key(name, mods)
	end
	if overlay then
		return act.overlay_key(name, mods)
	end
	if find.key(name, mods) or sidebar.key(name, mods) then
		return true
	end
	if name == "escape" and ui:cancel_drag() then
		return true
	end
	return nil
end

local function handle(event)
	local kind = event.type
	local reply = nil
	if event.editor ~= nil then
		docs.handle(event)
		find.editor_event(event)
		preview.follow(event)
		reply = kind ~= "viewport" or nil
	elseif kind == "move" then
		reply = moved(event)
	elseif kind == "leave" then
		reply = ui:point(layout, nil)
	elseif kind == "press" then
		reply = b.merge(pressed(event), preview.handle(event), { cursor = cursor_at(event) })
	elseif kind == "release" then
		reply = b.merge(released(event), preview.handle(event), { cursor = cursor_at(event) })
	elseif kind == "key" then
		reply = key(event)
	elseif kind == "command" then
		local overlay = S.overlay
		local taken = false
		if overlay and overlay.kind == "dialog" then
			-- Asked for over a dialog: the dialog goes unanswered (its `done` told nil), then the
			-- command runs.
			dialogs.cancel()
		elseif overlay and overlay.kind == "palette" then
			taken = palette.command(event.id)
		elseif overlay then
			taken = act.overlay_command(event.id)
		end
		if not taken then
			commands.run(event.id)
		end
		reply = true
	elseif kind == "text" then
		local _ = dialogs.text(event.field, event.text) or palette.text(event.field, event.text)
			or find.text(event.field, event.text) or search.text(event.field, event.text)
			or sheets.text(event.field, event.text)
		reply = true
	elseif kind == "focus" then
		local field = event.field
		S.field = field
		if field and docs.by_id(field) then
			S.focus = "editor"
		elseif field == "search-query" then
			S.focus, S.sidebar, S.search.in_results = "sidebar", "search", false
		elseif field == "find-query" or field == "find-replace" then
			S.focus = "editor"
		end
		reply = true
	elseif kind == "menu" then
		if not editor_menu.pick(event) then
			act.menu_pick(event.index)
		end
		reply = true
	elseif kind == "terminal_look" then
		-- The person changed how their terminal draws its cursor or its selection: the editors are
		-- drawn again with it (`area.options` reads it).
		reply = true
	elseif kind == "files" then
		for _, path in event.paths or {} do
			act.open_file(path, { focus = true })
		end
		reply = true
	end
	return kit.take(reply)
end

-- --- Starting -------------------------------------------------------------------------------------------

kit.call("boot", {}, function(boot, err)
	if err then
		S.boot_error = err.message or err.code
		return true
	end
	act.start(boot)
	return true
end)

return { render = render, settle = settle, handle = handle }
