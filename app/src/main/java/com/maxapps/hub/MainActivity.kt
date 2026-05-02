package com.maxapps.hub

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import com.maxapps.hub.ui.LauncherApp
import com.maxapps.hub.ui.theme.MaxAppsHubTheme

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            MaxAppsHubTheme {
                LauncherApp()
            }
        }
    }
}
