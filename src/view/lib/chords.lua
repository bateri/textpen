-- The keys that are not the same on every keyboard. Most of the page's keys are written with `mod`, the
-- main modifier of the host's system (⌘ on a Mac, Ctrl elsewhere, a terminal that draws the page itself
-- included: `pencere.keyboard()`), and say the same everywhere. These are the ones that cannot, each as
-- it is on a Mac and as it is anywhere else:
--
-- - ⌃ is a key of its own on a Mac and the main modifier elsewhere, so a key the page gave ⌃ there
--   (the next occurrence ⌃G, join lines ⌃J) is another key's off a Mac;
-- - a terminal that cannot send ⇧ or ⌥ with ⌃ (no Kitty keyboard protocol) sends ⌃⇧Z as ⌃Z and ⌃⌥C as
--   ⌃C, which is the interrupt: redo is ⌃Y, copy is ⌃⇧C (which the editor takes for the copy and the
--   page for a PDF's), the options of a search are ⌥C ⌥W ⌥R;
-- - a terminal cannot tell ⌃I from Tab, ⌃J and ⌃M from Enter, ⌃H from Backspace, nor ⌃[ from Esc, and
--   sends ⌃1 as 1: those are left alone, and a key that goes with ⌥ instead (⌥1–⌥9 go to a tab) is
--   what a terminal sends as Esc and the key.
-- What cannot be sent at all (⌃⇧ and ⌃⌥ with a letter on a terminal with no Kitty keyboard) is not
-- left without a way: every command is in the palette.

local mac = pencere.keyboard() == "mac"

local function pick(on_a_mac, elsewhere)
	if mac then
		return on_a_mac
	end
	return elsewhere
end

local chords = { mac = mac, pick = pick }

-- The editor's own keys and the page's for the lines.
chords.delete_line = pick({ "mod+shift+k", "ctrl+shift+k" }, "mod+shift+k")
chords.select_line = pick("mod+i", "alt+l")
chords.join_lines = pick("ctrl+j", "alt+j")
chords.redo = pick("mod+shift+z", "mod+y")
chords.comment = "mod+/"
-- Folding: the editor's keys on a Mac; elsewhere the page binds them in the editor (`area.lua`), as the
-- terminal sends them: Shift with a sign is the character it makes (`{`), where it says so.
chords.fold = pick("mod+alt+[", { "mod+shift+[", "mod+{" })
chords.unfold = pick("mod+alt+]", { "mod+shift+]", "mod+}" })
chords.fold_all = pick("ctrl+alt+[", "mod+alt+[")
chords.unfold_all = pick("ctrl+alt+]", "mod+alt+]")

-- The occurrences of what is selected, and the pages and matches of a PDF.
chords.select_next = pick("ctrl+g", "mod+d")
chords.go_to_page = pick("ctrl+g", "mod+g")
chords.next_match = pick("mod+g", "f3")
chords.previous_match = pick("mod+shift+g", "shift+f3")
chords.copy = pick("mod+c", "mod+shift+c")

-- The shell: ⌘J, with ⌃J the join of lines, on a Mac; Ctrl+Alt+T, what a desktop opens a terminal with.
chords.shell = pick("mod+j", "mod+alt+t")

-- A tab by its number.
function chords.tab(index)
	return pick("ctrl+" .. index, "alt+" .. index)
end

-- The options of a search (case, whole words, regular expression) and "all" (every match selected, every
-- one replaced).
chords.case = pick("mod+alt+c", "alt+c")
chords.word = pick("mod+alt+w", "alt+w")
chords.regex = pick("mod+alt+r", "alt+r")
chords.all = pick("mod+enter", "alt+enter")
chords.replace = "mod+alt+f"

-- Whether `mods` (a key event's) are those of `which` of the search options' chords, the key being
-- `key`: "c", "w" or "r" for the options.
function chords.option(key, mods)
	if key ~= "c" and key ~= "w" and key ~= "r" then
		return false
	end
	if mac then
		return mods.command == true and mods.alt == true
	end
	return mods.alt == true and mods.control ~= true and mods.command ~= true
end

-- Whether the key held `mods` is the "all" of a search (⌘⏎, ⌥⏎).
function chords.all_held(mods)
	if mac then
		return mods.command == true
	end
	return mods.alt == true and mods.control ~= true
end

return chords
