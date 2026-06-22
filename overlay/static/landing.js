// Decide where the CTA points: logged in -> /lite/, otherwise /login.
(async () => {
  try {
    const r = await fetch('/auth/me', { credentials: 'same-origin' });
    if (r.ok) {
      document.getElementById('cta').href = '/lite/';
    }
  } catch {}
})();
