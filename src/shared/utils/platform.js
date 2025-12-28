let cachedPlatform = null;

function normalizePlatform(raw) {
  const value = String(raw || '').toLowerCase();
  if (value === 'darwin' || value === 'macos' || value === 'mac') return 'macos';
  if (value === 'win32' || value === 'windows') return 'windows';
  if (value === 'linux') return 'linux';
  return value || 'unknown';
}

function detectFromUserAgent() {
  const ua = String(globalThis?.navigator?.userAgent || '').toLowerCase();
  if (ua.includes('mac os x') || ua.includes('macintosh')) return 'macos';
  if (ua.includes('windows')) return 'windows';
  if (ua.includes('linux')) return 'linux';
  return 'unknown';
}

export async function getPlatform() {
  if (cachedPlatform) return cachedPlatform;

  cachedPlatform = detectFromUserAgent();
  return cachedPlatform;
}

export async function isWindows() {
  return (await getPlatform()) === 'windows';
}

export async function isMacOS() {
  return (await getPlatform()) === 'macos';
}
