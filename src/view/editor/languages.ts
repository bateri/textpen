/**
 * What language a file is in, by its name or its first line, and the parser for it,
 * loaded when a file in it is first shown: the view crosses the terminal's stream once,
 * and every language travels as its own file the terminal keeps.
 */

import { LanguageDescription } from "@codemirror/language";
import type { LanguageSupport } from "@codemirror/language";
import { languages } from "@codemirror/language-data";

import { basename } from "../lib/files";

const BY_FIRST_LINE: [RegExp, string][] = [
  [/^#!.*\b(node|deno|bun)\b/, "JavaScript"],
  [/^#!.*\bpython[\d.]*\b/, "Python"],
  [/^#!.*\b(ruby)\b/, "Ruby"],
  [/^#!.*\b(perl)\b/, "Perl"],
  [/^#!.*\b(bash|sh|zsh|ksh|dash|fish)\b/, "Shell"],
  [/^<\?xml\b/, "XML"],
  [/^<!doctype html/i, "HTML"],
];

/** Names that say a language without an extension of its own. */
const BY_NAME: [RegExp, string][] = [
  [/^\.(bash|zsh)(rc|_profile|_aliases|env)?$|^\.profile$|^\.envrc$/, "Shell"],
  [/^\.?env(\..+)?$/, "Properties files"],
  [/^(GNU)?[Mm]akefile$|\.mk$/, "Shell"],
  [/^\.(gitconfig|editorconfig)$/, "Properties files"],
  [/^(tsconfig|jsconfig)(\..+)?\.json$|^\.?(eslintrc|prettierrc|babelrc)(\.json)?$/, "JSON"],
  [/^Containerfile$|^Dockerfile\..+$/, "Dockerfile"],
];

export const LANGUAGES = languages;

export function find(name: string): LanguageDescription | null {
  const lower = name.toLowerCase();
  return (
    languages.find(
      (l) => l.name.toLowerCase() === lower || l.alias.some((a) => a.toLowerCase() === lower),
    ) ?? null
  );
}

export function describe(path: string, firstLine: string): LanguageDescription | null {
  const name = basename(path);
  const byName = BY_NAME.find(([pattern]) => pattern.test(name));
  if (byName) return find(byName[1]);
  const found = LanguageDescription.matchFilename(languages, name);
  if (found) return found;
  const byLine = BY_FIRST_LINE.find(([pattern]) => pattern.test(firstLine));
  return byLine ? find(byLine[1]) : null;
}

const loading = new Map<string, Promise<LanguageSupport | null>>();

export function load(description: LanguageDescription): Promise<LanguageSupport | null> {
  let promise = loading.get(description.name);
  if (!promise) {
    // Markdown with the code in its fences read in the fence's language.
    promise = (
      description.name === "Markdown"
        ? import("@codemirror/lang-markdown").then(({ markdown, markdownLanguage }) =>
            markdown({ base: markdownLanguage, codeLanguages: languages }),
          )
        : description.load()
    ).catch(() => null);
    loading.set(description.name, promise);
  }
  return promise;
}
