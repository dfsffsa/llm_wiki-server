const tabLogin = document.getElementById('tab-login');
const tabRegister = document.getElementById('tab-register');
const form = document.getElementById('auth-form');
const submit = document.getElementById('submit-btn');
const errEl = document.getElementById('error');

let mode = 'login';
function setMode(m) {
  mode = m;
  tabLogin.classList.toggle('active', m === 'login');
  tabRegister.classList.toggle('active', m === 'register');
  submit.textContent = m === 'login' ? '登录' : '注册';
  errEl.textContent = '';
}
tabLogin.addEventListener('click', () => setMode('login'));
tabRegister.addEventListener('click', () => setMode('register'));

// /register URL defaults to register tab
if (location.pathname === '/register') setMode('register');

form.addEventListener('submit', async (e) => {
  e.preventDefault();
  errEl.textContent = '';
  submit.disabled = true;
  const data = new FormData(form);
  const body = JSON.stringify({
    email: data.get('email'),
    password: data.get('password'),
  });
  const path = mode === 'login' ? '/auth/login' : '/auth/register';
  try {
    const r = await fetch(path, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      credentials: 'same-origin',
      body,
    });
    if (r.ok) {
      location.href = '/lite/';
      return;
    }
    const d = await r.json().catch(() => ({}));
    errEl.textContent = d.error?.message || '请求失败';
  } catch (err) {
    errEl.textContent = '网络错误';
  } finally {
    submit.disabled = false;
  }
});

// already logged in -> straight to /lite/
(async () => {
  try {
    const r = await fetch('/auth/me', { credentials: 'same-origin' });
    if (r.ok) location.href = '/lite/';
  } catch {}
})();
