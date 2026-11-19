(async () => {
  const button = el("action"), status = el("status");
  const tab = await chrome.tabs.get(Number(new URLSearchParams(location.search).get("tab")));
  const url = tab?.url && new URL(tab.url);
  if (!url || url.protocol !== "https:") {
    button.hidden = true;
    status.textContent = msg("請在 HTTPS 登入頁開啟此選單。", "Open this menu on an HTTPS login page.");
    return;
  }
  const origin = url.origin, pattern = "https://" + url.hostname + "/*";
  const bytes = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(origin));
  const id = "site-" + [...new Uint8Array(bytes)].map(n => n.toString(16).padStart(2,"0")).join("");
  let enabled = (await chrome.storage.local.get(origin))[origin] === true &&
    await chrome.permissions.contains({origins:[pattern]});
  el("source").textContent = origin;
  function render() {
    button.textContent = enabled ? msg("停用此網站的儲存提示","Disable save prompts for this site")
      : msg("在此網站啟用儲存提示","Enable save prompts for this site");
    button.disabled = false;
  }
  render();
  button.onclick = async () => {
    button.disabled = true;
    try {
      if (enabled) {
        await chrome.storage.local.remove(origin);
        await chrome.scripting.unregisterContentScripts({ids:[id]}).catch(() => {});
        const others = Object.keys(await chrome.storage.local.get(null));
        if (!others.some(key => new URL(key).hostname === url.hostname)) await chrome.permissions.remove({origins:[pattern]});
        enabled = false;
      } else {
        if (!await chrome.permissions.request({origins:[pattern]})) {
          status.textContent = msg("尚未授權此網站。","Site permission was not granted.");
          return;
        }
        const scripts = await chrome.scripting.getRegisteredContentScripts({ids:[id]});
        if (!scripts.length) await chrome.scripting.registerContentScripts([{id,matches:[pattern],js:["content.js"],runAt:"document_start"}]);
        await chrome.storage.local.set({[origin]:true});
        enabled = true;
        await chrome.scripting.executeScript({target:{tabId:tab.id},files:["content.js"]});
      }
      status.textContent = enabled ? msg("下次提交登入表單時會提示儲存。","A prompt will appear when you submit a login form.") : "";
    } catch {
      status.textContent = msg("請重新載入網站，再開啟此選單確認設定。","Reload the site and reopen this menu to check the setting.");
    } finally { render(); }
  };
})().catch(() => { el("status").textContent = msg("無法讀取目前分頁。","Could not read this tab."); });
