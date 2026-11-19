// Installed only for hosts the user enabled. Match the exact origin as well (including the port).
(() => {
  if (window.top !== window || globalThis.argoVaultWatching) return;
  globalThis.argoVaultWatching = true;
  const origin = location.origin;
  let enabled = false;
  chrome.storage.local.get(origin).then(state => { enabled = state[origin] === true; });
  chrome.storage.onChanged.addListener((changes, area) => {
    if (area === "local" && changes[origin]) enabled = changes[origin].newValue === true;
  });

  const edited = new WeakSet();
  document.addEventListener("input", event => {
    if (enabled && event.isTrusted && event.target instanceof HTMLInputElement && event.target.type === "password") {
      edited.add(event.target);
    }
  }, true);
  document.addEventListener("submit", event => {
    if (!enabled || !event.isTrusted || !(event.target instanceof HTMLFormElement)) return;
    const fields = [...event.target.elements].filter(field =>
      field instanceof HTMLInputElement && !field.disabled && field.getClientRects().length);
    const passwords = fields.filter(field => field.type === "password");
    // Leave ambiguous multi-password / change-password forms alone.
    if (passwords.length !== 1) return;
    const password = passwords[0];
    if (!edited.has(password) || !password.value || password.autocomplete.includes("one-time-code")) return;
    edited.delete(password);
    const users = fields.filter(field => ["email", "text", "tel"].includes(field.type));
    const username = users.find(field => field.autocomplete.split(/\s+/).includes("username"))
      || users.find(field => field.type === "email")
      || fields.slice(0, fields.indexOf(password)).reverse().find(field => users.includes(field));
    chrome.runtime.sendMessage({type:"submit", user:username?.value || "", password:password.value}).catch(() => {});
  }, true);
})();
