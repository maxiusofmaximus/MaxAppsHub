package com.maxapps.hub.network

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
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
    val apkSize: Long
)

/**
 * Simple client for the GitHub Releases API.
 * Uses HttpURLConnection to avoid external dependencies.
 */
object GitHubApi {

    /**
     * Fetches the latest release for a given GitHub repository.
     * Returns null if no release is found or if the request fails.
     */
    suspend fun fetchLatestRelease(owner: String, repo: String): ReleaseInfo? =
        withContext(Dispatchers.IO) {
            try {
                val url = URL("https://api.github.com/repos/$owner/$repo/releases/latest")
                val connection = url.openConnection() as HttpURLConnection
                connection.requestMethod = "GET"
                connection.setRequestProperty("Accept", "application/vnd.github+json")
                connection.setRequestProperty("User-Agent", "MaxAppsHub-Launcher")
                connection.connectTimeout = 10_000
                connection.readTimeout = 10_000

                if (connection.responseCode != 200) {
                    return@withContext null
                }

                val json = connection.inputStream.bufferedReader().use { it.readText() }
                val release = JSONObject(json)

                val tagName = release.getString("tag_name")
                val releaseName = release.optString("name", tagName)

                // Find the first APK asset
                val assets = release.getJSONArray("assets")
                for (i in 0 until assets.length()) {
                    val asset = assets.getJSONObject(i)
                    val name = asset.getString("name")
                    if (name.endsWith(".apk")) {
                        return@withContext ReleaseInfo(
                            tagName = tagName,
                            releaseName = releaseName,
                            apkDownloadUrl = asset.getString("browser_download_url"),
                            apkFileName = name,
                            apkSize = asset.getLong("size")
                        )
                    }
                }

                null
            } catch (e: Exception) {
                e.printStackTrace()
                null
            }
        }
}
