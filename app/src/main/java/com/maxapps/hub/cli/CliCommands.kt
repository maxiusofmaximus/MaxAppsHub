package com.maxapps.hub.cli

import android.content.Context
import com.maxapps.hub.data.AppRegistry
import com.maxapps.hub.data.HubPrefs
import com.maxapps.hub.data.ManagedApp
import com.maxapps.hub.data.isNewerVersion
import com.maxapps.hub.install.downloadAndInstall
import com.maxapps.hub.install.getInstalledVersion
import com.maxapps.hub.network.GitHubApi
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import org.json.JSONArray
import org.json.JSONObject

class CliError(message: String) : Exception(message)

/** Descarga/instalación lanzada por `update`; la consulta `status` la muestra. */
data class UpdateJob(
    val packageId: String,
    val version: String,
    val state: String, // descargando | instalador (startActivity del instalador ya funcionó) | hecho | error
    val progress: Float,
    val error: String?
) {
    fun toJson(): JSONObject = JSONObject()
        .put("packageId", packageId)
        .put("version", version)
        .put("state", state)
        .put("percent", (progress * 100).toInt())
        .put("error", error ?: JSONObject.NULL)
}

/** Comandos del CLI por adb. Cada uno devuelve el `data` del resultado o lanza [CliError]. */
object CliCommands {

    /** El propio Hub: `update com.maxapps.hub` lo actualiza desde sus releases, igual que al resto. */
    private val hub = ManagedApp(
        name = "MaxApps Hub",
        packageId = "com.maxapps.hub",
        githubOwner = "maxiusofmaximus",
        githubRepo = "MaxAppsHub",
        icon = "📱",
        description = "El propio Hub",
        accentColorHex = 0xFF6366F1
    )

    private val managed get() = AppRegistry.apps + hub

    @Volatile
    private var lastJob: UpdateJob? = null

    suspend fun run(context: Context, cmd: String, pkg: String?, value: String?, force: Boolean = false): JSONObject =
        when (cmd) {
            "list" -> JSONObject().put("apps", checkApps(context, null))
            "check" -> JSONObject().put("apps", checkApps(context, pkg))
            "channel" -> setChannel(context, pkg, value)
            "update" -> startUpdate(context, pkg, force)
            "status" -> status(context)
            else -> throw CliError("cmd desconocido: '$cmd' (list|check|channel|update|status)")
        }

    /** `instalador` pasa a `hecho` cuando la versión instalada ya es la del trabajo. */
    private fun status(context: Context): JSONObject {
        val job = lastJob ?: return JSONObject().put("state", "idle")
        val done = job.state == "instalador" &&
            getInstalledVersion(context, job.packageId) == job.version.removePrefix("v")
        return (if (done) job.copy(state = "hecho") else job).toJson()
    }

    private suspend fun checkApps(context: Context, pkg: String?): JSONArray {
        val apps = if (pkg == null) managed else listOf(appFor(pkg))
        val rows = coroutineScope { apps.map { async { describe(context, it) } }.awaitAll() }
        return JSONArray(rows)
    }

    private suspend fun describe(context: Context, app: ManagedApp): JSONObject {
        val installed = getInstalledVersion(context, app.packageId)
        val alfa = HubPrefs.includePrereleases(context, app.packageId)
        val latest = GitHubApi.fetchRelease(app.githubOwner, app.githubRepo, alfa)
        return JSONObject()
            .put("name", app.name)
            .put("packageId", app.packageId)
            .put("installed", installed ?: JSONObject.NULL)
            .put("channel", channelName(alfa))
            .put("latest", latest?.tagName ?: JSONObject.NULL)
            .put("latestIsPrerelease", latest?.prerelease ?: false)
            .put("updateAvailable", installed != null && latest != null && isNewerVersion(latest.tagName, installed))
    }

    private fun setChannel(context: Context, pkg: String?, value: String?): JSONObject {
        val app = appFor(pkg)
        val alfa = when (value) {
            "alfa" -> true
            "estable" -> false
            else -> throw CliError("value debe ser 'alfa' o 'estable'")
        }
        HubPrefs.setIncludePrereleases(context, app.packageId, alfa)
        return JSONObject().put("packageId", app.packageId).put("channel", channelName(alfa))
    }

    /** `force` salta la comparación de versiones: instala la release del canal aunque la instalada parezca igual o mayor (p. ej. una build de pruebas 1.0.5 frente a 1.0.5-alpha.1). Android sigue rechazando bajar de versionCode. */
    private suspend fun startUpdate(context: Context, pkg: String?, force: Boolean): JSONObject {
        val app = appFor(pkg)
        val alfa = HubPrefs.includePrereleases(context, app.packageId)
        val release = GitHubApi.fetchRelease(app.githubOwner, app.githubRepo, alfa)
            ?: throw CliError("no hay APK disponible en el canal ${channelName(alfa)}")
        val installed = getInstalledVersion(context, app.packageId)
        if (!force && installed != null && !isNewerVersion(release.tagName, installed)) {
            throw CliError("ya tienes la última versión del canal (${release.tagName}); usa --force para reinstalar")
        }

        lastJob = UpdateJob(app.packageId, release.tagName, "descargando", 0f, null)
        downloadAndInstall(
            context = context,
            apkUrl = release.apkDownloadUrl,
            fileName = release.apkFileName,
            onProgress = { p -> lastJob = lastJob?.copy(progress = p) },
            onComplete = { lastJob = lastJob?.copy(state = "instalador", progress = 1f) },
            onError = { e -> lastJob = lastJob?.copy(state = "error", error = e) }
        )
        return JSONObject().put("packageId", app.packageId).put("version", release.tagName).put("started", true)
    }

    private fun appFor(pkg: String?): ManagedApp {
        val id = pkg ?: throw CliError("falta el extra 'pkg'")
        return managed.firstOrNull { it.packageId == id }
            ?: throw CliError("paquete no gestionado: $id")
    }

    private fun channelName(alfa: Boolean) = if (alfa) "alfa" else "estable"
}
