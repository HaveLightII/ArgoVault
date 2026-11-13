package com.havelight.pypasswords

import android.app.PendingIntent
import android.app.assist.AssistStructure
import android.content.Intent
import android.os.Build
import android.os.CancellationSignal
import android.service.autofill.AutofillService
import android.service.autofill.Dataset
import android.service.autofill.FillCallback
import android.service.autofill.FillRequest
import android.service.autofill.FillResponse
import android.service.autofill.SaveCallback
import android.service.autofill.SaveRequest
import android.text.InputType
import android.view.View
import android.view.autofill.AutofillId
import android.widget.RemoteViews
import androidx.annotation.RequiresApi

@RequiresApi(Build.VERSION_CODES.O)
class ArgoAutofillService : AutofillService() {

  private class Field(val id: AutofillId, val isPassword: Boolean, val userHint: Boolean)

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
    val cls = n.inputType and InputType.TYPE_MASK_CLASS
    val v = n.inputType and InputType.TYPE_MASK_VARIATION
    if (cls == InputType.TYPE_CLASS_TEXT &&
      (v == InputType.TYPE_TEXT_VARIATION_PASSWORD || v == InputType.TYPE_TEXT_VARIATION_WEB_PASSWORD)) return true
    if (cls == InputType.TYPE_CLASS_NUMBER && v == InputType.TYPE_NUMBER_VARIATION_PASSWORD) return true
    return htmlAttr(n, "type")?.equals("password", true) == true
  }

  private fun isUserHint(n: AssistStructure.ViewNode): Boolean {
    val h = n.autofillHints
    if (h?.any { it == View.AUTOFILL_HINT_USERNAME || it == View.AUTOFILL_HINT_EMAIL_ADDRESS } == true) return true
    return htmlAttr(n, "type")?.equals("email", true) == true
  }

  override fun onFillRequest(request: FillRequest, cancel: CancellationSignal, callback: FillCallback) {
    val structure = request.fillContexts.last().structure
    val pkg = structure.activityComponent?.packageName ?: ""
    if (pkg == packageName) {
      callback.onSuccess(null)
      return
    }
    val fields = ArrayList<Field>()
    var domain = ""
    var scheme = ""
    fun walk(n: AssistStructure.ViewNode) {
      val d = n.webDomain
      if (!d.isNullOrEmpty() && domain.isEmpty()) {
        domain = d
        scheme = n.webScheme ?: ""
      }
      val id = n.autofillId
      if (id != null && n.visibility == View.VISIBLE && isInput(n)) {
        fields.add(Field(id, isPassword(n), isUserHint(n)))
      }
      for (i in 0 until n.childCount) walk(n.getChildAt(i))
    }
    for (i in 0 until structure.windowNodeCount) walk(structure.getWindowNodeAt(i).rootViewNode)

    val pw = fields.firstOrNull { it.isPassword }
    if (pw == null) {
      callback.onSuccess(null)
      return
    }
    var user: Field? = fields.firstOrNull { !it.isPassword && it.userHint }
    if (user == null) {
      for (f in fields) {
        if (f === pw) break
        if (!f.isPassword) user = f
      }
    }

    val intent = Intent(this, AutofillAuthActivity::class.java)
      .putExtra("domain", domain)
      .putExtra("scheme", scheme)
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
    callback.onSuccess(FillResponse.Builder().addDataset(ds.build()).build())
  }

  override fun onSaveRequest(request: SaveRequest, callback: SaveCallback) {
    callback.onSuccess()
  }
}
