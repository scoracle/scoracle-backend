// Keep the existing reader service private to the local fleet.
const path = require('node:path');
const runtime = process.env.READER_RUNTIME_DIR;
if (!runtime) throw new Error('READER_RUNTIME_DIR is required');
const {createPublicProxy} = require('./reader-proxy.cjs');
createPublicProxy().listen(8302, '127.0.0.1');
process.env.NODE_ENV = 'production';
process.env.NO_PROXY = '';
process.env.no_proxy = '';
process.env.READER_CHROME_EXECUTABLE ||= require(path.join(runtime, 'node_modules/puppeteer')).executablePath();
process.env.OVERRIDE_CHROME_EXECUTABLE_PATH = path.join(__dirname, 'reader-chrome.sh');
const server = require(path.join(runtime, 'build/stand-alone/crawl.js')).default;
server.serviceReady().then(() => server.httpServer.listen(8301, '127.0.0.1')).catch(error => {
  console.error(error);
  process.exitCode = 1;
});
