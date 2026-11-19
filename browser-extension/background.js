// Credentials exist only in the worker until claimed by the extension's confirmation window.
const pending = new Map();
const confirmUrl = chrome.runtime.getURL("confirm.html");
function forget(id) {
  const request = pending.get(id);
  if (request) { clearTimeout(request.timer); pending.delete(id); }
}

async function message(data, sender) {
  if (data?.type === "claim") {
    if (sender.url !== confirmUrl + "#" + data.id) return null;
    const item = pending.get(data.id);
    forget(data.id);
    return item && Date.now() < item.expires ? item.login : null;
  }
  if (data?.type !== "submit" || !sender.tab || sender.frameId !== 0 || sender.id !== chrome.runtime.id) return;
  const url = new URL(sender.url);
  if (url.protocol !== "https:" || sender.origin !== url.origin) return;
  if (typeof data.user !== "string" || typeof data.password !== "string" ||
      !data.password.length || data.user.length > 4096 || data.password.length > 8192) return;
  if (!(await chrome.storage.local.get(url.origin))[url.origin] ||
      !await chrome.permissions.contains({origins:["https://" + url.hostname + "/*"]})) return;
  // A page may send more than one submit event for the same navigation.
  if (pending.size >= 8 || [...pending.values()].some(item => item.tab === sender.tab.id)) return;
  const id = crypto.randomUUID();
  const expires = Date.now() + 90000;
  pending.set(id, {tab:sender.tab.id, expires, timer:setTimeout(() => forget(id), 90000),
    login:{url:url.origin, user:data.user, password:data.password, expires}});
  try {
    await chrome.windows.create({url:confirmUrl + "#" + id, type:"popup", width:420, height:310, focused:true});
  } catch { forget(id); }
}

chrome.runtime.onMessage.addListener((data, sender, reply) => {
  message(data, sender).then(reply, () => reply(null));
  return true;
});


// A regular extension window stays alive while Chrome or Edge displays its site-permission dialog.
chrome.action.onClicked.addListener(tab => {
  if (Number.isInteger(tab.id)) chrome.windows.create({
    url:chrome.runtime.getURL("popup.html") + "?tab=" + tab.id,
    type:"popup", width:420, height:290, focused:true
  }).catch(() => {});
});
