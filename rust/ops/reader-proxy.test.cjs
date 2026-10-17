const test = require('node:test');
const assert = require('node:assert/strict');
const http = require('node:http');
const net = require('node:net');
const dns = require('node:dns/promises');
const {createPublicProxy} = require('./reader-proxy.cjs');

test('browser egress refuses private literals, private DNS, credentials and non-HTTP protocols', async () => {
  const tripwire = net.createServer(socket => { socket.destroy(); assert.fail('Reached private network'); });
  await new Promise(resolve => tripwire.listen(0, '127.0.0.1', resolve));
  const proxy = createPublicProxy();
  await new Promise(resolve => proxy.listen(0, '127.0.0.1', resolve));
  const originalLookup = dns.lookup;
  dns.lookup = async () => [{address: '8.8.8.8'}, {address: '127.0.0.1'}];
  function request(path, method = 'GET') {
    return new Promise((resolve, reject) => {
      const request = http.request({host:'127.0.0.1', port:proxy.address().port, method, path});
      request.on('connect', (response, socket) => {socket.destroy(); resolve(response.statusCode);});
      request.on('response', response => {response.resume(); resolve(response.statusCode);});
      request.on('error', error => {error.message = path + ': ' + error.message; reject(error);});
      request.setTimeout(3000, () => request.destroy(new Error('Proxy timeout')));
      request.end();
    });
  }
  try {
    for (const path of [
      `http://127.0.0.1:${tripwire.address().port}/`, 'http://10.0.0.1/',
      'http://169.254.169.254/', 'http://[::1]/', `http://[::ffff:127.0.0.1]:${tripwire.address().port}/`, 'http://[2001:db8::1]/',
      'http://attacker.example/', 'http://user:password@example.com/', 'file:///etc/passwd'
    ]) assert.equal(await request(path), 403, path);
    assert.equal(await request(`127.0.0.1:${tripwire.address().port}`, 'CONNECT'), 403);
    assert.equal(await request('attacker.example:443', 'CONNECT'), 403);
  } finally {
    dns.lookup = originalLookup;
    await new Promise(resolve => proxy.close(resolve));
    await new Promise(resolve => tripwire.close(resolve));
  }
});
