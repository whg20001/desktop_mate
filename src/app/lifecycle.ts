export function installLifecycle(dispose: () => void): void {
  window.addEventListener('beforeunload', dispose, { once: true });
}

