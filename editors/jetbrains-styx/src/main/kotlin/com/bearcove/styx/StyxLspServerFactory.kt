package com.bearcove.styx

import com.intellij.openapi.project.Project
import com.redhat.devtools.lsp4ij.server.ProcessStreamConnectionProvider
import com.redhat.devtools.lsp4ij.server.StreamConnectionProvider
import com.redhat.devtools.lsp4ij.LanguageServerFactory

class StyxLanguageServer : ProcessStreamConnectionProvider() {
    init {
        commands = listOf("styx", "@lsp")
    }
}

class StyxLspServerFactory : LanguageServerFactory {
    override fun createConnectionProvider(project: Project): StreamConnectionProvider {
        return StyxLanguageServer()
    }
}
