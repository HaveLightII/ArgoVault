package com.havelight.pypasswords

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.Settings
import android.view.autofill.AutofillManager
import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import android.util.Base64
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.biometric.BiometricManager
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import java.security.KeyStore
import java.security.MessageDigest
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

class PwBio(private val activity: FragmentActivity, private val webView: WebView) {
  private val prefs = activity.getSharedPreferences("pw_bio", Context.MODE_PRIVATE)
  private val strong = BiometricManager.Authenticators.BIOMETRIC_STRONG

  private fun alias(profile: String): String {
    val d = MessageDigest.getInstance("SHA-256").digest(profile.toByteArray(Charsets.UTF_8))
    return "pwbio_" + d.joinToString("") { "%02x".format(it) }.substring(0, 32)
  }

  private fun keyStore(): KeyStore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }

  private fun createKey(alias: String): SecretKey {
    val b = KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
      .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
      .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
      .setKeySize(256)
      .setUserAuthenticationRequired(true)
      .setInvalidatedByBiometricEnrollment(true)
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
      b.setUserAuthenticationParameters(0, KeyProperties.AUTH_BIOMETRIC_STRONG)
    } else {
      @Suppress("DEPRECATION")
      b.setUserAuthenticationValidityDurationSeconds(-1)
    }
    val g = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
    g.init(b.build())
    return g.generateKey()
  }

  private fun reply(id: String, code: String, data: String = "") {
    val js = "window.__pwBioDone&&window.__pwBioDone(\"$id\",\"$code\",\"$data\")"
    activity.runOnUiThread { webView.evaluateJavascript(js, null) }
  }

  private fun prompt(
    title: String,
    cancel: String,
    cipher: Cipher,
    onOk: (Cipher) -> Unit,
    onFail: (String, String) -> Unit
  ) {
    val cb = object : BiometricPrompt.AuthenticationCallback() {
      override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
        val c = result.cryptoObject?.cipher
        if (c == null) onFail("error", "nocrypto") else onOk(c)
      }

      override fun onAuthenticationError(errorCode: Int, errString: CharSequence) {
        val code = when (errorCode) {
          BiometricPrompt.ERROR_USER_CANCELED,
          BiometricPrompt.ERROR_NEGATIVE_BUTTON,
          BiometricPrompt.ERROR_CANCELED -> "cancel"
          BiometricPrompt.ERROR_LOCKOUT,
          BiometricPrompt.ERROR_LOCKOUT_PERMANENT -> "lockout"
          else -> "error"
        }
        onFail(code, errorCode.toString())
      }
    }
    val info = BiometricPrompt.PromptInfo.Builder()
      .setTitle(title)
      .setAllowedAuthenticators(strong)
      .setNegativeButtonText(cancel)
      .setConfirmationRequired(false)
      .build()
    BiometricPrompt(activity, ContextCompat.getMainExecutor(activity), cb)
      .authenticate(info, BiometricPrompt.CryptoObject(cipher))
  }

  @JavascriptInterface
  fun status(): String = when (BiometricManager.from(activity).canAuthenticate(strong)) {
    BiometricManager.BIOMETRIC_SUCCESS -> "ok"
    BiometricManager.BIOMETRIC_ERROR_NONE_ENROLLED -> "unenrolled"
    BiometricManager.BIOMETRIC_ERROR_NO_HARDWARE -> "nohw"
    BiometricManager.BIOMETRIC_ERROR_HW_UNAVAILABLE -> "unavail"
    else -> "unsupported"
  }

  @JavascriptInterface
  fun has(profile: String): Boolean {
    val a = alias(profile)
    return try { prefs.contains(a) && keyStore().containsAlias(a) } catch (e: Exception) { false }
  }

  @JavascriptInterface
  fun remove(profile: String) {
    val a = alias(profile)
    prefs.edit().remove(a).apply()
    try { keyStore().deleteEntry(a) } catch (e: Exception) { }
  }

  @JavascriptInterface
  fun enroll(profile: String, secretB64: String, title: String, cancel: String, id: String) {
    activity.runOnUiThread {
      val secret = try {
        Base64.decode(secretB64, Base64.NO_WRAP)
      } catch (e: Exception) {
        reply(id, "error", "badinput"); return@runOnUiThread
      }
      val a = alias(profile)
      try {
        remove(profile)
        val key = createKey(a)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, key)
        prompt(title, cancel, cipher, { c ->
          try {
            val ct = c.doFinal(secret)
            val v = Base64.encodeToString(c.iv, Base64.NO_WRAP) + ":" + Base64.encodeToString(ct, Base64.NO_WRAP)
            prefs.edit().putString(a, v).apply()
            reply(id, "ok")
          } catch (e: Exception) {
            remove(profile)
            reply(id, "error", e.javaClass.simpleName)
          } finally {
            secret.fill(0)
          }
        }, { code, d ->
          secret.fill(0)
          remove(profile)
          reply(id, code, d)
        })
      } catch (e: Exception) {
        secret.fill(0)
        remove(profile)
        reply(id, "error", e.javaClass.simpleName)
      }
    }
  }

  @JavascriptInterface
  fun unlock(profile: String, title: String, cancel: String, id: String) {
    activity.runOnUiThread {
      val a = alias(profile)
      val stored = prefs.getString(a, null)
      if (stored == null || !has(profile)) { reply(id, "missing"); return@runOnUiThread }
      val parts = stored.split(":")
      if (parts.size != 2) { remove(profile); reply(id, "missing"); return@runOnUiThread }
      try {
        val key = keyStore().getKey(a, null) as SecretKey
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, Base64.decode(parts[0], Base64.NO_WRAP)))
        prompt(title, cancel, cipher, { c ->
          try {
            val pt = c.doFinal(Base64.decode(parts[1], Base64.NO_WRAP))
            reply(id, "ok", Base64.encodeToString(pt, Base64.NO_WRAP))
            pt.fill(0)
          } catch (e: Exception) {
            reply(id, "error", e.javaClass.simpleName)
          }
        }, { code, d -> reply(id, code, d) })
      } catch (e: KeyPermanentlyInvalidatedException) {
        remove(profile)
        reply(id, "invalidated")
      } catch (e: Exception) {
        reply(id, "error", e.javaClass.simpleName)
      }
    }
  }

  @JavascriptInterface
  fun afEnabled(): Boolean {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return false
    return try {
      activity.getSystemService(AutofillManager::class.java)?.hasEnabledAutofillServices() == true
    } catch (e: Exception) {
      false
    }
  }

  @JavascriptInterface
  fun afSettings() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
    activity.runOnUiThread {
      try {
        activity.startActivity(
          Intent(Settings.ACTION_REQUEST_SET_AUTOFILL_SERVICE, Uri.parse("package:" + activity.packageName))
        )
      } catch (e: Exception) {
      }
    }
  }
}
