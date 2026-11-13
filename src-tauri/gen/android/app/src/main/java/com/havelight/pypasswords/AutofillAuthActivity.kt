package com.havelight.pypasswords

import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.util.Base64
import android.view.WindowManager
import android.view.autofill.AutofillId
import android.view.autofill.AutofillManager
import android.view.autofill.AutofillValue
import android.webkit.JavascriptInterface
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import android.service.autofill.Dataset
import android.widget.FrameLayout
import android.widget.RemoteViews
import androidx.activity.OnBackPressedCallback
import androidx.annotation.RequiresApi
import androidx.appcompat.app.AppCompatActivity
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.webkit.WebViewAssetLoader
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

@RequiresApi(Build.VERSION_CODES.O)
class AutofillAuthActivity : AppCompatActivity() {
  private lateinit var web: WebView
  private var passId: AutofillId? = null
  private var userId: AutofillId? = null

  @Suppress("DEPRECATION")
  private fun idExtra(name: String): AutofillId? =
    if (Build.VERSION.SDK_INT >= 33) intent.getParcelableExtra(name, AutofillId::class.java)
    else intent.getParcelableExtra<AutofillId>(name)

  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    window.setFlags(WindowManager.LayoutParams.FLAG_SECURE, WindowManager.LayoutParams.FLAG_SECURE)
    passId = idExtra("passId")
    userId = idExtra("userId")
    if (passId == null) {
      setResult(RESULT_CANCELED)
      finish()
      return
    }
    val req = JSONObject()
      .put("domain", intent.getStringExtra("domain") ?: "")
      .put("scheme", intent.getStringExtra("scheme") ?: "")
      .put("pkg", intent.getStringExtra("pkg") ?: "")

    web = WebView(this)
    val s = web.settings
    s.javaScriptEnabled = true
    s.domStorageEnabled = true
    s.allowFileAccess = false
    s.allowContentAccess = false
    s.setSupportZoom(false)
    val loader = WebViewAssetLoader.Builder()
      .addPathHandler("/assets/", WebViewAssetLoader.AssetsPathHandler(this))
      .build()
    web.webViewClient = object : WebViewClient() {
      override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? =
        loader.shouldInterceptRequest(request.url)

      override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean = true
    }
    web.addJavascriptInterface(PwAF(this, req.toString()), "PwAF")
    web.addJavascriptInterface(PwBio(this, web), "PwBio")

    val root = FrameLayout(this)
    root.addView(web, FrameLayout.LayoutParams(-1, -1))
    setContentView(root)
    ViewCompat.setOnApplyWindowInsetsListener(root) { v, insets ->
      val b = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.ime())
      v.setPadding(b.left, b.top, b.right, b.bottom)
      insets
    }
    onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
      override fun handleOnBackPressed() { cancelFlow() }
    })
    web.loadUrl("https://appassets.androidplatform.net/assets/argovault-af.html")
  }

  fun cancelFlow() {
    setResult(RESULT_CANCELED)
    finish()
  }

  fun deliver(user: String, pass: String) {
    val pid = passId ?: return cancelFlow()
    val pres = RemoteViews(packageName, android.R.layout.simple_list_item_1)
    pres.setTextViewText(android.R.id.text1, "ArgoVault")
    val b = Dataset.Builder()
    b.setValue(pid, AutofillValue.forText(pass), pres)
    val uid = userId
    if (uid != null && user.isNotEmpty()) b.setValue(uid, AutofillValue.forText(user), pres)
    setResult(RESULT_OK, Intent().putExtra(AutofillManager.EXTRA_AUTHENTICATION_RESULT, b.build()))
    finish()
  }

  override fun onDestroy() {
    if (::web.isInitialized) {
      web.removeJavascriptInterface("PwAF")
      web.removeJavascriptInterface("PwBio")
      web.destroy()
    }
    super.onDestroy()
  }
}

@RequiresApi(Build.VERSION_CODES.O)
class PwAF(private val act: AutofillAuthActivity, private val reqJson: String) {
  private val dataDir = act.applicationInfo.dataDir
  private val vaultDir = File(dataDir, "vault")

  @JavascriptInterface
  fun request(): String = reqJson

  @JavascriptInterface
  fun profiles(): String {
    val arr = JSONArray()
    vaultDir.listFiles()
      ?.filter { it.isFile && it.name.endsWith(".bin") }
      ?.map { it.name.removeSuffix(".bin") }
      ?.sorted()
      ?.forEach { arr.put(it) }
    return arr.toString()
  }

  @JavascriptInterface
  fun read(name: String, ext: String): String {
    if (ext != ".bin" && ext != ".bin.bak") return ""
    if (name.isEmpty() || name.contains('/') || name.contains('\\') || name.contains("..")) return ""
    val f = File(vaultDir, name + ext)
    return try {
      if (f.isFile && f.length() < 64L * 1024 * 1024) Base64.encodeToString(f.readBytes(), Base64.NO_WRAP) else ""
    } catch (e: Exception) {
      ""
    }
  }

  @JavascriptInterface
  fun settings(): String = try {
    val f = File(dataDir, "settings.json")
    if (f.isFile) f.readText() else "{}"
  } catch (e: Exception) {
    "{}"
  }

  @JavascriptInterface
  fun fill(user: String, pass: String) {
    act.runOnUiThread { act.deliver(user, pass) }
  }

  @JavascriptInterface
  fun cancel() {
    act.runOnUiThread { act.cancelFlow() }
  }
}
