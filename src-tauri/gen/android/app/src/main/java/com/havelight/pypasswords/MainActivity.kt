package com.havelight.pypasswords

import android.os.Bundle
import android.os.FileObserver
import android.view.View
import android.view.ViewGroup
import android.view.WindowManager
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import java.io.File

class MainActivity : TauriActivity() {
  private val observers = mutableListOf<FileObserver>()

  override fun onWebViewCreate(webView: WebView) {
    webView.addJavascriptInterface(PwBio(this, webView), "PwBio")
    super.onWebViewCreate(webView)
  }

  private fun flagFiles() = listOf(File(dataDir, "secure.flag"), File(filesDir, "secure.flag"))

  private fun applySecure() {
    val on = flagFiles().any {
      try { it.exists() && it.readText().trim() == "1" } catch (e: Exception) { false }
    }
    runOnUiThread {
      if (on) window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
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
