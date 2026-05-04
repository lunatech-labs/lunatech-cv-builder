// Captures the four README screenshots via headless Chrome / puppeteer-core.
//
// Prerequisites (handled by `make screenshots`):
//   - Dev server running on :3000 in dev mode (no Keycloak)
//   - Fixture CVs seeded (auto-seed on first boot — see src/db.rs)
//   - puppeteer-core installed under scripts/node_modules
//   - Google Chrome installed at the standard macOS path
//
// Output: docs/screenshots/{overview,editor,review,batch-review}.png

import puppeteer from 'puppeteer-core';
import { spawnSync } from 'node:child_process';
import { mkdirSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, '..');
const outDir = resolve(repoRoot, 'docs/screenshots');
mkdirSync(outDir, { recursive: true });

const BASE = 'http://127.0.0.1:3000';
const CHROME = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const VIEWPORT = { width: 1280, height: 900, deviceScaleFactor: 2 };

// Insert a fake review on whichever CV ends up first on the overview, so the
// "review modal" screenshot has something to render. Idempotent: skips if a
// review already exists for that CV. Done via psql so we don't need a Rust
// helper just for this.
function seedFakeReviewFor(cvId) {
  const sql = `
    INSERT INTO reviews (cv_id, user_id, overall_score, verdict, language, payload, yaml_snapshot)
    SELECT
      '${cvId}'::uuid,
      '00000000-0000-0000-0000-000000000000'::uuid,
      87, 'minor_improvements', 'en',
      jsonb_build_object(
        'overall_score', 87,
        'verdict', 'minor_improvements',
        'language', 'en',
        'report_markdown', E'## Overall Assessment\\n\\nThis CV is **client-ready with minor polish**. The eight criteria are mostly green; a handful of project entries would benefit from concrete impact numbers.\\n\\n## Per-Project Analysis\\n\\n**Project: NeoBank Solutions — Sep 2025 - present**\\n**Length:** 168 words — ✅ on target\\n\\n| Criterion | Status |\\n|---|---|\\n| Role & responsibilities | ✅ Present & Clear |\\n| People management | ✅ Present & Clear |\\n| Client interaction | ⚠️ Partial |\\n| Source of pride | ✅ Present & Clear |\\n| Added value | ✅ Present & Clear |\\n| Specific contributions | ✅ Present & Clear |\\n| Technologies | ✅ Present & Clear |\\n| Duration & dates | ✅ Present & Clear |\\n\\n*Notes:* Mention the size of the SRE team you handed off the on-call rotation to.\\n\\n## Summary of Recurring Issues\\n\\nProject entries occasionally drop the client-facing angle. Add one sentence per project naming the stakeholder you interacted with most often.',
        'improved_yaml', ''
      ),
      ''
    WHERE NOT EXISTS (SELECT 1 FROM reviews WHERE cv_id = '${cvId}'::uuid)
  `;
  const r = spawnSync('docker', [
    'exec', '-e', 'PGPASSWORD=cvbuilder', 'cv-builder-pg',
    'psql', '-U', 'cvbuilder', '-d', 'cvbuilder', '-c', sql,
  ], { encoding: 'utf8' });
  if (r.status !== 0) {
    console.error('Failed to seed fake review:', r.stderr);
    process.exit(1);
  }
}

async function fetchOverview() {
  const r = await fetch(`${BASE}/api/overview`);
  return r.json();
}

(async () => {
  console.log('Fetching overview to pick a target CV…');
  const overview = await fetchOverview();
  const target = (overview.my_cvs || [])[0];
  if (!target) {
    console.error('No CVs in DB — start the dev server with seeding first.');
    process.exit(1);
  }
  console.log(`  → using ${target.name} (${target.id})`);

  console.log('Seeding fake review (idempotent)…');
  seedFakeReviewFor(target.id);

  console.log('Launching Chrome…');
  const browser = await puppeteer.launch({
    executablePath: CHROME,
    headless: 'new',
    defaultViewport: VIEWPORT,
    args: ['--no-sandbox', '--disable-dev-shm-usage'],
  });
  const page = await browser.newPage();

  const shoot = async (filename) => {
    const out = resolve(outDir, filename);
    await page.screenshot({ path: out, type: 'png' });
    console.log(`  ✓ ${filename}`);
  };

  // 1) Overview
  console.log('1/4 Overview…');
  await page.goto(`${BASE}/`, { waitUntil: 'networkidle0' });
  await page.waitForSelector('#ov-my-grid .ov-card, #ov-all-list .ov-rank-row', { timeout: 5000 });
  await new Promise((r) => setTimeout(r, 500));
  await shoot('overview.png');

  // 2) Editor — pick a CV the dev user owns
  console.log('2/4 Editor…');
  await page.goto(`${BASE}/?id=${target.id}`, { waitUntil: 'networkidle0' });
  await page.waitForSelector('#cv-root', { timeout: 5000 });
  await new Promise((r) => setTimeout(r, 800));
  await shoot('editor.png');

  // 3) Review modal — open it from the cached review the page just loaded.
  console.log('3/4 Review modal…');
  await page.evaluate(() => {
    if (typeof openReviewModalFromCache === 'function') openReviewModalFromCache();
  });
  await page.waitForSelector('#review-modal-bg.visible', { timeout: 3000 });
  await new Promise((r) => setTimeout(r, 600));
  await shoot('review.png');

  // 4) Batch-review modal — back to overview, force-set admin and open.
  console.log('4/4 Batch-review modal…');
  await page.goto(`${BASE}/`, { waitUntil: 'networkidle0' });
  await page.waitForSelector('#ov-my-grid', { timeout: 5000 });
  await page.evaluate(() => {
    // The dev user is hardcoded non-admin server-side, but the admin
    // gate on the button is purely client-side (`meIsAdmin`). Flip it
    // for the screenshot only.
    window.meIsAdmin = true;
    if (typeof applyAdminVisibility === 'function') applyAdminVisibility();
    if (typeof openBatchModal === 'function') openBatchModal();
  });
  await page.waitForSelector('#batch-modal-bg.visible', { timeout: 3000 });
  await new Promise((r) => setTimeout(r, 600));
  await shoot('batch-review.png');

  await browser.close();
  console.log(`\nDone. Screenshots written to ${outDir}.`);
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
