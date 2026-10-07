package dev.omena.frchat

import androidx.compose.ui.window.Window
import androidx.compose.ui.window.application
import androidx.compose.ui.window.rememberWindowState
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import dev.omena.frchat.FrChatApp

fun main() = application {
    val windowState = rememberWindowState(size = DpSize(1280.dp, 820.dp))
    Window(
        title = "fr-chat",
        onCloseRequest = ::exitApplication,
        state = windowState,
    ) {
        FrChatApp()
    }
}
