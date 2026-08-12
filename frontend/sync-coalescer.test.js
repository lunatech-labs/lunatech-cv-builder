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
