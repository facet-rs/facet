package com.bearcove.styx

import com.intellij.openapi.project.Project
import com.redhat.devtools.lsp4ij.server.ProcessStreamConnectionProvider
import com.redhat.devtools.lsp4ij.server.StreamConnectionProvider
import com.redhat.devtools.lsp4ij.LanguageServerFactory
import java.io.File

class StyxLanguageServer : ProcessStreamConnectionProvider() {
    init {
        val styxPath = findStyx()
        commands = listOf(styxPath, "@lsp")
    }

    private fun findStyx(): String {
        // Common installation locations
        val candidates = listOf(
            // Cargo (Rust)
            "${System.getProperty("user.home")}/.cargo/bin/styx",
            // Homebrew on macOS (Apple Silicon)
            "/opt/homebrew/bin/styx",
            // Homebrew on macOS (Intel)
            "/usr/local/bin/styx",
            // Linux standard locations
            "/usr/bin/styx",
            "/usr/local/bin/styx",
            // User local bin
            "${System.getProperty("user.home")}/.local/bin/styx",
        )

        for (candidate in candidates) {
            if (File(candidate).canExecute()) {
                return candidate
            }
        }

        // Fall back to bare name, hope it's in PATH
        return "styx"
    }
}

class StyxLspServerFactory : LanguageServerFactory {
    override fun createConnectionProvider(project: Project): StreamConnectionProvider {
        return StyxLanguageServer()
    }
}
