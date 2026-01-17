package com.bearcove.styx

import com.intellij.psi.tree.IElementType

object StyxTokenTypes {
    val LBRACE = IElementType("LBRACE", StyxLanguage)
    val RBRACE = IElementType("RBRACE", StyxLanguage)
    val LPAREN = IElementType("LPAREN", StyxLanguage)
    val RPAREN = IElementType("RPAREN", StyxLanguage)
    val COMMA = IElementType("COMMA", StyxLanguage)
    val GT = IElementType("GT", StyxLanguage)
    val AT = IElementType("AT", StyxLanguage)
    val STRING = IElementType("STRING", StyxLanguage)
    val RAW_STRING = IElementType("RAW_STRING", StyxLanguage)
    val HEREDOC = IElementType("HEREDOC", StyxLanguage)
    val BARE_SCALAR = IElementType("BARE_SCALAR", StyxLanguage)
    val TAG = IElementType("TAG", StyxLanguage)
    val LINE_COMMENT = IElementType("LINE_COMMENT", StyxLanguage)
    val DOC_COMMENT = IElementType("DOC_COMMENT", StyxLanguage)
    val WHITESPACE = IElementType("WHITESPACE", StyxLanguage)
    val NEWLINE = IElementType("NEWLINE", StyxLanguage)
}
