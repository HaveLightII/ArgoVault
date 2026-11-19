const { test } = require('node:test');
const assert = require('node:assert/strict');
const { createHash, randomUUID } = require('node:crypto');
const { readFileSync } = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const source = file => readFileSync(path.join(__dirname, '..', file), 'utf8');
const manifest = JSON.parse(source('browser-extension/manifest.json'));
const extensionId = [...createHash('sha256').update(Buffer.from(manifest.key, 'base64'))
  .digest('hex').slice(0, 32)].map(n => String.fromCharCode(97 + parseInt(n, 16))).join('');
const extension = 'chrome-extension://' + extensionId + '/';
const origin = 'https://login.example.test:8443';
const page = { id: extensionId, tab: { id: 3 }, frameId: 0, origin, url: origin + '/login' };
const form = { type: 'submit', user: 'sample-user', password: 'example-password' };

function worker({ granted = true, enabled = true } = {}) {
  let onMessage;
  const windows = [];
  const chrome = {
    runtime: { id: extensionId, getURL: file => extension + file,
      onMessage: { addListener: listener => { onMessage = listener; } } },
    action: { onClicked: { addListener() {} } },
    windows: { create: async window => { windows.push(window); } },
    storage: { local: { get: async () => ({ [origin]: enabled }) } },
    permissions: { contains: async () => granted }
  };
  vm.runInNewContext(source('browser-extension/background.js'), {
    chrome, URL, crypto: { randomUUID }, setTimeout: () => 1, clearTimeout() {}
  });
  return { windows, send: (data, sender) => new Promise(resolve => onMessage(data, sender, resolve)) };
}

test('native host trusts the bundled extension ID', () => {
  assert.ok(source('src-tauri/src/browser/mod.rs').includes('"' + extension + '"'));
  assert.deepEqual(manifest.optional_host_permissions, ['https://*/*']);
  assert.equal(manifest.host_permissions, undefined);
});

test('a save requires the enabled top-level HTTPS origin and current site permission', async () => {
  for (const [sender, settings] of [
    [{ ...page, frameId: 1 }, {}],
    [{ ...page, id: 'other' }, {}],
    [{ ...page, origin: 'https://different.example.test' }, {}],
    [{ ...page, url: 'http://login.example.test', origin: 'http://login.example.test' }, {}],
    [{ ...page, url: 'https://login.example.test', origin: 'https://login.example.test' }, {}],
    [page, { granted: false }],
    [page, { enabled: false }]
  ]) {
    const instance = worker(settings);
    await instance.send(form, sender);
    assert.equal(instance.windows.length, 0);
  }
});

test('only the matching confirmation page can claim a submitted password, once', async () => {
  const instance = worker();
  await instance.send(form, page);
  await instance.send(form, page);
  assert.equal(instance.windows.length, 1);
  const url = new URL(instance.windows[0].url);
  assert.equal(url.href.split('#')[0], extension + 'confirm.html');
  assert.ok(!url.href.includes(form.password));
  const claim = { type: 'claim', id: url.hash.slice(1) };
  assert.equal(await instance.send(claim, page), null);
  assert.equal(await instance.send(claim, { url: extension + 'confirm.html#wrong' }), null);
  const received = await instance.send(claim, { url: url.href });
  assert.equal(received.url, origin);
  assert.equal(received.user, form.user);
  assert.equal(received.password, form.password);
  assert.equal(await instance.send(claim, { url: url.href }), null);
});
