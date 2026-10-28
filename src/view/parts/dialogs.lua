-- What comes over the page to ask: a question with a field to type a name or a path into, or a
-- choice of buttons (each answered with its first letter as well, where no other shares it); and
-- the notes that come and go at the bottom.

local kit = require("../lib/kit")

local b, S, act = kit.b, kit.S, kit.act

local dialogs = {}

local FIELD = "dialog-input"

-- Asks with a dialog: `spec` is { title =, message =, input = { value =, placeholder =, hint = },
-- actions = { { id =, label =, primary =, danger = }, ... } }; `done(action, value)` is told the
-- action chosen and what was typed, or nil where the person went back.
function act.ask(spec, done)
	local primary = 1
	for index, action in spec.actions do
		if action.primary then
			primary = index
		end
	end
	-- Each button answers to its first letter, where no other shares it.
	local counts, letters = {}, {}
	for _, action in spec.actions do
		local letter = string.lower(string.sub(action.label, 1, 1))
		counts[letter] = (counts[letter] or 0) + 1
	end
	for _, action in spec.actions do
		local letter = string.lower(string.sub(action.label, 1, 1))
		if counts[letter] == 1 then
			letters[letter] = action.id
		end
	end
	S.overlay = {
		kind = "dialog",
		spec = spec,
		value = spec.input and spec.input.value or "",
		chosen = primary,
		letters = letters,
		done = done,
	}
	if spec.input then
		S.field = FIELD
		kit.gather({ redraw = true, fill = { [FIELD] = spec.input.value or "" }, keyboard = { field = FIELD } })
	else
		S.field = nil
		kit.keyboard("page")
	end
end

local function answer(action)
	local overlay = S.overlay
	if not (overlay and overlay.kind == "dialog") then
		return
	end
	S.overlay = nil
	act.regain()
	if overlay.done then
		overlay.done(action, overlay.value)
	end
	kit.gather(true)
end

function dialogs.view(size)
	local overlay = S.overlay
	local spec = overlay.spec
	local box = b.div()
		:press("dialog")
		:col()
		:gap(10)
		:w(math.min(460, size.w - 32))
		:p(16)
		:rounded(10)
		:bg("raised2")
		:border(1, "border")
		:shadow(0, 18, 48, b.alpha("shadow", 0.45))
		:motion({ enter = { opacity = 0, dy = -6, seconds = 0.14 } })
		:id("dialog-box")
		:child(b.text(spec.title):weight(650):wrap())
	if spec.message then
		box:child(b.text(spec.message):color("dim"):wrap())
	end
	if spec.input then
		box:child(
			b.div()
				:text_field(FIELD)
				:h(30)
				:px(8)
				:rounded(6)
				:bg("background")
				:border(1, b.alpha("accent", 0.7))
				:child(b.text(spec.input.placeholder or ""))
		)
		if spec.input.hint then
			box:child(b.text(spec.input.hint):color("quiet"):text_size(11.5):wrap())
		end
	end
	local row = b.div():row():justify_end():gap(8):mt(4)
	for index, action in spec.actions do
		local tone = action.danger and "danger" or (action.primary and "primary" or nil)
		local letter = not spec.input and overlay.letters[string.lower(string.sub(action.label, 1, 1))] == action.id
		local element = kit.button("dialog-action:" .. index, action.label, { tone = tone, underline_first = letter })
		if index == overlay.chosen and not spec.input then
			element:border(1, b.alpha("foreground", 0.5))
		end
		row:child(element)
	end
	box:child(row)
	return b.div()
		:press("dialog-scrim")
		:overlay()
		:inset(0, 0, 0, 0)
		:bg(b.alpha("shadow", 0.32))
		:col()
		:items_center()
		:pt(math.max(math.floor(size.h * 0.18), 16))
		:child(box)
end

-- A press while a dialog is open: whether it was one of its own.
function dialogs.press(id)
	if id == "dialog-scrim" then
		answer(nil)
		return true
	end
	local index = id and tonumber(string.match(id, "^dialog%-action:(%d+)$"))
	if index then
		answer(S.overlay.spec.actions[index].id)
		return true
	end
	return id == "dialog"
end

function dialogs.text(field, words)
	if field == FIELD and S.overlay and S.overlay.kind == "dialog" then
		S.overlay.value = words
		return true
	end
	return false
end

-- A key while a dialog is open.
function dialogs.key(key, mods)
	local overlay = S.overlay
	local spec = overlay.spec
	if key == "escape" then
		answer(nil)
	elseif key == "enter" then
		local action = spec.input and spec.actions[1] or spec.actions[overlay.chosen]
		for _, candidate in spec.actions do
			if spec.input and candidate.primary then
				action = candidate
			end
		end
		answer(action.id)
	elseif not spec.input and (key == "left" or key == "right" or key == "tab") then
		local step = (key == "left" or (key == "tab" and mods.shift)) and -1 or 1
		overlay.chosen = (overlay.chosen - 1 + step) % #spec.actions + 1
	elseif not spec.input and not mods.command and not mods.control and not mods.alt then
		local id = overlay.letters[string.lower(key)]
		if id then
			answer(id)
		end
	end
	return true
end

-- --- Notes -----------------------------------------------------------------------------------------

function dialogs.toasts(bottom)
	local list = b.div():id("toasts"):overlay():inset(nil, 14, bottom + 10, nil):col():gap(6):items_end():max_w(460)
	for _, note in S.toasts do
		local color = kit.TONES[note.tone] or "accent"
		local id = "toast:" .. note.id
		list:child(
			b.div()
				:id(id)
				:row()
				:items_center()
				:gap(9)
				:pl(12)
				:pr(6)
				:py(5)
				:min_h(32)
				:rounded(8)
				:bg("raised3")
				:border(1, b.alpha(color, 0.45))
				:shadow(0, 8, 22, b.alpha("shadow", 0.35))
				:motion({ place = true, enter = { opacity = 0, dy = 8, seconds = 0.16 }, exit = { opacity = 0, seconds = 0.2 } })
				:child(b.div():size(7, 7):rounded(4):bg(color):no_shrink())
				:child(b.text(note.words):wrap():grow(1):min_w(0))
				:child(kit.icon_button("toast-x:" .. note.id, "x", "Dismiss", { size = 22, icon = 13 }))
		)
	end
	return list
end

-- The notes whose time is over go; the time the next one goes, for the page to be drawn again then.
function dialogs.expire(now)
	local next_at = nil
	local kept = {}
	for _, note in S.toasts do
		if note.until_ > now then
			kept[#kept + 1] = note
			next_at = next_at and math.min(next_at, note.until_) or note.until_
		end
	end
	S.toasts = kept
	return next_at
end

function dialogs.dismiss(id)
	local number = tonumber(string.match(id or "", "^toast%-x:(%d+)$"))
	if not number then
		return false
	end
	local kept = {}
	for _, note in S.toasts do
		if note.id ~= number then
			kept[#kept + 1] = note
		end
	end
	S.toasts = kept
	return true
end

return dialogs
