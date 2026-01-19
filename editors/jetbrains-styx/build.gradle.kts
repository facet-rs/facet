plugins {
    id("java")
    id("org.jetbrains.kotlin.jvm") version "2.0.21"
    id("org.jetbrains.intellij.platform") version "2.10.5"
}

group = "com.bearcove"
version = "0.1.0"

repositories {
    mavenCentral()
    intellijPlatform {
        defaultRepositories()
    }
}

dependencies {
    intellijPlatform {
        intellijIdeaCommunity("2024.3")
        pluginVerifier()
        zipSigner()
        plugin("com.redhat.devtools.lsp4ij", "0.17.0")
    }
}

kotlin {
    jvmToolchain(21)
}

tasks {
    patchPluginXml {
        sinceBuild.set("243")
        untilBuild.set("251.*")
    }
}
