const chinese = chrome.i18n.getUILanguage().startsWith("zh");
document.documentElement.lang = chinese ? "zh-Hant" : "en";
const msg = (zh, en) => chinese ? zh : en;
const el = id => document.getElementById(id);
