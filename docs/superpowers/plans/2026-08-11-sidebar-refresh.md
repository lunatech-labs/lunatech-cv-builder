# Sidebar Refresh Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the sidebar's "My CVs" list, score chips, and the overview page always reflect the current user's own actions (create, save, review, delete, admin batch-review), replacing the current fetch-once-on-navigation cache.

**Architecture:** A single function, `syncWorkspaceData()`, becomes the one place that fetches `GET /api/overview` and applies the result to both the sidebar and the overview page. It is built on a generic, independently-tested single-flight-with-queue coalescing primitive (`createCoalescedSync`), guards its own fetch with a 10s timeout, and surfaces a small failure indicator in the sidebar brand row when a sync fails. It is called (fire-and-forget) from every mutating action, and awaited from page load / navigation.

**Tech Stack:** Vanilla JS in `frontend/index.html` (no build step, no framework). One new plain JS file (`frontend/sync-coalescer.js`) tested with Node's built-in test runner (`node --test`, zero dependencies).

## Global Constraints

- The frontend stays a single static HTML page with no build tooling (per `CLAUDE.md`). No bundler, no npm package for the runtime code.
- Same-origin asset references use absolute paths (`/assets/...`, and now `/sync-coalescer.js`), matching the existing convention in `frontend/index.html`.
- The sync timeout is 10 seconds (chosen deliberately long: `/api/overview` is a pure DB read with no Claude call in its path, and a shorter timeout risks false positives when the server is briefly under load).
- No `.catch()` is ever required at a `syncWorkspaceData()` call site: `fetchAndApply()` (Task 3) never rejects, it always resolves (with `null` on failure), so the coalescer's reject path is only exercised by Task 1's unit tests, not by the real app.
- Design reference: `docs/superpowers/specs/2026-08-11-sidebar-refresh-design.md`. Read it if any step below seems to conflict with it; this plan implements it exactly.

---

## File Structure

- **Create** `frontend/sync-coalescer.js`: generic single-flight-with-queue coalescing primitive, `createCoalescedSync(work)`. No knowledge of `fetch` or the DOM. Loaded by `index.html` via a plain `<script src>` tag; also `require`-able from Node for testing (dual CommonJS/browser-global export, no bundler needed for either).
- **Create** `frontend/sync-coalescer.test.js`: Node test-runner tests for the primitive above.
- **Modify** `frontend/index.html`:
  - Sidebar-scoped state consolidated into one `_sidebarState` object (replaces `_sidebarOverviewCache` / `_sidebarFilter`, adds `collapsedSections`).
  - `sidebarSection()` reads collapsed state from `_sidebarState.collapsedSections` instead of losing it on every re-render.
  - New sidebar-brand failure badge (CSS + markup).
  - New `fetchAndApply()`, `applyOverviewData(data)`, `syncWorkspaceData` (replaces `ensureSidebarRendered()`, `refreshSidebarFromOverview()`, `renderOverview()`).
  - `routeView()` rewired to the new function.
  - `saveCv()`, `runReview()`, `applyBatchFrame()` each call `syncWorkspaceData()`. `deleteCv()` doesn't need its own call: it navigates to `/`, and `routeView()`'s overview branch already refreshes the sidebar (see design doc's call-site table).
- **Modify** `Makefile`: `test` target also runs the new Node test.
- **Modify** `CLAUDE.md`: mention the new Node test alongside the existing `cargo test` commands.

---

### Task 1: Generic coalescing primitive, tested standalone

**Files:**
- Create: `frontend/sync-coalescer.js`
- Create: `frontend/sync-coalescer.test.js`
- Modify: `Makefile:29-30` (the `test:` target)
- Modify: `CLAUDE.md:153-160` (the `## Tests` section)

**Interfaces:**
- Produces: `createCoalescedSync(work)` where `work` is a zero-argument function returning a `Promise`. Returns a `trigger` function (zero arguments) that, when called, returns a `Promise` resolving/rejecting with the result of some call to `work()` that started at-or-after this particular call to `trigger`. Concurrent/overlapping calls to `trigger` share one trailing call to `work()` rather than firing one each.

- [ ] **Step 1: Write the test file**

Create `frontend/sync-coalescer.test.js`:

```js
const test = require('node:test');
const assert = require('node:assert/strict');
const { createCoalescedSync } = require('./sync-coalescer.js');

function createDeferred() {
  var resolve, reject;
  var promise = new Promise(function (res, rej) { resolve = res; reject = rej; });
  return { promise: promise, resolve: resolve, reject: reject };
}

function makeControllableWork() {
  var calls = [];
  function work() {
    var d = createDeferred();
    calls.push(d);
    return d.promise;
  }
  return { work: work, calls: calls };
}

test('a single caller with nothing in flight triggers exactly one call', async function () {
  var ctrl = makeControllableWork();
  var trigger = createCoalescedSync(ctrl.work);

  var p = trigger();
  assert.equal(ctrl.calls.length, 1);
  ctrl.calls[0].resolve('result-a');
  assert.equal(await p, 'result-a');
});

test('callers that arrive while a call is in flight are coalesced into exactly one trailing round', async function () {
  var ctrl = makeControllableWork();
  var trigger = createCoalescedSync(ctrl.work);

  var pA = trigger(); // starts round 1 immediately
  assert.equal(ctrl.calls.length, 1);

  var pB = trigger(); // arrives mid-flight, queued
  var pC = trigger(); // arrives mid-flight, queued
  assert.equal(ctrl.calls.length, 1, 'B and C must not start their own requests');

  ctrl.calls[0].resolve('round-1');
  assert.equal(await pA, 'round-1');

  // Resolving round 1 must have started exactly one trailing round for B and C.
  assert.equal(ctrl.calls.length, 2, 'exactly one trailing request for B+C, not two');

  ctrl.calls[1].resolve('round-2');
  var results = await Promise.all([pB, pC]);
  assert.deepEqual(results, ['round-2', 'round-2']);
  assert.notEqual(results[0], 'round-1', 'B/C must not see the stale round-1 result');
});

test('a caller that arrives after a trailing round has already started waits for the next round', async function () {
  var ctrl = makeControllableWork();
  var trigger = createCoalescedSync(ctrl.work);

  var pA = trigger();
  var pB = trigger(); // queued, will share round 2
  ctrl.calls[0].resolve('round-1');
  await pA;
  assert.equal(ctrl.calls.length, 2);

  var pD = trigger(); // round 2 already running, D must queue for round 3
  ctrl.calls[1].resolve('round-2');
  var bResult = await pB;
  assert.equal(bResult, 'round-2');
  assert.equal(ctrl.calls.length, 3, 'D must get its own round-3 request');

  ctrl.calls[2].resolve('round-3');
  assert.equal(await pD, 'round-3');
});

test('rejection propagates to every waiter of that round, and the scheduler recovers for the next call', async function () {
  var ctrl = makeControllableWork();
  var trigger = createCoalescedSync(ctrl.work);

  var pA = trigger();
  var pB = trigger();
  ctrl.calls[0].reject(new Error('boom'));

  await assert.rejects(pA, /boom/);
  await assert.rejects(pB, /boom/);

  // The scheduler must have recovered: a fresh call starts a fresh request.
  var pC = trigger();
  assert.equal(ctrl.calls.length, 2);
  ctrl.calls[1].resolve('back-to-normal');
  assert.equal(await pC, 'back-to-normal');
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `node --test frontend/sync-coalescer.test.js`
Expected: FAIL. `require('./sync-coalescer.js')` throws (`Cannot find module`), since the file doesn't exist yet.

- [ ] **Step 3: Write the implementation**

Create `frontend/sync-coalescer.js`:

```js
// Generic single-flight-with-queue coalescer: turns any zero-argument async
// function into one where concurrent callers share in-flight work, and each
// caller is guaranteed to be resolved by a call to `work()` that started
// at-or-after their own call to the returned trigger function, never by one
// that predates it. See
// docs/superpowers/specs/2026-08-11-sidebar-refresh-design.md ("Concurrency:
// coalescing with a queue").
function createCoalescedSync(work) {
  var running = false;
  var waiters = [];

  function runRound() {
    running = true;
    var thisRoundWaiters = waiters; // snapshot: only calls that arrived before this round started
    waiters = [];
    return work().then(function (result) {
      running = false;
      thisRoundWaiters.forEach(function (w) { w.resolve(result); });
      if (waiters.length) runRound();
      return result;
    }, function (err) {
      running = false;
      thisRoundWaiters.forEach(function (w) { w.reject(err); });
      if (waiters.length) runRound();
      throw err;
    });
  }

  return function trigger() {
    if (!running) return runRound();
    return new Promise(function (resolve, reject) {
      waiters.push({ resolve: resolve, reject: reject });
    });
  };
}

if (typeof module !== 'undefined' && module.exports) {
  module.exports = { createCoalescedSync: createCoalescedSync };
}
```

- [ ] **Step 4: Run it to verify it passes**

Run: `node --test frontend/sync-coalescer.test.js`
Expected: PASS, 4 tests, 0 failures.

- [ ] **Step 5: Wire it into `make test`**

In `Makefile`, change:

```makefile
test:
	cargo test
```

to:

```makefile
test:
	cargo test
	node --test frontend/sync-coalescer.test.js
```

Also update the `help` target's description line for consistency:

```makefile
	@echo "  make test        — cargo test (Postgres must be up)"
```

to:

```makefile
	@echo "  make test        — cargo test (Postgres must be up) + frontend coalescer test"
```

- [ ] **Step 6: Update `CLAUDE.md`**

In the `## Tests` section, change:

```markdown
```bash
docker-compose up -d
cargo test                       # all 48 tests
cargo test --test api            # integration tests only
cargo test --lib pdf             # pdf unit tests only
```
```

to:

```markdown
```bash
docker-compose up -d
cargo test                       # all 48 tests
cargo test --test api            # integration tests only
cargo test --lib pdf             # pdf unit tests only
node --test frontend/sync-coalescer.test.js   # frontend sync-coalescing primitive (no build step, no deps)
```
```

- [ ] **Step 7: Run the full `make test` to confirm both suites pass**

Run: `make test` (requires `docker-compose up -d` first, per existing project convention)
Expected: cargo test's existing suite passes unchanged, followed by the new Node test's 4 passes.

- [ ] **Step 8: Commit**

```bash
git add frontend/sync-coalescer.js frontend/sync-coalescer.test.js Makefile CLAUDE.md
git commit -m "feat: add generic coalescing primitive for workspace sync, with tests"
```

---

### Task 2: Sidebar state consolidation and collapse-state fix

**Files:**
- Modify: `frontend/index.html` (sidebar section, roughly lines 2104-2231 in the current file; search for `_sidebarOverviewCache` to locate)

**Interfaces:**
- Consumes: none new (pure refactor of existing sidebar code).
- Produces: `_sidebarState` object with shape `{ overviewCache: object|null, filter: string, collapsedSections: Set<string> }`, replacing the standalone `_sidebarOverviewCache` and `_sidebarFilter` globals used by later tasks.

This task is a pure refactor plus one bug fix (collapsed sections silently re-expanding on re-render), with no behavior change to fetching yet; that comes in Task 3. It is fully testable today, before the new sync engine exists.

- [ ] **Step 1: Replace the two standalone globals with one state object**

In `frontend/index.html`, find:

```js
var _sidebarOverviewCache = null;
var _sidebarFilter = '';
```

Replace with:

```js
var _sidebarState = { overviewCache: null, filter: '', collapsedSections: new Set() };
```

- [ ] **Step 2: Update every reference to the old globals**

In `ensureSidebarRendered()`, find:

```js
async function ensureSidebarRendered() {
  if (!_sidebarOverviewCache) {
    try {
      _sidebarOverviewCache = await fetch('/api/overview').then(function(r) { return r.json(); });
    } catch (e) {
      console.warn('Sidebar: failed to load overview', e);
      return;
    }
    // First load also surfaces me / admin state for the rest of the app.
    var data = _sidebarOverviewCache;
    meId = data.me && data.me.id;
    meIsAdmin = !!(data.me && data.me.is_admin);
    renderSidebarUser(data.me);
    applyAdminVisibility();
  }
  renderSidebarList();
}
```

Replace with (unchanged behavior, just the renamed field; this function is deleted entirely in Task 3, so this is a minimal intermediate edit):

```js
async function ensureSidebarRendered() {
  if (!_sidebarState.overviewCache) {
    try {
      _sidebarState.overviewCache = await fetch('/api/overview').then(function(r) { return r.json(); });
    } catch (e) {
      console.warn('Sidebar: failed to load overview', e);
      return;
    }
    // First load also surfaces me / admin state for the rest of the app.
    var data = _sidebarState.overviewCache;
    meId = data.me && data.me.id;
    meIsAdmin = !!(data.me && data.me.is_admin);
    renderSidebarUser(data.me);
    applyAdminVisibility();
  }
  renderSidebarList();
}
```

In `refreshSidebarFromOverview()`, find:

```js
function refreshSidebarFromOverview(data) {
  _sidebarOverviewCache = data;
  renderSidebarUser(data.me);
  renderSidebarList();
}
```

Replace with:

```js
function refreshSidebarFromOverview(data) {
  _sidebarState.overviewCache = data;
  renderSidebarUser(data.me);
  renderSidebarList();
}
```

In `renderSidebarList()`, find:

```js
function renderSidebarList() {
  var data = _sidebarOverviewCache || { my_cvs: [], all_cvs: [] };
  var scroll = document.getElementById('sidebar-scroll');
  if (!scroll) return;

  var myCvs = data.my_cvs || [];
  // Exclude my own CVs from the "Lunatech" section so they don't show twice.
  var otherCvs = (data.all_cvs || []).filter(function(cv) {
    return cv.owner_id !== meId;
  });

  var filter = (_sidebarFilter || '').toLowerCase();
```

Replace with:

```js
function renderSidebarList() {
  var data = _sidebarState.overviewCache || { my_cvs: [], all_cvs: [] };
  var scroll = document.getElementById('sidebar-scroll');
  if (!scroll) return;

  var myCvs = data.my_cvs || [];
  // Exclude my own CVs from the "Lunatech" section so they don't show twice.
  var otherCvs = (data.all_cvs || []).filter(function(cv) {
    return cv.owner_id !== meId;
  });

  var filter = (_sidebarState.filter || '').toLowerCase();
```

In the `DOMContentLoaded` search-input listener, find:

```js
document.addEventListener('DOMContentLoaded', function() {
  var input = document.getElementById('sidebar-search');
  if (input) {
    input.addEventListener('input', function(e) {
      _sidebarFilter = e.target.value || '';
      renderSidebarList();
    });
  }
});
```

Replace with:

```js
document.addEventListener('DOMContentLoaded', function() {
  var input = document.getElementById('sidebar-search');
  if (input) {
    input.addEventListener('input', function(e) {
      _sidebarState.filter = e.target.value || '';
      renderSidebarList();
    });
  }
});
```

- [ ] **Step 3: Make collapsed sections survive re-renders**

In `sidebarSection()`, find:

```js
  return '<div class="sidebar-section" data-section="' + key + '">' +
           '<div class="sidebar-section-title">' +
             '<span><span class="chev">▾</span> ' + escHtml(label) + '</span>' +
             '<span class="count">' + totalCount + '</span>' +
           '</div>' +
           '<ul class="sidebar-list">' + rows + '</ul>' +
         '</div>';
```

Replace with:

```js
  var collapsedClass = _sidebarState.collapsedSections.has(key) ? ' collapsed' : '';
  return '<div class="sidebar-section' + collapsedClass + '" data-section="' + key + '">' +
           '<div class="sidebar-section-title">' +
             '<span><span class="chev">▾</span> ' + escHtml(label) + '</span>' +
             '<span class="count">' + totalCount + '</span>' +
           '</div>' +
           '<ul class="sidebar-list">' + rows + '</ul>' +
         '</div>';
```

In `renderSidebarList()`, find the section-title click handler:

```js
  Array.prototype.forEach.call(scroll.querySelectorAll('.sidebar-section-title'), function(el) {
    el.addEventListener('click', function() {
      el.parentNode.classList.toggle('collapsed');
    });
  });
```

Replace with (toggles the tracked state as well as the DOM class, so the next re-render picks it up):

```js
  Array.prototype.forEach.call(scroll.querySelectorAll('.sidebar-section-title'), function(el) {
    el.addEventListener('click', function() {
      var section = el.parentNode;
      var key = section.getAttribute('data-section');
      if (_sidebarState.collapsedSections.has(key)) {
        _sidebarState.collapsedSections.delete(key);
      } else {
        _sidebarState.collapsedSections.add(key);
      }
      section.classList.toggle('collapsed');
    });
  });
```

- [ ] **Step 4: Verify manually**

There is no frontend test framework for DOM behavior (by design, see `CLAUDE.md`). Verify by hand:

1. `make dev`, open `http://127.0.0.1:3000/`.
2. In the sidebar, click the "Lunatech CVs" section title to collapse it.
3. Type a character into the sidebar search box (this calls `renderSidebarList()` again, the same re-render path a background sync will later use).
4. Confirm "Lunatech CVs" is still collapsed after typing. Before this fix it would silently re-expand on every re-render, but note today it re-expands on *every* render already, since `sidebarSection()` never wrote a `collapsed` class before this task; this step confirms the fix, not a regression.
5. Clear the search box, confirm both sections still render their items correctly (no behavior change to the actual list contents).

- [ ] **Step 5: Commit**

```bash
git add frontend/index.html
git commit -m "refactor: consolidate sidebar state, preserve collapsed sections across re-renders"
```

---

### Task 3: Sync engine (`syncWorkspaceData`) and failure indicator

**Files:**
- Modify: `frontend/index.html`

**Interfaces:**
- Consumes: `createCoalescedSync` (Task 1, loaded via `<script src="/sync-coalescer.js">`), `_sidebarState` (Task 2).
- Produces: `syncWorkspaceData()`, a zero-argument function that is fire-and-forget safe, safe to await, and never rejects. Used by Task 4's call sites and by `routeView()` in this task.

This task swaps the fetch/apply engine but does not yet add any new call sites beyond the ones that already existed (page load, navigation, landing on the overview page). After this task, the app should behave identically to before from a user's perspective, except for the new failure badge. The actual bug fixes (new CV appearing, score updating, etc.) land in Task 4.

- [ ] **Step 1: Load the coalescer script**

In `frontend/index.html`, find:

```html
<script src="https://cdnjs.cloudflare.com/ajax/libs/js-yaml/4.1.0/js-yaml.min.js"></script>
<script src="https://cdn.jsdelivr.net/npm/marked@12.0.2/marked.min.js"></script>
```

Replace with:

```html
<script src="https://cdnjs.cloudflare.com/ajax/libs/js-yaml/4.1.0/js-yaml.min.js"></script>
<script src="https://cdn.jsdelivr.net/npm/marked@12.0.2/marked.min.js"></script>
<script src="/sync-coalescer.js"></script>
```

- [ ] **Step 2: Add the failure badge's CSS**

Find:

```css
.sidebar-brand-sub {
  font-size: 10px; font-weight: 400; letter-spacing: .04em;
  color: var(--ui-muted); margin-top: 3px;
}
```

Replace with:

```css
.sidebar-brand-sub {
  font-size: 10px; font-weight: 400; letter-spacing: .04em;
  color: var(--ui-muted); margin-top: 3px;
}
.sidebar-sync-warn {
  display: none;
  align-items: center; justify-content: center;
  width: 13px; height: 13px; border-radius: 50%;
  background: var(--ui-accent-soft); color: var(--ui-warn);
  font-size: 9px; font-weight: 700; line-height: 1;
  margin-left: 4px; cursor: default;
}
.sidebar-sync-warn.visible { display: inline-flex; }
```

- [ ] **Step 3: Add the failure badge's markup**

Find:

```html
      <div class="sidebar-brand-text">
        <span class="sidebar-brand-mark">Lunatech</span>
        <span class="sidebar-brand-sub">CV Builder</span>
      </div>
```

Replace with:

```html
      <div class="sidebar-brand-text">
        <span class="sidebar-brand-mark">Lunatech</span>
        <span class="sidebar-brand-sub">CV Builder<span class="sidebar-sync-warn" id="sidebar-sync-warn" title=""
          >!</span></span>
      </div>
```

- [ ] **Step 4: Replace the sidebar-sync section with the new engine**

Find (this spans the old cache globals through `refreshSidebarFromOverview`; Task 2 already renamed the globals inside `ensureSidebarRendered` / `refreshSidebarFromOverview`, so match against the post-Task-2 text):

```js
var _sidebarState = { overviewCache: null, filter: '', collapsedSections: new Set() };

async function ensureSidebarRendered() {
  if (!_sidebarState.overviewCache) {
    try {
      _sidebarState.overviewCache = await fetch('/api/overview').then(function(r) { return r.json(); });
    } catch (e) {
      console.warn('Sidebar: failed to load overview', e);
      return;
    }
    // First load also surfaces me / admin state for the rest of the app.
    var data = _sidebarState.overviewCache;
    meId = data.me && data.me.id;
    meIsAdmin = !!(data.me && data.me.is_admin);
    renderSidebarUser(data.me);
    applyAdminVisibility();
  }
  renderSidebarList();
}

// Called by renderOverview() so the cache stays fresh after a save / delete.
function refreshSidebarFromOverview(data) {
  _sidebarState.overviewCache = data;
  renderSidebarUser(data.me);
  renderSidebarList();
}
```

Replace with:

```js
var _sidebarState = { overviewCache: null, filter: '', collapsedSections: new Set() };

var SYNC_TIMEOUT_MS = 10000;

function showSyncWarning(message) {
  var el = document.getElementById('sidebar-sync-warn');
  if (!el) return;
  el.title = message;
  el.classList.add('visible');
}

function clearSyncWarning() {
  var el = document.getElementById('sidebar-sync-warn');
  if (!el) return;
  el.classList.remove('visible');
}

// Applies one /api/overview response to every consumer: the sidebar and the
// overview page. Safe to call even while the editor is the visible view,
// since showView() only toggles a CSS class rather than removing the
// overview page's DOM nodes.
function applyOverviewData(data) {
  meId = data.me && data.me.id;
  meIsAdmin = !!(data.me && data.me.is_admin);
  _sidebarState.overviewCache = data;
  renderSidebarUser(data.me);
  renderSidebarList();
  applyAdminVisibility();

  var displayName = (data.me && (data.me.name || data.me.email)) || 'there';
  document.getElementById('ov-me-name').textContent = displayName;
  document.getElementById('ov-user-name').textContent = displayName;
  document.getElementById('ov-user-info').classList.add('visible');
  fillStats('ov-mine', data.stats.mine);
  fillStats('ov-co', data.stats.company);
  renderTopList(data.top_cvs || []);
  renderAllList(data.all_cvs || []);
  renderMyGrid(data.my_cvs || []);
}

// The one place that fetches /api/overview. Never rejects: on any failure
// (including a timeout) it logs, shows the sidebar's failure badge, and
// resolves with null, so fire-and-forget callers never produce an unhandled
// promise rejection. See docs/superpowers/specs/2026-08-11-sidebar-refresh-design.md.
async function fetchAndApply() {
  var controller = new AbortController();
  var timeoutId = setTimeout(function() { controller.abort(); }, SYNC_TIMEOUT_MS);
  try {
    var res = await fetch('/api/overview', { signal: controller.signal });
    if (!res.ok) throw new Error('GET /api/overview failed: ' + res.status);
    var data = await res.json();
    applyOverviewData(data);
    clearSyncWarning();
    return data;
  } catch (e) {
    console.warn('Workspace sync failed', e);
    showSyncWarning(
      'Workspace data may be out of date (last refresh failed at ' +
      new Date().toLocaleTimeString() +
      '). Will retry automatically on your next save, review, or navigation.'
    );
    return null;
  } finally {
    clearTimeout(timeoutId);
  }
}

// Single-flight-with-queue coalesced: any number of simultaneous/staggered
// callers share a bounded number of underlying fetches, and each caller is
// guaranteed to be resolved by a fetch that started at-or-after their own
// call, never by one that predates it. This relies on every call site
// awaiting its own mutation's response before calling syncWorkspaceData(),
// so the mutation is already durably persisted by the time the sync call
// happens. Do not simplify this to "share the in-flight promise" without
// re-reading the design doc's Concurrency section, since that version can
// hand a caller a stale response.
var syncWorkspaceData = createCoalescedSync(fetchAndApply);
```

- [ ] **Step 5: Remove `renderOverview()` and rewire `routeView()`**

Find:

```js
async function renderOverview() {
  var data;
  try {
    data = await fetch('/api/overview').then(function(r) { return r.json(); });
  } catch (e) {
    alert('Could not load the overview: ' + e.message);
    return;
  }
  meId = data.me && data.me.id;
  meIsAdmin = !!(data.me && data.me.is_admin);
  var displayName = (data.me && (data.me.name || data.me.email)) || 'there';
  document.getElementById('ov-me-name').textContent = displayName;
  document.getElementById('ov-user-name').textContent = displayName;
  document.getElementById('ov-user-info').classList.add('visible');

  fillStats('ov-mine', data.stats.mine);
  fillStats('ov-co', data.stats.company);

  renderTopList(data.top_cvs || []);
  renderAllList(data.all_cvs || []);
  renderMyGrid(data.my_cvs || []);
  applyAdminVisibility();
  // Keep the sidebar in sync with the overview's view of the world.
  refreshSidebarFromOverview(data);
}
```

Delete it entirely (its job is now `applyOverviewData()` + `fetchAndApply()`, called via `syncWorkspaceData()` from `routeView()` below).

Find:

```js
async function routeView() {
  // Sidebar gets refreshed on every navigation so the active highlight
  // tracks the current CV. We await this so meId / meIsAdmin are set
  // before openCvInEditor runs — otherwise `currentOwnerId !== meId`
  // triggers a false-positive Read-only state on the first navigation.
  await ensureSidebarRendered();

  var params = new URLSearchParams(location.search);
  if (params.get('id')) {
    showView('editor');
    await openCvInEditor(params.get('id'));
    setTimeout(scalePreview, 0);
    highlightSidebarItem(params.get('id'));
  } else if (params.get('new') === '1') {
    showView('editor');
    openBlankEditor();
    setTimeout(scalePreview, 0);
    highlightSidebarItem(null);
  } else {
    showView('overview');
    await renderOverview();
    highlightSidebarItem(null);
  }
}
```

Replace with:

```js
async function routeView() {
  var params = new URLSearchParams(location.search);
  var goingToOverview = !params.get('id') && params.get('new') !== '1';

  // Full re-fetch on the very first navigation (meId isn't known yet, and we
  // await this so meId / meIsAdmin are set before openCvInEditor runs;
  // otherwise `currentOwnerId !== meId` triggers a false-positive Read-only
  // state) and every time the overview page itself is the destination
  // (it shows company-wide stats/rankings that can move independently of
  // this tab's own actions). Navigating between CVs in the editor otherwise
  // just re-renders from whatever's already cached, kept fresh by the
  // mutation call sites in saveCv() / runReview() / deleteCv(), not by
  // re-fetching on every click.
  if (meId === null || goingToOverview) {
    await syncWorkspaceData();
  } else {
    renderSidebarList();
  }

  if (params.get('id')) {
    showView('editor');
    await openCvInEditor(params.get('id'));
    setTimeout(scalePreview, 0);
    highlightSidebarItem(params.get('id'));
  } else if (params.get('new') === '1') {
    showView('editor');
    openBlankEditor();
    setTimeout(scalePreview, 0);
    highlightSidebarItem(null);
  } else {
    showView('overview');
    highlightSidebarItem(null);
  }
}
```

Note: `ensureSidebarRendered()` no longer exists after Step 4 above, since it was inside the block replaced there, along with `refreshSidebarFromOverview()`. Nothing further to delete for either of them.

- [ ] **Step 6: Verify manually (no regressions)**

1. `make dev`, open `http://127.0.0.1:3000/`. Confirm the overview page loads: stats tiles, "My CVs", "Lunatech CVs", top-ranked list all populate as before.
2. Click into a CV, confirm the editor loads normally and the sidebar highlights it.
3. Navigate back to the overview (click the brand/logo), confirm it still refreshes (stats/lists match current DB state).
4. Open the browser's devtools Network tab, right-click the `/api/overview` request and choose "Block request URL" (or otherwise force it to fail, e.g. temporarily stop the backend). Trigger a navigation (click between two CVs, or reload with `?id=` cleared to hit the overview branch). Wait up to 10 seconds. Confirm: a small "!" appears next to "CV Builder" in the sidebar brand row, hovering it shows a tooltip mentioning the data may be out of date, and the browser console shows a `Workspace sync failed` warning.
5. Unblock the request (or restart the backend), trigger another sync (navigate to the overview again), confirm the "!" badge disappears once the sync succeeds again.

- [ ] **Step 7: Commit**

```bash
git add frontend/index.html
git commit -m "feat: replace sidebar's fetch-once cache with the coalesced sync engine"
```

---

### Task 4: Wire mutation call sites

**Files:**
- Modify: `frontend/index.html`

**Interfaces:**
- Consumes: `syncWorkspaceData()` (Task 3).

This is the task that actually fixes the reported bugs: a newly created/saved CV appearing in "My CVs" without navigating away, the sidebar's score chip updating after a review, and the admin batch-review completion actually refreshing something (replacing the dead `loadOverview()` call).

- [ ] **Step 1: `saveCv()` (fixes "new CV doesn't appear" / "renamed CV doesn't update")**

Find:

```js
    savedYaml = yaml;
    refreshStatus();
    return true;
```

Replace with:

```js
    savedYaml = yaml;
    refreshStatus();
    syncWorkspaceData();
    return true;
```

- [ ] **Step 2: `runReview()` (fixes "score doesn't update")**

Find:

```js
    cachedReview = payload;
    cachedReviewAt = new Date().toISOString();
    refreshReviewBadge();
    openReviewModal(cachedReview, cachedReviewAt);
```

Replace with:

```js
    cachedReview = payload;
    cachedReviewAt = new Date().toISOString();
    refreshReviewBadge();
    syncWorkspaceData();
    openReviewModal(cachedReview, cachedReviewAt);
```

- [ ] **Step 3: `deleteCv()` — superseded, no change needed**

`deleteCv()` already navigates to `/` after a delete, and `routeView()`'s overview branch
refreshes the sidebar and rankings on its own — no explicit `syncWorkspaceData()` call is
needed here. An earlier version of this plan had `deleteCv()` fire its own explicit call
"to make the refresh explicit," but that just guarantees two serialized round trips instead
of one (the coalescer's own correctness guarantee means a queued caller always gets a fresh
round, never the in-flight one). See the design doc's call-site table for the final reasoning.
Leave `deleteCv()` as-is and move on to Step 4.

- [ ] **Step 4: `applyBatchFrame()` (fixes the dead `loadOverview()` call)**

Find:

```js
  if (snap.completed_at) {
    document.getElementById('batch-state-pill').textContent =
      snap.failed && snap.failed.length > 0 ? 'Done with errors' : 'Done';
    if (typeof loadOverview === 'function') loadOverview();
  }
```

Replace with:

```js
  if (snap.completed_at) {
    document.getElementById('batch-state-pill').textContent =
      snap.failed && snap.failed.length > 0 ? 'Done with errors' : 'Done';
    syncWorkspaceData();
  }
```

- [ ] **Step 5: Verify manually**

1. `make dev`, open `http://127.0.0.1:3000/`.
2. Click "+ New CV", paste in minimal YAML (at least a `name:` key), click **Save** (not Review). Without navigating away, confirm the new CV now appears under "My CVs" in the sidebar.
3. Rename the CV (edit the `name:` field) and Save again; confirm the sidebar's label updates without navigating away.
4. Open an existing CV, click **Delete**, confirm it disappears from "My CVs" in the sidebar.
5. If `ANTHROPIC_API_KEY` is configured (see `CLAUDE.md`'s env var docs; without it this step isn't testable end-to-end): open a CV, click **Review**, wait for it to finish, and confirm the sidebar's score chip for that CV updates from "not reviewed yet" (or its old score) to the new score, without navigating away.
6. If you're an admin (`ADMIN_EMAILS` configured) and `ANTHROPIC_API_KEY` is set: open "Review all CVs", run a batch, and confirm the sidebar and the overview's rankings update once the batch modal reports "Done". If either prerequisite isn't available, verify by code inspection instead: confirm `applyBatchFrame()`'s `completed_at` branch calls `syncWorkspaceData()` (Step 4 above) rather than the old dead `loadOverview()` reference.

- [ ] **Step 6: Commit**

```bash
git add frontend/index.html
git commit -m "fix: refresh sidebar after save, review, delete, and batch-review"
```
