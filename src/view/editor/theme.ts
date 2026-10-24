/**
 * The editor in the terminal's colours. A terminal gives its view a handful of colours
 * (the roles of its `context`): its background and foreground, a dimmed foreground, an accent,
 * and the colours it says success, danger, warning and information with. Code needs
 * about seven kinds told apart, so each kind takes one of them as it is — they were
 * chosen by the theme's author to read on that background — and the few in between
 * are mixed from two. Every value is a CSS variable, so a theme that changes while
 * the view is open recolours it without a rebuild.
 */

import { HighlightStyle } from "@codemirror/language";
import { EditorView } from "@codemirror/view";
import { tags as t } from "@lezer/highlight";

const keyword = "var(--syntax-keyword)";
const string = "var(--syntax-string)";
const number = "var(--syntax-number)";
const type = "var(--syntax-type)";
const fn = "var(--syntax-function)";
const property = "var(--syntax-property)";
const comment = "var(--syntax-comment)";
const punctuation = "var(--syntax-punctuation)";
const tag = "var(--syntax-tag)";
const attribute = "var(--syntax-attribute)";
const special = "var(--syntax-special)";

export const highlight = HighlightStyle.define([
  {
    tag: [t.keyword, t.controlKeyword, t.moduleKeyword, t.operatorKeyword, t.definitionKeyword],
    color: keyword,
  },
  { tag: [t.modifier, t.self], color: keyword, fontStyle: "italic" },
  { tag: [t.string, t.special(t.string), t.character, t.docString], color: string },
  { tag: [t.regexp, t.escape], color: special },
  { tag: [t.number, t.bool, t.null, t.atom, t.unit], color: number },
  { tag: [t.constant(t.variableName), t.standard(t.variableName)], color: number },
  { tag: [t.typeName, t.className, t.namespace, t.definition(t.typeName)], color: type },
  {
    tag: [t.function(t.variableName), t.function(t.propertyName), t.macroName, t.labelName],
    color: fn,
  },
  { tag: [t.propertyName, t.special(t.propertyName)], color: property },
  { tag: [t.definition(t.variableName), t.definition(t.propertyName)], color: "var(--fg)" },
  {
    tag: [t.comment, t.lineComment, t.blockComment, t.docComment],
    color: comment,
    fontStyle: "italic",
  },
  { tag: [t.operator, t.punctuation, t.bracket, t.separator, t.derefOperator], color: punctuation },
  { tag: [t.tagName, t.angleBracket], color: tag },
  { tag: [t.attributeName], color: attribute },
  { tag: [t.attributeValue], color: string },
  { tag: [t.meta, t.annotation, t.processingInstruction, t.documentMeta], color: punctuation },
  // Markdown and the like.
  { tag: t.heading, color: keyword, fontWeight: "700" },
  { tag: [t.heading1, t.heading2], color: keyword, fontWeight: "700" },
  { tag: t.emphasis, fontStyle: "italic" },
  { tag: t.strong, fontWeight: "700" },
  { tag: t.strikethrough, textDecoration: "line-through" },
  { tag: [t.link], color: type, textDecoration: "underline" },
  { tag: [t.url], color: type },
  { tag: t.quote, color: comment, fontStyle: "italic" },
  { tag: t.monospace, color: string },
  { tag: t.list, color: number },
  { tag: t.contentSeparator, color: punctuation },
  // Diffs.
  { tag: t.inserted, color: "var(--ok)" },
  { tag: t.deleted, color: "var(--bad)" },
  { tag: t.changed, color: "var(--warn)" },
  { tag: t.invalid, color: "var(--bad)", textDecoration: "underline wavy" },
]);

export const editorTheme = EditorView.theme({
  "&": {
    height: "100%",
    color: "var(--fg)",
    backgroundColor: "var(--editor-bg)",
    fontSize: "var(--editor-font-size)",
  },
  "&.cm-focused": { outline: "none" },
  ".cm-scroller": {
    fontFamily: "var(--font)",
    lineHeight: "var(--editor-line-height)",
    overscrollBehavior: "contain",
  },
  ".cm-content": { padding: "6px 0 40vh", caretColor: "var(--accent)" },
  ".cm-line": { padding: "0 18px 0 6px" },
  ".cm-cursor, .cm-dropCursor": { borderLeft: "2px solid var(--accent)", marginLeft: "-1px" },
  "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection":
    { backgroundColor: "var(--selection)" },
  "&:not(.cm-focused) .cm-selectionBackground": { backgroundColor: "var(--selection-blur)" },
  ".cm-activeLine": { backgroundColor: "var(--active-line)" },
  ".cm-gutters": {
    backgroundColor: "var(--editor-bg)",
    color: "var(--fg-3)",
    border: "none",
    paddingLeft: "4px",
  },
  ".cm-lineNumbers .cm-gutterElement": { padding: "0 6px 0 10px", minWidth: "3ch" },
  ".cm-activeLineGutter": { backgroundColor: "transparent", color: "var(--fg)" },
  ".cm-foldGutter .cm-gutterElement": { padding: "0 4px", color: "var(--fg-3)" },
  ".cm-foldPlaceholder": {
    backgroundColor: "var(--bg-3)",
    border: "1px solid var(--line)",
    color: "var(--fg-2)",
    borderRadius: "4px",
    padding: "0 4px",
    margin: "0 2px",
  },
  ".cm-matchingBracket, &.cm-focused .cm-matchingBracket": {
    backgroundColor: "var(--bracket)",
    outline: "1px solid var(--bracket-line)",
    borderRadius: "2px",
    color: "inherit",
  },
  ".cm-nonmatchingBracket, &.cm-focused .cm-nonmatchingBracket": {
    backgroundColor: "color-mix(in srgb, var(--bad) 22%, transparent)",
    color: "inherit",
  },
  ".cm-searchMatch": {
    backgroundColor: "var(--match)",
    outline: "1px solid var(--match-line)",
    borderRadius: "2px",
  },
  ".cm-searchMatch.cm-searchMatch-selected": {
    backgroundColor: "var(--match-strong)",
    outline: "1px solid var(--warn)",
  },
  ".cm-selectionMatch": { backgroundColor: "var(--selection-match)", borderRadius: "2px" },
  ".cm-highlightSpace": {
    backgroundImage: "radial-gradient(circle at 50% 55%, var(--whitespace) 11%, transparent 5%)",
  },
  ".cm-highlightTab": {
    backgroundImage:
      "linear-gradient(to right, transparent 20%, var(--whitespace) 20%, var(--whitespace) 80%, transparent 80%)",
    backgroundSize: "100% 1px",
    backgroundPosition: "0 55%",
    backgroundRepeat: "no-repeat",
  },
  ".cm-trailingSpace": { backgroundColor: "color-mix(in srgb, var(--bad) 18%, transparent)" },
  ".cm-panels": { backgroundColor: "var(--bg)", color: "var(--fg)" },
  ".cm-panels.cm-panels-top": { borderBottom: "1px solid var(--line)" },
  ".cm-panels.cm-panels-bottom": { borderTop: "1px solid var(--line)" },
  ".cm-tooltip": {
    backgroundColor: "var(--surface-raised)",
    border: "none",
    borderRadius: "8px",
    boxShadow: "var(--shadow-pop)",
    color: "var(--fg)",
    overflow: "hidden",
  },
  ".cm-tooltip-autocomplete > ul": {
    fontFamily: "var(--font)",
    maxHeight: "14em",
    padding: "4px",
  },
  ".cm-tooltip-autocomplete > ul > li": {
    padding: "2px 8px !important",
    borderRadius: "5px",
    lineHeight: "1.55",
  },
  ".cm-tooltip-autocomplete > ul > li[aria-selected]": {
    backgroundColor: "var(--selected-strong)",
    color: "var(--fg)",
  },
  ".cm-completionMatchedText": {
    textDecoration: "none",
    color: "var(--accent)",
    fontWeight: "600",
  },
  ".cm-completionDetail": { color: "var(--fg-3)", fontStyle: "normal", marginLeft: "1ch" },
  ".cm-completionIcon": { opacity: "0.7", width: "1.4em" },
  ".cm-placeholder": { color: "var(--fg-3)" },
  ".cm-specialChar": { color: "var(--bad)" },
  // Vim's block cursor and its command line.
  ".cm-fat-cursor": {
    background: "var(--accent) !important",
    color: "var(--accent-fg) !important",
    outline: "none !important",
  },
  "&:not(.cm-focused) .cm-fat-cursor": {
    background: "none !important",
    outline: "1px solid var(--accent) !important",
    color: "transparent !important",
  },
  ".cm-vim-panel": {
    padding: "3px 12px",
    fontFamily: "var(--font)",
    backgroundColor: "var(--bg-2)",
    minHeight: "1.6em",
  },
  ".cm-vim-panel input": {
    background: "transparent",
    border: "none",
    outline: "none",
    color: "var(--fg)",
    fontFamily: "var(--font)",
  },
});
