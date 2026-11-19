(async () => {
  el("title").textContent = msg("儲存到 ArgoVault？","Save to ArgoVault?");
  const button = el("action"), status = el("status");
  button.textContent = msg("開啟 ArgoVault","Open ArgoVault");
  el("cancel").textContent = msg("取消","Cancel");
  el("cancel").onclick = () => window.close();
  let login = await chrome.runtime.sendMessage({type:"claim",id:location.hash.slice(1)});
  function clear() { login = null; button.disabled = true; }
  function expired() { clear(); status.textContent = msg("請求已過期，請重新提交登入表單。","Request expired. Submit the login form again."); }
  if (!login || Date.now() >= login.expires) return expired();
  const deadline = login.expires;
  setTimeout(expired, deadline - Date.now());
  addEventListener("pagehide", clear, {once:true});
  el("source").textContent = login.url;
  el("user").textContent = login.user;
  button.disabled = false;
  button.onclick = async () => {
    if (!login || Date.now() >= deadline) return expired();
    button.disabled = true;
    try {
      const {url,user,password} = login;
      if (!(await chrome.storage.local.get(url))[url] ||
          !await chrome.permissions.contains({origins:["https://"+new URL(url).hostname+"/*"]})) return expired();
      if (!login || Date.now() >= deadline) return expired();
      const result = await chrome.runtime.sendNativeMessage("com.havelight.argovault", {url,user,password});
      if (!result?.ok) throw new Error();
      clear(); window.close();
    } catch {
      if (!login) return;
      status.textContent = msg("無法交給 App。請在 ArgoVault 設定中連接 Chrome／Edge，或完成目前的儲存請求後重試。","Could not reach the app. Connect Chrome / Edge in ArgoVault settings, or finish the current save request and retry.");
      button.disabled = false;
    }
  };
})().catch(() => { el("status").textContent = msg("請重新提交登入表單。","Submit the login form again."); });
