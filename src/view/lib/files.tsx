/**
 * Files as the view shows them: their names, their directories, and an icon by kind in
 * one of the terminal's colours.
 */

import {
  IconBinary,
  IconBraces,
  IconBrandCpp,
  IconBrandCSharp,
  IconBrandCss3,
  IconBrandDocker,
  IconBrandGit,
  IconBrandGolang,
  IconBrandGraphql,
  IconBrandHtml5,
  IconBrandJavascript,
  IconBrandKotlin,
  IconBrandNpm,
  IconBrandPhp,
  IconBrandPython,
  IconBrandReact,
  IconBrandRust,
  IconBrandSass,
  IconBrandSvelte,
  IconBrandSwift,
  IconBrandTypescript,
  IconBrandVue,
  IconCoffee,
  IconDatabase,
  IconDiamond,
  IconFile,
  IconFileCode,
  IconFileDiff,
  IconFileSettings,
  IconFileText,
  IconFileTypePdf,
  IconFileZip,
  IconJson,
  IconLetterC,
  IconLicense,
  IconLock,
  IconMarkdown,
  IconPhoto,
  IconTable,
  IconTerminal2,
  IconToml,
} from "@tabler/icons-react";
import type { Icon } from "@tabler/icons-react";

export function basename(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1);
}

export function dirname(path: string): string {
  const at = path.lastIndexOf("/");
  return at < 0 ? "" : path.slice(0, at);
}

export function extension(path: string): string {
  const name = basename(path);
  const dot = name.lastIndexOf(".");
  return dot <= 0 ? "" : name.slice(dot + 1).toLowerCase();
}

export function join(dir: string, name: string): string {
  return dir === "" ? name : `${dir}/${name}`;
}

/** Whether `path` is `dir` or lies in it. */
export function within(path: string, dir: string): boolean {
  return dir === "" || path === dir || path.startsWith(`${dir}/`);
}

/** The directories a path lies in, outermost first: `a/b/c.ts` → `a`, `a/b`. */
export function ancestors(path: string): string[] {
  const parts = path.split("/");
  return parts.slice(0, -1).map((_, i) => parts.slice(0, i + 1).join("/"));
}

/** A path for people: `~` for the home directory. */
export function friendly(path: string, home: string): string {
  if (home && (path === home || path.startsWith(`${home}/`))) return `~${path.slice(home.length)}`;
  return path;
}

type Tone = "accent" | "info" | "ok" | "warn" | "bad" | "muted";

interface Kind {
  icon: Icon;
  tone: Tone;
}

const BY_NAME: Record<string, Kind> = {
  dockerfile: { icon: IconBrandDocker, tone: "info" },
  "docker-compose.yml": { icon: IconBrandDocker, tone: "info" },
  "docker-compose.yaml": { icon: IconBrandDocker, tone: "info" },
  "compose.yaml": { icon: IconBrandDocker, tone: "info" },
  "compose.yml": { icon: IconBrandDocker, tone: "info" },
  ".gitignore": { icon: IconBrandGit, tone: "bad" },
  ".gitattributes": { icon: IconBrandGit, tone: "bad" },
  ".gitmodules": { icon: IconBrandGit, tone: "bad" },
  "package.json": { icon: IconBrandNpm, tone: "bad" },
  "package-lock.json": { icon: IconLock, tone: "muted" },
  "cargo.lock": { icon: IconLock, tone: "muted" },
  "yarn.lock": { icon: IconLock, tone: "muted" },
  "pnpm-lock.yaml": { icon: IconLock, tone: "muted" },
  "poetry.lock": { icon: IconLock, tone: "muted" },
  license: { icon: IconLicense, tone: "warn" },
  "license.md": { icon: IconLicense, tone: "warn" },
  "license.txt": { icon: IconLicense, tone: "warn" },
  copying: { icon: IconLicense, tone: "warn" },
  makefile: { icon: IconFileSettings, tone: "warn" },
  ".editorconfig": { icon: IconFileSettings, tone: "muted" },
  ".env": { icon: IconFileSettings, tone: "warn" },
};

const BY_EXTENSION: Record<string, Kind> = {
  ts: { icon: IconBrandTypescript, tone: "info" },
  mts: { icon: IconBrandTypescript, tone: "info" },
  cts: { icon: IconBrandTypescript, tone: "info" },
  tsx: { icon: IconBrandReact, tone: "info" },
  jsx: { icon: IconBrandReact, tone: "warn" },
  js: { icon: IconBrandJavascript, tone: "warn" },
  mjs: { icon: IconBrandJavascript, tone: "warn" },
  cjs: { icon: IconBrandJavascript, tone: "warn" },
  json: { icon: IconJson, tone: "warn" },
  jsonc: { icon: IconJson, tone: "warn" },
  json5: { icon: IconJson, tone: "warn" },
  md: { icon: IconMarkdown, tone: "accent" },
  mdx: { icon: IconMarkdown, tone: "accent" },
  markdown: { icon: IconMarkdown, tone: "accent" },
  py: { icon: IconBrandPython, tone: "warn" },
  pyi: { icon: IconBrandPython, tone: "warn" },
  rs: { icon: IconBrandRust, tone: "bad" },
  go: { icon: IconBrandGolang, tone: "info" },
  html: { icon: IconBrandHtml5, tone: "bad" },
  htm: { icon: IconBrandHtml5, tone: "bad" },
  css: { icon: IconBrandCss3, tone: "accent" },
  scss: { icon: IconBrandSass, tone: "bad" },
  sass: { icon: IconBrandSass, tone: "bad" },
  less: { icon: IconBrandCss3, tone: "accent" },
  vue: { icon: IconBrandVue, tone: "ok" },
  svelte: { icon: IconBrandSvelte, tone: "bad" },
  swift: { icon: IconBrandSwift, tone: "bad" },
  kt: { icon: IconBrandKotlin, tone: "accent" },
  kts: { icon: IconBrandKotlin, tone: "accent" },
  java: { icon: IconCoffee, tone: "bad" },
  c: { icon: IconLetterC, tone: "info" },
  h: { icon: IconLetterC, tone: "accent" },
  cc: { icon: IconBrandCpp, tone: "info" },
  cpp: { icon: IconBrandCpp, tone: "info" },
  cxx: { icon: IconBrandCpp, tone: "info" },
  hpp: { icon: IconBrandCpp, tone: "accent" },
  cs: { icon: IconBrandCSharp, tone: "accent" },
  php: { icon: IconBrandPhp, tone: "accent" },
  rb: { icon: IconDiamond, tone: "bad" },
  graphql: { icon: IconBrandGraphql, tone: "bad" },
  gql: { icon: IconBrandGraphql, tone: "bad" },
  sh: { icon: IconTerminal2, tone: "ok" },
  bash: { icon: IconTerminal2, tone: "ok" },
  zsh: { icon: IconTerminal2, tone: "ok" },
  fish: { icon: IconTerminal2, tone: "ok" },
  sql: { icon: IconDatabase, tone: "warn" },
  toml: { icon: IconToml, tone: "muted" },
  yml: { icon: IconFileSettings, tone: "bad" },
  yaml: { icon: IconFileSettings, tone: "bad" },
  ini: { icon: IconFileSettings, tone: "muted" },
  conf: { icon: IconFileSettings, tone: "muted" },
  cfg: { icon: IconFileSettings, tone: "muted" },
  env: { icon: IconFileSettings, tone: "warn" },
  lock: { icon: IconLock, tone: "muted" },
  csv: { icon: IconTable, tone: "ok" },
  tsv: { icon: IconTable, tone: "ok" },
  diff: { icon: IconFileDiff, tone: "warn" },
  patch: { icon: IconFileDiff, tone: "warn" },
  txt: { icon: IconFileText, tone: "muted" },
  log: { icon: IconFileText, tone: "muted" },
  png: { icon: IconPhoto, tone: "accent" },
  jpg: { icon: IconPhoto, tone: "accent" },
  jpeg: { icon: IconPhoto, tone: "accent" },
  gif: { icon: IconPhoto, tone: "accent" },
  webp: { icon: IconPhoto, tone: "accent" },
  ico: { icon: IconPhoto, tone: "accent" },
  svg: { icon: IconPhoto, tone: "warn" },
  pdf: { icon: IconFileTypePdf, tone: "bad" },
  zip: { icon: IconFileZip, tone: "muted" },
  gz: { icon: IconFileZip, tone: "muted" },
  tar: { icon: IconFileZip, tone: "muted" },
  wasm: { icon: IconBinary, tone: "accent" },
  xml: { icon: IconFileCode, tone: "warn" },
  lua: { icon: IconFileCode, tone: "info" },
  zig: { icon: IconFileCode, tone: "warn" },
  ex: { icon: IconFileCode, tone: "accent" },
  exs: { icon: IconFileCode, tone: "accent" },
  hs: { icon: IconFileCode, tone: "accent" },
  proto: { icon: IconBraces, tone: "info" },
};

export function kindOf(path: string): Kind {
  const name = basename(path).toLowerCase();
  return (
    BY_NAME[name] ??
    (name.startsWith(".env") ? BY_NAME[".env"] : undefined) ??
    (name.startsWith("dockerfile") ? BY_NAME.dockerfile : undefined) ??
    BY_EXTENSION[extension(path)] ?? { icon: IconFile, tone: "muted" }
  );
}

export function FileIcon({ path, size = 15 }: { path: string; size?: number }) {
  const { icon: Glyph, tone } = kindOf(path);
  return <Glyph className={`file-icon tone-${tone}`} size={size} stroke={1.6} aria-hidden />;
}

export function formatBytes(size: number): string {
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(size < 10 * 1024 ? 1 : 0)} KB`;
  return `${(size / 1024 / 1024).toFixed(1)} MB`;
}

/** How long ago, in words: `just now`, `4 min ago`, `yesterday`. */
export function ago(at: number, now = Date.now()): string {
  const seconds = Math.max(0, Math.round((now - at) / 1000));
  if (seconds < 45) return "just now";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.round(hours / 24);
  return days === 1 ? "yesterday" : `${days} days ago`;
}
