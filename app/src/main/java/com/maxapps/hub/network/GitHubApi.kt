package com.maxapps.hub.network

import com.maxapps.hub.data.Version
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL

/**
 * Represents a GitHub release with its download info.
 */
data class ReleaseInfo(
    val tagName: String,
    val releaseName: String,
    val apkDownloadUrl: String,
    val apkFileName: String,
    val apkSize: Long,
    val prerelease: Boolean
)

/**
 * Simple client for the GitHub Releases API.
 * Uses HttpURLConnection to avoid external dependencies.
 */
object GitHubApi {

    /**
     * Canal estable: la release "latest" de GitHub (nunca una prerelease).
     * Canal alfa: la de mayor versión semver entre las no borradores, prereleases incluidas.
     * Devuelve null si no hay una release con APK o si la petición falla.
     */
    suspend fun fetchRelease(owner: String, repo: String, includePrereleases: Boolean): ReleaseInfo? =
        withContext(Dispatchers.IO) {
            try {
                val base = "https://api.github.com/repos/$owner/$repo/releases"
                if (includePrereleases) {
                    newestRelease(JSONArray(get("$base?per_page=15") ?: return@withContext null))
                } else {
                    get("$base/latest")?.let { JSONObject(it).toReleaseInfo() }
                }
            } catch (e: Exception) {
                e.printStackTrace()
                null
            }
        }

    private fun get(url: String): String? {
        val connection = URL(url).openConnection() as HttpURLConnection
        connection.requestMethod = "GET"
        connection.setRequestProperty("Accept", "application/vnd.github+json")
        connection.setRequestProperty("User-Agent", "MaxAppsHub-Launcher")
        connection.connectTimeout = 10_000
        connection.readTimeout = 10_000
        if (connection.responseCode != 200) return null
        return connection.inputStream.bufferedReader().use { it.readText() }
    }

    private fun newestRelease(releases: JSONArray): ReleaseInfo? =
        (0 until releases.length())
            .map { releases.getJSONObject(it) }
            .filter { !it.optBoolean("draft", false) }
            .mapNotNull { json -> json.toReleaseInfo()?.let { r -> Version.parse(r.tagName)?.let { it to r } } }
            .maxByOrNull { it.first }?.second

    /** Null si la release no tiene ningún asset .apk. */
    private fun JSONObject.toReleaseInfo(): ReleaseInfo? {
        val tagName = getString("tag_name")
        val assets = getJSONArray("assets")
        for (i in 0 until assets.length()) {
            val asset = assets.getJSONObject(i)
            val name = asset.getString("name")
            if (name.endsWith(".apk")) {
                return ReleaseInfo(
                    tagName = tagName,
                    releaseName = optString("name", tagName),
                    apkDownloadUrl = asset.getString("browser_download_url"),
                    apkFileName = name,
                    apkSize = asset.getLong("size"),
                    prerelease = optBoolean("prerelease", false)
                )
            }
        }
        return null
    }
}
