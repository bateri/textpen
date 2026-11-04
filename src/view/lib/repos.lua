-- The repositories of the project as the page holds them: what each said, one message at a time, made
-- into one view by the paths from the project's root (`S.git`), so that what asks what git says of a file
-- does not have to know which repository said it. Only what shows the repositories themselves asks for
-- them: `many` where they are in the project's folders, `repos` in their order, `by_root` by their folder.

local repos = {}

-- One view of what `known` has of the `roots` (`known[root]` is what that one said, if it said), none
-- where the project has no repository. One the project is in that says nothing is no repository any more;
-- one of those in the folders that has not said yet is only not known.
function repos.merged(roots, known)
	if #roots == 0 then
		return nil
	end
	local many = not (#roots == 1 and roots[1] == "")
	if not many and known[""] == nil then
		return nil
	end
	local view = { many = many, changes = {}, ignored = {}, stats = {}, repos = {}, by_root = {} }
	for _, root in roots do
		local git = known[root]
		local repo = { root = root, known = git ~= nil }
		if git then
			repo.branch, repo.ahead, repo.behind = git.branch, git.ahead, git.behind
			for path, state in git.changes or {} do
				view.changes[path] = state
			end
			for _, path in git.ignored or {} do
				view.ignored[#view.ignored + 1] = path
			end
			for path, lines in git.stats or {} do
				view.stats[path] = lines
			end
		end
		view.repos[#view.repos + 1] = repo
		view.by_root[root] = repo
	end
	if not many then
		view.branch, view.ahead, view.behind = view.repos[1].branch, view.repos[1].ahead, view.repos[1].behind
	end
	return view
end

-- How many there are, in words.
function repos.counted(count)
	return count .. (count == 1 and " repo" or " repos")
end

-- The repository `path` is in, of `view`: none for a path in none.
function repos.of(view, path)
	if not (view and path) then
		return nil
	end
	if not view.many then
		return view.repos[1]
	end
	local at = string.find(path, "/", 1, true)
	while at do
		local repo = view.by_root[string.sub(path, 1, at - 1)]
		if repo then
			return repo
		end
		at = string.find(path, "/", at + 1, true)
	end
	return nil
end

return repos
