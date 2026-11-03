package com.maxapps.hub.install

import android.app.DownloadManager
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import android.util.Log
import android.widget.Toast
import androidx.core.content.FileProvider
import java.io.File

/**
 * Gets the installed version of a package, or null if not installed.
 */
fun getInstalledVersion(context: Context, packageId: String): String? {
    return try {
        val info = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            context.packageManager.getPackageInfo(packageId, PackageManager.PackageInfoFlags.of(0))
        } else {
            @Suppress("DEPRECATION")
            context.packageManager.getPackageInfo(packageId, 0)
        }
        info.versionName
    } catch (e: PackageManager.NameNotFoundException) {
        null
    }
}

/**
 * Downloads an APK using DownloadManager and triggers installation when complete.
 */
fun downloadAndInstall(
    context: Context,
    apkUrl: String,
    fileName: String,
    onProgress: (Float) -> Unit,
    onComplete: () -> Unit,
    onError: (String) -> Unit
) {
    try {
        // Use a more accessible directory for external apps
        val downloadDir = context.getExternalFilesDir(null) ?: context.filesDir
        val targetFile = File(downloadDir, fileName)
        if (targetFile.exists()) {
            targetFile.delete()
        }

        val request = DownloadManager.Request(Uri.parse(apkUrl))
            .setTitle(fileName)
            .setDescription("Descargando actualización…")
            .setDestinationUri(Uri.fromFile(targetFile))
            .setNotificationVisibility(DownloadManager.Request.VISIBILITY_VISIBLE)

        val downloadManager = context.getSystemService(Context.DOWNLOAD_SERVICE) as DownloadManager
        val downloadId = downloadManager.enqueue(request)

        // Monitor download progress
        val receiver = object : BroadcastReceiver() {
            override fun onReceive(ctx: Context, intent: Intent) {
                val id = intent.getLongExtra(DownloadManager.EXTRA_DOWNLOAD_ID, -1)
                if (id == downloadId) {
                    context.unregisterReceiver(this)

                    val query = DownloadManager.Query().setFilterById(downloadId)
                    val cursor = downloadManager.query(query)
                    if (cursor.moveToFirst()) {
                        val statusIdx = cursor.getColumnIndex(DownloadManager.COLUMN_STATUS)
                        val status = cursor.getInt(statusIdx)
                        if (status == DownloadManager.STATUS_SUCCESSFUL) {
                            onProgress(1f)
                            // onComplete solo si el instalador se abrió de verdad
                            installApk(context, targetFile, onLaunched = onComplete, onError = onError)
                        } else {
                            onError("La descarga falló")
                        }
                    }
                    cursor.close()
                }
            }
        }

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            context.registerReceiver(
                receiver,
                IntentFilter(DownloadManager.ACTION_DOWNLOAD_COMPLETE),
                Context.RECEIVER_EXPORTED
            )
        } else {
            @Suppress("UnspecifiedRegisterReceiverFlag")
            context.registerReceiver(
                receiver,
                IntentFilter(DownloadManager.ACTION_DOWNLOAD_COMPLETE)
            )
        }

        // Poll for progress updates
        Thread {
            var downloading = true
            while (downloading) {
                val query = DownloadManager.Query().setFilterById(downloadId)
                val cursor = downloadManager.query(query)
                if (cursor.moveToFirst()) {
                    val statusIdx = cursor.getColumnIndex(DownloadManager.COLUMN_STATUS)
                    val bytesIdx = cursor.getColumnIndex(DownloadManager.COLUMN_BYTES_DOWNLOADED_SO_FAR)
                    val totalIdx = cursor.getColumnIndex(DownloadManager.COLUMN_TOTAL_SIZE_BYTES)

                    val status = cursor.getInt(statusIdx)
                    val bytesDownloaded = cursor.getLong(bytesIdx)
                    val totalBytes = cursor.getLong(totalIdx)

                    if (totalBytes > 0) {
                        onProgress(bytesDownloaded.toFloat() / totalBytes.toFloat())
                    }

                    if (status == DownloadManager.STATUS_SUCCESSFUL ||
                        status == DownloadManager.STATUS_FAILED) {
                        downloading = false
                    }
                }
                cursor.close()
                Thread.sleep(300)
            }
        }.start()

    } catch (e: Exception) {
        onError("Error: ${e.message}")
    }
}

/**
 * Opens the system installer for the given APK file.
 */
fun installApk(context: Context, file: File, onLaunched: () -> Unit = {}, onError: (String) -> Unit = {}) {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
        if (!context.packageManager.canRequestPackageInstalls()) {
            val intent = Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES).apply {
                data = Uri.parse("package:${context.packageName}")
                flags = Intent.FLAG_ACTIVITY_NEW_TASK
            }
            context.startActivity(intent)
            Toast.makeText(context, "Por favor autoriza la instalación de aplicaciones", Toast.LENGTH_LONG).show()
            onError("falta el permiso para instalar aplicaciones desconocidas")
            return
        }
    }
    try {
        val uri = FileProvider.getUriForFile(
            context,
            "${context.packageName}.fileprovider",
            file
        )
        val intent = Intent(Intent.ACTION_VIEW).apply {
            setDataAndType(uri, "application/vnd.android.package-archive")
            // CLEAR_TASK: una tarea vieja del instalador reutilizada se tragaba el intent nuevo
            flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK or
                Intent.FLAG_GRANT_READ_URI_PERMISSION
        }
        context.startActivity(intent)
        Log.i("MaxAppsHubCli", "installApk launched")
        onLaunched()
    } catch (e: Exception) {
        Toast.makeText(context, "No se pudo abrir el instalador: ${e.message}", Toast.LENGTH_LONG).show()
        onError("No se pudo abrir el instalador: ${e.message}")
    }
}
