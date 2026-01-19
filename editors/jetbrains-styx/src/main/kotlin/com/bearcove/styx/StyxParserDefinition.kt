package com.bearcove.styx

import com.intellij.lang.ASTNode
import com.intellij.lang.ParserDefinition
import com.intellij.lang.PsiParser
import com.intellij.lexer.Lexer
import com.intellij.lexer.EmptyLexer
import com.intellij.openapi.project.Project
import com.intellij.psi.FileViewProvider
import com.intellij.psi.PsiElement
import com.intellij.psi.PsiFile
import com.intellij.psi.tree.IFileElementType
import com.intellij.psi.tree.TokenSet
import com.intellij.extapi.psi.PsiFileBase

class StyxFile(viewProvider: FileViewProvider) : PsiFileBase(viewProvider, StyxLanguage) {
    override fun getFileType() = StyxFileType
}

class StyxParserDefinition : ParserDefinition {
    companion object {
        val FILE = IFileElementType(StyxLanguage)
    }

    override fun createLexer(project: Project?): Lexer = EmptyLexer()

    override fun createParser(project: Project?): PsiParser = PsiParser { root, builder ->
        val marker = builder.mark()
        while (!builder.eof()) {
            builder.advanceLexer()
        }
        marker.done(root)
        builder.treeBuilt
    }

    override fun getFileNodeType(): IFileElementType = FILE

    override fun getWhitespaceTokens(): TokenSet = TokenSet.EMPTY

    override fun getCommentTokens(): TokenSet = TokenSet.EMPTY

    override fun getStringLiteralElements(): TokenSet = TokenSet.EMPTY

    override fun createElement(node: ASTNode): PsiElement =
        throw UnsupportedOperationException("Not implemented")

    override fun createFile(viewProvider: FileViewProvider): PsiFile = StyxFile(viewProvider)
}
