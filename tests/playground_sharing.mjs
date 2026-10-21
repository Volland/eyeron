import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import vm from 'node:vm';

const html = readFileSync(new URL('../playground.html', import.meta.url), 'utf8');
const sharingCode = html.slice(html.indexOf('      function updateShareButtons()'), html.indexOf('      function ensureWorker()'));
const restoreCode = html.slice(html.indexOf('      async function restore()'), html.indexOf('      for (const name of bundledExamplesN3)'));
const snapshot = {
  v: 1,
  sources: [
    { name: 'rules.n3', text: '# Socrates, Σωκράτης, ソクラテス\n'.repeat(30), baseIri: null },
    { name: 'https://example.org/data.trig', text: '<urn:a> <urn:p> "01" .', baseIri: 'https://example.org/data.trig' },
  ],
  mode: '--check-proof', rdf: true, stream: false, streamMessages: false,
  baseIri: 'https://example.org/base/', maxBackwardDepth: '8',
  proof: { name: 'saved-proof.n3', text: '<urn:proof> <urn:p> "proof text" .' }, example: '',
};
const rawGistUrl = 'https://gist.githubusercontent.com/test/123/raw/revision/eyeron-playground-state.json';
const plain = (value) => JSON.parse(JSON.stringify(value));

function page(overrides = {}) {
  const fields = new Map();
  const local = new Map();
  const session = new Map();
  const copied = [];
  let saved = 0;
  const storage = (map) => ({ getItem: (key) => map.get(key) ?? null, setItem: (key, value) => map.set(key, value), removeItem: (key) => map.delete(key) });
  const context = vm.createContext({
    URL, URLSearchParams, TextEncoder, TextDecoder, Uint8Array, Blob, Response, CompressionStream, DecompressionStream,
    btoa, atob, AbortController, setTimeout, clearTimeout,
    location: { href: 'https://example.org/playground?example=socrates.n3&stateurl=old#state=old' },
    busy: false, sharing: false, restoring: false, sources: [{ name: '-', text: 'default', baseIri: null }],
    stateKey: 'autosave', gistTokenKey: 'gist-token', bundledExamplesN3: ['socrates.n3'],
    baseForName: (name) => name === '-' ? null : new URL(name, 'https://example.org/playground').href,
    byId: (id) => {
      if (!fields.has(id)) fields.set(id, { value: '', hidden: true, focus() { this.focused = true; }, select() { this.selected = true; } });
      return fields.get(id);
    },
    proof: { setValue: (text) => { context.proofText = text; } },
    state: () => snapshot,
    navigator: { clipboard: { writeText: async (text) => { copied.push(text); } } },
    document: { execCommand: () => false },
    prompt: () => null,
    sessionStorage: storage(session), localStorage: storage(local),
    fetch: async () => { throw new Error('Unexpected network request'); },
    fetchSource: async () => { throw new Error('Unexpected state URL'); },
    loadInputs: async (action) => action(new AbortController().signal),
    showSources() {}, updateOptions() {},
    scheduleSave: () => { if (!context.restoring) saved++; },
    ...overrides,
  });
  vm.runInContext(sharingCode + '\n' + restoreCode, context);
  return { context, fields, local, session, copied, saved: () => saved };
}

test('compressed share links preserve Unicode, all inputs, base IRIs, options, and proof', async () => {
  const { context } = page();
  const token = await context.packState(snapshot);
  assert.ok(token.startsWith('g.'));
  assert.deepEqual(plain(await context.unpackState(token)), snapshot);
  const url = new URL(await context.makeShareUrl(snapshot));
  assert.equal(url.pathname, '/playground');
  assert.equal(url.search, '');
  assert.ok(url.hash.startsWith('#state=g.'));
  assert.deepEqual(plain(await context.unpackState(url.hash.slice(7))), snapshot);
});

test('JSON links work without compression support and malformed states are rejected', async () => {
  const { context } = page({ CompressionStream: undefined });
  const token = await context.packState(snapshot);
  assert.ok(token.startsWith('j.'));
  assert.deepEqual(plain(await context.unpackState(token)), snapshot);
  for (const token of ['', 'g.', 'unknown.payload', 'j.a', 'j.eyJ2Ijo5OX0']) await assert.rejects(context.unpackState(token));
  for (const state of [null, { ...snapshot, v: 99 }, { ...snapshot, sources: [] }, { ...snapshot, sources: [null] },
    { ...snapshot, mode: '--unknown' }, { ...snapshot, rdf: 'false' }, { ...snapshot, proof: { text: 'missing name' } }]) {
    assert.throws(() => context.normalizeState(state));
  }
  const compressed = await page().context.packState(snapshot);
  context.DecompressionStream = undefined;
  await assert.rejects(context.unpackState(compressed), /cannot open compressed links/);
});

test('Gist sharing uploads an unlisted snapshot and returns a short state URL', async () => {
  let request;
  const { context } = page({ fetch: async (url, options) => {
    request = { url, options };
    return Response.json({ files: { 'eyeron-playground-state.json': { raw_url: rawGistUrl } } });
  } });
  const link = new URL(await context.createGistShareUrl(snapshot, 'test-token'));
  assert.equal(request.url, 'https://api.github.com/gists');
  assert.equal(request.options.method, 'POST');
  assert.equal(request.options.headers.Authorization, 'Bearer test-token');
  assert.equal(request.options.credentials, 'omit');
  assert.equal(request.options.referrerPolicy, 'no-referrer');
  const body = JSON.parse(request.options.body);
  assert.equal(body.public, false);
  assert.deepEqual(JSON.parse(body.files['eyeron-playground-state.json'].content), snapshot);
  assert.equal(link.searchParams.get('stateurl'), rawGistUrl);
  assert.equal(link.searchParams.get('example'), null);
  assert.equal(link.hash, '');
  assert.ok(!link.href.includes('test-token'));
});

test('Gist errors are specific and rejected credentials are removed', async () => {
  const { context, session } = page({ fetch: async () => Response.json({ message: 'Bad credentials' }, { status: 401 }) });
  session.set('gist-token', 'rejected-token');
  await assert.rejects(context.createGistShareUrl(snapshot, 'rejected-token'), /HTTP 401: Bad credentials/);
  assert.equal(session.get('gist-token'), undefined);
  context.fetch = async () => Response.json({ files: {} });
  await assert.rejects(context.createGistShareUrl(snapshot, 'test-token'), /did not return the Gist state URL/);
  context.fetch = async () => { throw new DOMException('Aborted', 'AbortError'); };
  await assert.rejects(context.createGistShareUrl(snapshot, 'test-token'), /timed out/);
});

test('clipboard rejection leaves a selectable link for manual copying', async () => {
  const { context, fields } = page({ navigator: { clipboard: { writeText: async () => { throw new Error('Clipboard blocked'); } } } });
  assert.equal(await context.copyShareUrl('https://example.org/shared'), false);
  assert.equal(fields.get('share-result').hidden, false);
  assert.equal(fields.get('share-url').value, 'https://example.org/shared');
  assert.ok(fields.get('share-url').focused && fields.get('share-url').selected);
});

test('sharing cancellation uploads nothing; Gist tokens stay outside the shared snapshot', async () => {
  const { context, fields, session, copied } = page();
  await context.sharePlayground(true);
  assert.equal(fields.get('share-status').textContent, 'Gist sharing cancelled.');
  assert.equal(session.size, 0);
  context.prompt = () => 'Bearer test-token';
  context.fetch = async (_, options) => {
    assert.ok(!options.body.includes('test-token'));
    return Response.json({ files: { 'eyeron-playground-state.json': { raw_url: rawGistUrl } } });
  };
  await context.sharePlayground(true);
  assert.equal(session.get('gist-token'), 'test-token');
  assert.equal(fields.get('share-status').textContent, 'Gist share link copied.');
  assert.equal(fields.get('create-gist-share').disabled, false);
  assert.equal(new URL(copied[0]).searchParams.get('stateurl'), rawGistUrl);
});

test('shared state overrides local autosave and example links without refetching inputs', async () => {
  const { context, local, fields } = page();
  const token = await context.packState(snapshot);
  context.location.href = 'https://example.org/playground?example=socrates.n3#state=' + token;
  local.set('autosave', JSON.stringify({ ...snapshot, sources: [{ name: '-', text: 'local data', baseIri: null }] }));
  await context.restore();
  assert.deepEqual(plain(context.sources), snapshot.sources);
  assert.equal(fields.get('mode').value, snapshot.mode);
  assert.equal(fields.get('max-backward-depth').value, '8');
  assert.equal(context.proofText, snapshot.proof.text);
});

test('state URLs restore Gist snapshots; invalid links preserve the existing autosave', async () => {
  const { context, fields, local, saved } = page({ fetchSource: async (url) => {
    assert.equal(url, rawGistUrl);
    return { text: JSON.stringify(snapshot) };
  } });
  context.location.href = 'https://example.org/playground?stateurl=' + encodeURIComponent(rawGistUrl);
  await context.restore();
  assert.deepEqual(plain(context.sources), snapshot.sources);
  assert.equal(fields.get('share-status').textContent, 'Opened shared playground. Changes save locally.');
  const savesBefore = saved();
  local.set('autosave', 'keep this autosave');
  context.location.href = 'https://example.org/playground#state=invalid';
  await context.restore();
  assert.match(fields.get('share-status').textContent, /Could not open shared playground/);
  assert.equal(local.get('autosave'), 'keep this autosave');
  assert.equal(saved(), savesBefore);
});
