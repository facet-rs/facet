package com.bearcove.styx

import com.intellij.openapi.fileTypes.LanguageFileType
import javax.swing.Icon

object StyxFileType : LanguageFileType(StyxLanguage) {
    override fun getName(): String = "Styx"
    override fun getDescription(): String = "Styx configuration file"
    override fun getDefaultExtension(): String = "styx"
    override fun getIcon(): Icon? = null // TODO: Add icon
}
