/**
 * The page with no file open: where the project is, what to do first and the keys for
 * it, and the files opened lately.
 */

import {
  IconCommand,
  IconFilePlus,
  IconGitBranch,
  IconKeyboard,
  IconSearch,
  IconTerminal2,
  IconFileSearch,
} from "@tabler/icons-react";

import { FileIcon, basename, dirname, friendly } from "../lib/files";
import { openFile } from "../state/actions";
import { command, run } from "../state/commands";
import { useStore } from "../state/store";
import { Keys } from "./ui";

const ACTIONS = [
  { id: "files", icon: IconFileSearch },
  { id: "new-file", icon: IconFilePlus },
  { id: "search", icon: IconSearch },
  { id: "commands", icon: IconCommand },
  { id: "shell", icon: IconTerminal2 },
  { id: "help", icon: IconKeyboard },
];

/** textpen's mark, which its icon is drawn from too (`tools/icon.mjs`). */
function Mark() {
  // Lines of text, the last one being written, with the cursor at its end.
  return (
    <svg width="22" height="22" viewBox="0 0 24 24" fill="currentColor">
      <rect x="3" y="3.8" width="18" height="3" rx="1.5" opacity="0.4" />
      <rect x="6.5" y="9.8" width="11.5" height="3" rx="1.5" opacity="0.7" />
      <rect x="3" y="15.8" width="6" height="3" rx="1.5" />
      <rect x="10.6" y="14.3" width="2.2" height="6" rx="1.1" />
    </svg>
  );
}

export function Welcome() {
  const boot = useStore((s) => s.boot);
  const git = useStore((s) => s.git);
  const recent = useStore((s) => s.recent);
  const short = useStore((s) => s.short);
  if (!boot) return null;
  const actions = ACTIONS.map((a) => ({ ...a, command: command(a.id) })).filter(
    (a) => a.command && (!a.command.when || a.command.when()),
  );
  return (
    <div className="welcome">
      <div className="welcome-inner">
        <div className="welcome-mark" aria-hidden>
          <Mark />
        </div>
        <h1 className="welcome-title">{boot.rootName}</h1>
        <div className="welcome-where">
          <span className="ellipsis">{friendly(boot.root, boot.home)}</span>
          {git && (
            <span className="welcome-branch">
              <IconGitBranch size={13} stroke={1.8} /> {git.branch}
            </span>
          )}
          {boot.remote && <span className="welcome-host">on {boot.hostname}</span>}
        </div>
        <div className="welcome-actions">
          {actions.map(({ id, icon: Icon, command: c }) => (
            <button key={id} type="button" className="welcome-action" onClick={() => run(id)}>
              <Icon size={16} stroke={1.6} />
              <span className="welcome-action-title">{c!.title.replace(/…$/, "")}</span>
              {c!.keys?.[0] && <Keys spec={c!.keys[0]} quiet />}
            </button>
          ))}
        </div>
        {recent.length > 0 && !short && (
          <div className="welcome-recent">
            <div className="welcome-label">Recent</div>
            {recent.slice(0, 6).map((path) => (
              <button
                key={path}
                type="button"
                className="welcome-file"
                onClick={() => void openFile(path, { focus: true })}
              >
                <FileIcon path={path} size={14} />
                <span className="ellipsis">
                  {basename(path)}
                  <span className="faint"> {dirname(path)}</span>
                </span>
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
