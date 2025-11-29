package com.atakmap.android.hive.plugin.ui.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

/**
 * HIVE Plugin color scheme - tactical colors suitable for field use
 */

// Primary colors - tactical green
private val HiveGreen = Color(0xFF4CAF50)
private val HiveGreenDark = Color(0xFF388E3C)
private val HiveGreenLight = Color(0xFFC8E6C9)

// Secondary colors - amber for warnings/attention
private val HiveAmber = Color(0xFFFFC107)
private val HiveAmberDark = Color(0xFFFFA000)

// Error colors
private val HiveRed = Color(0xFFE53935)
private val HiveRedDark = Color(0xFFC62828)

// Background colors optimized for daylight and night use
private val DarkBackground = Color(0xFF121212)
private val DarkSurface = Color(0xFF1E1E1E)
private val LightBackground = Color(0xFFF5F5F5)
private val LightSurface = Color(0xFFFFFFFF)

private val DarkColorScheme = darkColorScheme(
    primary = HiveGreen,
    onPrimary = Color.White,
    primaryContainer = HiveGreenDark,
    onPrimaryContainer = HiveGreenLight,

    secondary = HiveAmber,
    onSecondary = Color.Black,
    secondaryContainer = HiveAmberDark,
    onSecondaryContainer = Color.White,

    tertiary = Color(0xFF03A9F4),  // Info blue
    onTertiary = Color.White,

    error = HiveRed,
    onError = Color.White,
    errorContainer = HiveRedDark,
    onErrorContainer = Color.White,

    background = DarkBackground,
    onBackground = Color.White,
    surface = DarkSurface,
    onSurface = Color.White,
    surfaceVariant = Color(0xFF2D2D2D),
    onSurfaceVariant = Color(0xFFCACACA),

    outline = Color(0xFF5C5C5C)
)

private val LightColorScheme = lightColorScheme(
    primary = HiveGreenDark,
    onPrimary = Color.White,
    primaryContainer = HiveGreenLight,
    onPrimaryContainer = HiveGreenDark,

    secondary = HiveAmberDark,
    onSecondary = Color.Black,
    secondaryContainer = Color(0xFFFFECB3),
    onSecondaryContainer = HiveAmberDark,

    tertiary = Color(0xFF0288D1),  // Info blue
    onTertiary = Color.White,

    error = HiveRedDark,
    onError = Color.White,
    errorContainer = Color(0xFFFFCDD2),
    onErrorContainer = HiveRedDark,

    background = LightBackground,
    onBackground = Color.Black,
    surface = LightSurface,
    onSurface = Color.Black,
    surfaceVariant = Color(0xFFE0E0E0),
    onSurfaceVariant = Color(0xFF424242),

    outline = Color(0xFF9E9E9E)
)

/**
 * HIVE Plugin theme for Jetpack Compose UI
 *
 * Provides tactical-appropriate colors that work in both daylight
 * and night conditions. Defaults to dark theme for reduced eye strain.
 */
@Composable
fun HivePluginTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    content: @Composable () -> Unit
) {
    val colorScheme = if (darkTheme) DarkColorScheme else LightColorScheme

    MaterialTheme(
        colorScheme = colorScheme,
        content = content
    )
}
