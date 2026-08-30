const socketUrl = process.argv[2];
if (!socketUrl) {
  console.error('Usage: node scripts/audit-webview.mjs <devtools-websocket-url>');
  process.exit(2);
}

const socket = new WebSocket(socketUrl);
const diagnostics = [];
const timeout = setTimeout(() => {
  console.error('Timed out while reading the WebView');
  socket.close();
  process.exit(1);
}, 10_000);

socket.addEventListener('open', () => {
  socket.send(JSON.stringify({ id: 1, method: 'Runtime.enable' }));
  socket.send(JSON.stringify({
    id: 2,
    method: 'Runtime.evaluate',
    params: {
      returnByValue: true,
      expression: `(() => {
        const canvas = document.querySelector('#character-canvas');
        const status = document.querySelector('#status');
        const gl = canvas?.getContext('webgl2') ?? canvas?.getContext('webgl');
        let alphaPixels = null;
        let maxAlpha = null;
        let glError = null;
        if (gl && canvas) {
          const pixels = new Uint8Array(canvas.width * canvas.height * 4);
          gl.readPixels(0, 0, canvas.width, canvas.height, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
          alphaPixels = 0;
          maxAlpha = 0;
          for (let index = 3; index < pixels.length; index += 4) {
            if (pixels[index] > 0) alphaPixels += 1;
            if (pixels[index] > maxAlpha) maxAlpha = pixels[index];
          }
          glError = gl.getError();
        }
        return {
          title: document.title,
          statusText: document.querySelector('[data-status-text]')?.textContent,
          statusKind: status?.dataset.kind ?? null,
          statusHidden: status?.classList.contains('is-hidden') ?? false,
          canvas: canvas ? { width: canvas.width, height: canvas.height, cssWidth: canvas.clientWidth, cssHeight: canvas.clientHeight } : null,
          webgl: gl ? { version: gl.getParameter(gl.VERSION), renderer: gl.getParameter(gl.RENDERER), alphaPixels, maxAlpha, glError } : null,
          resources: performance.getEntriesByType('resource').map(entry => ({ name: entry.name, bytes: entry.transferSize })).filter(entry => entry.name.includes('LinGuang')),
        };
      })()`,
    },
  }));
});

socket.addEventListener('message', (event) => {
  const message = JSON.parse(event.data);
  if (message.method === 'Runtime.exceptionThrown' || message.method === 'Runtime.consoleAPICalled' || message.method === 'Log.entryAdded') {
    diagnostics.push(message.params);
  }
  if (message.id !== 2) return;
  clearTimeout(timeout);
  console.log(JSON.stringify({ result: message.result?.result?.value ?? message.result, diagnostics }, null, 2));
  socket.close();
});

socket.addEventListener('error', (event) => {
  clearTimeout(timeout);
  console.error(event);
  process.exit(1);
});
