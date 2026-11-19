package com.havelight.pypasswords

import android.content.Context
import android.content.Intent
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import org.json.JSONObject
import java.util.UUID

// Only the single-use ticket crosses the activity boundary. Process death discards the login.
internal object WebsiteSave {
  private const val EXTRA = "com.havelight.pypasswords.WEBSITE_SAVE"
  private const val TTL = 90000L
  private val handler = Handler(Looper.getMainLooper())
  private var ticket: String? = null
  private var deadline = 0L
  private var login: JSONObject? = null
  private val expire = Runnable { login = null; ticket = null }

  fun stage(context: Context, url: String, pkg: String, user: String, password: String): Intent {
    handler.removeCallbacks(expire)
    val id = UUID.randomUUID().toString()
    ticket = id
    deadline = SystemClock.elapsedRealtime() + TTL
    login = JSONObject().put("url", url).put("pkg", pkg).put("user", user).put("password", password)
    handler.postDelayed(expire, TTL)
    return Intent(context, MainActivity::class.java).putExtra(EXTRA, id)
      .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
  }

  fun ticket(intent: Intent): String? = intent.getStringExtra(EXTRA)?.also { intent.removeExtra(EXTRA) }

  fun take(id: String): JSONObject? {
    if (id != ticket) return null
    val result = login.takeIf { SystemClock.elapsedRealtime() < deadline }
    expire.run()
    handler.removeCallbacks(expire)
    return result
  }
}
