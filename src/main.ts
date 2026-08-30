import './styles.css';
import { bootstrap } from './app/bootstrap';
import { installLifecycle } from './app/lifecycle';

void bootstrap()
  .then(installLifecycle)
  .catch((error: unknown) => {
    const status = document.querySelector<HTMLDivElement>('#status');
    const statusText = document.querySelector<HTMLSpanElement>('[data-status-text]');
    if (status) status.dataset.kind = 'error';
    if (statusText) statusText.textContent = error instanceof Error ? error.message : '桌面伴侣启动失败';
    console.error('[bootstrap]', error);
  });

