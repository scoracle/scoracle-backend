// Pin public DNS results for every browser HTTP request and HTTPS tunnel.
const http = require('node:http');
const net = require('node:net');
const dns = require('node:dns/promises');

const private4 = new net.BlockList();
for (const [address, prefix] of [['0.0.0.0',8],['10.0.0.0',8],['100.64.0.0',10],['127.0.0.0',8],
  ['169.254.0.0',16],['172.16.0.0',12],['192.0.0.0',24],['192.0.2.0',24],['192.168.0.0',16],
  ['198.18.0.0',15],['198.51.100.0',24],['203.0.113.0',24],['224.0.0.0',4],['240.0.0.0',4]]) private4.addSubnet(address,prefix,'ipv4');
function nonPublic(address) {
  if (net.isIPv4(address)) return private4.check(address,'ipv4');
  if (!net.isIPv6(address)) return true;
  const [a,b] = address.split(':').map(x => parseInt(x || '0',16));
  return (a & 0xe000) !== 0x2000 || (a === 0x2001 && (b < 0x200 || b === 0xdb8))
    || a === 0x2002 || (a === 0x3fff && b < 0x1000);
}
function createPublicProxy() {
  async function destination(url) {
    if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password) throw new Error('Invalid publisher URL');
    const host = url.hostname.replace(/^\[|\]$/g, '');
    const addresses = net.isIP(host) ? [{address: host, family: net.isIP(host)}] : await dns.lookup(host, {all: true});
    if (!addresses.length || addresses.some(({address}) => nonPublic(address))) throw new Error('Non-public publisher address');
    return {host, autoSelectFamily: true, lookup: (_, options, callback) => {
      if (options.all) callback(null, addresses);
      else callback(null, addresses[0].address, addresses[0].family);
    }};
  }
  const server = http.createServer(async (request, response) => {
    try {
      const url = new URL(request.url);
      if (url.protocol !== 'http:') throw new Error('HTTPS requires CONNECT');
      const pinned = await destination(url);
      const headers = {...request.headers, host: url.host};
      delete headers['proxy-connection'];
      const upstream = http.request({...pinned, port: url.port || 80, path: url.pathname + url.search, method: request.method, headers}, incoming => {
        response.writeHead(incoming.statusCode, incoming.headers);
        incoming.pipe(response);
      });
      upstream.setTimeout(60000, () => upstream.destroy());
      upstream.on('error', () => response.destroy());
      request.on('aborted', () => upstream.destroy());
      response.on('close', () => upstream.destroy());
      request.pipe(upstream);
    } catch (_) {
      response.writeHead(403).end();
    }
  });
  server.on('connect', async (request, socket, head) => {
    try {
      const url = new URL('https://' + request.url);
      const pinned = await destination(url);
      const upstream = net.connect({...pinned, port: Number(url.port || 443)});
      socket.setTimeout(60000, () => socket.destroy());
      upstream.setTimeout(60000, () => upstream.destroy());
      upstream.once('connect', () => {
        socket.write('HTTP/1.1 200 Connection Established\r\n\r\n');
        if (head.length) upstream.write(head);
        upstream.pipe(socket);
        socket.pipe(upstream);
      });
      upstream.on('error', () => socket.destroy());
      socket.on('error', () => upstream.destroy());
      socket.on('close', () => upstream.destroy());
      upstream.on('close', () => socket.destroy());
    } catch (_) {
      socket.end('HTTP/1.1 403 Forbidden\r\nConnection: close\r\n\r\n');
    }
  });
  server.on('clientError', (_, socket) => socket.destroy());
  return server;
}
module.exports = {createPublicProxy};
