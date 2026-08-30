import { writeFile } from 'node:fs/promises';

const [socketUrl, outputPath] = process.argv.slice(2);
if (!socketUrl || !outputPath) {
  console.error('Usage: node scripts/capture-webview.mjs <devtools-websocket-url> <output.png>');
  process.exit(2);
}

const socket = new WebSocket(socketUrl);
const timeout = setTimeout(() => {
  console.error('Timed out while capturing the WebView');
  socket.close();
  process.exit(1);
}, 10_000);

socket.addEventListener('open', () => {
  socket.send(JSON.stringify({
    id: 1,
    method: 'Page.captureScreenshot',
    params: { format: 'png', fromSurface: true, captureBeyondViewport: false },
  }));
});

socket.addEventListener('message', async (event) => {
  const message = JSON.parse(event.data);
  if (message.id !== 1) return;
  clearTimeout(timeout);
  if (!message.result?.data) throw new Error(JSON.stringify(message.error ?? message));
  await writeFile(outputPath, Buffer.from(message.result.data, 'base64'));
  console.log(outputPath);
  socket.close();
});

socket.addEventListener('error', (event) => {
  clearTimeout(timeout);
  console.error(event);
  process.exit(1);
});
