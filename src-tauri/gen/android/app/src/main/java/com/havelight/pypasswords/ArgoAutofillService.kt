package com.havelight.pypasswords

import android.app.PendingIntent
import android.app.assist.AssistStructure
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.os.CancellationSignal
import android.service.autofill.AutofillService
import android.service.autofill.Dataset
import android.service.autofill.FillCallback
import android.service.autofill.FillRequest
import android.service.autofill.FillResponse
import android.service.autofill.SaveCallback
import android.service.autofill.SaveInfo
import android.service.autofill.SaveRequest
import android.text.InputType
import android.view.View
import android.view.autofill.AutofillId
import android.widget.RemoteViews
import androidx.annotation.RequiresApi
import androidx.webkit.WebViewFeature
import java.net.URI
import java.security.MessageDigest

@RequiresApi(Build.VERSION_CODES.O)
class ArgoAutofillService : AutofillService() {

  private class Field(val node: AssistStructure.ViewNode, val id: AutofillId, val domain: String,
                      val scheme: String, val form: AssistStructure.ViewNode)

  private fun fields(structure: AssistStructure): List<Field> {
    val result = mutableListOf<Field>()
    fun walk(node: AssistStructure.ViewNode, domain: String, scheme: String, form: AssistStructure.ViewNode) {
      val host = node.webDomain
      val site = host ?: domain
      val protocol = if (host == null) scheme else if (Build.VERSION.SDK_INT >= 28) node.webScheme.orEmpty() else ""
      val group = if (node.htmlInfo?.tag.equals("form", true) || host != null && host != domain) node else form
      val id = node.autofillId
      if (id != null && isInput(node)) result.add(Field(node, id, site, protocol, group))
      for (i in 0 until node.childCount) walk(node.getChildAt(i), site, protocol, group)
    }
    for (i in 0 until structure.windowNodeCount) {
      val root = structure.getWindowNodeAt(i).rootViewNode
      walk(root, "", "", root)
    }
    return result
  }

  private fun website(field: Field): String? {
    if (field.domain.isBlank() || field.scheme !in listOf("", "https", "http")) return null
    val url = runCatching { URI("${field.scheme.ifEmpty { "https" }}://${field.domain}") }.getOrNull() ?: return null
    if (url.host == null || url.rawAuthority != field.domain || url.userInfo != null) return null
    return if (field.scheme.isEmpty()) field.domain else url.toString()
  }

  // 簽署憑證 SHA-256（多重簽署時排序後以逗號相接）與顯示名稱；查不到套件時回傳 null。
  @Suppress("DEPRECATION")
  private fun appInfo(pkg: String): Pair<String, String>? = runCatching {
    val signers = if (Build.VERSION.SDK_INT >= 28)
      packageManager.getPackageInfo(pkg, PackageManager.GET_SIGNING_CERTIFICATES).signingInfo?.apkContentsSigners
    else packageManager.getPackageInfo(pkg, PackageManager.GET_SIGNATURES).signatures
    val cert = signers.orEmpty().map { s ->
      MessageDigest.getInstance("SHA-256").digest(s.toByteArray()).joinToString("") { "%02x".format(it) }
    }.sorted().joinToString(",")
    val label = packageManager.getApplicationLabel(packageManager.getApplicationInfo(pkg, 0)).toString()
    if (cert.isEmpty()) null else Pair(cert, label)
  }.getOrNull()

  private fun autocomplete(n: AssistStructure.ViewNode, hint: String) =
    htmlAttr(n, "autocomplete")?.split(' ')?.any { it.equals(hint, true) } == true

  private fun htmlAttr(n: AssistStructure.ViewNode, name: String): String? =
    n.htmlInfo?.attributes?.firstOrNull { it.first.equals(name, true) }?.second

  private fun isInput(n: AssistStructure.ViewNode): Boolean {
    if (n.autofillType != View.AUTOFILL_TYPE_TEXT) return false
    val tag = n.htmlInfo?.tag
    if (tag != null) return tag.equals("input", true)
    return n.className?.contains("EditText") == true
  }

  private fun isPassword(n: AssistStructure.ViewNode): Boolean {
    if (n.autofillHints?.any { it == View.AUTOFILL_HINT_PASSWORD } == true) return true
    if (isNewPassword(n) || autocomplete(n, "current-password")) return true
    val cls = n.inputType and InputType.TYPE_MASK_CLASS
    val v = n.inputType and InputType.TYPE_MASK_VARIATION
    if (cls == InputType.TYPE_CLASS_TEXT &&
      (v == InputType.TYPE_TEXT_VARIATION_PASSWORD || v == InputType.TYPE_TEXT_VARIATION_WEB_PASSWORD)) return true
    if (cls == InputType.TYPE_CLASS_NUMBER && v == InputType.TYPE_NUMBER_VARIATION_PASSWORD) return true
    return htmlAttr(n, "type")?.equals("password", true) == true
  }

  private fun isNewPassword(n: AssistStructure.ViewNode) =
    autocomplete(n, "new-password") || n.autofillHints?.any { it == "newPassword" } == true

  private fun isUserHint(n: AssistStructure.ViewNode): Boolean {
    val h = n.autofillHints
    if (h?.any { it == View.AUTOFILL_HINT_USERNAME || it == View.AUTOFILL_HINT_EMAIL_ADDRESS } == true) return true
    return autocomplete(n, "username") || htmlAttr(n, "type")?.equals("email", true) == true
  }

  override fun onFillRequest(request: FillRequest, cancel: CancellationSignal, callback: FillCallback) {
    val structure = request.fillContexts.last().structure
    val pkg = structure.activityComponent?.packageName ?: ""
    if (pkg == packageName) {
      callback.onSuccess(null)
      return
    }
    val visible = fields(structure).filter { it.node.visibility == View.VISIBLE }
    val focused = visible.firstOrNull { it.node.isFocused }
    val form = focused?.let { f -> visible.filter { it.form === f.form } }
    val pw = form?.firstOrNull { isPassword(it.node) } ?: visible.firstOrNull { isPassword(it.node) }
    if (pw == null) {
      callback.onSuccess(null)
      return
    }
    val inputs = visible.filter { it.form === pw.form && it.domain == pw.domain && it.scheme == pw.scheme }
    val user = inputs.firstOrNull { !isPassword(it.node) && isUserHint(it.node) }
      ?: inputs.takeWhile { it !== pw }.lastOrNull { !isPassword(it.node) }

    val app = if (pw.domain.isBlank()) appInfo(pkg) else null
    val intent = Intent(this, AutofillAuthActivity::class.java)
      .putExtra("domain", pw.domain)
      .putExtra("cert", app?.first.orEmpty())
      .putExtra("label", app?.second.orEmpty())
      .putExtra("scheme", pw.scheme)
      .putExtra("pkg", pkg)
      .putExtra("passId", pw.id)
    if (user != null) intent.putExtra("userId", user.id)
    val flags = PendingIntent.FLAG_CANCEL_CURRENT or
      (if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) PendingIntent.FLAG_MUTABLE else 0)
    val sender = PendingIntent
      .getActivity(this, (System.nanoTime() and 0x7fffffff).toInt(), intent, flags)
      .intentSender

    val zh = resources.configuration.locales.get(0).language == "zh"
    val pres = RemoteViews(packageName, R.layout.autofill_item)
    pres.setTextViewText(R.id.afTitle, "ArgoVault")
    pres.setTextViewText(R.id.afSub, if (zh) "解鎖並填入帳號密碼" else "Unlock to fill credentials")
    val ds = Dataset.Builder()
    ds.setValue(pw.id, null, pres)
    if (user != null) ds.setValue(user.id, null, pres)
    ds.setAuthentication(sender)
    val response = FillResponse.Builder().addDataset(ds.build())
    val passwords = inputs.filter { isPassword(it.node) }
    val saveField = passwords.firstOrNull { isNewPassword(it.node) } ?: passwords.singleOrNull()
    val url = if (pw.domain.isBlank()) null else saveField?.let { website(it) }
    if (saveField != null && (url != null || app != null) && (focused == null || focused.form === pw.form)
      && WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) {
      val save = SaveInfo.Builder(SaveInfo.SAVE_DATA_TYPE_PASSWORD, arrayOf(saveField.id))
        .setFlags(SaveInfo.FLAG_SAVE_ON_ALL_VIEWS_INVISIBLE)
        .setDescription(
          if (url == null) (if (zh) "App：${app?.second}（$pkg）" else "App: ${app?.second} ($pkg)")
          else if (zh) "請確認 $pkg 提供的網站為 $url" else "Check the website reported by $pkg: $url")
      if (user != null) save.setOptionalIds(arrayOf(user.id))
      response.setSaveInfo(save.build()).setClientState(Bundle().apply {
        putInt("request", request.id)
        putString("url", url)
        putString("cert", app?.first)
        putString("pkg", pkg)
        putParcelable("password", saveField.id)
        putParcelable("user", user?.id)
      })
    }
    callback.onSuccess(response.build())
  }

  @Suppress("DEPRECATION")
  override fun onSaveRequest(request: SaveRequest, callback: SaveCallback) {
    val state = request.clientState
    val structure = request.fillContexts.lastOrNull { it.requestId == state?.getInt("request") }?.structure
    val pkg = structure?.activityComponent?.packageName
    val url = state?.getString("url")
    val cert = state?.getString("cert")
    if (structure != null && state != null && pkg != null && pkg != packageName && pkg == state.getString("pkg") && (url != null) != (cert != null)) {
      val fields = fields(structure)
      val pw = fields.firstOrNull { it.id == state.getParcelable<AutofillId>("password") }
      val app = if (cert != null) appInfo(pkg) else null
      // 網站：同一網站；App：欄位不屬於任何網頁，且簽章與填入請求時相同。
      fun same(f: Field) = if (cert != null) f.domain.isBlank() && app?.first == cert else website(f) == url
      if (pw != null && isPassword(pw.node) && same(pw)) {
        fun value(field: Field?) = field?.node?.autofillValue?.takeIf { it.isText }?.textValue?.toString().orEmpty()
        val password = value(pw)
        if (password.isNotEmpty()) {
          val user = fields.firstOrNull { it.id == state.getParcelable<AutofillId>("user") && it.form === pw.form && same(it) }
          val intent = WebsiteSave.stage(this, url.orEmpty(), pkg, value(user), password, cert.orEmpty(), app?.second.orEmpty())
          // App 登入後常直接結束畫面；系統是透過該畫面啟動 IntentSender，畫面已結束時會無聲失敗（Android 14 模擬器實測），故 App 來源由服務直接啟動。
          if (cert == null && Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            val flags = PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_ONE_SHOT or PendingIntent.FLAG_CANCEL_CURRENT
            callback.onSuccess(PendingIntent.getActivity(this, 0, intent, flags).intentSender)
          } else {
            startActivity(intent)
            callback.onSuccess()
          }
          return
        }
      }
    }
    val zh = resources.configuration.locales.get(0).language == "zh"
    callback.onFailure(if (zh) "無法讀取登入資料，請重新提交。" else "Could not read this login. Submit the form again.")
  }
}
