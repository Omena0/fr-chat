import org.jetbrains.compose.desktop.application.dsl.TargetFormat

plugins {
    alias(libs.plugins.kotlinJvm)
    alias(libs.plugins.composeMultiplatform)
    alias(libs.plugins.composeCompiler)
}

dependencies {
    implementation(project(":app:shared"))
    implementation(compose.desktop.currentOs)
    implementation(libs.kotlinx.coroutines.swing)
    implementation(libs.compose.ui.tooling.preview)
}

compose.desktop {
    application {
        mainClass = "dev.omena.frchat.MainKt"
        nativeDistributions {
            targetFormats(TargetFormat.Deb)
            packageName = "fr-chat"
            packageVersion = "0.1.0"
        }
    }
}
