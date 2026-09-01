import './styles.css';
import { bootstrap } from './app/bootstrap';

void bootstrap()
  .then((dispose) => window.addEventListener('beforeunload', dispose, { once: true }))
  .catch((error: unknown) => {
    const status = document.querySelector<HTMLDivElement>('#status');
    const statusText = document.querySelector<HTMLSpanElement>('[data-status-text]');
    if (status) status.dataset.kind = 'error';
    if (statusText) statusText.textContent = error instanceof Error ? error.message : '桌面伴侣启动失败';
    console.error('[bootstrap]', error);
  });
