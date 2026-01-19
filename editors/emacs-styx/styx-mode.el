;;; styx-mode.el --- Major mode for Styx configuration files -*- lexical-binding: t; -*-

;; Copyright (C) 2024 Bearcove
;; Author: Bearcove
;; URL: https://github.com/bearcove/styx
;; Version: 0.1.0
;; Package-Requires: ((emacs "27.1"))
;; Keywords: languages, configuration

;;; Commentary:

;; Major mode for editing Styx configuration files.
;; Provides syntax highlighting and LSP integration via eglot or lsp-mode.

;;; Code:

(defgroup styx nil
  "Major mode for Styx configuration files."
  :group 'languages
  :prefix "styx-")

(defcustom styx-indent-offset 2
  "Indentation offset for Styx mode."
  :type 'integer
  :group 'styx)

;; Syntax table
(defvar styx-mode-syntax-table
  (let ((table (make-syntax-table)))
    ;; Comments: // to end of line
    (modify-syntax-entry ?/ ". 12" table)
    (modify-syntax-entry ?\n ">" table)
    ;; Strings
    (modify-syntax-entry ?\" "\"" table)
    ;; Brackets
    (modify-syntax-entry ?\{ "(}" table)
    (modify-syntax-entry ?\} "){" table)
    (modify-syntax-entry ?\( "()" table)
    (modify-syntax-entry ?\) ")(" table)
    ;; Word constituents
    (modify-syntax-entry ?_ "w" table)
    (modify-syntax-entry ?- "w" table)
    table)
  "Syntax table for `styx-mode'.")

;; Font-lock keywords
(defvar styx-font-lock-keywords
  `(
    ;; Doc comments
    ("///.*$" . font-lock-doc-face)
    ;; Line comments
    ("//[^/].*$" . font-lock-comment-face)
    ;; Tags: @name
    ("@[A-Za-z_][A-Za-z0-9_-]*" . font-lock-type-face)
    ;; Unit: bare @
    ("@\\(?:[^A-Za-z_]\\|$\\)" . font-lock-constant-face)
    ;; Attributes: key>value
    ("\\([A-Za-z_][A-Za-z0-9_-]*\\)\\s-*>" 1 font-lock-variable-name-face)
    ;; Attribute arrow
    (">" . font-lock-keyword-face)
    ;; Heredoc delimiters
    ("<<\\([A-Za-z_][A-Za-z0-9_]*\\)" 1 font-lock-string-face)
    ;; Raw strings
    ("r#*\"" . font-lock-string-face)
    )
  "Font-lock keywords for `styx-mode'.")

;; Indentation
(defun styx-indent-line ()
  "Indent current line for Styx mode."
  (interactive)
  (let ((indent 0)
        (pos (- (point-max) (point))))
    (save-excursion
      (beginning-of-line)
      (if (bobp)
          (setq indent 0)
        (let ((cur-indent 0))
          ;; Look at previous non-blank line
          (forward-line -1)
          (while (and (not (bobp)) (looking-at "^\\s-*$"))
            (forward-line -1))
          (setq cur-indent (current-indentation))
          ;; Check if previous line opens a block
          (end-of-line)
          (if (looking-back "[{(]\\s-*$" (line-beginning-position))
              (setq indent (+ cur-indent styx-indent-offset))
            (setq indent cur-indent))
          ;; Check if current line closes a block
          (forward-line 1)
          (beginning-of-line)
          (when (looking-at "^\\s-*[})]")
            (setq indent (max 0 (- indent styx-indent-offset)))))))
    (indent-line-to indent)
    (when (> (- (point-max) pos) (point))
      (goto-char (- (point-max) pos)))))

;;;###autoload
(define-derived-mode styx-mode prog-mode "Styx"
  "Major mode for editing Styx configuration files."
  :syntax-table styx-mode-syntax-table
  (setq-local comment-start "// ")
  (setq-local comment-end "")
  (setq-local comment-start-skip "//+\\s-*")
  (setq-local indent-line-function #'styx-indent-line)
  (setq-local font-lock-defaults '(styx-font-lock-keywords)))

;;;###autoload
(add-to-list 'auto-mode-alist '("\\.styx\\'" . styx-mode))

;; LSP integration (eglot)
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '(styx-mode . ("styx" "lsp"))))

;; LSP integration (lsp-mode)
(with-eval-after-load 'lsp-mode
  (add-to-list 'lsp-language-id-configuration '(styx-mode . "styx"))
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection '("styx" "lsp"))
    :activation-fn (lsp-activate-on "styx")
    :server-id 'styx-lsp)))

(provide 'styx-mode)

;;; styx-mode.el ends here
