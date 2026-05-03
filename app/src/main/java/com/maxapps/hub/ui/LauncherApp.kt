package com.maxapps.hub.ui

import android.app.DownloadManager
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.Settings
import android.widget.Toast
import androidx.compose.animation.*
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.FileProvider
import com.maxapps.hub.data.AppRegistry
import com.maxapps.hub.data.ManagedApp
import com.maxapps.hub.network.GitHubApi
import com.maxapps.hub.network.ReleaseInfo
import com.maxapps.hub.ui.theme.*
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import java.io.File

// State for each managed app
data class AppState(
    val app: ManagedApp,
    val installedVersion: String? = null,
    val latestRelease: ReleaseInfo? = null,
    val isLoading: Boolean = true,
    val isDownloading: Boolean = false,
    val downloadProgress: Float = 0f,
    val error: String? = null
)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LauncherApp() {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val isDark = MaterialTheme.colorScheme.background == DarkBackground

    var appStates by remember {
        mutableStateOf(
            AppRegistry.apps.map { app ->
                AppState(
                    app = app,
                    installedVersion = getInstalledVersion(context, app.packageId)
                )
            }
        )
    }

    // Fetch latest releases on launch
    LaunchedEffect(Unit) {
        appStates = appStates.map { state ->
            state.copy(isLoading = true)
        }
        appStates = appStates.map { state ->
            val release = GitHubApi.fetchLatestRelease(
                state.app.githubOwner,
                state.app.githubRepo
            )
            state.copy(
                latestRelease = release,
                isLoading = false,
                error = if (release == null) "No se pudo obtener información" else null
            )
        }
    }

    Scaffold(
        containerColor = MaterialTheme.colorScheme.background,
        topBar = {
            TopAppBar(
                title = {},
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = Color.Transparent
                )
            )
        }
    ) { paddingValues ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(paddingValues)
                .verticalScroll(rememberScrollState())
        ) {
            // Header
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 24.dp),
                horizontalAlignment = Alignment.CenterHorizontally
            ) {
                // Logo
                Box(
                    modifier = Modifier
                        .size(72.dp)
                        .clip(RoundedCornerShape(20.dp))
                        .background(
                            Brush.linearGradient(
                                colors = listOf(Indigo, Pink)
                            )
                        ),
                    contentAlignment = Alignment.Center
                ) {
                    Text("📱", fontSize = 36.sp)
                }

                Spacer(modifier = Modifier.height(16.dp))

                Text(
                    text = "MaxApps Hub",
                    fontSize = 28.sp,
                    fontWeight = FontWeight.ExtraBold,
                    color = MaterialTheme.colorScheme.onBackground
                )

                Spacer(modifier = Modifier.height(4.dp))

                Text(
                    text = "Gestiona tus aplicaciones",
                    fontSize = 16.sp,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
            }

            Spacer(modifier = Modifier.height(32.dp))

            // App cards
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 16.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp)
            ) {
                appStates.forEachIndexed { index, state ->
                    AppCard(
                        state = state,
                        isDark = isDark,
                        onInstall = {
                            val release = state.latestRelease ?: return@AppCard
                            appStates = appStates.toMutableList().also { list ->
                                list[index] = state.copy(isDownloading = true, downloadProgress = 0f)
                            }
                            downloadAndInstall(
                                context = context,
                                apkUrl = release.apkDownloadUrl,
                                fileName = "FlashcardsQuimica_${release.tagName}.apk",
                                onProgress = { progress ->
                                    appStates = appStates.toMutableList().also { list ->
                                        list[index] = list[index].copy(downloadProgress = progress)
                                    }
                                },
                                onComplete = {
                                    appStates = appStates.toMutableList().also { list ->
                                        list[index] = list[index].copy(
                                            isDownloading = false,
                                            installedVersion = getInstalledVersion(context, state.app.packageId)
                                        )
                                    }
                                },
                                onError = { error ->
                                    appStates = appStates.toMutableList().also { list ->
                                        list[index] = list[index].copy(
                                            isDownloading = false,
                                            error = error
                                        )
                                    }
                                }
                            )
                        },
                        onOpen = {
                            val intent = context.packageManager.getLaunchIntentForPackage(state.app.packageId)
                            if (intent != null) {
                                context.startActivity(intent)
                            }
                        },
                        onRefresh = {
                            scope.launch {
                                appStates = appStates.toMutableList().also { list ->
                                    list[index] = state.copy(isLoading = true, error = null)
                                }
                                val release = GitHubApi.fetchLatestRelease(
                                    state.app.githubOwner,
                                    state.app.githubRepo
                                )
                                appStates = appStates.toMutableList().also { list ->
                                    list[index] = list[index].copy(
                                        latestRelease = release,
                                        isLoading = false,
                                        installedVersion = getInstalledVersion(context, state.app.packageId),
                                        error = if (release == null) "No se pudo obtener información" else null
                                    )
                                }
                            }
                        }
                    )
                }
            }

            Spacer(modifier = Modifier.height(32.dp))

            // Footer
            Text(
                text = "MaxApps Hub v1.0.0",
                fontSize = 12.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.5f),
                textAlign = TextAlign.Center,
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(bottom = 24.dp)
            )
        }
    }
}

@Composable
fun AppCard(
    state: AppState,
    isDark: Boolean,
    onInstall: () -> Unit,
    onOpen: () -> Unit,
    onRefresh: () -> Unit
) {
    val accentColor = Color(state.app.accentColorHex)
    val hasUpdate = state.installedVersion != null &&
            state.latestRelease != null &&
            state.installedVersion != state.latestRelease.tagName.removePrefix("v")
    val isInstalled = state.installedVersion != null
    val isUpToDate = isInstalled && !hasUpdate

    Card(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(20.dp),
        colors = CardDefaults.cardColors(
            containerColor = if (isDark) DarkSurfaceVariant else LightSurfaceVariant
        ),
        elevation = CardDefaults.cardElevation(defaultElevation = 2.dp)
    ) {
        Column(modifier = Modifier.padding(20.dp)) {
            // Top row: icon + info + refresh
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically
            ) {
                // App icon
                Box(
                    modifier = Modifier
                        .size(56.dp)
                        .clip(RoundedCornerShape(16.dp))
                        .background(
                            Brush.linearGradient(
                                colors = listOf(accentColor, accentColor.copy(alpha = 0.6f))
                            )
                        ),
                    contentAlignment = Alignment.Center
                ) {
                    Text(state.app.icon, fontSize = 28.sp)
                }

                Spacer(modifier = Modifier.width(16.dp))

                // App name and description
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = state.app.name,
                        fontSize = 18.sp,
                        fontWeight = FontWeight.Bold,
                        color = MaterialTheme.colorScheme.onSurface
                    )
                    Text(
                        text = state.app.description,
                        fontSize = 13.sp,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        lineHeight = 18.sp,
                        maxLines = 2
                    )
                }

                // Refresh button
                IconButton(onClick = onRefresh) {
                    Icon(
                        Icons.Default.Refresh,
                        contentDescription = "Actualizar",
                        tint = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                }
            }

            Spacer(modifier = Modifier.height(16.dp))

            // Version info
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(12.dp))
                    .background(MaterialTheme.colorScheme.background.copy(alpha = 0.6f))
                    .padding(12.dp),
                horizontalArrangement = Arrangement.SpaceBetween
            ) {
                Column {
                    Text(
                        text = "Instalada",
                        fontSize = 11.sp,
                        fontWeight = FontWeight.SemiBold,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        letterSpacing = 1.sp
                    )
                    Text(
                        text = state.installedVersion ?: "No instalada",
                        fontSize = 16.sp,
                        fontWeight = FontWeight.Bold,
                        color = if (isInstalled) Emerald else MaterialTheme.colorScheme.onSurfaceVariant
                    )
                }

                Column(horizontalAlignment = Alignment.End) {
                    Text(
                        text = "Última",
                        fontSize = 11.sp,
                        fontWeight = FontWeight.SemiBold,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        letterSpacing = 1.sp
                    )
                    if (state.isLoading) {
                        CircularProgressIndicator(
                            modifier = Modifier.size(20.dp),
                            strokeWidth = 2.dp,
                            color = accentColor
                        )
                    } else {
                        Text(
                            text = state.latestRelease?.tagName ?: "—",
                            fontSize = 16.sp,
                            fontWeight = FontWeight.Bold,
                            color = if (hasUpdate) Pink else MaterialTheme.colorScheme.onSurface
                        )
                    }
                }
            }

            Spacer(modifier = Modifier.height(16.dp))

            // Download progress
            if (state.isDownloading) {
                Column {
                    LinearProgressIndicator(
                        progress = { state.downloadProgress },
                        modifier = Modifier
                            .fillMaxWidth()
                            .height(6.dp)
                            .clip(RoundedCornerShape(3.dp)),
                        color = accentColor,
                        trackColor = MaterialTheme.colorScheme.outline
                    )
                    Spacer(modifier = Modifier.height(8.dp))
                    Text(
                        text = "Descargando… ${(state.downloadProgress * 100).toInt()}%",
                        fontSize = 13.sp,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        textAlign = TextAlign.Center,
                        modifier = Modifier.fillMaxWidth()
                    )
                }
            }

            // Error message
            if (state.error != null && !state.isLoading && !state.isDownloading) {
                Text(
                    text = "⚠️ ${state.error}",
                    fontSize = 13.sp,
                    color = Color(0xFFEF4444),
                    modifier = Modifier.fillMaxWidth()
                )
                Spacer(modifier = Modifier.height(8.dp))
            }

            // Action buttons
            if (!state.isDownloading) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(12.dp)
                ) {
                    if (isUpToDate) {
                        // Up to date — Open button
                        Button(
                            onClick = onOpen,
                            modifier = Modifier
                                .weight(1f)
                                .height(48.dp),
                            shape = RoundedCornerShape(14.dp),
                            colors = ButtonDefaults.buttonColors(
                                containerColor = Emerald
                            )
                        ) {
                            Icon(
                                Icons.Default.PlayArrow,
                                contentDescription = null,
                                modifier = Modifier.size(20.dp)
                            )
                            Spacer(modifier = Modifier.width(8.dp))
                            Text("Abrir", fontWeight = FontWeight.Bold)
                        }
                    } else if (hasUpdate) {
                        // Has update — Update + Open
                        Button(
                            onClick = onInstall,
                            modifier = Modifier
                                .weight(1f)
                                .height(48.dp),
                            shape = RoundedCornerShape(14.dp),
                            colors = ButtonDefaults.buttonColors(
                                containerColor = accentColor
                            )
                        ) {
                            Icon(
                                Icons.Default.KeyboardArrowUp,
                                contentDescription = null,
                                modifier = Modifier.size(20.dp)
                            )
                            Spacer(modifier = Modifier.width(4.dp))
                            Text("Actualizar", fontWeight = FontWeight.Bold)
                        }
                        OutlinedButton(
                            onClick = onOpen,
                            modifier = Modifier
                                .weight(1f)
                                .height(48.dp),
                            shape = RoundedCornerShape(14.dp)
                        ) {
                            Icon(
                                Icons.Default.PlayArrow,
                                contentDescription = null,
                                modifier = Modifier.size(20.dp)
                            )
                            Spacer(modifier = Modifier.width(4.dp))
                            Text("Abrir", fontWeight = FontWeight.Bold)
                        }
                    } else if (!isInstalled && state.latestRelease != null) {
                        // Not installed — Install button
                        Button(
                            onClick = onInstall,
                            modifier = Modifier
                                .weight(1f)
                                .height(48.dp),
                            shape = RoundedCornerShape(14.dp),
                            colors = ButtonDefaults.buttonColors(
                                containerColor = accentColor
                            )
                        ) {
                            Icon(
                                Icons.Default.Add,
                                contentDescription = null,
                                modifier = Modifier.size(20.dp)
                            )
                            Spacer(modifier = Modifier.width(8.dp))
                            Text("Instalar", fontWeight = FontWeight.Bold)
                        }
                    }
                }
            }
        }
    }
}

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
                            onComplete()
                            // Trigger install
                            installApk(context, targetFile)
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
fun installApk(context: Context, file: File) {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
        if (!context.packageManager.canRequestPackageInstalls()) {
            val intent = Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES).apply {
                data = Uri.parse("package:${context.packageName}")
                flags = Intent.FLAG_ACTIVITY_NEW_TASK
            }
            context.startActivity(intent)
            Toast.makeText(context, "Por favor autoriza la instalación de aplicaciones", Toast.LENGTH_LONG).show()
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
            flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_GRANT_READ_URI_PERMISSION
        }
        context.startActivity(intent)
    } catch (e: Exception) {
        Toast.makeText(context, "No se pudo abrir el instalador: ${e.message}", Toast.LENGTH_LONG).show()
    }
}
