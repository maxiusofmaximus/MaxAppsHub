package com.maxapps.hub

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log
import com.maxapps.hub.cli.CliCommands
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import org.json.JSONObject
import java.io.File

/**
 * Control por adb, sin tocar la pantalla (ver README, sección «CLI»).
 * Va protegido con android.permission.DUMP: solo `adb shell` lo posee.
 * Cada comando escribe su resultado en cli-result.json (ver [writeResult]).
 */
class CliReceiver : BroadcastReceiver() {

    override fun onReceive(context: Context, intent: Intent) {
        val pending = goAsync()
        val cmd = intent.getStringExtra("cmd").orEmpty()
        CoroutineScope(Dispatchers.IO).launch {
            try {
                val result = runCatching {
                    CliCommands.run(context, cmd, intent.getStringExtra("pkg"), intent.getStringExtra("value"))
                }
                writeResult(context, cmd, result)
            } catch (e: Exception) {
                Log.e(TAG, "no se pudo escribir el resultado de '$cmd'", e)
            } finally {
                pending.finish()
            }
        }
    }

    /** Escribe en un temporal y lo renombra: el cliente nunca lee un JSON a medio escribir. */
    private fun writeResult(context: Context, cmd: String, result: Result<JSONObject>) {
        val json = JSONObject()
            .put("cmd", cmd)
            .put("ok", result.isSuccess)
            .put("data", result.getOrNull() ?: JSONObject.NULL)
            .put("error", result.exceptionOrNull()?.message ?: JSONObject.NULL)
            .put("t", System.currentTimeMillis())
        Log.i(TAG, json.toString())
        val dir = context.getExternalFilesDir(null) ?: context.filesDir
        val tmp = File(dir, "$RESULT_FILE.tmp")
        tmp.writeText(json.toString())
        tmp.renameTo(File(dir, RESULT_FILE))
    }

    private companion object {
        const val TAG = "MaxAppsHubCli"
        const val RESULT_FILE = "cli-result.json"
    }
}
