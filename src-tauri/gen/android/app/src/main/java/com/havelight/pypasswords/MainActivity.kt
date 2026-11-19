package com.havelight.pypasswords

import android.content.Intent
import android.os.Bundle
import android.os.FileObserver
import android.view.View
import android.view.ViewGroup
import android.view.WindowManager
import android.webkit.WebView
import android.widget.Toast
import androidx.webkit.JavaScriptReplyProxy
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import androidx.activity.OnBackPressedCallback
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import java.io.File

class MainActivity : TauriActivity() {
  private val observers = mutableListOf<FileObserver>()

  private var saveChannel: JavaScriptReplyProxy? = null
  private var saveTicket: String? = null
  private var delivered = false
  private var foreground = false

  private fun acceptSave(intent: Intent) {
    val id = WebsiteSave.ticket(intent) ?: return
    saveTicket = id
    delivered = false
    applySecure()
    signalSave()
  }

  // A resumed Activity can still have a hidden WebView. Wait for the page to claim
  // the request after visibilitychange before handing it the credentials.
  private fun signalSave() {
    if (foreground && !delivered && saveTicket != null) saveChannel?.postMessage("pending")
  }

  private fun deliverSave() {
    val channel = saveChannel ?: return
    val id = saveTicket ?: return
    if (!foreground || delivered) return
    val login = WebsiteSave.take(id)
    if (login == null) {
      val zh = resources.configuration.locales[0].language == "zh"
      Toast.makeText(this, if (zh) "儲存請求已失效，請重新提交網站表單。" else "Save request expired. Submit the form again.", Toast.LENGTH_LONG).show()
      clearSave()
      return
    }
    delivered = true
    channel.postMessage(login.put("id", id).toString())
  }

  private fun clearSave() {
    saveTicket?.let { WebsiteSave.take(it) }
    saveTicket = null
    delivered = false
    applySecure()
  }

  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    acceptSave(intent)
  }

  override fun onResume() {
    super.onResume()
    foreground = true
    signalSave()
  }

  override fun onStop() {
    foreground = false
    if (saveTicket != null) {
      saveChannel?.postMessage("{}")
      clearSave()
    }
    super.onStop()
  }

  override fun onWebViewCreate(webView: WebView) {
    webView.addJavascriptInterface(PwBio(this, webView), "PwBio")
    if (WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) {
      val origins = setOf("http://tauri.localhost", "https://tauri.localhost")
      WebViewCompat.addWebMessageListener(webView, "WebsiteSave", origins) { _, message, _, mainFrame, channel ->
        if (mainFrame) {
          when (message.data) {
            "ready" -> { saveChannel = channel; signalSave() }
            "claim" -> if (channel === saveChannel) deliverSave()
            saveTicket -> clearSave()
          }
        }
      }
    }
    super.onWebViewCreate(webView)
  }

  private fun flagFiles() = listOf(File(dataDir, "secure.flag"), File(filesDir, "secure.flag"))

  private fun applySecure() {
    val on = flagFiles().any {
      try { it.exists() && it.readText().trim() == "1" } catch (e: Exception) { false }
    }
    runOnUiThread {
      if (on || saveTicket != null) window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
      else window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
    }
  }

  private fun watch(dir: File) {
    @Suppress("DEPRECATION")
    val observer = object : FileObserver(
      dir.path,
      FileObserver.CLOSE_WRITE or FileObserver.MOVED_TO or FileObserver.CREATE
    ) {
      override fun onEvent(event: Int, path: String?) {
        if (path == "secure.flag") applySecure()
      }
    }
    observer.startWatching()
    observers.add(observer)
  }

  private fun findWebView(v: View): WebView? {
    if (v is WebView) return v
    if (v is ViewGroup) {
      for (i in 0 until v.childCount) {
        val r = findWebView(v.getChildAt(i))
        if (r != null) return r
      }
    }
    return null
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    acceptSave(intent)
    applySecure()
    watch(dataDir)
    if (filesDir.path != dataDir.path) watch(filesDir)
    val root = findViewById<View>(android.R.id.content)
    ViewCompat.setOnApplyWindowInsetsListener(root) { view, insets ->
      val types = WindowInsetsCompat.Type.systemBars() or
        WindowInsetsCompat.Type.displayCutout() or
        WindowInsetsCompat.Type.ime()
      val bars = insets.getInsets(types)
      view.setPadding(bars.left, bars.top, bars.right, bars.bottom)
      WindowInsetsCompat.CONSUMED
    }
    ViewCompat.requestApplyInsets(root)
    onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
      override fun handleOnBackPressed() {
        val wv = findWebView(root)
        if (wv == null) { finish(); return }
        wv.evaluateJavascript("(function(){return window.__pwBack?window.__pwBack():false})()") { r ->
          if (r != "true") finish()
        }
      }
    })
  }

  override fun onDestroy() {
    observers.forEach { it.stopWatching() }
    observers.clear()
    super.onDestroy()
  }
}
