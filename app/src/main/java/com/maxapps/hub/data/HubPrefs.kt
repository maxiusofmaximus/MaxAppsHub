package com.maxapps.hub.data

import android.content.Context

/** Preferencias persistentes del Hub. Hoy: el canal (estable/alfa) de cada app. */
object HubPrefs {
    private const val FILE = "maxappshub"

    fun includePrereleases(context: Context, packageId: String): Boolean =
        prefs(context).getBoolean(alphaKey(packageId), false)

    fun setIncludePrereleases(context: Context, packageId: String, value: Boolean) {
        prefs(context).edit().putBoolean(alphaKey(packageId), value).apply()
    }

    private fun prefs(context: Context) = context.getSharedPreferences(FILE, Context.MODE_PRIVATE)

    private fun alphaKey(packageId: String) = "alpha_$packageId"
}
