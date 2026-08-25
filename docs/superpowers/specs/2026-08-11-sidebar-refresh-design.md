# Sidebar refresh design

Date: 2026-08-11
Status: approved, ready for implementation planning

## Problem

The sidebar's "My CVs" list and score chips are driven by `_sidebarOverviewCache`, a
client-side cache fetched once from `GET /api/overview`. That cache is only refreshed
when the user lands on the overview page (`renderOverview()`). Concretely this causes:

- Creating a new CV and saving it (directly, or indirectly via Review / Preview PDF /
  Export PDF, all of which save first) does not add it to "My CVs" while the user stays
  in the editor.
- Running a review updates the in-editor badge but not the sidebar's score chip for that
  CV.
- A pre-existing, unrelated bug in the same area: the admin batch-review completion
  handler (`applyBatchFrame()` in `frontend/index.html`) calls `loadOverview()` to refresh
  things once a batch finishes. That function does not exist anywhere in the file, so the
  call is silently a no-op (guarded by `typeof loadOverview === 'function'`). Batch-review
  completion currently refreshes nothing.

## Scope

This design fixes staleness caused by the current user's own actions in the current
browser tab: create, save, review, delete, and admin batch-review. It does not address
staleness caused by other users or other tabs (for example, someone else finishing a
review while your sidebar is open). That is an explicit non-goal for this change.

## Architecture

### `syncWorkspaceData()`

A single function becomes the one place that fetches `/api/overview` and applies the
result. It takes no arguments, performs the fetch itself, and applies the response to:

- The sidebar cache and render (`renderSidebarUser`, `renderSidebarList`).
- The overview page's stats tiles and CV lists/grids (`fillStats`, `renderTopList`,
  `renderAllList`, `renderMyGrid`, `applyAdminVisibility`).
- `meId` / `meIsAdmin`.

Because `showView()` only toggles a CSS `active` class rather than removing DOM nodes,
writing to the overview page's elements is harmless even while the editor is the
currently visible view. There is no need to branch on which view is active.

`renderOverview()` as a separate fetch-and-apply function goes away. Landing on the
overview page becomes just another caller of `syncWorkspaceData()`, awaited the same way
page load / navigation already is.

### Call sites

| Call site | Awaited? | Purpose |
|---|---|---|
| `routeView()` (page load / navigation) | Yes | Needs `meId` / `meIsAdmin` set before deciding whether the current CV is read-only. |
| `saveCv()`, after a successful create or update | No (fire and forget) | Fixes the "new CV does not appear" and "renamed CV does not update" cases. Covers Save, Review's implicit save, and Preview/Export PDF's implicit save for free, since they all funnel through `saveCv()`. |
| `runReview()`, after the review response is persisted | No | Fixes the "score does not update" case. |
| `applyBatchFrame()`, on batch completion (`snap.completed_at`) | No | Replaces the dead `loadOverview()` call. Fires once, when the whole batch finishes, not per CV. |

`deleteCv()` has no explicit call site of its own: deleting a CV navigates to `/`, and
`routeView()`'s overview branch (above) is what refreshes the sidebar and rankings. An
earlier version of this design had `deleteCv()` also fire its own explicit call "to make
the refresh explicit"; that turned out to guarantee two serialized round trips instead of
one (the coalescer's own correctness guarantee means a queued caller always gets a fresh
round, never the in-flight one), so it was dropped in favor of relying solely on the
navigation's own refresh.

No caller reads the resolved value of the returned promise for data. The only thing any
caller "consumes" is the side effect (DOM already updated by the time the promise
settles). `routeView()` awaits purely for that sequencing, not to inspect a value.

### Concurrency: coalescing with a queue

Several call sites can trigger a sync close together (for example `runReview()`'s
internal `saveCv()` call, or a "Save & leave" navigation to the overview page, where
`saveCv()`'s own call lands near the one `routeView()` fires for the overview
destination). To avoid firing redundant concurrent requests, and to avoid a
subtle race where an older in-flight response could overwrite a newer one,
`syncWorkspaceData()` uses single-flight coalescing with a queue of waiters, not a single
shared promise.

The scheduler itself is generic (it has no knowledge of `fetch` or the DOM; it just
coalesces calls to whatever async function it wraps), so it can be extracted and tested
in isolation (see Testing below). `syncWorkspaceData()` is that generic scheduler
instantiated with `fetchAndApply` as the wrapped function:

```js
// Generic, no knowledge of fetch/DOM. Lives in its own file.
function createCoalescedSync(work) {
  var running = false;
  var waiters = [];               // callers who arrived mid-flight, need the NEXT round

  function trigger() {
    if (!running) return runRound();
    return new Promise(function(resolve, reject) {
      waiters.push({ resolve: resolve, reject: reject });
    });
  }

  function runRound() {
    running = true;
    var thisRoundWaiters = waiters;   // snapshot: only calls that arrived before this round started
    waiters = [];
    return work().then(function(result) {
      running = false;
      thisRoundWaiters.forEach(function(w) { w.resolve(result); });
      if (waiters.length) runRound();
      return result;
    }, function(err) {
      running = false;
      thisRoundWaiters.forEach(function(w) { w.reject(err); });
      if (waiters.length) runRound();
      throw err;
    });
  }

  return trigger;
}

// In index.html:
var syncWorkspaceData = createCoalescedSync(fetchAndApply);
```

Behavior this guarantees, for any number of simultaneous or staggered callers:

- If nothing is in flight, a caller directly triggers and holds the fetch's own promise.
- If a fetch is already in flight, a caller is queued and resolved by the next round, a
  fetch that only starts after that caller registered, and therefore is guaranteed fresh
  with respect to that caller's already-committed mutation (every call site awaits its
  own mutation's response before calling `syncWorkspaceData()`, so the mutation is durably
  persisted before the sync call happens).
- Any number of callers that arrive during the same in-flight window share exactly one
  trailing round, not one fetch per caller. Requests are bounded at roughly one plus one
  per busy period, never a one-to-one pile-up with callers.
- A caller never receives a stale response that predates its own already-committed
  change.

This is more machinery than any current caller strictly needs (all of them are fire and
forget; a simpler "share the in-flight promise" version would behave identically for
today's call sites). It is deliberately chosen as insurance against a future caller that
awaits the result and consumes it directly, where the simpler version could silently hand
back a stale snapshot. The precondition this design relies on (callers must await their
own mutation before calling `syncWorkspaceData()`) must stay documented with a comment on
the function, so a future edit does not simplify this back into a racy version without
understanding why it is written this way.

### Timeout and failure UX

`fetchAndApply()` wraps its `fetch('/api/overview')` call with an `AbortController` and a
10 second timeout. 10 seconds is intentionally on the longer side: `/api/overview` is a
pure database read (no Claude call in this path), so it normally resolves in well under a
second even at hundreds of rows. A shorter timeout (5s) risks false positives when the
server is briefly under load rather than actually stuck.

On any failure (timeout or otherwise):

- The error is logged with `console.warn`, matching the existing pattern in
  `ensureSidebarRendered()`.
- `fetchAndApply()` resolves (it does not reject) so that fire-and-forget callers never
  produce an unhandled promise rejection.
- A small indicator appears: a hidden-by-default "!" badge next to "CV Builder" in the
  sidebar brand row, with a native `title` tooltip (consistent with the existing
  score-chip convention) explaining that workspace data may be out of date and will
  retry automatically on the next save, review, or navigation. The badge is placed at the
  application level (next to the brand mark), not on the user's own avatar or name, since
  a stale-refresh failure reads as an application-level condition rather than an
  account-level one.
- The badge clears automatically the next time a sync succeeds.
- There is no dedicated retry loop. The next user action (save, review, delete, navigate)
  naturally triggers a fresh attempt.

### Sidebar section collapse state

`renderSidebarList()` currently rebuilds `#sidebar-scroll`'s entire `innerHTML` on every
render, including each section's `collapsed` class, computed fresh each time. Since
`syncWorkspaceData()` can now re-render the sidebar list in the background (not only on
user-initiated navigation), a section the user manually collapsed would silently
re-expand the next time a background sync fires.

Fix: track collapsed sections explicitly, in a small `Set`, rather than relying only on
the DOM class, and read from it when generating each section's markup so a re-render
preserves whatever the user had toggled. This is in-session only (a plain JS variable),
not persisted across a page reload, since the reported problem is specifically about the
background-resync reset, not about surviving reloads.

While touching this code, the two existing sidebar-scoped globals
(`_sidebarOverviewCache`, `_sidebarFilter`) are consolidated with the new collapsed-set
into one object:

```js
var _sidebarState = { overviewCache: null, filter: '', collapsedSections: new Set() };
```

This is a net reduction in `index.html`'s global count (two globals become one object),
not a further increase, and stays scoped to the sidebar-specific state this change
already touches. `meId` / `meIsAdmin` are not folded in, since they are used far more
broadly (editor read-only checks, admin visibility) than just the sidebar.

The coalescing scheduler's own bookkeeping (`running` / `waiters` in the `createCoalescedSync`
closure above) needs no new globals in `index.html` at all, since it is extracted into its
own file (see Testing below) and lives as closure-local state there.

## Testing

There is no frontend test framework in this repository by design (the frontend stays a
single static HTML page with no build step). The coalescing logic above is the one piece
of this change subtle enough to warrant more than manual verification, since its failure
mode (a caller silently consuming a stale response) is exactly the kind of thing manual
browser testing is unlikely to reliably exercise.

Approach: extract the generic coalescing primitive into its own small file with no
knowledge of `fetch` or the DOM (it takes an async function and returns a scheduler). That
primitive is then testable by a small standalone script using Node's built-in test
runner (`node --test`, no framework, no build step, consistent with the project's
existing use of plain Node for `scripts/screenshots.mjs`), asserting call counts and
resolution grouping against a fake, artificially delayed async function. This test is
wired into `make test` so it runs as part of the normal verification habit rather than
being a command someone has to remember to run separately.

The `fetchAndApply()` half (the real network call and DOM writes) is verified manually,
by exercising the app in a browser: create a CV and confirm it appears in "My CVs"
without navigating away, run a review and confirm the sidebar score chip updates, delete
a CV and confirm it disappears, and run an admin batch review and confirm the sidebar and
rankings update once it completes.

## Performance

Checked against the concrete case of 200 to 300 CVs in the database:

- The relevant queries are index-backed (`reviews_cv_id_created_at_idx` backs the
  per-CV latest-review lookup, `cvs_updated_at_idx` / `cvs_user_id_updated_at_idx` back
  the ordering), so `all_cvs_with_review()`'s per-row LATERAL join stays an index lookup
  per row rather than a scan. At this row count, all four queries behind `/api/overview`
  execute in low single-digit milliseconds combined.
- The response payload (`all_cvs` plus `my_cvs` plus `top_cvs`, capped at
  `TOP_CVS_LIMIT = 10`) serializes to roughly 60 to 120 KB of JSON at 300 rows, trivial to
  transfer and parse.
- Rebuilding the sidebar's `innerHTML`, plus the overview page's stats tiles and its
  three lists (top, all, mine), for a few hundred CVs combined is still a few
  milliseconds of DOM work, not a bottleneck at this scale.
- The real (pre-existing, unrelated to this change) cost driver is that the
  `/api/overview` handler runs its four queries sequentially rather than concurrently
  (`handlers.rs`). This is already paid once per navigation today; this change makes it
  paid once per mutating action too. At the current scale that stays comfortably fast.
  If the workspace grows toward thousands of CVs, that sequential-query cost, not the
  sync mechanism designed here, is the first thing to revisit (for example running the
  four queries concurrently, or paginating `all_cvs_with_review()`).
- During a single Review action specifically, this design results in exactly two
  `/api/overview` requests, not one: one right after the implicit save (fast, surfaces a
  newly created or renamed CV immediately), and one after the review response persists
  20 to 60 seconds later (surfaces the new score). They do not coalesce, since the gap
  between them is far longer than a single `/api/overview` round trip. This is intentional:
  dropping the first call to save a request would reintroduce the "new CV does not
  appear" bug for a create-then-immediately-review flow.

## Accepted limitations (explicitly not fixed by this change)

- Admin batch-review only resyncs the sidebar and rankings once, when the whole batch
  completes, not progressively as each CV finishes. The batch modal already shows live
  per-CV progress on its own; this is judged sufficient.
- Cross-tab and cross-user staleness (changes made by someone else, or in another tab,
  while your sidebar is open) is out of scope, per the Scope section above.
- Scroll position within the sidebar's CV list is not explicitly preserved across a
  background resync. Given the list is rebuilt wholesale on every render already (not
  something this change introduces), any jump is minor and inconsistent (browsers
  generally keep a scrollable container's `scrollTop` across an `innerHTML` replace
  unless the new content is shorter than the previous scroll position), and not worth
  solving as part of this change.
