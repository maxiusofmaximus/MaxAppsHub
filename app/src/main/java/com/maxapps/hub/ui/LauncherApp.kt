package com.maxapps.hub.ui

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
import com.maxapps.hub.data.AppRegistry
import com.maxapps.hub.data.HubPrefs
import com.maxapps.hub.data.ManagedApp
import com.maxapps.hub.data.isNewerVersion
import com.maxapps.hub.install.downloadAndInstall
import com.maxapps.hub.install.getInstalledVersion
import com.maxapps.hub.network.GitHubApi
import com.maxapps.hub.network.ReleaseInfo
import com.maxapps.hub.ui.theme.*
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

private const val HUB_OWNER = "maxiusofmaximus"
private const val HUB_REPO = "maxappshub"

// State for the launcher's own update
data class HubUpdateState(
    val installedVersion: String? = null,
    val latestRelease: ReleaseInfo? = null,
    val isLoading: Boolean = false,
    val isDownloading: Boolean = false,
    val downloadProgress: Float = 0f,
    val error: String? = null,
    val checked: Boolean = false
)

// State for each managed app
data class AppState(
    val app: ManagedApp,
    val installedVersion: String? = null,
    val latestRelease: ReleaseInfo? = null,
    val includePrereleases: Boolean = false,
    val isLoading: Boolean = true,
    val isDownloading: Boolean = false,
    val downloadProgress: Float = 0f,
    val error: String? = null
)

private fun List<AppState>.updated(index: Int, transform: (AppState) -> AppState): List<AppState> =
    toMutableList().also { it[index] = transform(it[index]) }

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
                    installedVersion = getInstalledVersion(context, app.packageId),
                    includePrereleases = HubPrefs.includePrereleases(context, app.packageId)
                )
            }
        )
    }

    var hubState by remember {
        mutableStateOf(
            HubUpdateState(installedVersion = getInstalledVersion(context, context.packageName))
        )
    }

    fun checkHubUpdate() {
        scope.launch {
            hubState = hubState.copy(isLoading = true, error = null)
            val release = GitHubApi.fetchRelease(HUB_OWNER, HUB_REPO, includePrereleases = false)
            hubState = hubState.copy(
                latestRelease = release,
                installedVersion = getInstalledVersion(context, context.packageName),
                isLoading = false,
                checked = true,
                error = if (release == null) "No se pudo obtener información" else null
            )
        }
    }

    fun refreshApp(index: Int) {
        scope.launch {
            appStates = appStates.updated(index) { it.copy(isLoading = true, error = null) }
            val current = appStates[index]
            val release = GitHubApi.fetchRelease(current.app.githubOwner, current.app.githubRepo, current.includePrereleases)
            appStates = appStates.updated(index) {
                it.copy(
                    latestRelease = release,
                    isLoading = false,
                    installedVersion = getInstalledVersion(context, it.app.packageId),
                    error = if (release == null) "No se pudo obtener información" else null
                )
            }
        }
    }

    // Fetch latest releases on launch
    LaunchedEffect(Unit) {
        checkHubUpdate()
        appStates = appStates.map { state ->
            state.copy(isLoading = true)
        }
        appStates = appStates.map { state ->
            val release = GitHubApi.fetchRelease(
                state.app.githubOwner,
                state.app.githubRepo,
                state.includePrereleases
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
                HubUpdateCard(
                    state = hubState,
                    isDark = isDark,
                    onCheck = { checkHubUpdate() },
                    onUpdate = {
                        hubState.latestRelease?.let { release ->
                            hubState = hubState.copy(isDownloading = true, downloadProgress = 0f, error = null)
                            downloadAndInstall(
                                context = context,
                                apkUrl = release.apkDownloadUrl,
                                fileName = release.apkFileName,
                                onProgress = { progress ->
                                    hubState = hubState.copy(downloadProgress = progress)
                                },
                                onComplete = {
                                    hubState = hubState.copy(
                                        isDownloading = false,
                                        installedVersion = getInstalledVersion(context, context.packageName)
                                    )
                                },
                                onError = { error ->
                                    hubState = hubState.copy(isDownloading = false, error = error)
                                }
                            )
                        }
                    }
                )
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
                                fileName = release.apkFileName,
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
                        onRefresh = { refreshApp(index) },
                        onTogglePrereleases = { enabled ->
                            HubPrefs.setIncludePrereleases(context, state.app.packageId, enabled)
                            appStates = appStates.updated(index) { it.copy(includePrereleases = enabled) }
                            refreshApp(index)
                        }
                    )
                }
            }

            Spacer(modifier = Modifier.height(32.dp))

            // Footer
            Text(
                text = "MaxApps Hub v${getInstalledVersion(context, context.packageName) ?: "—"}",
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
fun HubUpdateCard(
    state: HubUpdateState,
    isDark: Boolean,
    onCheck: () -> Unit,
    onUpdate: () -> Unit
) {
    val accentColor = Indigo
    val latestTag = state.latestRelease?.tagName
    val hasUpdate = latestTag != null &&
            state.installedVersion != null &&
            isNewerVersion(latestTag, state.installedVersion)

    Card(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(20.dp),
        colors = CardDefaults.cardColors(
            containerColor = if (isDark) DarkSurfaceVariant else LightSurfaceVariant
        ),
        elevation = CardDefaults.cardElevation(defaultElevation = 2.dp)
    ) {
        Column(modifier = Modifier.padding(20.dp)) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically
            ) {
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
                    Text("📱", fontSize = 28.sp)
                }

                Spacer(modifier = Modifier.width(16.dp))

                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = "MaxApps Hub",
                        fontSize = 18.sp,
                        fontWeight = FontWeight.Bold,
                        color = MaterialTheme.colorScheme.onSurface
                    )
                    Text(
                        text = "Launcher de aplicaciones",
                        fontSize = 13.sp,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        lineHeight = 18.sp
                    )
                }

                IconButton(onClick = onCheck) {
                    Icon(
                        Icons.Default.Refresh,
                        contentDescription = "Buscar actualizaciones",
                        tint = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                }
            }

            Spacer(modifier = Modifier.height(16.dp))

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
                        text = state.installedVersion ?: "—",
                        fontSize = 16.sp,
                        fontWeight = FontWeight.Bold,
                        color = Emerald
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
                            text = latestTag ?: "—",
                            fontSize = 16.sp,
                            fontWeight = FontWeight.Bold,
                            color = if (hasUpdate) Pink else MaterialTheme.colorScheme.onSurface
                        )
                    }
                }
            }

            Spacer(modifier = Modifier.height(16.dp))

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

            if (state.error != null && !state.isLoading && !state.isDownloading) {
                Text(
                    text = "⚠️ ${state.error}",
                    fontSize = 13.sp,
                    color = Color(0xFFEF4444),
                    modifier = Modifier.fillMaxWidth()
                )
                Spacer(modifier = Modifier.height(8.dp))
            }

            if (!state.isDownloading) {
                if (hasUpdate) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(12.dp)
                    ) {
                        Button(
                            onClick = onUpdate,
                            modifier = Modifier
                                .weight(1f)
                                .height(48.dp),
                            shape = RoundedCornerShape(14.dp),
                            colors = ButtonDefaults.buttonColors(containerColor = accentColor)
                        ) {
                            Icon(
                                Icons.Default.KeyboardArrowUp,
                                contentDescription = null,
                                modifier = Modifier.size(20.dp)
                            )
                            Spacer(modifier = Modifier.width(8.dp))
                            Text("Actualizar Hub", fontWeight = FontWeight.Bold)
                        }
                        OutlinedButton(
                            onClick = onCheck,
                            modifier = Modifier.height(48.dp),
                            shape = RoundedCornerShape(14.dp)
                        ) {
                            Text("Buscar", fontWeight = FontWeight.Bold)
                        }
                    }
                } else {
                    Button(
                        onClick = onCheck,
                        modifier = Modifier
                            .fillMaxWidth()
                            .height(48.dp),
                        shape = RoundedCornerShape(14.dp),
                        colors = ButtonDefaults.buttonColors(containerColor = accentColor)
                    ) {
                        Icon(
                            Icons.Default.Refresh,
                            contentDescription = null,
                            modifier = Modifier.size(20.dp)
                        )
                        Spacer(modifier = Modifier.width(8.dp))
                        Text(
                            text = if (state.checked && state.error == null) "Estás en la última versión" else "Buscar actualizaciones",
                            fontWeight = FontWeight.Bold
                        )
                    }
                }
            }
        }
    }
}

@Composable
fun AppCard(
    state: AppState,
    isDark: Boolean,
    onInstall: () -> Unit,
    onOpen: () -> Unit,
    onRefresh: () -> Unit,
    onTogglePrereleases: (Boolean) -> Unit
) {
    val accentColor = Color(state.app.accentColorHex)
    val hasUpdate = state.installedVersion != null &&
            state.latestRelease != null &&
            isNewerVersion(state.latestRelease.tagName, state.installedVersion)
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

            // Canal de actualización
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically
            ) {
                Text(
                    text = "Incluir alfas",
                    fontSize = 14.sp,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.weight(1f)
                )
                Switch(checked = state.includePrereleases, onCheckedChange = onTogglePrereleases)
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
