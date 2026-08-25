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
      // The next round settles its own waiters directly; this call is
      // fire-and-forget from here, so a rejection in that later round must
      // not become an unhandled rejection on this unreferenced chain.
      if (waiters.length) runRound().catch(function () {});
      return result;
    }, function (err) {
      running = false;
      thisRoundWaiters.forEach(function (w) { w.reject(err); });
      if (waiters.length) runRound().catch(function () {});
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
