package com.maxapps.hub.data

/**
 * Represents an app managed by the launcher.
 */
data class ManagedApp(
    val name: String,
    val packageId: String,
    val githubOwner: String,
    val githubRepo: String,
    val icon: String,
    val description: String,
    val accentColorHex: Long
)

/**
 * Registry of all apps that this launcher can manage.
 * Add new apps here as they are created.
 */
object AppRegistry {
    val apps = listOf(
        ManagedApp(
            name = "Flashcards Química",
            packageId = "com.flashcards.quimica",
            githubOwner = "maxiusofmaximus",
            githubRepo = "APK-Android-Ley-de-Gases",
            icon = "🧪",
            description = "Tarjetas de estudio interactivas sobre la Ley de Gases, con modo examen y repaso.",
            accentColorHex = 0xFF6366F1
        )
    )
}
