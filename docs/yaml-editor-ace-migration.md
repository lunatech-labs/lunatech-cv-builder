# Replacing the YAML editor with Ace

Decision record for why the hand-rolled YAML editor was replaced with the [Ace](https://ace.c9.io/) editor library, why Ace was chosen over CodeMirror 6, and why the CDN load is hardened the way it is. Claims below are backed by measurements taken during the work (manual testing in a real browser — computed-style/API reads via devtools, not visual impressions), not assumptions, see the "How this was verified" note under each finding. No automated (e.g. Playwright) test coverage was added for the frontend as part of this work — see "What was deliberately not done" below.

## Why this started

A code review on `frontend/index.html` flagged that `refresh()` (in the old hand-rolled editor) rebuilt the highlight overlay via `hl.innerHTML = ...` without re-syncing `hl.scrollTop`/`scrollLeft` afterward, so an edit could leave the highlight layer scrolled out of alignment with the textarea until the next scroll event. That was a real, confirmed bug, fixed directly.

Investigating it surfaced a second, harder problem: even after the fix, fast scrolling with text visible still showed a brief flicker/desync at the highlight boundary. That flicker turned out to be inherent to the two-layer overlay architecture (a real `<textarea>` plus a `pointer-events: none` div mirroring its scroll position via JS), not a bug in this implementation of it. The same class of library (`react-simple-code-editor` and similar) has the same limitation, because native scroll happens on the compositor thread and the JS-driven mirror necessarily lags by at least one event. Editors that don't have this problem (CodeMirror, Monaco, Ace) don't reconcile two layers at all, they render the visible text themselves and hide the real input element, so there's only ever one thing that scrolls.

Git history showed this project had already lived through one iteration of this exact tradeoff: `2f19c8a` removed a JS-driven scroll-sync mechanism, calling it a "hack" in the commit message, in favor of a single native scroll container. That redesign had a side effect nobody flagged at the time: `Page Up`/`Page Down` stopped working, because the textarea no longer owned its own scroll. A later commit (`af1b72b`) reintroduced JS-driven scroll mirroring specifically to fix paging, reopening the flicker-prone mechanism `2f19c8a` had deliberately removed, with no discussion of that tradeoff in the commit message. That pattern (a mechanism removed for being fragile, later reintroduced to fix something else, with no record of why) is exactly what this document exists to prevent from happening a third time.

Given a hand-rolled two-layer overlay editor cannot fix the flicker without becoming a different architecture, the real options were: hand-implement paging against a single scroll container (more code, still homegrown, still a two-layer editor with the coverage gaps that implies, e.g. no code folding), or adopt a real editor library. We chose to evaluate the second option.

## What was compared: Ace vs CodeMirror 6

Monaco was ruled out without a spike. Its idiomatic setup wants a bundler, and its feature surface (web workers for language services, etc.) is built for an IDE, not a single YAML panel in a no-build-step app.

Ace and CodeMirror 6 were each spiked in an isolated git worktree, wiring only the YAML panel to the library with its default theme, then compared on:

### Loading style

Ace ships as a single UMD `<script src="...">`, the same loading pattern the app already uses for `js-yaml`/`marked`. CodeMirror 6 is ES-module-only; loading it without a bundler means `<script type="module">` plus dynamic `import()` from an ESM CDN (`esm.sh`), a loading style new to this codebase. Neither needs a build step, but Ace needed zero new *pattern*.

### Maintenance and ecosystem

Checked live (2026-08-24): `@codemirror/view` had shipped a release within the prior 24 hours; Snyk rated the CodeMirror ecosystem "Sustainable" with roughly 4.9M weekly downloads. Ace's `ace-builds` had a healthy but slower release cadence (latest release about a month prior) and no open CVEs on either package. Both are actively maintained; CodeMirror 6 has materially more momentum.

### Native paging (Page Up/Down)

**How this was verified:** loaded 200 lines into each editor, focused it, pressed `PageDown` twice, and read back the actual scroll position (not a screenshot).

Ace paged correctly with zero configuration (each press moved the same roughly 615px). CodeMirror 6, wired with only `basicSetup`, didn't move its own scroller at all on the first pass, the outer wrapper div scrolled instead, because `.cm-editor` had no explicit height, so CodeMirror never became the sole owner of the scroll viewport. That produced the exact symptom manually observed later: the first `PageDown` snapped the caret to the bottom, and the second overshot by several lines, because native caret-paging (against the wrapper) and CodeMirror's own re-layout were fighting over what "one page" means. Adding `.cm-editor { height: 100% }` plus `.cm-scroller { overflow: auto }` fixed it completely, both presses then moved by an identical, consistent amount. Conclusion: both are correct once configured; Ace needed no configuration to get there.

### Theming

**How this was verified:** read `getComputedStyle()` on rendered tokens after applying each library's theme, not eyeballed.

Both reached pixel-exact matches to the existing Ferrite palette (`#1E5F9E` keys, `#1E6A45` strings, `#6B6A62` italic comments, `#FCFBF7` surface). Ace's theme mechanism (a plain scoped stylesheet keyed to its own CSS class names) was close to a direct port of the app's pre-existing `.hl-*` classes. CodeMirror 6's theme uses tagged-rule matching (`HighlightStyle.define`) and hit one real trap along the way: pinning `@codemirror/language`/`@lezer/highlight` to *exact* versions on the ESM CDN created a second, distinct copy of those modules from the one CodeMirror's own internals resolve via `^`-range imports, so the whole theme applied with zero visible effect and no error, fixed by matching version *ranges* instead of pinning exact versions. That failure mode is specific to CodeMirror's modular, multi-package CDN loading; Ace's single self-contained script has no equivalent.

### Value-type highlighting coverage

Checked directly against each library's actual YAML grammar (Ace's `mode-yaml.js`; CodeMirror's `@lezer/yaml`), not assumed. Ace's mode tokenizes numbers and booleans as distinct token types. CodeMirror's grammar tags every plain scalar, `42`, `true`, or a name, as the same generic "content" node, since YAML doesn't syntactically distinguish value types without a schema. A permanent gap in the official language package, not a configuration miss.

### Code folding: the deciding factor

**How this was verified:** walked the live syntax tree (`syntaxTree(view.state)` for CodeMirror, `session.getFoldWidgetRange()` for Ace) and queried each library's own fold-range computation directly, then confirmed visually by actually folding rows in a loaded fixture CV.

This app's YAML schema is fundamentally a set of repeated list entries (`experiences`, `projects`, `education`, `certifications`). Folding one entry without swallowing the rest of the list is the single most useful thing folding could do here.

- **Ace**: folds every entry correctly, including the first, verified directly against its fold-range API (returns exactly the entry's own line range).
- **CodeMirror 6**: `@codemirror/lang-yaml` only registers `BlockSequence`/`Pair`/`BlockLiteral` as foldable, not `Item` (one list entry) or `BlockMapping` (its contents). Folding a field inside any entry fell back to folding the *entire remaining list*. Registering `Item`/`BlockMapping` as foldable via a small extension fixed every entry **except the first** in any list: CodeMirror's fold resolver (`foldable()` in `@codemirror/language`) always prefers the outermost node that starts on the clicked line, and a list's first entry starts at the exact same character position as the list itself, so the whole-list candidate always wins there. Fixing that fully means replacing CodeMirror's default fold-gutter resolution, not extending it, out of scope for what a language-package extension can fix.

Since every CV in this schema is a list of entries, this isn't an edge case, it's the common case, and it's the one dimension where CodeMirror 6 had a real, unfixable-without-major-work gap.

## Decision

**Ace**, on the strength of the folding finding weighed against this app's actual content, plus the zero-configuration native paging and the closer match to the app's existing CDN-script loading pattern. CodeMirror 6's ecosystem-momentum advantage was acknowledged and explicitly set aside: it matters more for a growing feature surface than it does for a single YAML panel today.

## What was implemented

- `attachEditor()` in `frontend/index.html` mounts Ace against `#ace-yaml`, `ace/mode/yaml`, and a custom `ace/theme/ferrite` mapping Ace's token classes to the existing `--ui-*` palette.
- **`getYaml()` / `setYaml(v)`** are the single choke point every other part of the file uses to read/write the editor's content, never the Ace instance or the DOM directly. This replaced an earlier `Object.defineProperty` shim on a hidden `<textarea>` (kept initially so the roughly 10 existing call sites didn't need touching); the shim was removed once those call sites were migrated, since it was a permanent, confusing indirection for what should have been a one-time bridging step.
- **Read-only mode** is now actually enforced. It previously was not: `setReadonly()` only hid owner-only buttons and showed a chip, the textarea itself stayed editable for a non-owner viewing someone else's CV, a pre-existing gap, not introduced by this migration, fixed while the code was already being touched (`yamlEditor.setReadOnly(yes)`).
- **Graceful degradation.** Ace is the app's first CDN dependency at UI-subsystem scale, a bigger blast radius than `js-yaml`/`marked` if it fails to load (the whole editing surface, not one feature). If `ace` is undefined for any reason, `attachEditor()` falls back to a plain, fully functional `<textarea>` with a visible warning banner, instead of a blank, unusable panel. `getYaml()`/`setYaml()`/`setReadonly()` all branch on whether `yamlEditor` is set, so the fallback is transparent to every caller.
- **Security hardening of the CDN load**, added over three follow-up passes as each gap was found:
  1. SRI hashes on the two initial `<script>` tags (`ace.js`, `mode-yaml.js`), cross-checked against cdnjs's own published SRI manifest and independently recomputed.
  2. `useWorker: false`: Ace's YAML mode otherwise starts a background worker fetched from the CDN at runtime via a self-derived path with **no** SRI, streaming every edit into unpinned code. `jsyaml.load()` in `updateCV()` already surfaces parse errors, so the worker added no functionality worth that gap.
  3. `ext-searchbox.js`/`ext-settings_menu.js` pinned as static SRI-verified `<script>` tags (Ace's Find and Settings-menu commands otherwise lazy-load these the same unpinned way), plus `ace.config.set('packaged', false)` to fail closed on anything not already pinned: an unregistered module fails to load visibly instead of silently falling back to an unpinned CDN fetch.

## What was deliberately not done

- **Vendoring Ace locally** (checking the built files into the repo instead of loading from a CDN) was considered and set aside: SRI plus the load-failure fallback closes most of the risk at much lower cost. Left as an open path if CDN reliability turns out to be an actual observed problem in practice, not a preemptive build.
- **The "no build tooling" constraint** (see `CLAUDE.md`) is unaffected either way, both the CDN load and a hypothetical future vendoring are static-file approaches, neither needs a bundler.
- **`ace/keyboard/hash_handler`'s `enableKeyboardAccessibility` option** was considered and rejected. It doesn't just add ARIA support: it remaps Tab (focusing the editor no longer drops straight into edit mode, an extra Enter is required) and binds Escape to blur-and-exit, for every user of the editor, not only screen-reader users. Ace's accessible surface is also a row-windowed approximation of the document (the hidden input mirrors only the current line, or a few extra lines on Windows), not parity with a native `<textarea>` even with the option on. Rejected to avoid changing editing behavior for sighted keyboard users; revisit only as a deliberate, tested UX change, not a checkbox fix.
- **Automated frontend test coverage (e.g. Playwright).** All verification in this doc was manual (devtools console/API reads during the spikes, no scripted browser automation, and no test suite checked in). The frontend has no automated test suite at all today (see `CLAUDE.md` — `cargo test` covers the Rust backend only). Adding Playwright coverage for the editor is left as future work, not attempted here.

## Known gaps (not introduced by this migration, not yet closed)

- **`js-yaml` and `marked`** (the two other CDN `<script>` tags in this file) have no SRI, unlike Ace's. `marked.parse(review.report_markdown)` is also written straight into `innerHTML` with no sanitization — and unlike the Ace risk (which requires a CDN compromise), this one is reachable with no CDN involved at all: `GET /api/cvs/{id}` exposes `latest_review` to *any* authenticated user, not just the CV's owner (see CLAUDE.md's API scope table), so a CV whose content induces Claude to emit HTML/script into `report_markdown` would execute that markup in the browser of every user who later opens that CV's review, not just the person who wrote it. That makes this a higher-urgency item than its "known gap" framing here suggests, even though it predates this migration. Fixing it belongs in its own PR, not bundled into Ace work — tracked here so the CDN-hardening story above doesn't read as complete, and flagged for prioritization independent of this doc.
