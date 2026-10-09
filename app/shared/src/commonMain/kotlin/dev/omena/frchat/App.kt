package dev.omena.frchat

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

private val Ink = Color(0xFF111719)
private val Rail = Color(0xFF0B1113)
private val Panel = Color(0xFF182124)
private val PanelRaised = Color(0xFF202C30)
private val Line = Color(0xFF2B393D)
private val TextPrimary = Color(0xFFF1F5F3)
private val TextMuted = Color(0xFF9BAAA7)
private val Mint = Color(0xFF69D5B0)
private val Coral = Color(0xFFFF8D72)

external fun frChatInit(homeserver: String, storePath: String): String
external fun frChatBeginAuth(providerId: String, request: String): String
external fun frChatContinueAuth(providerId: String, response: String): String
external fun frChatAccounts(): String
external fun frChatServers(accountRef: String): String
external fun frChatSync(accountRef: String): String
external fun frChatExecute(request: String): String
external fun frChatConversations(accountRef: String): String
external fun frChatTimeline(accountRef: String, conversationRef: String, limit: Int): String
external fun frChatDescriptor(): String
external fun frChatFree(ptr: Long)

object FrChatNative {
    init {
        try {
            System.loadLibrary("frchat_ffi")
        } catch (_: UnsatisfiedLinkError) {
        }
    }
}

@Composable
fun FrChatApp() {
    MaterialTheme(
        colorScheme = darkColorScheme(
            primary = Mint,
            secondary = Coral,
            background = Ink,
            surface = Panel,
            onPrimary = Ink,
            onSecondary = Ink,
            onBackground = TextPrimary,
            onSurface = TextPrimary,
        ),
    ) {
        var showProviderStatus by remember { mutableStateOf(false) }
        Surface(modifier = Modifier.fillMaxSize(), color = Ink) {
            BoxWithConstraints(modifier = Modifier.fillMaxSize()) {
                val showNavigation = maxWidth >= 740.dp
                Row(modifier = Modifier.fillMaxSize()) {
                    ServerRail()
                    if (showNavigation) {
                        NavigationPane()
                    }
                    EmptyConversation(
                        onConnect = { showProviderStatus = true },
                        modifier = Modifier.weight(1f),
                    )
                }
            }
        }
        if (showProviderStatus) {
            ProviderStatusDialog(onDismiss = { showProviderStatus = false })
        }
    }
}

@Composable
private fun ServerRail() {
    Column(
        modifier = Modifier
            .width(72.dp)
            .fillMaxHeight()
            .background(Rail)
            .padding(vertical = 16.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Box(
            modifier = Modifier
                .size(44.dp)
                .background(Mint, RoundedCornerShape(15.dp)),
            contentAlignment = Alignment.Center,
        ) {
            Text("fr", color = Ink, fontWeight = FontWeight.Black, fontSize = 17.sp)
        }
        Spacer(modifier = Modifier.height(4.dp))
        RailMark("+")
        Spacer(modifier = Modifier.weight(1f))
        RailMark("?")
        RailMark("A")
    }
}

@Composable
private fun RailMark(label: String) {
    Box(
        modifier = Modifier
            .size(44.dp)
            .background(Panel, CircleShape)
            .border(1.dp, Line, CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        Text(label, color = TextMuted, fontSize = 18.sp, fontWeight = FontWeight.Medium)
    }
}

@Composable
private fun NavigationPane() {
    Column(
        modifier = Modifier
            .width(264.dp)
            .fillMaxHeight()
            .background(Panel)
            .border(width = 1.dp, color = Line),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(58.dp)
                .padding(horizontal = 18.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text("Servers", color = TextPrimary, fontWeight = FontWeight.Bold, fontSize = 16.sp)
            Text("...", color = TextMuted, fontSize = 19.sp)
        }
        Box(modifier = Modifier.fillMaxWidth().height(1.dp).background(Line))
        Column(modifier = Modifier.padding(horizontal = 14.dp, vertical = 18.dp)) {
            Text("DIRECT MESSAGES", color = TextMuted, fontSize = 11.sp, fontWeight = FontWeight.Bold)
            Spacer(modifier = Modifier.height(16.dp))
            Text("No conversations", color = TextMuted, fontSize = 14.sp)
            Spacer(modifier = Modifier.height(24.dp))
            Text("CHANNELS", color = TextMuted, fontSize = 11.sp, fontWeight = FontWeight.Bold)
            Spacer(modifier = Modifier.height(16.dp))
            Text("Channels appear here", color = TextMuted, fontSize = 14.sp)
        }
    }
}

@Composable
private fun EmptyConversation(onConnect: () -> Unit, modifier: Modifier = Modifier) {
    Column(modifier = modifier.fillMaxHeight().background(Ink)) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(58.dp)
                .border(width = 1.dp, color = Line)
                .padding(horizontal = 22.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("fr-chat", color = TextPrimary, fontSize = 15.sp, fontWeight = FontWeight.Bold)
            Spacer(modifier = Modifier.width(10.dp))
            Text("/", color = TextMuted)
            Spacer(modifier = Modifier.width(10.dp))
            Text("Matrix", color = TextMuted, fontSize = 14.sp)
        }
        Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 28.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Box(
                    modifier = Modifier
                        .size(76.dp)
                        .background(PanelRaised, RoundedCornerShape(24.dp)),
                    contentAlignment = Alignment.Center,
                ) {
                    Text("f", color = Mint, fontSize = 38.sp, fontWeight = FontWeight.Black)
                }
                Spacer(modifier = Modifier.height(24.dp))
                Text(
                    "Your communities start here",
                    color = TextPrimary,
                    fontSize = 25.sp,
                    fontWeight = FontWeight.Bold,
                    textAlign = TextAlign.Center,
                )
                Spacer(modifier = Modifier.height(10.dp))
                Text(
                    "Connect a Matrix account to see your servers, channels, and conversations.",
                    color = TextMuted,
                    fontSize = 14.sp,
                    lineHeight = 21.sp,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.width(340.dp),
                )
                Spacer(modifier = Modifier.height(24.dp))
                Button(
                    onClick = onConnect,
                    shape = RoundedCornerShape(7.dp),
                    colors = ButtonDefaults.buttonColors(containerColor = Mint, contentColor = Ink),
                ) {
                    Text("Connect Matrix account", fontWeight = FontWeight.Bold)
                }
                Spacer(modifier = Modifier.height(14.dp))
                Text("Linux preview · account connection is not available yet", color = TextMuted, fontSize = 12.sp)
            }
        }
    }
}

@Composable
private fun ProviderStatusDialog(onDismiss: () -> Unit) {
    androidx.compose.material3.AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = Panel,
        title = { Text("Matrix connection", color = TextPrimary) },
        text = {
            Text(
                "The encrypted local store and core contracts are ready. The Matrix provider and sign-in flow are the next implementation step.",
                color = TextMuted,
                lineHeight = 21.sp,
            )
        },
        confirmButton = {
            Button(
                onClick = onDismiss,
                colors = ButtonDefaults.buttonColors(containerColor = Mint, contentColor = Ink),
            ) { Text("Close") }
        },
    )
}
