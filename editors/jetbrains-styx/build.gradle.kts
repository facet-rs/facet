plugins {
    id("java")
    id("org.jetbrains.kotlin.jvm") version "1.9.0"
    id("org.jetbrains.intellij") version "1.16.0"
}

group = "com.bearcove"
version = "0.1.0"

repositories {
    mavenCentral()
}

intellij {
    version.set("2023.2")
    type.set("IC") // IntelliJ IDEA Community Edition
    plugins.set(listOf("com.redhat.devtools.lsp4ij:0.0.2"))
}

tasks {
    withType<JavaCompile> {
        sourceCompatibility = "17"
        targetCompatibility = "17"
    }
    withType<org.jetbrains.kotlin.gradle.tasks.KotlinCompile> {
        kotlinOptions.jvmTarget = "17"
    }

    patchPluginXml {
        sinceBuild.set("232")
        untilBuild.set("242.*")
    }
}
